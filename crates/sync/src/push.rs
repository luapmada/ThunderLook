//! Report sur le serveur des changements faits dans ThunderLook (`imap_pending`, rempli par `thunderlook-mailbox`).
//!
//! Les lignes sont rejouées dans l'ordre. Après un déplacement, une copie ou un dépôt, le nouvel UID est retrouvé dans
//! le dossier d'arrivée par l'en-tête Message-ID, et l'état local (lu, indicateur) y est reporté ; sans Message-ID, la
//! copie locale est retirée et la relève suivante la reprend du serveur. Une ligne en échec est gardée avec son erreur
//! et retentée à la relève suivante (5 tentatives).

use std::path::Path;

use futures::TryStreamExt;
use thunderlook_store::rusqlite::{Connection, OptionalExtension, params};

use crate::{ImapSession, Result, account::Account};

const MAX_ATTEMPTS: i64 = 5;

fn e(x: impl std::fmt::Display) -> String {
    x.to_string()
}

struct Pending {
    id: i64,
    kind: String,
    mailbox: String,
    uid: Option<u32>,
    message: Option<i64>,
    arg: serde_json::Value,
}

async fn select(s: &mut ImapSession, current: &mut Option<String>, mailbox: &str) -> Result<()> {
    if current.as_deref() != Some(mailbox) {
        s.select(mailbox).await.map_err(|x| format!("{mailbox} : {x}"))?;
        *current = Some(mailbox.to_string());
    }
    Ok(())
}

async fn store(s: &mut ImapSession, uid: u32, query: &str) -> Result<()> {
    let _: Vec<_> = s.uid_store(uid.to_string(), query).await.map_err(e)?.try_collect().await.map_err(e)?;
    Ok(())
}

/// Nouvel UID d'un message arrivé dans `mailbox` (sélectionné), d'après son Message-ID ; le plus grand UID qui n'est
/// pas déjà connu localement dans ce dossier.
async fn find_uid(s: &mut ImapSession, conn: &Connection, folder: i64, message_id: &str) -> Result<Option<u32>> {
    let quoted = message_id.replace('\\', "\\\\").replace('"', "\\\"");
    let found = s.uid_search(format!("HEADER Message-ID \"{quoted}\"")).await.map_err(e)?;
    let mut uids: Vec<u32> = found.into_iter().collect();
    uids.sort_unstable();
    for uid in uids.into_iter().rev() {
        let known = conn
            .query_row("SELECT 1 FROM message WHERE folder_id = ?1 AND imap_uid = ?2", params![folder, uid], |_| Ok(()))
            .optional()
            .map_err(e)?
            .is_some();
        if !known {
            return Ok(Some(uid));
        }
    }
    Ok(None)
}

