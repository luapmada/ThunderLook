//! Étape 5 du J9 : contrôle de l'iCalendar.
//! 1. Aller-retour : chaque calendrier de la source est exporté en `.ics`, relu, et ses occurrences 2023-2026
//!    comparées à celles d'origine.
//! 2. Import d'un `.ics` d'Outlook : occurrences d'une année au format de `target/tl-scripts/outlook_occ.ps1`
//!    (objet seul, sans dossier), à comparer avec celles qu'Outlook calcule.
//! Usage : `cargo run -p thunderlook-calendar --example ics_roundtrip -- <dossier de données> <source> [<fichier .ics> <année> <sortie>]`

use std::{collections::BTreeMap, path::Path};

use chrono::{DateTime, NaiveDate};
use thunderlook_calendar::{Appointment, expand::occurrences, ics, tz::TimeZone};

fn key(a: &Appointment, from: i64, to: i64) -> Vec<String> {
    let tz = TimeZone::PARIS;
    let fmt = |ms: i64| tz.to_local(DateTime::from_timestamp_millis(ms).unwrap().naive_utc()).format("%Y-%m-%d %H:%M").to_string();
    occurrences(a, from, to)
        .into_iter()
        .map(|o| {
            let subject = o.subject.split(['\r', '\n']).map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
            format!("{}|{}|{}|{}", fmt(o.start_ms), fmt(o.end_ms), u8::from(o.all_day), subject)
        })
        .collect()
}

fn bound(y: i32) -> i64 {
    TimeZone::PARIS.to_utc(NaiveDate::from_ymd_opt(y, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap()).and_utc().timestamp_millis()
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = Path::new(&args[0]);
    let source: i64 = args[1].parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    let cal = thunderlook_calendar::load(&conn)?;
    let calendars = thunderlook_calendar::view::calendars(&conn)?;
    let (from, to) = (bound(2023), bound(2027));
    let mut failures = 0;
    for c in calendars.iter().filter(|c| cal.appointments.iter().any(|a| a.source_id == source && format!("c{}", a.folder_id) == c.id)) {
        let items: Vec<&Appointment> = cal.appointments.iter().filter(|a| format!("c{}", a.folder_id) == c.id).collect();
        let text = ics::export(&items, &c.name, &|a| thunderlook_calendar::description(a, dir));
        let (name, back) = ics::parse(&text, -1, -1)?;
        let mut want: BTreeMap<String, i32> = BTreeMap::new();
        for a in &items {
            for k in key(a, from, to) {
                *want.entry(k).or_default() += 1;
            }
        }
        for a in &back {
            for k in key(a, from, to) {
                *want.entry(k).or_default() -= 1;
            }
        }
        let diff: Vec<_> = want.iter().filter(|(_, v)| **v != 0).collect();
        let series = items.iter().filter(|a| a.recurrence.is_some()).count();
        let occ: usize = items.iter().map(|a| key(a, from, to).len()).sum();
        println!(
            "{} : {} éléments ({series} séries), {} octets d'iCalendar, relus {} (nom {:?}) ; occurrences 2023-2026 : {occ}, écarts : {}",
            c.name,
            items.len(),
            text.len(),
            back.len(),
            name,
            diff.len()
        );
        for (k, v) in diff.iter().take(10) {
            println!("   {} {k}", if **v > 0 { "manque après relecture :" } else { "en trop après relecture :" });
        }
        failures += diff.len();
    }
    if let (Some(file), Some(year), Some(out)) = (args.get(2), args.get(3), args.get(4)) {
        let year: i32 = year.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
        let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
        let (name, items) = ics::parse(&text, -1, -1)?;
        let lines: Vec<String> = items.iter().flat_map(|a| key(a, bound(year), bound(year + 1))).collect();
        std::fs::write(out, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
        println!(
            "{file} : « {} », {} éléments ({} séries, {} exceptions) ; {} occurrences en {year} → {out}",
            name.unwrap_or_default(),
            items.len(),
            items.iter().filter(|a| a.recurrence.is_some()).count(),
            items.iter().filter_map(|a| a.recurrence.as_ref()).map(|r| r.exceptions.len()).sum::<usize>(),
            lines.len()
        );
    }
    if failures > 0 {
        return Err(format!("{failures} écart(s) à l'aller-retour"));
    }
    Ok(())
}
