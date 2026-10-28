//! Calendrier à la souris : déplacer ou redimensionner un rendez-vous par glisser-déposer. Les écritures passent par
//! `thunderlook_calendar::write`.

use chrono::{Duration, NaiveDate};
use tauri::{AppHandle, Emitter, Manager, State};
use thunderlook_calendar::{
    Busy,
    expand,
    tz::TimeZone,
    write::{self, Edit},
};

use crate::{CalendarCache, clear_calendar, data_dir, with_calendar};

const FORMAT: &str = "%Y-%m-%dT%H:%M";

fn local(ms: i64) -> chrono::NaiveDateTime {
    TimeZone::local().to_local(chrono::DateTime::from_timestamp_millis(ms).map(|t| t.naive_utc()).unwrap_or_default())
}

/// `a<id>` ou `a<id>@<date d'origine>`.
fn parse_id(id: &str) -> Result<(i64, Option<NaiveDate>), String> {
    let rest = id.strip_prefix('a').ok_or("rendez-vous inconnu")?;
    let (n, day) = match rest.split_once('@') {
        Some((n, d)) => (n, Some(NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| format!("date invalide : {d}"))?)),
        None => (rest, None),
    };
    Ok((n.parse().map_err(|_| "rendez-vous inconnu")?, day))
}

/// Décale le début et la fin d'un rendez-vous (minutes, négatives vers le passé) : déplacer = même décalage des deux,
/// redimensionner = décalage de la fin seule. Un rendez-vous simple est réécrit par `move_appointment` ; pour une série,
/// seule l'occurrence `a<id>@<jour>` change (`update_occurrence`).
#[tauri::command(async)]
pub fn appointment_shift(app: AppHandle, cache: State<CalendarCache>, id: String, start_shift_min: i64, end_shift_min: i64) -> Result<String, String> {
    let (n, day) = parse_id(&id)?;
    let dir = data_dir(&app)?;
    let mut conn = thunderlook_store::open(&dir).map_err(|x| x.to_string())?;
    let found = with_calendar(&cache, &conn, &dir, |cal| {
        let a = cal.appointments.iter().find(|a| a.id == n)?;
        let o = match day {
            None => expand::occurrences(a, i64::MIN / 4, i64::MAX / 4).into_iter().next()?,
            Some(d) => {
                let ms = |t: chrono::NaiveDateTime| TimeZone::local().to_utc(t).and_utc().timestamp_millis();
                let noon = d.and_hms_opt(12, 0, 0).unwrap_or_default();
                expand::occurrences(a, ms(noon - Duration::days(90)), ms(noon + Duration::days(90))).into_iter().find(|o| o.original_date == Some(d))?
            }
        };
        let busy = match a.busy {
            Busy::Free => 0,
            Busy::Tentative => 1,
            Busy::Busy => 2,
            Busy::OutOfOffice => 3,
            Busy::WorkingElsewhere => 4,
        };
        Some((o, busy, a.reminder_minutes))
    })?;
    let (o, busy, reminder) = found.ok_or("rendez-vous introuvable")?;
    // Un événement de la journée entière ne se décale que de jours entiers (glisser dans la vue Mois), début et fin ensemble.
    if o.all_day && (start_shift_min != end_shift_min || start_shift_min % 1440 != 0) {
        return Err("Un événement de la journée entière se déplace dans sa boîte « Rendez-vous ».".into());
    }
    let start = local(o.start_ms) + Duration::minutes(start_shift_min);
    let end = local(o.end_ms) + Duration::minutes(end_shift_min);
    if end <= start {
        return Err("La fin est avant le début.".into());
    }
    let (start, end) = (start.format(FORMAT).to_string(), end.format(FORMAT).to_string());
    thunderlook_safety::snapshot_edit(&conn, &dir, n)?;
    let result = match day {
        None => write::move_appointment(&mut conn, n, &start, &end),
        Some(d) => {
            let edit = Edit {
                subject: o.subject.clone(),
                location: o.location.clone().unwrap_or_default(),
                start,
                end,
                all_day: o.all_day,
                reminder: reminder.map(|m| m as i32),
                busy,
                ..Default::default()
            };
            write::update_occurrence(&mut conn, &dir, n, &d.format("%Y-%m-%d").to_string(), &edit)
        }
    };
    result?;
    clear_calendar(&app.state::<CalendarCache>())?;
    let _ = app.emit("calendar-changed", ());
    Ok(if day.is_some() { "Occurrence déplacée".into() } else { "Rendez-vous déplacé".into() })
}
