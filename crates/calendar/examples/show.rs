//! Affiche les rendez-vous dont l'objet contient un texte : champs, périodicité brute et `.ics` produit.
//! Usage : `cargo run -p thunderlook-calendar --example show -- <dossier de données> <texte>`

use std::path::Path;

use thunderlook_calendar::{ics, recur::minutes_to_local};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = Path::new(&args[0]);
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    let cal = thunderlook_calendar::load(&conn)?;
    for a in cal.appointments.iter().filter(|a| a.subject.contains(args[1].as_str())) {
        println!("m{} « {} » début {} fin {} journée={} dossier {}", a.id, a.subject, a.start_ms, a.end_ms, a.all_day, a.folder_id);
        if let Some(r) = &a.recurrence {
            println!(
                "  {:?} {:?} période {} fin {:?} du {} au {} ({}–{} min) premier jour {}",
                r.frequency,
                r.pattern,
                r.period,
                r.end,
                minutes_to_local(r.start_date),
                minutes_to_local(r.end_date),
                r.start_offset,
                r.end_offset,
                r.first_day_of_week
            );
            println!("  supprimées : {:?}", r.deleted.iter().map(|m| minutes_to_local(*m).date().to_string()).collect::<Vec<_>>());
            println!("  modifiées : {:?}", r.modified.iter().map(|m| minutes_to_local(*m).date().to_string()).collect::<Vec<_>>());
            for x in &r.exceptions {
                println!("  exception {} → {}–{} {:?}", minutes_to_local(x.original_start), minutes_to_local(x.start), minutes_to_local(x.end), x.subject);
            }
        }
        let text = ics::export(&[a], "x", &|_| None);
        for l in text.lines().skip_while(|l| !l.starts_with("BEGIN:VEVENT")) {
            println!("    {l}");
        }
    }
    Ok(())
}
