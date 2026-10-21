//! Règles d'un dossier de données (le flux des PST est importé à l'ouverture), après import éventuel de `.rwz`.
//! Usage : `cargo run -p thunderlook-mailbox --example rules -- <dossier de données> [fichier.rwz…]
//!   [--run f<dossier> [--subfolders] [--apply] [--only <origine>]]`
//! `--run` exécute toutes les règles actives (d'une seule origine avec `--only`) sur le dossier : simulation, ou
//! application avec `--apply`.
//! Attention : l'ouverture migre la base au schéma courant ; travailler sur une copie.

use std::path::Path;

use thunderlook_mailbox::Mailbox;
use thunderlook_model::{RuleRunRequest, RunFilter};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let mut mb = Mailbox::open(Path::new(&args[0])).expect("ouverture du dossier de données");
    let rwz_files: Vec<&String> = args[1..].iter().take_while(|a| !a.starts_with("--")).collect();
    for rwz in rwz_files {
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
        for w in &r.warnings {
            println!("      ⚠ {w}");
        }
    }
    println!("{} règles, {} en échec", rules.len(), rules.iter().filter(|r| r.error.is_some()).count());
    let Some(folder) = opt("--run") else { return };
    let only = opt("--only");
    let rule_ids: Vec<String> =
        rules.iter().filter(|r| r.active && only.as_ref().is_none_or(|o| r.origin.contains(o.as_str()))).map(|r| r.id.clone()).collect();
    let req = RuleRunRequest {
        rule_ids,
        folder_id: folder,
        subfolders: args.iter().any(|a| a == "--subfolders"),
        filter: RunFilter::All,
        apply: args.iter().any(|a| a == "--apply"),
        only: Vec::new(),
    };
    let report = mb.run_rules(&req).expect("exécution");
    println!(
        "
== {} : {} messages parcourus, {} avec au moins une action, {} ms",
        if report.applied { "APPLICATION" } else { "SIMULATION" },
        report.scanned,
        report.changed,
        report.elapsed_ms
    );
    for e in &report.rules {
        match &e.skipped {
            Some(why) => println!("{} — écartée : {why}", e.name),
            None => println!("{} — {} message(s)", e.name, e.matched),
        }
        for a in &e.actions {
            println!("      → {a}");
        }
        for a in &e.not_run {
            println!("      ✗ {a}");
        }
        for s in e.samples.iter().take(3) {
            println!("        · {s}");
        }
    }
    for e in &report.errors {
        println!("ERREUR {e}");
    }
}
