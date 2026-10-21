//! Déplacement d'un message d'un compte vers un autre (dossier de test) : dépôt dans le compte d'arrivée, puis
//! seulement suppression à la source. Passages successifs sur les deux comptes, état affiché à chaque fois.
//! Usage : `cargo run -p thunderlook-sync --example cross_move -- <dossier de test> <source de départ> <source d'arrivée> <objet>`

use std::path::Path;

use thunderlook_mailbox::Mailbox;
use thunderlook_model::{MessageOp, Selection};
use thunderlook_store::rusqlite::params;
use thunderlook_sync::sync_account_blocking;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = Path::new(&args[0]);
    let (from, to) = (args[1].parse::<i64>().map_err(|e| e.to_string())?, args[2].parse::<i64>().map_err(|e| e.to_string())?);
    let subject = &args[3];
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    let inbox = |s: i64| -> Result<i64, String> {
        conn.query_row("SELECT id FROM folder WHERE source_id = ?1 AND role = 'inbox'", [s], |r| r.get(0)).map_err(|e| e.to_string())
    };
    let id: i64 = conn
        .query_row("SELECT m.id FROM message m JOIN folder f ON f.id = m.folder_id WHERE f.source_id = ?1 AND m.subject = ?2", params![from, subject], |r| r.get(0))
        .map_err(|e| format!("« {subject} » : {e}"))?;
    let state = || -> String {
        let pending: Vec<String> = conn
            .prepare("SELECT kind || '@' || account_id FROM imap_pending ORDER BY id")
            .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
            .unwrap_or_default();
        let place: String = conn
            .query_row(
                "SELECT f.source_id || ':' || f.name || ' uid ' || COALESCE(m.imap_uid, '-') FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "absent".into());
        format!("message {id} : {place} ; en attente {pending:?}")
    };
    Mailbox::open(dir)?.apply(&Selection::ids(vec![format!("m{id}")]), &MessageOp::Move { folder_id: format!("f{}", inbox(to)?) })?;
    println!("après le déplacement : {}", state());
    for pass in 1..=2 {
        for s in [from, to] {
            let r = sync_account_blocking(dir, s)?;
            println!("passage {pass}, source {s} : report {:?} ; {}", r.push_error, state());
        }
    }
    Ok(())
}
