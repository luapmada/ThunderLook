//! Réparation de l'incident du 21/10 : les messages classés par erreur dans les dossiers de règles d'un compte
//! (`depuis`) sont ramenés dans les dossiers de même nom IMAP de l'autre compte (`vers`) ; dépôt d'abord, suppression
//! ensuite. N'agit que si chaque dossier contient exactement le nombre attendu.
//! Usage : `cargo run -p thunderlook-sync --example rehome -- <dossier de données> <depuis> <vers> <nom IMAP>=<nombre>…`

use std::path::Path;

use thunderlook_mailbox::Mailbox;
use thunderlook_model::{MessageOp, Selection};
use thunderlook_store::rusqlite::params;
use thunderlook_sync::sync_account_blocking;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = Path::new(&args[0]);
    let (from, to) = (args[1].parse::<i64>().map_err(|e| e.to_string())?, args[2].parse::<i64>().map_err(|e| e.to_string())?);
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    let folder = |source: i64, imap: &str| -> Result<i64, String> {
        conn.query_row("SELECT id FROM folder WHERE source_id = ?1 AND imap_name = ?2", params![source, imap], |r| r.get(0))
            .map_err(|e| format!("{imap} (source {source}) : {e}"))
    };
    let mut plan = Vec::new();
    for spec in &args[3..] {
        let (imap, n) = spec.rsplit_once('=').ok_or("<nom IMAP>=<nombre> attendu")?;
        let expected: usize = n.parse().map_err(|e| format!("{spec} : {e}"))?;
        let (src, dst) = (folder(from, imap)?, folder(to, imap)?);
        let ids: Vec<i64> = conn
            .prepare("SELECT id FROM message WHERE folder_id = ?1 AND imap_uid IS NOT NULL")
            .map_err(|e| e.to_string())?
            .query_map([src], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        if ids.len() != expected {
            return Err(format!("{imap} : {} messages, {expected} attendus ; rien n'est fait", ids.len()));
        }
        plan.push((imap.to_string(), dst, ids));
    }
    let mut mb = Mailbox::open(dir)?;
    for (imap, dst, ids) in &plan {
        let sel = Selection::ids(ids.iter().map(|id| format!("m{id}")).collect());
        let r = mb.apply(&sel, &MessageOp::Move { folder_id: format!("f{dst}") })?;
        println!("{imap} : {}", r.message);
    }
    drop(mb);
    for pass in 1..=4 {
        for s in [to, from] {
            let r = sync_account_blocking(dir, s)?;
            if r.push_error.is_some() {
                println!("passage {pass}, source {s} : erreur de report {:?}", r.push_error);
            }
        }
        let left: Vec<(i64, String, i64)> = conn
            .prepare("SELECT account_id, kind, COUNT(*) FROM imap_pending GROUP BY account_id, kind")
            .map_err(|e| e.to_string())?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        println!("après le passage {pass} : en attente {left:?}");
        if left.is_empty() {
            break;
        }
    }
    Ok(())
}
