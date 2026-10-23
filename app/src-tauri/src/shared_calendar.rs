//! Calendrier de ThunderLook : commandes de la boîte « Rendez-vous » (création, modification, suppression dans la base,
//! sans Outlook).

use tauri::{AppHandle, Emitter, Manager, State};
use thunderlook_calendar::{
    Appointment, Busy,
    recur::Frequency,
    tz::TimeZone,
    write::{Edit, Repeat, RepeatEnd},
};
use thunderlook_model::calendar::{AppointmentForm, CalendarInfo};

use crate::{CalendarCache, clear_calendar, data_dir, now_ms, with_calendar};

/// Plus de lien avec Outlook : les calendriers de la base sont tous modifiables dans ThunderLook.
pub fn start(_app: &AppHandle) {}

pub fn tick(_app: &AppHandle) {}

/// Marque les calendriers de la base (dossiers `c<id>`) comme modifiables.
pub fn mark_shared(_app: &AppHandle, _conn: &thunderlook_store::rusqlite::Connection, calendars: &mut [CalendarInfo]) {
    for c in calendars.iter_mut() {
        c.shared = c.id.strip_prefix('c').is_some_and(|x| x.parse::<i64>().is_ok());
        c.outlook_default = false;
    }
}

/// Mention des calendriers affichés.
pub fn note(_app: &AppHandle) -> String {
    "Calendrier de ThunderLook.".into()
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
        series_date: if a.recurrence.is_some() { fmt(a.start_ms).split('T').next().unwrap_or_default().to_string() } else { String::new() },
        repeat_every: 1,
        repeat_count: 10,
        ..Default::default()
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
        let (n, day) = message_id(&id)?;
        let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
        let mut form = with_calendar(&cache, &conn, &dir, |cal| cal.appointments.iter().find(|a| a.id == n).map(|a| form_of(a, &dir)))?.ok_or_else(|| "rendez-vous introuvable".to_string())?;
        if form.recurring {
            match day {
                Some(day) => {
                    // Occurrence : jour de l'occurrence, heure et durée de la série.
                    let parse = |s: &str| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").map_err(|_| format!("date invalide : {s}"));
                    let (start, end) = (parse(&form.start)?, parse(&form.end)?);
                    let day = chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d").map_err(|_| format!("date invalide : {day}"))?;
                    let new_start = day.and_time(start.time());
                    form.start = new_start.format("%Y-%m-%dT%H:%M").to_string();
                    form.end = (new_start + (end - start)).format("%Y-%m-%dT%H:%M").to_string();
                    form.occurrence = day.format("%Y-%m-%d").to_string();
                    form.scope = "occurrence".into();
                }
                None => form.scope = "series".into(),
            }
        }
        return Ok(form);
    }
    // Nouveau : à l'heure demandée (sinon l'heure pleine suivante) ; durée et rappel des Options (défauts d'Outlook : 30 minutes, 15 minutes).
    let opts = crate::options_cmds::load(&app);
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
        end: fmt(start + chrono::Duration::minutes(i64::from(opts.default_duration))),
        reminder: opts.default_reminder,
        repeat_every: 1,
        repeat_count: 10,
        busy: 2,
        ..Default::default()
    })
}

/// Périodicité saisie dans le formulaire (`None` : "Jamais" ou "Inchangée").
fn repeat_of(f: &AppointmentForm) -> Option<Repeat> {
    let frequency = match f.repeat.as_str() {
        "daily" => Frequency::Daily,
        "weekly" => Frequency::Weekly,
        "monthly" => Frequency::Monthly,
        "yearly" => Frequency::Yearly,
        _ => return None,
    };
    Some(Repeat {
        frequency,
        interval: f.repeat_every.max(1),
        end: match f.repeat_end.as_str() {
            "count" => RepeatEnd::Count(f.repeat_count.max(1)),
            "until" => RepeatEnd::Until(f.repeat_until.clone()),
            _ => RepeatEnd::Never,
        },
    })
}

