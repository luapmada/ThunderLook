//! Calendrier commun avec Outlook (J9, étape 4) : synchronisation en arrière-plan et commandes de la boîte
//! « Rendez-vous ». Outlook garde les rendez-vous ; ThunderLook les lit et y écrit par `thunderlook_outlook`.

use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicI64, Ordering},
};

use tauri::{AppHandle, Emitter, Manager, State};
use thunderlook_calendar::{Appointment, Busy, tz::TimeZone};
use thunderlook_model::calendar::{AppointmentForm, CalendarInfo};

use crate::{CalendarCache, clear_calendar, data_dir, now_ms, with_calendar};

/// Intervalle des synchronisations automatiques.
const EVERY_MS: i64 = 5 * 60_000;

#[derive(Default)]
pub struct OutlookStatus {
    running: AtomicBool,
    last_try_ms: AtomicI64,
    error: Mutex<Option<String>>,
}

/// Lance une synchronisation (une seule à la fois) ; l'interface suit l'événement « calendar-changed ».
pub fn start(app: &AppHandle) {
    let st = app.state::<OutlookStatus>();
    st.last_try_ms.store(now_ms(), Ordering::SeqCst);
    if st.running.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let r = data_dir(&app).and_then(|d| thunderlook_outlook::calendar::sync(&d, false));
        let st = app.state::<OutlookStatus>();
        match &r {
            Ok(rep) => {
                *st.error.lock().unwrap_or_else(|p| p.into_inner()) = None;
                if rep.added + rep.updated + rep.removed > 0 {
                    let _ = clear_calendar(&app.state::<CalendarCache>());
                }
            }
            Err(e) => *st.error.lock().unwrap_or_else(|p| p.into_inner()) = Some(e.clone()),
        }
        st.running.store(false, Ordering::SeqCst);
        let _ = app.emit("calendar-changed", ());
    });
}

/// Appelé par l'ordonnanceur chaque minute.
pub fn tick(app: &AppHandle) {
    if now_ms() - app.state::<OutlookStatus>().last_try_ms.load(Ordering::SeqCst) >= EVERY_MS {
        start(app);
    }
}

