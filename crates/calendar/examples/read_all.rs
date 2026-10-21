//! Étape 1 du J9 : lecture de tous les rendez-vous d'un dossier de données, avec le bilan et les échecs.
//! Usage : `cargo run -p thunderlook-calendar --example read_all -- <dossier de données>`

use std::{collections::BTreeMap, path::Path};

use thunderlook_calendar::recur::{End, Frequency};

fn main() -> Result<(), String> {
    let dir = std::env::args().nth(1).ok_or("dossier de données manquant")?;
    let conn = thunderlook_store::open(Path::new(&dir)).map_err(|e| e.to_string())?;
    let cal = thunderlook_calendar::load(&conn)?;
    let n = cal.appointments.len();
    let series: Vec<_> = cal.appointments.iter().filter_map(|a| a.recurrence.as_ref().map(|r| (a, r))).collect();
    let mut freq: BTreeMap<String, usize> = BTreeMap::new();
    for (_, r) in &series {
        *freq.entry(format!("{:?}", r.frequency)).or_default() += 1;
    }
    let exceptions: usize = series.iter().map(|(_, r)| r.exceptions.len()).sum();
    let deleted: usize = series.iter().map(|(_, r)| r.deleted.len()).sum();
    let never = series.iter().filter(|(_, r)| r.end == End::Never).count();
    println!("rendez-vous lus : {n} ; échecs : {}", cal.issues.len());
    println!("journée entière : {}", cal.appointments.iter().filter(|a| a.all_day).count());
    println!("réunions : {}", cal.appointments.iter().filter(|a| a.is_meeting).count());
    println!("avec rappel : {}", cal.appointments.iter().filter(|a| a.reminder_minutes.is_some()).count());
    println!("avec catégorie : {}", cal.appointments.iter().filter(|a| !a.categories.is_empty()).count());
    println!("séries : {} ({freq:?}) ; sans fin : {never} ; exceptions : {exceptions} ; occurrences supprimées : {deleted}", series.len());
    println!("séries non grégoriennes : {}", series.iter().filter(|(_, r)| r.calendar_type != 0 && r.calendar_type != 1).count());
    let mut zones: BTreeMap<String, usize> = BTreeMap::new();
    for (a, _) in &series {
        *zones.entry(a.time_zone_name.clone().unwrap_or_else(|| "(aucun)".into())).or_default() += 1;
    }
    println!("fuseaux des séries : {zones:?}");
    let daily_weekly = series.iter().filter(|(_, r)| matches!(r.frequency, Frequency::Daily | Frequency::Weekly)).count();
    println!("séries quotidiennes ou hebdomadaires : {daily_weekly}");
    for i in cal.issues.iter().take(20) {
        println!("ÉCHEC m{} « {} » : {}", i.id, i.subject, i.detail);
    }
    Ok(())
}
