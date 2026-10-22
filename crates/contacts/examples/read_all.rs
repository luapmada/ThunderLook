//! Étape 1 du J10 : lecture des contacts d'un dossier de données, couverture des champs et échecs.
//! Usage : `cargo run -p thunderlook-contacts --example read_all -- <dossier de données> [id du dossier]`

use std::{collections::BTreeMap, path::Path};

fn main() -> Result<(), String> {
    let dir = std::env::args().nth(1).ok_or("dossier de données manquant")?;
    let folder = std::env::args().nth(2).map(|f| f.parse::<i64>().map_err(|e| e.to_string())).transpose()?;
    let conn = thunderlook_store::open(Path::new(&dir)).map_err(|e| e.to_string())?;
    let all = thunderlook_contacts::load(&conn, folder, None)?;
    let c = &all.contacts;
    let count = |f: &dyn Fn(&thunderlook_contacts::Contact) -> bool| c.iter().filter(|x| f(x)).count();
    println!("contacts lus : {} ; échecs : {}", c.len(), all.issues.len());
    println!("classer sous : {}", count(&|x| !x.file_as.is_empty()));
    println!("prénom : {} ; nom : {}", count(&|x| !x.given_name.is_empty()), count(&|x| !x.surname.is_empty()));
    println!("société : {} ; fonction : {}", count(&|x| !x.company.is_empty()), count(&|x| !x.job_title.is_empty()));
    let mut emails: BTreeMap<usize, usize> = BTreeMap::new();
    for x in c {
        *emails.entry(x.emails.len()).or_default() += 1;
    }
    println!("adresses électroniques (nombre → contacts) : {emails:?}");
    let mut phones: BTreeMap<&str, usize> = BTreeMap::new();
    for x in c {
        for (l, _) in &x.phones {
            *phones.entry(l).or_default() += 1;
        }
    }
    println!("téléphones : {phones:?}");
    println!(
        "adresses : bureau {} ; domicile {} ; autre {}",
        count(&|x| !x.work.is_empty()),
        count(&|x| !x.home.is_empty()),
        count(&|x| !x.other.is_empty())
    );
    println!("anniversaire : {} ; photo : {} ; catégories : {}", count(&|x| x.birthday_ms.is_some()), count(&|x| x.has_picture), count(&|x| !x.categories.is_empty()));
    println!("page web : {} ; messagerie instantanée : {}", count(&|x| !x.web_page.is_empty()), count(&|x| !x.im_address.is_empty()));
    println!("sans nom ni adresse : {}", count(&|x| x.display_name.is_empty() && x.emails.is_empty()));
    for i in all.issues.iter().take(20) {
        println!("ÉCHEC m{} « {} » : {}", i.id, i.subject, i.detail);
    }
    if let Some(x) = c.iter().find(|x| !x.phones.is_empty() && !x.emails.is_empty() && !x.home.is_empty()) {
        println!("exemple : {x:#?}");
    }
    Ok(())
}