/// Rattache le message local `id` à son nouvel UID dans `mailbox`, et y reporte son état lu / indicateur.
async fn attach(s: &mut ImapSession, conn: &Connection, data_dir: &Path, current: &mut Option<String>, account: &Account, id: i64, mailbox: &str) -> Result<()> {
    let row: Option<(i64, Option<String>, bool, i64, String, Option<i64>)> = conn
        .query_row(
            "SELECT folder_id, internet_message_id, is_read, flag_status, eml_path, last_verb FROM message WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .optional()
        .map_err(e)?;
    let Some((folder, message_id, read, flag, eml, verb)) = row else { return Ok(()) };
    // Le message a pu changer de dossier local depuis : on ne rattache que s'il est bien dans `mailbox`.
    let local_mailbox: Option<String> = conn.query_row("SELECT imap_name FROM folder WHERE id = ?1", [folder], |r| r.get(0)).map_err(e)?;
    if local_mailbox.as_deref() != Some(mailbox) {
        return Ok(());
    }
    select(s, current, mailbox).await?;
    let uid = match message_id.as_deref().filter(|m| !m.is_empty()) {
        Some(mid) => find_uid(s, conn, folder, &format!("<{}>", mid.trim_matches(['<', '>']))).await?,
        None => None,
    };
    match uid {
        Some(uid) => {
            conn.execute("UPDATE message SET imap_uid = ?2 WHERE id = ?1", params![id, uid]).map_err(e)?;
            store(s, uid, if read { r"+FLAGS (\Seen)" } else { r"-FLAGS (\Seen)" }).await?;
            store(s, uid, if flag == 2 { r"+FLAGS (\Flagged)" } else { r"-FLAGS (\Flagged)" }).await?;
            // Répondu ou transféré (`PidTagLastVerbExecuted`) : 104 transféré, sinon répondu.
            match verb {
                Some(104) => store(s, uid, "+FLAGS ($Forwarded)").await?,
                Some(_) => store(s, uid, r"+FLAGS (\Answered)").await?,
                None => {}
            }
        }
        None => {
            // Sans Message-ID retrouvable : la relève reprendra le message du serveur.
            conn.execute("DELETE FROM message_fts WHERE rowid = ?1", [id]).map_err(e)?;
            conn.execute("DELETE FROM message WHERE id = ?1", [id]).map_err(e)?;
            let _ = std::fs::remove_file(thunderlook_store::resolve(data_dir, &eml));
            let _ = account;
        }
    }
    Ok(())
}

/// Garde-fou avant de retirer du serveur un message déplacé hors du compte : sa copie locale existe (ligne `message`
/// et `.eml` non vide, qui commence comme un message). Sinon l'opération reste en attente, avec son erreur.
pub fn local_copy_ok(conn: &Connection, data_dir: &Path, id: i64) -> Result<()> {
    let eml: Option<String> = conn.query_row("SELECT eml_path FROM message WHERE id = ?1", [id], |r| r.get(0)).optional().map_err(e)?;
    let eml = eml.ok_or_else(|| format!("copie locale du message m{id} absente : il n'est pas retiré du serveur"))?;
    let path = thunderlook_store::resolve(data_dir, &eml);
    let mut head = [0u8; 64];
    let n = std::fs::File::open(&path).and_then(|mut f| std::io::Read::read(&mut f, &mut head)).unwrap_or(0);
    if n == 0 || !head[..n].contains(&b':') {
        return Err(format!("copie locale du message m{id} illisible ({eml}) : il n'est pas retiré du serveur"));
    }
    Ok(())
}

/// Quitte la boîte sélectionnée avant de la renommer ou de la supprimer, sans CLOSE (qui effacerait les messages
/// marqués « supprimé » par un autre client) : EXAMINE de la Boîte de réception, en lecture seule.
async fn leave(s: &mut ImapSession, current: &mut Option<String>) -> Result<()> {
    if current.take().is_some() {
        s.examine("INBOX").await.map_err(e)?;
    }
    Ok(())
}

/// Boîte `mailbox` sur le serveur : son séparateur (`Some("")` sans hiérarchie), `None` si elle n'existe pas.
async fn listed(s: &mut ImapSession, mailbox: &str) -> Result<Option<String>> {
    let names: Vec<_> = s.list(Some(""), Some(mailbox)).await.map_err(e)?.try_collect().await.map_err(e)?;
    Ok(names.iter().find(|n| n.name() == mailbox).map(|n| n.delimiter().unwrap_or("").to_string()))
}

/// Abonnements de `mailbox` et de ses sous-dossiers : (nom complet, partie qui suit `mailbox`).
async fn subscribed_under(s: &mut ImapSession, mailbox: &str) -> Result<Vec<(String, String)>> {
    let names: Vec<_> = s.lsub(Some(""), Some(&format!("{mailbox}*"))).await.map_err(e)?.try_collect().await.map_err(e)?;
    Ok(names
        .iter()
        .filter_map(|n| {
            let delimiter = n.delimiter().unwrap_or("");
            thunderlook_mailbox::imap_under(n.name(), mailbox, delimiter).map(|rest| (n.name().to_string(), rest.to_string()))
        })
        .collect())
}

/// Après un `RENAME` : les opérations antérieures encore en attente (en échec) suivent le nouveau nom.
pub fn retarget_pending(conn: &Connection, account: i64, before: i64, old: &str, new: &str, delimiter: &str) -> Result<()> {
    let rows: Vec<(i64, String, String)> = conn
        .prepare("SELECT id, mailbox, arg FROM imap_pending WHERE account_id = ?1 AND id < ?2")
        .map_err(e)?
        .query_map(params![account, before], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(e)?
        .collect::<std::result::Result<_, _>>()
        .map_err(e)?;
    let renamed = |name: &str| thunderlook_mailbox::imap_under(name, old, delimiter).map(|rest| format!("{new}{rest}"));
    for (id, mailbox, arg) in rows {
        let mut value: serde_json::Value = serde_json::from_str(&arg).unwrap_or(serde_json::Value::Null);
        let to = value.get("to").and_then(|v| v.as_str()).and_then(renamed);
        if let Some(to) = &to {
            value["to"] = serde_json::Value::String(to.clone());
        }
        let mb = renamed(&mailbox);
        if mb.is_some() || to.is_some() {
            conn.execute("UPDATE imap_pending SET mailbox = ?2, arg = ?3 WHERE id = ?1", params![id, mb.unwrap_or(mailbox), value.to_string()])
                .map_err(e)?;
        }
    }
    Ok(())
}

/// Rejoue une opération ; `Ok(false)` : à différer sans compter de tentative (suppression qui attend un dépôt).
async fn run(s: &mut ImapSession, conn: &Connection, data_dir: &Path, current: &mut Option<String>, account: &Account, p: &Pending) -> Result<bool> {
    let target = p.arg.get("to").and_then(|v| v.as_str()).map(str::to_string);
    match p.kind.as_str() {
        "flags" => {
            let uid = p.uid.ok_or("UID manquant")?;
            select(s, current, &p.mailbox).await?;
            for (key, op) in [("add", "+FLAGS"), ("remove", "-FLAGS")] {
                if let Some(list) = p.arg.get(key).and_then(|v| v.as_array()) {
                    let flags: Vec<&str> = list.iter().filter_map(|f| f.as_str()).collect();
                    if !flags.is_empty() {
                        store(s, uid, &format!("{op} ({})", flags.join(" "))).await?;
                    }
                }
            }
        }
        "move" | "copy" => {
            let (uid, to) = (p.uid.ok_or("UID manquant")?, target.ok_or("dossier de destination manquant")?);
            select(s, current, &p.mailbox).await?;
            if p.kind == "move" {
                s.uid_mv(uid.to_string(), &to).await.map_err(e)?;
            } else {
                s.uid_copy(uid.to_string(), &to).await.map_err(e)?;
            }
            if let Some(id) = p.message {
                attach(s, conn, data_dir, current, account, id, &to).await?;
            }
        }
        "expunge" => {
            // Déplacement vers un autre compte : rien n'est supprimé tant que le dépôt n'est pas fait.
            if let Some(waiting) = p.arg.get("after_append").and_then(|v| v.as_i64()) {
                let pending: bool = conn
                    .query_row("SELECT EXISTS (SELECT 1 FROM imap_pending WHERE kind = 'append' AND message_id = ?1)", [waiting], |r| r.get(0))
                    .map_err(e)?;
                if pending {
                    return Ok(false);
                }
            }
            // Message gardé localement hors du compte (règle ou déplacement vers un PST) : retiré du serveur
            // seulement si sa copie locale est bien là (ligne et `.eml` lisible, non vide).
            if let Some(id) = p.message {
                local_copy_ok(conn, data_dir, id)?;
            }
            let uid = p.uid.ok_or("UID manquant")?;
            select(s, current, &p.mailbox).await?;
            store(s, uid, r"+FLAGS (\Deleted)").await?;
            let _: Vec<_> = s.uid_expunge(uid.to_string()).await.map_err(e)?.try_collect().await.map_err(e)?;
        }
        "append" => {
            let id = p.message.ok_or("message manquant")?;
            let to = target.unwrap_or_else(|| p.mailbox.clone());
            let row: Option<(String, bool, Option<i64>)> = conn
                .query_row("SELECT eml_path, is_read, received_at FROM message WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .optional()
                .map_err(e)?;
            let Some((eml, read, received)) = row else { return Ok(true) };
            let bytes = std::fs::read(thunderlook_store::resolve(data_dir, &eml)).map_err(|x| format!("{eml} : {x}"))?;
            // Les brouillons de ThunderLook portent « X-Unsent » ; rien à retirer pour le serveur.
            // Date de réception d'origine (INTERNALDATE), sinon le serveur daterait le message du dépôt.
            let date = received.and_then(|t| {
                use chrono::TimeZone;
                // Entre guillemets (date-time de la RFC 3501) : async-imap insère la valeur telle quelle.
                chrono::Local.timestamp_millis_opt(t).single().map(|d| d.format("\"%d-%b-%Y %H:%M:%S %z\"").to_string())
            });
            s.append(&to, Some(if read { r"(\Seen)" } else { "()" }), date.as_deref(), &bytes).await.map_err(e)?;
            *current = None;
            attach(s, conn, data_dir, current, account, id, &to).await?;
        }
        "mkdir" => {
            s.create(&p.mailbox).await.map_err(e)?;
            // Abonnement : Outlook n'affiche en IMAP que les dossiers abonnés.
            s.subscribe(&p.mailbox).await.map_err(e)?;
        }
        "rename" => {
            let to = target.ok_or("nouveau nom manquant")?;
            // Le dossier sélectionné peut être celui qu'on renomme (ou l'un de ses sous-dossiers).
            leave(s, current).await?;
            // Noms abonnés du dossier et de ses sous-dossiers, avant (le serveur renomme les sous-dossiers avec lui).
            let subscribed = subscribed_under(s, &p.mailbox).await?;
            if let Err(x) = s.rename(&p.mailbox, &to).await {
                // Déjà fait (relance après une coupure) : l'ancien nom n'existe plus, le nouveau si.
                if !(listed(s, &to).await?.is_some() && listed(s, &p.mailbox).await?.is_none()) {
                    return Err(format!("{} → {to} : {x}", p.mailbox));
                }
            }
            // Abonnements : Outlook n'affiche en IMAP que les dossiers abonnés, et tous les serveurs ne les déplacent pas.
            for (old, rest) in &subscribed {
                let _ = s.unsubscribe(old).await;
                let _ = s.subscribe(format!("{to}{rest}")).await;
            }
            if subscribed.is_empty() {
                let _ = s.subscribe(&to).await;
            }
            // Opérations plus anciennes restées en échec : elles visent désormais le nouveau nom.
            let delimiter = listed(s, &to).await?.unwrap_or_default();
            retarget_pending(conn, account.id, p.id, &p.mailbox, &to, &delimiter)?;
        }
        "rmdir" => {
            leave(s, current).await?;
            let _ = s.unsubscribe(&p.mailbox).await;
            if let Err(x) = s.delete(&p.mailbox).await {
                // Déjà supprimé : rien à faire.
                if listed(s, &p.mailbox).await?.is_some() {
                    return Err(format!("{} : {x}", p.mailbox));
                }
            }
        }
        other => return Err(format!("opération « {other} » non prise en charge")),
    }
    Ok(true)
}

/// Rejoue les opérations en attente du compte ; renvoie le nombre d'opérations faites et la dernière erreur.
pub async fn push(s: &mut ImapSession, conn: &Connection, data_dir: &Path, account: &Account) -> Result<(u32, Option<String>)> {
    let rows: Vec<Pending> = conn
        .prepare("SELECT id, kind, mailbox, uid, message_id, arg FROM imap_pending WHERE account_id = ?1 AND attempts < ?2 ORDER BY id")
        .map_err(e)?
        .query_map(params![account.id, MAX_ATTEMPTS], |r| {
            Ok(Pending {
                id: r.get(0)?,
                kind: r.get(1)?,
                mailbox: r.get(2)?,
                uid: r.get(3)?,
                message: r.get(4)?,
                arg: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or(serde_json::Value::Null),
            })
        })
        .map_err(e)?
        .collect::<std::result::Result<_, _>>()
        .map_err(e)?;
    let (mut done, mut last_error) = (0, None);
    let mut current = None;
    for p in rows {
        match run(s, conn, data_dir, &mut current, account, &p).await {
            Ok(true) => {
                conn.execute("DELETE FROM imap_pending WHERE id = ?1", [p.id]).map_err(e)?;
                done += 1;
            }
            Ok(false) => {}
            Err(err) => {
                conn.execute("UPDATE imap_pending SET attempts = attempts + 1, last_error = ?2 WHERE id = ?1", params![p.id, err]).map_err(e)?;
                last_error = Some(format!("{} ({}) : {err}", p.kind, p.mailbox));
            }
        }
    }
    Ok((done, last_error))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Point ouvert du J7 : un message déplacé hors du compte (règle vers un PST) n'est retiré du serveur que si sa
    /// copie locale est écrite.
    #[test]
    fn server_copy_kept_without_local_copy() {
        let dir = std::env::temp_dir().join(format!("tl-push-guard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = thunderlook_store::open(&dir).unwrap();
        c.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (1, 'pst', 'x', 'x', 'x', 0, 0)", []).unwrap();
        c.execute("INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position) VALUES (1, NULL, 0, 'Archives', 'folder', NULL, 0, 0)", [])
            .unwrap();
        let folder = c.last_insert_rowid();
        c.execute(
            "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, message_flags, is_read, flag_status,
                importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
             VALUES (?1, 0, 'IPM.Note', 's', '', '', '', '', 0, 1, 0, 1, 0, 0, 0, 'text', '', 'eml/1/0000/1.eml', x'')",
            [folder],
        )
        .unwrap();
        let id = c.last_insert_rowid();
        assert!(local_copy_ok(&c, &dir, id).unwrap_err().contains("illisible"), "fichier absent");
        let path = thunderlook_store::resolve(&dir, "eml/1/0000/1.eml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"").unwrap();
        assert!(local_copy_ok(&c, &dir, id).is_err(), "fichier vide");
        std::fs::write(&path, b"From: a@example.org\r\nSubject: s\r\n\r\ncorps\r\n").unwrap();
        assert!(local_copy_ok(&c, &dir, id).is_ok());
        assert!(local_copy_ok(&c, &dir, id + 1).unwrap_err().contains("absente"), "ligne absente");
        drop(c);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