/// Marque les calendriers communs avec Outlook.
pub fn mark_shared(app: &AppHandle, conn: &thunderlook_store::rusqlite::Connection, calendars: &mut [CalendarInfo]) {
    let Some(l) = data_dir(app).ok().and_then(|d| thunderlook_outlook::calendar::linked(&d)) else { return };
    for c in calendars.iter_mut() {
        let Some(id) = c.id.strip_prefix('c').and_then(|x| x.parse::<i64>().ok()) else { continue };
        let nid: Option<(i64, u32)> = conn.query_row("SELECT source_id, nid FROM folder WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).ok();
        c.shared = nid.is_some_and(|(s, n)| s == l.source_id && l.folders.contains(&n));
        c.outlook_default = c.shared && nid.is_some_and(|(_, n)| n == l.default_folder);
    }
}

/// Mention des calendriers communs affichés : heure de la dernière synchronisation, ou pourquoi elle échoue.
pub fn note(app: &AppHandle) -> String {
    let st = app.state::<OutlookStatus>();
    let last = data_dir(app).ok().and_then(|d| thunderlook_outlook::calendar::linked(&d)).map(|l| l.last_sync_ms).unwrap_or(0);
    let when = if last > 0 {
        chrono::DateTime::from_timestamp_millis(last).map(|t| t.with_timezone(&chrono::Local).format("%d/%m à %H:%M").to_string()).unwrap_or_default()
    } else {
        String::new()
    };
    match st.error.lock().unwrap_or_else(|p| p.into_inner()).clone() {
        Some(e) if when.is_empty() => format!("Calendrier commun avec Outlook : {e}"),
        Some(e) => format!("Calendrier commun avec Outlook, tel qu'à la synchronisation du {when}. {e}"),
        None if st.running.load(Ordering::SeqCst) && when.is_empty() => "Calendrier commun avec Outlook : synchronisation en cours…".into(),
        None => format!("Calendrier commun avec Outlook — synchronisé le {when}."),
    }
}

fn local(ms: i64) -> chrono::NaiveDateTime {
    TimeZone::PARIS.to_local(chrono::DateTime::from_timestamp_millis(ms).map(|t| t.naive_utc()).unwrap_or_default())
}

fn form_of(a: &Appointment, dir: &std::path::Path) -> AppointmentForm {
    let fmt = |ms: i64| local(ms).format("%Y-%m-%dT%H:%M").to_string();
    AppointmentForm {
        id: Some(format!("a{}", a.id)),
        calendar_id: format!("c{}", a.folder_id),
        subject: a.subject.clone(),
        location: a.location.clone().unwrap_or_default(),
        start: fmt(a.start_ms),
        end: fmt(a.end_ms),
        all_day: a.all_day,
        reminder: a.reminder_minutes.map(|m| m as i32),
        busy: match a.busy {
            Busy::Free => 0,
            Busy::Tentative => 1,
            Busy::Busy => 2,
            Busy::OutOfOffice => 3,
            Busy::WorkingElsewhere => 4,
        },
        // Outlook termine le texte par une fin de ligne : retirée, elle ne s'accumule pas d'un enregistrement à l'autre.
        body: thunderlook_calendar::description(a, dir).map(|b| b.trim_end().to_string()).unwrap_or_default(),
        categories: a.categories.join(", "),
        private: a.private,
        recurring: a.recurrence.is_some(),
    }
}

fn message_id(id: &str) -> Result<(i64, Option<String>), String> {
    let rest = id.strip_prefix('a').ok_or("rendez-vous inconnu")?;
    let (n, day) = match rest.split_once('@') {
        Some((n, d)) => (n, Some(d.to_string())),
        None => (rest, None),
    };
    Ok((n.parse().map_err(|_| "rendez-vous inconnu")?, day))
}

/// Boîte « Rendez-vous » : rendez-vous existant (`id`), ou nouveau dans `calendar_id` à partir de `start`.
#[tauri::command(async)]
pub fn appointment_form(app: AppHandle, cache: State<CalendarCache>, id: Option<String>, calendar_id: Option<String>, start: Option<String>) -> Result<AppointmentForm, String> {
    let dir = data_dir(&app)?;
    if let Some(id) = id {
        let (n, _) = message_id(&id)?;
        let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
        return with_calendar(&cache, &conn, &dir, |cal| cal.appointments.iter().find(|a| a.id == n).map(|a| form_of(a, &dir)))?.ok_or_else(|| "rendez-vous introuvable".to_string());
    }
    // Nouveau : à l'heure demandée (sinon l'heure pleine suivante), 30 minutes, rappel 15 minutes (défauts d'Outlook).
    let start = match start {
        Some(s) => chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M").map_err(|_| format!("date invalide : {s}"))?,
        None => {
            let now = local(now_ms());
            now.date().and_hms_opt(now.time().format("%H").to_string().parse::<u32>().unwrap_or(8), 0, 0).unwrap_or(now) + chrono::Duration::hours(1)
        }
    };
    let fmt = |t: chrono::NaiveDateTime| t.format("%Y-%m-%dT%H:%M").to_string();
    Ok(AppointmentForm {
        id: None,
        calendar_id: calendar_id.unwrap_or_default(),
        start: fmt(start),
        end: fmt(start + chrono::Duration::minutes(30)),
        reminder: Some(15),
        busy: 2,
        ..Default::default()
    })
}

fn edit_of(f: &AppointmentForm) -> thunderlook_outlook::calendar::Edit {
    thunderlook_outlook::calendar::Edit {
        subject: f.subject.clone(),
        location: f.location.clone(),
        start: f.start.clone(),
        end: f.end.clone(),
        all_day: f.all_day,
        reminder: f.reminder,
        busy: f.busy,
        body: f.body.clone(),
        categories: f.categories.clone(),
        private: f.private,
    }
}

fn done(app: &AppHandle, r: Result<thunderlook_outlook::calendar::SyncReport, String>, what: &str) -> Result<String, String> {
    let st = app.state::<OutlookStatus>();
    let out = r.map(|_| what.to_string());
    if out.is_ok() {
        *st.error.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
    clear_calendar(&app.state::<CalendarCache>())?;
    let _ = app.emit("calendar-changed", ());
    out
}

/// Enregistre le rendez-vous dans Outlook (création ou modification), puis relit son calendrier.
#[tauri::command(async)]
pub fn appointment_save(app: AppHandle, form: AppointmentForm) -> Result<String, String> {
    if form.subject.trim().is_empty() && form.location.trim().is_empty() {
        return Err("Indiquez un objet ou un lieu.".into());
    }
    if !form.all_day && form.end < form.start {
        return Err("La fin est avant le début.".into());
    }
    let dir = data_dir(&app)?;
    let r = match &form.id {
        Some(id) => thunderlook_outlook::calendar::update(&dir, message_id(id)?.0, edit_of(&form)),
        None => {
            let folder: i64 = form.calendar_id.strip_prefix('c').and_then(|c| c.parse().ok()).ok_or("choisissez un calendrier commun avec Outlook")?;
            thunderlook_outlook::calendar::create(&dir, folder, edit_of(&form))
        }
    };
    done(&app, r, "Rendez-vous enregistré dans Outlook")
}

/// Supprime le rendez-vous dans Outlook : toute la série (`series`), ou seulement l'occurrence affichée.
#[tauri::command(async)]
pub fn appointment_delete(app: AppHandle, id: String, series: bool) -> Result<String, String> {
    let dir = data_dir(&app)?;
    let (n, day) = message_id(&id)?;
    let occurrence = if series { None } else { day };
    let what = if occurrence.is_some() { "Occurrence supprimée dans Outlook" } else { "Rendez-vous supprimé dans Outlook" };
    done(&app, thunderlook_outlook::calendar::delete(&dir, n, occurrence), what)
}

/// Synchronisation demandée par l'interface (ouverture du calendrier, F9).
#[tauri::command(async)]
pub fn outlook_sync(app: AppHandle) {
    start(&app);
}
