//! Copie un jeu de règles vers un compte, active les règles à l'arrivée de ce compte, puis report et relève.
//! Usage : `cargo run -p thunderlook-sync --example copy_rules -- <dossier de données> <origine du jeu> <source du compte>`

use std::path::Path;

use thunderlook_mailbox::Mailbox;
use thunderlook_sync::{account, sync_account_blocking};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, origin, source) = (Path::new(&args[0]), &args[1], args[2].parse::<i64>().map_err(|e| e.to_string())?);
    let (n, created) = Mailbox::open(dir)?.copy_rules_to_account(origin, source)?;
    println!("{n} règles copiées ; dossiers créés : {created:?}");
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    account::set_rules_on_arrival(&conn, source, true)?;
    println!("règles à l'arrivée activées pour la source {source}");
    let r = sync_account_blocking(dir, source)?;
    println!("report + relève : {} nouveau(x), erreur de report {:?}, erreur d'envoi {:?}", r.new_messages, r.push_error, r.send_error);
    Ok(())
}
