//! Étape 5 du J9 : CSV d'Outlook.
//! 1. Export des occurrences d'un calendrier sur la plage de l'exemple d'Outlook, et comparaison colonne par colonne
//!    des lignes communes (même objet, même début).
//! 2. Aller-retour : l'exemple est relu puis réécrit ; les champs gardés doivent être identiques.
//! Usage : `cargo run -p thunderlook-calendar --example csv_check -- <dossier de données> <calendrier> <exemple.csv> <sortie.csv>`

use std::{collections::HashMap, path::Path};

use chrono::NaiveDate;
use thunderlook_calendar::{csv, expand::occurrences, tz::TimeZone};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = Path::new(&args[0]);
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    let cal = thunderlook_calendar::load(&conn)?;
    let list = thunderlook_calendar::view::calendars(&conn)?;
    let folder: i64 = list.iter().find(|c| c.name == args[1]).ok_or("calendrier introuvable")?.id[1..].parse().unwrap();
    let sample_text = std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?;
    let sample = csv::records(&sample_text);

    // Plage de l'exemple (dates de début), comme la boîte d'export d'Outlook la propose.
    let starts: Vec<NaiveDate> = sample.iter().skip(1).filter_map(|r| NaiveDate::parse_from_str(r.get(1)?, "%d/%m/%Y").ok()).collect();
    let (first, last) = (*starts.iter().min().ok_or("exemple vide")?, *starts.iter().max().unwrap());
    let ms = |d: NaiveDate| TimeZone::PARIS.to_utc(d.and_hms_opt(0, 0, 0).unwrap()).and_utc().timestamp_millis();
    let mut rows: Vec<_> = cal
        .appointments
        .iter()
        .filter(|a| a.folder_id == folder)
        .flat_map(|a| occurrences(a, ms(first), ms(last + chrono::Duration::days(1))).into_iter().map(move |o| (a, o)))
        .collect();
    rows.sort_by_key(|(_, o)| o.start_ms);
    let text = csv::export(&rows, &|a| thunderlook_calendar::description(a, dir));
    std::fs::write(&args[3], &text).map_err(|e| e.to_string())?;
    let ours = csv::records(&text);
    println!("plage {first} – {last} ; exemple : {} lignes ; export : {} lignes → {}", sample.len() - 1, ours.len() - 1, args[3]);
    if ours[0] != sample[0] {
        return Err(format!("en-têtes différents : {:?}", ours[0]));
    }

    // Lignes communes : même objet, même date et heure de début.
    let key = |r: &Vec<String>| (r[0].trim().to_string(), r[1].clone(), r[2].clone());
    let mine: HashMap<_, &Vec<String>> = ours.iter().skip(1).map(|r| (key(r), r)).collect();
    let (mut common, mut same) = (0, vec![0usize; 22]);
    let mut diffs: HashMap<usize, Vec<String>> = HashMap::new();
    for r in sample.iter().skip(1) {
        let Some(m) = mine.get(&key(r)) else { continue };
        common += 1;
        for i in 0..22 {
            let norm = |s: &str| s.replace("\r\n", "\n").trim().to_string();
            if norm(&r[i]) == norm(&m[i]) {
                same[i] += 1;
            } else {
                diffs.entry(i).or_default().push(format!("« {} » {} : Outlook {:?} / ThunderLook {:?}", r[0], r[1], r[i], m[i]));
            }
        }
    }
    println!("lignes communes (objet + début) : {common}");
    for (i, h) in csv::HEADER.iter().enumerate() {
        println!("  {h:<28} {}/{common}", same[i]);
        for d in diffs.get(&i).into_iter().flatten().take(3) {
            println!("      {d}");
        }
    }

    // Aller-retour de l'exemple : relu puis réécrit.
    let back = csv::parse(&sample_text, -1, -1)?;
    let rewritten: Vec<_> = back.iter().map(|a| (a, occurrences(a, i64::MIN / 4, i64::MAX / 4).remove(0))).collect();
    let again = csv::records(&csv::export(&rewritten, &|a| a.description.clone()));
    let kept = [0, 1, 2, 3, 4, 5, 6, 7, 8, 13, 14, 15, 16, 17, 20, 21];
    let bad = sample.iter().zip(&again).skip(1).filter(|(a, b)| kept.iter().any(|&i| a[i].replace("\r\n", "\n").trim() != b[i].replace("\r\n", "\n").trim())).count();
    println!("aller-retour de l'exemple : {} lignes relues, {bad} différente(s) sur les colonnes gardées", back.len());
    Ok(())
}
