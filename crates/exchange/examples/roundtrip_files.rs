//! Aller-retour complet sur des COPIES : relecture d'un dossier écrit par MPE, modification et création dans ThunderLook,
//! report dans les fichiers, relecture. Jamais sur la vraie base ni le vrai dossier d'échange.
//! Usage : `cargo run -p thunderlook-exchange --example roundtrip_files -- <données copiées> <dossier d'échange copié>`
//! N'affiche que des comptes et les objets des éléments de test.

use std::{env, path::Path, process::ExitCode, time::Duration};

use thunderlook_exchange::auto::{Params, run_pass};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [data, dir] = args.as_slice() else {
        eprintln!("usage : roundtrip_files <données copiées> <dossier d'échange copié>");
        return ExitCode::FAILURE;
    };
    let (data, dir) = (Path::new(data), Path::new(dir));
    if data.to_string_lossy().contains("rea-outlook-clone\\data") || dir.to_string_lossy().contains("mpe-test") {
        eprintln!("refusé : à faire sur des copies seulement");
        return ExitCode::FAILURE;
    }
    let mut conn = thunderlook_store::open(data).expect("base");
    let p = Params { stable_gap: Duration::from_millis(10), min_age: Duration::ZERO, ignore_threshold: true, auto_export: false, ..Params::default() };

    let r = run_pass(&mut conn, data, dir, &p);
    println!("1. relecture : {:?}, créés {}, modifiés {}, conflits {}, suppressions {}", r.outcome, r.created, r.modified, r.aside.conflicts, r.aside.deletions);

    // Report sans modification : rien à écrire.
    let w = thunderlook_exchange::write_back(&conn, data, dir, false);
    println!("2. report sans modification : {w:?}");

    // Modifications dans ThunderLook : un rendez-vous simple, une fiche ; et un rendez-vous créé.
    let cal = thunderlook_calendar::load(&conn).expect("calendrier");
    let map: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(data.join("exchange-map.json")).unwrap_or_default()).unwrap_or_default();
    let mapped = |kind: &str| -> Vec<String> { map.get(kind).and_then(|m| m.as_object()).map(|m| m.values().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default() };
    let events = mapped("event");
    let a = cal.appointments.iter().find(|a| a.recurrence.is_none() && events.contains(&format!("a{}", a.id)) && a.start_ms > 1_790_000_000_000).expect("un rendez-vous relié");
    let edit = |subject: String, start_ms: i64| {
        let t = |ms: i64| chrono::DateTime::from_timestamp_millis(ms).unwrap().naive_utc();
        let paris = thunderlook_calendar::tz::TimeZone::PARIS;
        thunderlook_calendar::write::Edit {
            subject,
            location: "ZZ-TEST lieu".into(),
            start: paris.to_local(t(start_ms)).format("%Y-%m-%dT%H:%M").to_string(),
            end: paris.to_local(t(start_ms + 3_600_000)).format("%Y-%m-%dT%H:%M").to_string(),
            all_day: false,
            ..Default::default()
        }
    };
    thunderlook_calendar::write::update(&mut conn, data, a.id, &edit(format!("{} ZZ-TEST", a.subject), a.start_ms)).expect("modification");
    println!("   rendez-vous modifié : a{}", a.id);
    let folder: i64 = conn.query_row("SELECT folder_id FROM message WHERE id = ?1", [a.id], |r| r.get(0)).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let new_id = thunderlook_calendar::write::create(&mut conn, data, folder, &edit("ZZ-TEST nouveau rendez-vous".into(), 1_799_000_000_000)).expect("création");
    println!("   rendez-vous créé : a{new_id} (calendrier {folder})");
    let contacts = mapped("contact");
    let pid: i64 = contacts.iter().filter_map(|c| c.get(1..)?.parse().ok()).next().expect("une fiche reliée");
    let mut form = thunderlook_contacts::write::form_of(&conn, data, pid).expect("fiche");
    form.job_title = "ZZ-TEST fonction".into();
    thunderlook_contacts::write::update(&mut conn, data, pid, &form).expect("fiche modifiée");
    println!("   fiche modifiée : p{pid}");

    let dry = thunderlook_exchange::write_back(&conn, data, dir, true);
    println!("3. report à blanc : {dry:?}");
    let w = thunderlook_exchange::write_back(&conn, data, dir, false);
    println!("4. report : {w:?}");

    let again = thunderlook_exchange::import_changes(&conn, data, dir).expect("relecture");
    println!("5. relecture après report : {} changements, {} conflits", again.changes.len(), again.conflicts.len());
    let r = run_pass(&mut conn, data, dir, &Params { recheck: true, ..p });
    println!("6. passage suivant : {:?}, créés {}, modifiés {}", r.outcome, r.created, r.modified);
    let w = thunderlook_exchange::write_back(&conn, data, dir, true);
    println!("7. second report à blanc : {w:?}");
    ExitCode::SUCCESS
}