fn edit_of(f: &AppointmentForm) -> Edit {
    Edit {
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
        // Une série existante garde sa périodicité ici : elle ne change que par `update_series`.
        repeat: if f.id.is_some() { None } else { repeat_of(f) },
    }
}

/// Modification de la série : le premier jour reste `series_date`, seules l'heure de début et la durée du formulaire comptent.
fn series_edit_of(f: &AppointmentForm) -> Result<Edit, String> {
    let parse = |s: &str| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").map_err(|_| format!("date invalide : {s}"));
    let (start, end) = (parse(&f.start)?, parse(&f.end)?);
    let first = chrono::NaiveDate::parse_from_str(&f.series_date, "%Y-%m-%d").map_err(|_| format!("date invalide : {}", f.series_date))?;
    let new_start = first.and_time(start.time());
    let fmt = |t: chrono::NaiveDateTime| t.format("%Y-%m-%dT%H:%M").to_string();
    Ok(Edit { start: fmt(new_start), end: fmt(new_start + (end - start)), ..edit_of(f) })
}

fn done(app: &AppHandle, r: Result<(), String>, what: &str) -> Result<String, String> {
    r?;
    clear_calendar(&app.state::<CalendarCache>())?;
    let _ = app.emit("calendar-changed", ());
    Ok(what.to_string())
}

/// Enregistre le rendez-vous dans la base (création ou modification).
#[tauri::command(async)]
pub fn appointment_save(app: AppHandle, form: AppointmentForm) -> Result<String, String> {
    if form.subject.trim().is_empty() && form.location.trim().is_empty() {
        return Err("Indiquez un objet ou un lieu.".into());
    }
    if !form.all_day && form.end < form.start {
        return Err("La fin est avant le début.".into());
    }
    let dir = data_dir(&app)?;
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    if let (Some(id), true) = (&form.id, form.recurring) {
        let n = message_id(id)?.0;
        if form.scope == "occurrence" {
            let r = thunderlook_calendar::write::update_occurrence(&mut conn, &dir, n, &form.occurrence, &edit_of(&form));
            return done(&app, r, "Occurrence enregistrée");
        }
        let r = thunderlook_calendar::write::update_series(&mut conn, &dir, n, &series_edit_of(&form)?, repeat_of(&form));
        return match r {
            Ok(c) => {
                let lost = c.lost_exceptions + c.lost_deleted;
                let msg = if lost == 0 { "Série enregistrée".to_string() } else { format!("Série enregistrée ({lost} occurrence(s) modifiée(s) ou supprimée(s) rétablie(s) par le changement de périodicité)") };
                done(&app, Ok(()), &msg)
            }
            Err(e) => done(&app, Err(e), ""),
        };
    }
    let r = match &form.id {
        Some(id) => thunderlook_calendar::write::update(&mut conn, &dir, message_id(id)?.0, &edit_of(&form)),
        None => {
            let folder: i64 = form.calendar_id.strip_prefix('c').and_then(|c| c.parse().ok()).ok_or("choisissez un calendrier")?;
            thunderlook_calendar::write::create(&mut conn, &dir, folder, &edit_of(&form)).map(|_| ())
        }
    };
    done(&app, r, "Rendez-vous enregistré")
}

/// Supprime le rendez-vous : toute la série (`series`), ou seulement l'occurrence affichée.
#[tauri::command(async)]
pub fn appointment_delete(app: AppHandle, id: String, series: bool) -> Result<String, String> {
    let dir = data_dir(&app)?;
    let (n, day) = message_id(&id)?;
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    match day {
        Some(day) if !series => done(&app, thunderlook_calendar::write::delete_occurrence(&mut conn, n, &day), "Occurrence supprimée"),
        _ => done(&app, thunderlook_calendar::write::delete(&mut conn, n), "Rendez-vous supprimé"),
    }
}

/// Demande de l'interface à l'ouverture du calendrier (ancienne synchronisation avec Outlook) : rien à faire.
#[tauri::command(async)]
pub fn outlook_sync(_app: AppHandle) {}
