//! Report sur le serveur des changements faits dans ThunderLook (J7) : chaque écriture du J3 sur un message d'un
//! compte IMAP ajoute une ligne à `imap_pending` (schéma v8), que `thunderlook-sync` rejoue avant la relève. Sans
//! effet pour les messages des fichiers PST.
//!
//! Un message déplacé perd son UID (le serveur en attribuera un nouveau) : tant que le déplacement n'est pas reporté,
//! un nouveau déplacement modifie la même ligne, et un retour au dossier d'origine l'annule.

use thunderlook_store::rusqlite::{Connection, OptionalExtension, params};

use crate::{Result, err};

/// Changement d'un message, vu des écritures du J3.
pub(crate) enum Change {
    Seen { id: i64, on: bool },
    Flagged { id: i64, on: bool },
    /// `from` : dossier avant le déplacement (la ligne `message` peut déjà porter le nouveau).
    Moved { id: i64, from: i64, to: i64 },
    Copied { source: i64, copy: i64, to: i64 },
    /// Appelé avant la suppression de la ligne.
    Deleted { id: i64 },
    /// Contenu réécrit (brouillon enregistré) : l'ancienne version est retirée du serveur, la nouvelle y est déposée.
    Rewritten { id: i64 },
}

struct Place {
    account: Option<i64>,
    source: i64,
    mailbox: Option<String>,
}

fn place(conn: &Connection, folder: i64) -> Result<Place> {
    conn.query_row(
        "SELECT a.id, f.source_id, f.imap_name FROM folder f LEFT JOIN account a ON a.source_id = f.source_id WHERE f.id = ?1",
        [folder],
        |r| Ok(Place { account: r.get(0)?, source: r.get(1)?, mailbox: r.get(2)? }),
    )
    .map_err(err)
}

