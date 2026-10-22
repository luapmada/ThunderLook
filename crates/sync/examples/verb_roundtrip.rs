//! Point ouvert du J7 : répondre puis transférer un message du compte de test (vers le compte lui-même), et vérifier
//! sur le serveur que l'original porte `\Answered` puis `$Forwarded`. Nettoyage : drapeaux de l'original remis comme
//! avant, messages de test supprimés du serveur (Boîte de réception et Éléments envoyés), relève finale.
//! Usage : `cargo run -p thunderlook-sync --example verb_roundtrip -- <dossier de test> <source>`

use std::path::Path;

use futures::TryStreamExt;
use thunderlook_mailbox::Mailbox;
use thunderlook_model::ComposeKind;
use thunderlook_store::rusqlite::params;
use thunderlook_sync::{account, imap_login, sync_account_blocking};

fn e(x: impl std::fmt::Display) -> String {
    x.to_string()
}

/// Session sur le compte, pour lire ou nettoyer ce que ThunderLook a fait.
fn with_session<T>(dir: &Path, source: i64, f: impl AsyncFnOnce(&mut thunderlook_sync::ImapSession) -> Result<T, String>) -> Result<T, String> {
    let conn = thunderlook_store::open(dir).map_err(e)?;
    let a = account::account(&conn, source)?;
    let password = account::password(&a.address)?;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(e)?;
    rt.block_on(async {
        let mut s = imap_login(&a.endpoints.imap, &a.endpoints.username, &password).await?;
        let r = f(&mut s).await;
        let _ = s.logout().await;
        r
    })
}

fn server_flags(dir: &Path, source: i64, uid: u32) -> Result<String, String> {
    with_session(dir, source, async |s| {
        s.examine("INBOX").await.map_err(e)?;
        let f: Vec<_> = s.uid_fetch(uid.to_string(), "(UID FLAGS)").await.map_err(e)?.try_collect().await.map_err(e)?;
        Ok(f.first().map(|f| format!("{:?}", f.flags().collect::<Vec<_>>())).unwrap_or_else(|| "(absent)".into()))
    })
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, source) = (Path::new(&args[0]), args[1].parse::<i64>().map_err(e)?);
    let conn = thunderlook_store::open(dir).map_err(e)?;
    let address = account::account(&conn, source)?.address;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    sync_account_blocking(dir, source)?;
    let (original, uid): (i64, u32) = conn
        .query_row(
            "SELECT m.id, m.imap_uid FROM message m JOIN folder f ON f.id = m.folder_id
             WHERE f.source_id = ?1 AND f.role = 'inbox' AND m.imap_uid IS NOT NULL AND m.internet_message_id <> '' ORDER BY m.imap_uid LIMIT 1",
            [source],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(e)?;
    let before = server_flags(dir, source, uid)?;
    println!("original m{original} (INBOX uid {uid}) ; drapeaux avant : {before}");

    for (kind, label) in [(ComposeKind::Reply, "réponse"), (ComposeKind::Forward, "transfert")] {
        let mut mb = Mailbox::open(dir)?;
        let mut d = mb.compose(kind, Some(&format!("m{original}")))?;
        d.to = address.clone();
        d.subject = format!("TL-verbe {label} {stamp}");
        mb.send_draft(&d)?;
        drop(mb);
        let local: Option<i64> = conn.query_row("SELECT last_verb FROM message WHERE id = ?1", [original], |r| r.get(0)).map_err(e)?;
        let r = sync_account_blocking(dir, source)?;
        println!(
            "{label} envoyée : {} envoi(s), erreurs {:?} / {:?} ; état local {local:?} ; drapeaux serveur : {}",
            r.sent,
            r.send_error,
            r.push_error,
            server_flags(dir, source, uid)?
        );
    }

    // Nettoyage : drapeaux de l'original remis, messages de test supprimés du serveur.
    let tag = format!("TL-verbe");
    let removed = with_session(dir, source, async |s| {
        s.select("INBOX").await.map_err(e)?;
        let restore = if before.contains("Answered") { "+FLAGS (\\Answered)" } else { "-FLAGS (\\Answered)" };
        let _: Vec<_> = s.uid_store(uid.to_string(), restore).await.map_err(e)?.try_collect().await.map_err(e)?;
        if !before.contains("Forwarded") {
            let _: Vec<_> = s.uid_store(uid.to_string(), "-FLAGS ($Forwarded)").await.map_err(e)?.try_collect().await.map_err(e)?;
        }
        let mut n = 0;
        for _ in 0..10 {
            n = 0;
            for mailbox in ["INBOX", "INBOX.Sent"] {
                s.select(mailbox).await.map_err(e)?;
                let found = s.uid_search(format!("SUBJECT \"{tag} \" SUBJECT \"{stamp}\"")).await.map_err(e)?;
                for u in &found {
                    let _: Vec<_> = s.uid_store(u.to_string(), "+FLAGS (\\Deleted)").await.map_err(e)?.try_collect().await.map_err(e)?;
                    let _: Vec<_> = s.uid_expunge(u.to_string()).await.map_err(e)?.try_collect().await.map_err(e)?;
                }
                n += found.len();
            }
            // Deux messages envoyés : deux copies dans Éléments envoyés et deux reçus.
            if n >= 4 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
        Ok(n)
    })?;
    sync_account_blocking(dir, source)?;
    let left: i64 = conn
        .query_row("SELECT COUNT(*) FROM message m JOIN folder f ON f.id = m.folder_id WHERE f.source_id = ?1 AND m.subject LIKE '%' || ?2", params![source, stamp.to_string()], |r| r.get(0))
        .map_err(e)?;
    println!("nettoyage : {removed} message(s) de test supprimé(s) du serveur, {left} restant(s) en local ; drapeaux de l'original : {}", server_flags(dir, source, uid)?);
    Ok(())
}
