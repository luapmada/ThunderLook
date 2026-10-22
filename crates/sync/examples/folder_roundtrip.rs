//! Point ouvert du J7 : créer, renommer, supprimer (vers « Éléments supprimés », puis définitivement) un dossier d'un
//! compte dans ThunderLook, avec un sous-dossier, et vérifier chaque étape sur le serveur (LIST, LSUB), puis par une
//! relève (pas de doublon local). Compte de test seulement, dossier de données de test.
//! Usage : `cargo run -p thunderlook-sync --example folder_roundtrip -- <dossier de test> <source>`

use std::path::Path;

use futures::TryStreamExt;
use thunderlook_mailbox::Mailbox;
use thunderlook_model::{FolderOp, MessageOp, Selection};
use thunderlook_store::rusqlite::{Connection, params};
use thunderlook_sync::{account, imap_login, sync_account_blocking};

fn e(x: impl std::fmt::Display) -> String {
    x.to_string()
}

/// Dossiers (LIST) et abonnements (LSUB) du serveur qui contiennent `needle`.
fn server(dir: &Path, source: i64, needle: &str) -> Result<(Vec<String>, Vec<String>), String> {
    let conn = thunderlook_store::open(dir).map_err(e)?;
    let a = account::account(&conn, source)?;
    let password = account::password(&a.address)?;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(e)?;
    rt.block_on(async {
        let mut s = imap_login(&a.endpoints.imap, &a.endpoints.username, &password).await?;
        let list: Vec<_> = s.list(Some(""), Some("*")).await.map_err(e)?.try_collect().await.map_err(e)?;
        let lsub: Vec<_> = s.lsub(Some(""), Some("*")).await.map_err(e)?.try_collect().await.map_err(e)?;
        let _ = s.logout().await;
        let pick = |v: &[async_imap::types::Name]| {
            let mut out: Vec<String> = v.iter().map(|n| n.name().to_string()).filter(|n| n.contains(needle)).collect();
            out.sort();
            out
        };
        Ok((pick(&list), pick(&lsub)))
    })
}

fn local(conn: &Connection, source: i64, needle: &str) -> Result<Vec<String>, String> {
    conn.prepare("SELECT imap_name FROM folder WHERE source_id = ?1 AND imap_name LIKE '%' || ?2 || '%' ORDER BY imap_name")
        .map_err(e)?
        .query_map(params![source, needle], |r| r.get(0))
        .map_err(e)?
        .collect::<Result<_, _>>()
        .map_err(e)
}

fn step(dir: &Path, conn: &Connection, source: i64, label: &str) -> Result<(), String> {
    let r = sync_account_blocking(dir, source)?;
    let left: i64 = conn.query_row("SELECT COUNT(*) FROM imap_pending", [], |r| r.get(0)).map_err(e)?;
    println!("{label} : report + relève, erreur {:?}, opérations restantes {left}", r.push_error);
    let (list, lsub) = server(dir, source, "TL-dossier")?;
    println!("  serveur LIST {list:?}\n  serveur LSUB {lsub:?}\n  local        {:?}", local(conn, source, "TL-dossier")?);
    Ok(())
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, source) = (Path::new(&args[0]), args[1].parse::<i64>().map_err(e)?);
    let conn = thunderlook_store::open(dir).map_err(e)?;
    let inbox: i64 =
        conn.query_row("SELECT id FROM folder WHERE source_id = ?1 AND role = 'inbox'", [source], |r| r.get(0)).map_err(e)?;
    let mut mb = Mailbox::open(dir)?;
    let parent = mb.folder_op(&FolderOp::Create { parent_id: format!("f{inbox}"), name: "TL-dossier test".into() })?.created_id.ok_or("id")?;
    let sub = mb.folder_op(&FolderOp::Create { parent_id: parent.clone(), name: "Sous".into() })?.created_id.ok_or("id")?;
    step(dir, &conn, source, "1. création (dossier et sous-dossier)")?;
    // Un message de la Boîte de réception dans le sous-dossier : il doit garder son UID au renommage.
    let msg: i64 = conn
        .query_row("SELECT id FROM message WHERE folder_id = ?1 AND imap_uid IS NOT NULL ORDER BY id LIMIT 1", [inbox], |r| r.get(0))
        .map_err(e)?;
    mb.apply(&Selection::ids(vec![format!("m{msg}")]), &MessageOp::Move { folder_id: sub.clone() })?;
    step(dir, &conn, source, "1b. message déplacé dans le sous-dossier")?;
    let uid = |conn: &Connection| -> Result<(Option<i64>, String), String> {
        conn.query_row("SELECT m.imap_uid, f.imap_name FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1", [msg], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(e)
    };
    println!("  message : {:?}", uid(&conn)?);

    // Le séparateur « . » dans le nouveau nom suit la règle des créations (U+2024 sur le serveur).
    let parent = conn
        .query_row("SELECT 'f' || id FROM folder WHERE source_id = ?1 AND imap_name LIKE '%TL-dossier test'", [source], |r| r.get::<_, String>(0))
        .map_err(e)?;
    mb.folder_op(&FolderOp::Rename { id: parent.clone(), name: "TL-dossier renommé v2.1".into() })?;
    step(dir, &conn, source, "2. renommage")?;
    println!("  message : {:?} (même UID attendu)", uid(&conn)?);
    mb.apply(&Selection::ids(vec![format!("m{msg}")]), &MessageOp::Move { folder_id: format!("f{inbox}") })?;

    mb.folder_op(&FolderOp::Delete { id: parent.clone() })?;
    step(dir, &conn, source, "3. suppression (vers Éléments supprimés)")?;

    mb.folder_op(&FolderOp::Delete { id: parent })?;
    step(dir, &conn, source, "4. suppression définitive")?;
    Ok(())
}