fn uid_of(conn: &Connection, id: i64) -> Result<Option<i64>> {
    conn.query_row("SELECT imap_uid FROM message WHERE id = ?1", [id], |r| r.get(0)).map_err(err)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

fn push(conn: &Connection, account: i64, kind: &str, mailbox: &str, uid: Option<i64>, message: Option<i64>, arg: String) -> Result<()> {
    conn.execute(
        "INSERT INTO imap_pending (account_id, kind, mailbox, uid, message_id, arg, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![account, kind, mailbox, uid, message, arg, now_ms()],
    )
    .map(|_| ())
    .map_err(err)
}

/// Déplacement, copie ou dépôt encore en attente pour ce message (il n'a pas encore d'UID).
fn pending_for(conn: &Connection, id: i64) -> Result<Option<(i64, String, String, Option<i64>)>> {
    conn.query_row(
        "SELECT id, kind, mailbox, uid FROM imap_pending WHERE message_id = ?1 AND kind IN ('move', 'copy', 'append') ORDER BY id DESC LIMIT 1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .optional()
    .map_err(err)
}

fn flag_arg(flag: &str, on: bool) -> String {
    format!(r#"{{"{}":["{}"]}}"#, if on { "add" } else { "remove" }, flag.replace('\\', "\\\\"))
}

fn to_arg(mailbox: &str) -> String {
    serde_json::json!({ "to": mailbox }).to_string()
}

pub(crate) fn queue(conn: &Connection, change: Change) -> Result<()> {
    match change {
        Change::Seen { id, on } | Change::Flagged { id, on } => {
            let flag = if matches!(change, Change::Seen { .. }) { r"\Seen" } else { r"\Flagged" };
            let folder: i64 = conn.query_row("SELECT folder_id FROM message WHERE id = ?1", [id], |r| r.get(0)).map_err(err)?;
            let p = place(conn, folder)?;
            // Sans UID (déplacement en attente), l'état local est reporté après le déplacement.
            if let (Some(a), Some(mb), Some(uid)) = (p.account, p.mailbox, uid_of(conn, id)?) {
                push(conn, a, "flags", &mb, Some(uid), Some(id), flag_arg(flag, on))?;
            }
        }
        Change::Moved { id, from, to } => {
            if from == to {
                return Ok(());
            }
            let (src, dst) = (place(conn, from)?, place(conn, to)?);
            let same_account = src.account.is_some() && src.source == dst.source;
            match (uid_of(conn, id)?, src.mailbox.as_deref(), pending_for(conn, id)?) {
                (Some(uid), Some(mb), _) => {
                    let a = src.account.expect("compte");
                    match dst.mailbox.as_deref().filter(|_| same_account) {
                        Some(target) => push(conn, a, "move", mb, Some(uid), Some(id), to_arg(target))?,
                        None => push(conn, a, "expunge", mb, Some(uid), Some(id), "{}".into())?,
                    }
                    conn.execute("UPDATE message SET imap_uid = NULL WHERE id = ?1", [id]).map_err(err)?;
                }
                (None, _, Some((row, kind, mailbox, uid))) => match dst.mailbox.as_deref() {
                    // Retour au dossier d'origine avant report : rien à faire sur le serveur.
                    Some(target) if kind == "move" && target == mailbox && same_account => {
                        conn.execute("DELETE FROM imap_pending WHERE id = ?1", [row]).map_err(err)?;
                        conn.execute("UPDATE message SET imap_uid = ?2 WHERE id = ?1", params![id, uid]).map_err(err)?;
                    }
                    Some(target) if dst.account.is_some() && (same_account || kind == "append") => {
                        conn.execute("UPDATE imap_pending SET arg = ?2 WHERE id = ?1", params![row, to_arg(target)]).map_err(err)?;
                    }
                    _ if kind == "move" => {
                        conn.execute("UPDATE imap_pending SET kind = 'expunge', arg = '{}' WHERE id = ?1", [row]).map_err(err)?;
                    }
                    _ => {
                        conn.execute("DELETE FROM imap_pending WHERE id = ?1", [row]).map_err(err)?;
                    }
                },
                _ => {}
            }
            // Message local (PST, rédaction) qui arrive dans un dossier du compte : déposé sur le serveur.
            if uid_of(conn, id)?.is_none() && pending_for(conn, id)?.is_none() {
                if let (Some(a), Some(target)) = (dst.account, dst.mailbox.as_deref()) {
                    push(conn, a, "append", target, None, Some(id), "{}".into())?;
                }
            }
        }
        Change::Copied { source, copy, to } => {
            let src_folder: i64 = conn.query_row("SELECT folder_id FROM message WHERE id = ?1", [source], |r| r.get(0)).map_err(err)?;
            let (src, dst) = (place(conn, src_folder)?, place(conn, to)?);
            let (Some(a), Some(target)) = (dst.account, dst.mailbox.as_deref()) else { return Ok(()) };
            match (uid_of(conn, source)?, src.mailbox.as_deref()) {
                (Some(uid), Some(mb)) if src.source == dst.source => push(conn, a, "copy", mb, Some(uid), Some(copy), to_arg(target))?,
                _ => push(conn, a, "append", target, None, Some(copy), "{}".into())?,
            }
        }
        Change::Deleted { id } => {
            let folder: i64 = conn.query_row("SELECT folder_id FROM message WHERE id = ?1", [id], |r| r.get(0)).map_err(err)?;
            let p = place(conn, folder)?;
            match (uid_of(conn, id)?, p.mailbox.as_deref(), pending_for(conn, id)?) {
                (Some(uid), Some(mb), _) => push(conn, p.account.expect("compte"), "expunge", mb, Some(uid), None, "{}".into())?,
                (None, _, Some((row, kind, _, _))) if kind == "move" => {
                    conn.execute("UPDATE imap_pending SET kind = 'expunge', arg = '{}', message_id = NULL WHERE id = ?1", [row]).map_err(err)?;
                }
                (None, _, Some((row, _, _, _))) => {
                    conn.execute("DELETE FROM imap_pending WHERE id = ?1", [row]).map_err(err)?;
                }
                _ => {}
            }
            // Les opérations encore liées à ce message (drapeaux) n'ont plus de cible locale.
            conn.execute("UPDATE imap_pending SET message_id = NULL WHERE message_id = ?1", [id]).map_err(err)?;
        }
        Change::Rewritten { id } => {
            let folder: i64 = conn.query_row("SELECT folder_id FROM message WHERE id = ?1", [id], |r| r.get(0)).map_err(err)?;
            let p = place(conn, folder)?;
            let (Some(a), Some(mb)) = (p.account, p.mailbox.as_deref()) else { return Ok(()) };
            if let Some(uid) = uid_of(conn, id)? {
                push(conn, a, "expunge", mb, Some(uid), None, "{}".into())?;
                conn.execute("UPDATE message SET imap_uid = NULL WHERE id = ?1", [id]).map_err(err)?;
            }
            // Un dépôt déjà en attente enverra la dernière version du `.eml`.
            if pending_for(conn, id)?.is_none() {
                push(conn, a, "append", mb, None, Some(id), "{}".into())?;
            }
        }
    }
    Ok(())
}

/// Dossier créé dans ThunderLook sous un dossier d'un compte : son nom sur le serveur est calculé (sous la Boîte de
/// réception au premier niveau, comme l'exige Hostinger) et sa création est mise en attente. Sans effet hors compte.
pub(crate) fn folder_created(conn: &Connection, folder: i64) -> Result<()> {
    let (source, parent, name): (i64, Option<i64>, String) =
        conn.query_row("SELECT source_id, parent_id, name FROM folder WHERE id = ?1", [folder], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(err)?;
    let Some(account): Option<i64> = conn.query_row("SELECT id FROM account WHERE source_id = ?1", [source], |r| r.get(0)).optional().map_err(err)?
    else {
        return Ok(());
    };
    let parent_name: Option<String> = match parent {
        Some(p) => conn.query_row("SELECT imap_name FROM folder WHERE id = ?1", [p], |r| r.get(0)).map_err(err)?,
        None => None,
    };
    let inbox: Option<String> = conn
        .query_row("SELECT imap_name FROM folder WHERE source_id = ?1 AND role = 'inbox' AND imap_name IS NOT NULL", [source], |r| r.get(0))
        .optional()
        .map_err(err)?;
    let base = parent_name.or(inbox).unwrap_or_else(|| "INBOX".into());
    // Séparateur du serveur : celui qui suit « INBOX » dans un nom existant (« INBOX.Sent »), sinon « / ».
    let delimiter = conn
        .query_row(
            "SELECT substr(imap_name, length(?2) + 1, 1) FROM folder WHERE source_id = ?1 AND imap_name LIKE ?2 || '_%' LIMIT 1",
            params![source, base],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(err)?
        .or_else(|| {
            conn.query_row("SELECT substr(imap_name, 6, 1) FROM folder WHERE source_id = ?1 AND imap_name LIKE 'INBOX_%' LIMIT 1", [source], |r| r.get(0))
                .optional()
                .ok()
                .flatten()
        })
        .unwrap_or_else(|| "/".into());
    let imap_name = format!("{base}{delimiter}{name}");
    conn.execute("UPDATE folder SET imap_name = ?2 WHERE id = ?1", params![folder, imap_name]).map_err(err)?;
    push(conn, account, "mkdir", &imap_name, None, None, "{}".into())
}

/// Classe un message dans un dossier en dehors des opérations de l'interface (envoi : Boîte d'envoi vers Éléments
/// envoyés) ; le changement est reporté sur le serveur comme un déplacement.
pub fn file_message(conn: &Connection, id: i64, folder: i64) -> Result<()> {
    let tx = conn.unchecked_transaction().map_err(err)?;
    let from: i64 = tx.query_row("SELECT folder_id FROM message WHERE id = ?1", [id], |r| r.get(0)).map_err(err)?;
    tx.execute("UPDATE message SET folder_id = ?2 WHERE id = ?1", params![id, folder]).map_err(err)?;
    queue(&tx, Change::Moved { id, from, to: folder })?;
    tx.commit().map_err(err)
}
