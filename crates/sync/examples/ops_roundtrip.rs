//! Report sur le serveur des opérations faites dans ThunderLook, sur le compte de test (dossier de données de test).
//! Prérequis : deux messages « <préfixe> A » et « <préfixe> B » dans la Boîte de réception (envoyés de l'extérieur).
//! Usage : `cargo run -p thunderlook-sync --example ops_roundtrip -- <dossier de test> <source> <préfixe> [--cleanup]`
//! `--cleanup` : supprime définitivement les messages du préfixe (EXPUNGE sur le serveur) et s'arrête.

use std::path::Path;

use thunderlook_mailbox::Mailbox;
use thunderlook_model::{FlagAction, MessageOp, Selection};
use thunderlook_sync::sync_account_blocking;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, source, prefix) = (Path::new(&args[0]), args[1].parse::<i64>().map_err(|e| e.to_string())?, &args[2]);
    let r = sync_account_blocking(dir, source)?;
    println!("relève initiale : {} nouveaux", r.new_messages);
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    if args.iter().any(|a| a == "--cleanup") {
        let ids: Vec<String> = conn
            .prepare("SELECT 'm' || m.id FROM message m JOIN folder f ON f.id = m.folder_id WHERE f.source_id = ?1 AND m.subject LIKE ?2")
            .map_err(|e| e.to_string())?
            .query_map(thunderlook_store::rusqlite::params![source, format!("{prefix}%")], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let n = ids.len();
        Mailbox::open(dir)?.apply(&Selection::ids(ids), &MessageOp::DeletePermanently)?;
        let r = sync_account_blocking(dir, source)?;
        println!("{n} message(s) supprimé(s) définitivement ; report : {:?} ; retirés à la relève : {}", r.push_error, r.removed);
        return Ok(());
    }
    let folder = |role: &str| -> Result<i64, String> {
        conn.query_row("SELECT id FROM folder WHERE source_id = ?1 AND role = ?2", thunderlook_store::rusqlite::params![source, role], |r| r.get(0))
            .map_err(|e| format!("{role} : {e}"))
    };
    let (inbox, junk) = (folder("inbox")?, folder("junk")?);
    let msg = |subject: &str| -> Result<i64, String> {
        conn.query_row("SELECT id FROM message WHERE folder_id = ?1 AND subject = ?2", thunderlook_store::rusqlite::params![inbox, subject], |r| r.get(0))
            .map_err(|e| format!("« {subject} » : {e}"))
    };
    let (a, b) = (msg(&format!("{prefix} A"))?, msg(&format!("{prefix} B"))?);
    let mut mb = Mailbox::open(dir)?;
    let sel = |id: i64| Selection::ids(vec![format!("m{id}")]);
    mb.apply(&sel(a), &MessageOp::Read { read: true })?;
    mb.apply(&sel(a), &MessageOp::Flag { action: FlagAction::Today })?;
    mb.apply(&sel(a), &MessageOp::Copy { folder_id: format!("f{junk}") })?;
    mb.apply(&sel(b), &MessageOp::Move { folder_id: format!("f{junk}") })?;
    mb.apply(&sel(b), &MessageOp::Move { folder_id: format!("f{inbox}") })?;
    mb.apply(&sel(b), &MessageOp::Move { folder_id: format!("f{junk}") })?;
    let copy: i64 = conn
        .query_row("SELECT id FROM message WHERE folder_id = ?1 AND subject = ?2", thunderlook_store::rusqlite::params![junk, format!("{prefix} A")], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let pending: Vec<String> = conn
        .prepare("SELECT kind || ' ' || mailbox || ' ' || COALESCE(uid, '-') || ' ' || arg FROM imap_pending ORDER BY id")
        .map_err(|e| e.to_string())?
        .query_map([], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    println!("en attente avant suppression de la copie :\n  {}", pending.join("\n  "));
    mb.apply(&sel(copy), &MessageOp::DeletePermanently)?;
    let left: i64 = conn.query_row("SELECT COUNT(*) FROM imap_pending", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    println!("en attente après suppression de la copie : {left}");
    drop(mb);
    for pass in 1..=2 {
        let r = sync_account_blocking(dir, source)?;
        println!(
            "report + relève {pass} : {} nouveaux, {} drapeaux, {} retirés, erreur de report : {:?}",
            r.new_messages, r.updated_flags, r.removed, r.push_error
        );
    }
    let rows: Vec<String> = conn
        .prepare(
            "SELECT f.name || ' | uid ' || COALESCE(m.imap_uid, '-') || ' | ' || m.subject || ' | lu ' || m.is_read || ' | indicateur ' || m.flag_status
             FROM message m JOIN folder f ON f.id = m.folder_id WHERE f.source_id = ?1 AND m.subject LIKE ?2 ORDER BY f.name",
        )
        .map_err(|e| e.to_string())?
        .query_map(thunderlook_store::rusqlite::params![source, format!("{prefix}%")], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    println!("copie locale après relève :\n  {}", rows.join("\n  "));
    let left: i64 = conn.query_row("SELECT COUNT(*) FROM imap_pending", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    println!("opérations encore en attente : {left}");
    Ok(())
}
