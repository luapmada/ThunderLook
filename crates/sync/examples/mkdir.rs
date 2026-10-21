//! Création d'un dossier dans ThunderLook au premier niveau d'un compte, puis report et relève (dossier de test).
//! Usage : `cargo run -p thunderlook-sync --example mkdir -- <dossier de test> <source> <nom>`

use std::path::Path;

use thunderlook_mailbox::Mailbox;
use thunderlook_model::FolderOp;
use thunderlook_sync::sync_account_blocking;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, source, name) = (Path::new(&args[0]), args[1].parse::<i64>().map_err(|e| e.to_string())?, &args[2]);
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    let top: i64 = conn.query_row("SELECT id FROM folder WHERE source_id = ?1 AND role = 'ipm'", [source], |r| r.get(0)).map_err(|e| e.to_string())?;
    Mailbox::open(dir)?.folder_op(&FolderOp::Create { parent_id: format!("f{top}"), name: name.clone() })?;
    let (imap, pending): (Option<String>, i64) = conn
        .query_row(
            "SELECT f.imap_name, (SELECT COUNT(*) FROM imap_pending WHERE kind = 'mkdir') FROM folder f WHERE f.source_id = ?1 AND f.name = ?2",
            thunderlook_store::rusqlite::params![source, name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    println!("dossier local créé : nom IMAP {imap:?}, créations en attente {pending}");
    let r = sync_account_blocking(dir, source)?;
    println!("report + relève : {} dossiers, erreur de report {:?}", r.folders, r.push_error);
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM folder WHERE source_id = ?1 AND imap_name = ?2", thunderlook_store::rusqlite::params![source, imap], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    println!("après relève : {n} dossier(s) local(aux) pour {imap:?} (1 attendu, pas de doublon)");
    Ok(())
}
