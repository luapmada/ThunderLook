//! Contacts d'un dossier en lignes TSV (id, puis les champs que `target/tl-scripts/outlook_contacts.ps1` lit dans
//! Outlook), pour la comparaison avec Outlook.
//! Usage : `cargo run -p thunderlook-contacts --example dump -- <dossier de données> <id du dossier>`

use std::path::Path;

fn main() -> Result<(), String> {
    let dir = std::env::args().nth(1).ok_or("dossier de données manquant")?;
    let folder = std::env::args().nth(2).ok_or("id du dossier manquant")?.parse::<i64>().map_err(|e| e.to_string())?;
    let conn = thunderlook_store::open(Path::new(&dir)).map_err(|e| e.to_string())?;
    let all = thunderlook_contacts::load(&conn, Some(folder), None)?;
    let clean = |s: &str| s.replace(['\t', '\r', '\n'], " ");
    for c in &all.contacts {
        let email = |i: usize| c.emails.get(i).map(|e| e.address.clone()).unwrap_or_default();
        let phone = |l: &str| c.phones.iter().find(|(x, _)| *x == l).map(|(_, v)| v.clone()).unwrap_or_default();
        let birthday = c.birthday_ms.map(|ms| {
            let (y, m, d) = thunderlook_contacts::day(ms);
            format!("{y:04}-{m:02}-{d:02}")
        });
        let fields = [
            c.id.to_string(),
            c.display_name.clone(),
            c.file_as.clone(),
            c.company.clone(),
            email(0),
            email(1),
            email(2),
            phone("Mobile"),
            phone("Bureau"),
            phone("Domicile"),
            c.home.city.clone(),
            c.work.city.clone(),
            c.categories.join("; "),
            birthday.unwrap_or_default(),
            c.has_picture.to_string(),
        ];
        println!("{}", fields.iter().map(|f| clean(f)).collect::<Vec<_>>().join("\t"));
    }
    Ok(())
}
