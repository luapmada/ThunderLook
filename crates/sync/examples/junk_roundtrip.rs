//! Un message classé en indésirable puis ressorti vers la Boîte de réception (dossier de test, compte de test).
//! Usage : `cargo run -p thunderlook-sync --example junk_roundtrip -- <dossier de test> <source> <objet>`

use std::path::Path;

use thunderlook_mailbox::Mailbox;
use thunderlook_model::{MessageOp, Selection};
use thunderlook_store::rusqlite::params;
use thunderlook_sync::sync_account_blocking;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, source, subject) = (Path::new(&args[0]), args[1].parse::<i64>().map_err(|e| e.to_string())?, &args[2]);
    sync_account_blocking(dir, source)?;
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    let folder = |role: &str| -> Result<i64, String> {
        conn.query_row("SELECT id FROM folder WHERE source_id = ?1 AND role = ?2", params![source, role], |r| r.get(0)).map_err(|e| e.to_string())
    };
    let (inbox, junk) = (folder("inbox")?, folder("junk")?);
    let id: i64 = conn
        .query_row("SELECT m.id FROM message m WHERE m.subject = ?1", [subject], |r| r.get(0))
        .map_err(|e| format!("« {subject} » : {e}"))?;
    let place = || -> String {
        conn.query_row(
            "SELECT f.name || ' (uid ' || COALESCE(m.imap_uid, '-') || ')' FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.subject = ?1",
            [subject],
            |r| r.get(0),
        )
        .unwrap_or_else(|e| format!("introuvable ({e})"))
    };
    for (label, target) in [("vers Courrier indésirable", junk), ("hors de Courrier indésirable", inbox)] {
        let r = Mailbox::open(dir)?.apply(&Selection::ids(vec![format!("m{id}")]), &MessageOp::Move { folder_id: format!("f{target}") });
        println!("déplacement {label} : {:?}", r.map(|o| o.message));
        let s = sync_account_blocking(dir, source)?;
        println!("  report {:?} ; message maintenant dans : {}", s.push_error, place());
    }
    Ok(())
}
