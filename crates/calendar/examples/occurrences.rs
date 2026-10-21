//! Étape 2 du J9 : occurrences d'une année dans les calendriers d'une source, au format de
//! `target/tl-scripts/outlook_occ.ps1` (dossier|début|fin|journée entière|objet, heure de Paris), pour comparer avec
//! les occurrences calculées par Outlook.
//! Usage : `cargo run -p thunderlook-calendar --example occurrences -- <dossier de données> <source> <année>`

use std::{collections::HashMap, path::Path};

use chrono::{DateTime, NaiveDate};
use thunderlook_calendar::{expand::occurrences, tz::TimeZone};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let conn = thunderlook_store::open(Path::new(&args[0])).map_err(|e| e.to_string())?;
    let source: i64 = args[1].parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
    let year: i32 = args[2].parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
    let mut folders: HashMap<i64, String> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, name FROM folder WHERE source_id = ?1 AND container_class LIKE 'IPF.Appointment%'")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([source], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|e| e.to_string())?;
        for r in rows {
            let (id, name) = r.map_err(|e| e.to_string())?;
            folders.insert(id, name);
        }
    }
    let tz = TimeZone::PARIS;
    let bound = |y: i32| tz.to_utc(NaiveDate::from_ymd_opt(y, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap()).and_utc().timestamp_millis();
    let (from, to) = (bound(year), bound(year + 1));
    let fmt = |ms: i64| tz.to_local(DateTime::from_timestamp_millis(ms).unwrap().naive_utc()).format("%Y-%m-%d %H:%M").to_string();
    let cal = thunderlook_calendar::load(&conn)?;
    let (mut single, mut serial, mut exceptions) = (0, 0, 0);
    for a in cal.appointments.iter().filter(|a| a.source_id == source) {
        let Some(folder) = folders.get(&a.folder_id) else { continue };
        for o in occurrences(a, from, to) {
            match (a.recurrence.is_some(), o.exception) {
                (false, _) => single += 1,
                (true, false) => serial += 1,
                (true, true) => exceptions += 1,
            }
            let subject = o.subject.split(['\r', '\n']).filter(|s| !s.trim().is_empty()).map(str::trim).collect::<Vec<_>>().join(" ");
            println!("{folder}|{}|{}|{}|{}", fmt(o.start_ms), fmt(o.end_ms), u8::from(o.all_day), subject.trim());
        }
    }
    eprintln!("{year} : {single} rendez-vous simples, {serial} occurrences de séries, {exceptions} exceptions");
    Ok(())
}
