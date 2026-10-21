//! Jeu de règles d'un compte (dossier de test) : import d'un flux de règles, copie vers le compte (dossiers créés),
//! relève, puis règles du compte appliquées aux nouveaux messages de la Boîte de réception (comme à l'arrivée).
//! Usage : `cargo run -p thunderlook-sync --example account_rules -- <dossier de test> <source> <flux de règles>`

use std::path::Path;

use thunderlook_mailbox::Mailbox;
use thunderlook_model::{RuleRunRequest, RunFilter};
use thunderlook_sync::sync_account_blocking;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, source, stream) = (Path::new(&args[0]), args[1].parse::<i64>().map_err(|e| e.to_string())?, Path::new(&args[2]));
    let mut mb = Mailbox::open(dir)?;
    let n = mb.import_rwz(stream)?;
    let origin = std::path::absolute(stream).map_err(|e| e.to_string())?.to_string_lossy().into_owned();
    println!("{n} règles importées");
    let (copied, created) = mb.copy_rules_to_account(&origin, source)?;
    println!("{copied} règles copiées vers le compte ; dossiers créés : {created:?}");
    drop(mb);
    let r = sync_account_blocking(dir, source)?;
    println!("report + relève : {} nouveaux dans la Boîte de réception, erreur de report {:?}", r.new_inbox_ids.len(), r.push_error);
    let mut mb = Mailbox::open(dir)?;
    let set = format!("account:{source}");
    let rules = mb.rules()?;
    let ids: Vec<String> = rules.iter().filter(|r| r.origin == set && r.active).map(|r| r.id.clone()).collect();
    for r in rules.iter().filter(|r| r.origin == set && !r.warnings.is_empty()) {
        println!("  ⚠ {} : {:?}", r.name, r.warnings);
    }
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    let inbox: i64 = conn.query_row("SELECT id FROM folder WHERE source_id = ?1 AND role = 'inbox'", [source], |r| r.get(0)).map_err(|e| e.to_string())?;
    let report = mb.run_rules(&RuleRunRequest {
        rule_ids: ids,
        folder_id: format!("f{inbox}"),
        subfolders: false,
        filter: RunFilter::All,
        apply: true,
        only: r.new_inbox_ids.iter().map(|id| format!("m{id}")).collect(),
    })?;
    println!("règles du compte sur {} nouveau(x) message(s) : {} modifié(s), erreurs {:?}", report.scanned, report.changed, report.errors);
    for e in report.rules.iter().filter(|e| e.matched > 0) {
        println!("  {} : {} message(s) → {:?}", e.name, e.matched, e.actions);
    }
    drop(mb);
    let r = sync_account_blocking(dir, source)?;
    println!("report : erreur {:?}", r.push_error);
    Ok(())
}
