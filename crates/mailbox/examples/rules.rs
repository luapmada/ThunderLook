//! Règles d'un dossier de données (le flux des PST est importé à l'ouverture), après import éventuel de `.rwz`.
//! Usage : `cargo run -p thunderlook-mailbox --example rules -- <dossier de données> [fichier.rwz…]`
//! Attention : l'ouverture migre la base au schéma courant ; travailler sur une copie.

use std::path::Path;

use thunderlook_mailbox::Mailbox;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut mb = Mailbox::open(Path::new(&args[0])).expect("ouverture du dossier de données");
    for rwz in &args[1..] {
        let n = mb.import_rwz(Path::new(rwz)).expect("import du .rwz");
        println!("{rwz} : {n} règles importées");
    }
    let rules = mb.rules().expect("lecture des règles");
    let mut origin = None;
    for r in &rules {
        if origin != Some(&r.origin) {
            origin = Some(&r.origin);
            println!("== {} ({})", r.origin_label, r.origin);
        }
        let state = r.error.as_deref().map_or(String::new(), |e| format!(" ÉCHEC : {e}"));
        println!("[{}] {} {:?}{state}", r.position, if r.active { "☑" } else { "☐" }, r.name);
        for line in &r.description {
            println!("      {line}");
        }
    }
    println!("{} règles, {} en échec", rules.len(), rules.iter().filter(|r| r.error.is_some()).count());
}
