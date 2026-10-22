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
    /// Réponse ou transfert envoyé : `\Answered` ou `$Forwarded` ajouté à l'original.
    Verb { id: i64, forwarded: bool },
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
        Change::Seen { .. } | Change::Flagged { .. } | Change::Verb { .. } => {
            let (id, on, flag) = match change {
                Change::Seen { id, on } => (id, on, r"\Seen"),
                Change::Flagged { id, on } => (id, on, r"\Flagged"),
                Change::Verb { id, forwarded } => (id, true, if forwarded { "$Forwarded" } else { r"\Answered" }),
                _ => unreachable!(),
            };
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
                        // Vers un autre compte : la suppression attend que le dépôt dans l'autre compte ait réussi.
                        None if dst.account.is_some() && dst.mailbox.is_some() => {
                            push(conn, a, "expunge", mb, Some(uid), None, serde_json::json!({ "after_append": id }).to_string())?
                        }
                        None => push(conn, a, "expunge", mb, Some(uid), Some(id), "{}".into())?,
                    }
                    conn.execute("UPDATE message SET imap_uid = NULL WHERE id = ?1", [id]).map_err(err)?;
                }
                (None, _, Some((row, kind, mailbox, uid))) => match dst.mailbox.as_deref() {
                    // Retour au dossier d'origine avant report : rien à faire sur le serveur.
                    Some(target) if kind == "move" && target == mailbox && same_account => {
                        conn.execute("DELETE FROM imap_pending WHERE id = ?1", [row]).map_err(err)?;
                        // Dossier et UID d'origine ensemble : l'UID ne vaut que dans son dossier.
                        conn.execute("UPDATE message SET folder_id = ?2, imap_uid = ?3 WHERE id = ?1", params![id, to, uid]).map_err(err)?;
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

fn account_of(conn: &Connection, source: i64) -> Result<Option<i64>> {
    conn.query_row("SELECT id FROM account WHERE source_id = ?1", [source], |r| r.get(0)).optional().map_err(err)
}

/// Séparateur IMAP du compte de la source : celui qui suit `base` (sinon « INBOX ») dans un nom existant
/// (« INBOX.Sent »), sinon « / ».
pub(crate) fn delimiter(conn: &Connection, source: i64, base: &str) -> Result<String> {
    Ok(conn
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
        .unwrap_or_else(|| "/".into()))
}

/// Nom affiché, nom IMAP et séparateur d'un dossier `name` placé sous `parent` dans un compte : sous la Boîte de
/// réception au premier niveau (comme l'exige Hostinger), séparateur remplacé dans le nom ([`server_leaf`]).
fn server_name(conn: &Connection, source: i64, parent: Option<i64>, name: &str) -> Result<(String, String, String)> {
    let parent_name: Option<String> = match parent {
        Some(p) => conn.query_row("SELECT imap_name FROM folder WHERE id = ?1", [p], |r| r.get(0)).map_err(err)?,
        None => None,
    };
    let inbox: Option<String> = conn
        .query_row("SELECT imap_name FROM folder WHERE source_id = ?1 AND role = 'inbox' AND imap_name IS NOT NULL", [source], |r| r.get(0))
        .optional()
        .map_err(err)?;
    let base = parent_name.or(inbox).unwrap_or_else(|| "INBOX".into());
    let delimiter = delimiter(conn, source, &base)?;
    let leaf = server_leaf(name, &delimiter);
    let imap_name = format!("{base}{delimiter}{}", crate::imap_utf7::encode(&leaf));
    Ok((leaf, imap_name, delimiter))
}

/// Dossier créé dans ThunderLook sous un dossier d'un compte : son nom sur le serveur est calculé (sous la Boîte de
/// réception au premier niveau, comme l'exige Hostinger) et sa création est mise en attente. Sans effet hors compte.
pub(crate) fn folder_created(conn: &Connection, folder: i64) -> Result<()> {
    let (source, parent, name): (i64, Option<i64>, String) =
        conn.query_row("SELECT source_id, parent_id, name FROM folder WHERE id = ?1", [folder], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(err)?;
    let Some(account) = account_of(conn, source)? else { return Ok(()) };
    let (leaf, imap_name, _) = server_name(conn, source, parent, &name)?;
    conn.execute("UPDATE folder SET name = ?2, imap_name = ?3 WHERE id = ?1", params![folder, leaf, imap_name]).map_err(err)?;
    push(conn, account, "mkdir", &imap_name, None, None, "{}".into())
}

/// Le nom IMAP `name` est `old` ou l'un de ses sous-dossiers : renvoie ce qui suit `old` (vide pour `old` lui-même).
pub fn under<'a>(name: &'a str, old: &str, delimiter: &str) -> Option<&'a str> {
    let rest = name.strip_prefix(old)?;
    (rest.is_empty() || (!delimiter.is_empty() && rest.starts_with(delimiter))).then_some(rest)
}

/// Dossier d'un compte renommé ou déplacé dans ThunderLook (renommer, déplacer, supprimer vers « Éléments
/// supprimés », annuler l'une de ces opérations), appelé une fois la ligne `folder` à jour (parent, nom) : nouveau nom
/// IMAP du dossier et de ses sous-dossiers (le serveur les renomme avec lui, RFC 3501 § 6.3.5), cibles des règles qui
/// les désignent (`imap:<source>:<nom>`), et `RENAME` mis en attente. Sans effet hors compte.
pub(crate) fn folder_relocated(conn: &Connection, folder: i64) -> Result<()> {
    let (source, parent, name, old): (i64, Option<i64>, String, Option<String>) = conn
        .query_row("SELECT source_id, parent_id, name, imap_name FROM folder WHERE id = ?1", [folder], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .map_err(err)?;
    let (Some(account), Some(old)) = (account_of(conn, source)?, old) else { return Ok(()) };
    let (leaf, new, delimiter) = server_name(conn, source, parent, &name)?;
    conn.execute("UPDATE folder SET name = ?2 WHERE id = ?1", params![folder, leaf]).map_err(err)?;
    if new == old {
        return Ok(());
    }
    // Le dossier et tous ses sous-dossiers : préfixe remplacé.
    let all: Vec<(i64, String)> = conn
        .prepare("SELECT id, imap_name FROM folder WHERE source_id = ?1 AND imap_name IS NOT NULL")
        .map_err(err)?
        .query_map([source], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(err)?
        .collect::<std::result::Result<_, _>>()
        .map_err(err)?;
    for (id, imap) in all {
        if let Some(rest) = under(&imap, &old, &delimiter) {
            conn.execute("UPDATE folder SET imap_name = ?2 WHERE id = ?1", params![id, format!("{new}{rest}")]).map_err(err)?;
        }
    }
    retarget_rules(conn, source, &old, &new, &delimiter, &leaf)?;
    push(conn, account, "rename", &old, None, None, to_arg(&new))
}

/// Règles dont une action vise `old` ou l'un de ses sous-dossiers (`imap:<source>:<nom>`) : la cible suit le nouveau
/// nom, et le nom affiché du dossier renommé lui-même devient `display`.
fn retarget_rules(conn: &Connection, source: i64, old: &str, new: &str, delimiter: &str, display: &str) -> Result<()> {
    let prefix = format!("imap:{source}:");
    let rows: Vec<(i64, String)> = conn
        .prepare("SELECT id, rule_json FROM rule")
        .map_err(err)?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(err)?
        .collect::<std::result::Result<_, _>>()
        .map_err(err)?;
    for (id, json) in rows {
        let Ok(mut rule) = serde_json::from_str::<thunderlook_rules::Rule>(&json) else { continue };
        let mut changed = false;
        for value in rule.elements.iter_mut().flat_map(|e| e.values.iter_mut()) {
            let thunderlook_rules::Value::Folder { folder_entry_id, name, .. } = value else { continue };
            let Some(target) = std::str::from_utf8(folder_entry_id).ok().and_then(|t| t.strip_prefix(prefix.as_str())) else { continue };
            let Some(rest) = under(target, old, delimiter) else { continue };
            if rest.is_empty() {
                *name = display.to_string();
            }
            *folder_entry_id = crate::rules::imap_target(source, &format!("{new}{rest}"));
            changed = true;
        }
        if changed {
            conn.execute("UPDATE rule SET rule_json = ?2 WHERE id = ?1", params![id, serde_json::to_string(&rule).map_err(err)?]).map_err(err)?;
        }
    }
    Ok(())
}

/// Dossier d'un compte supprimé définitivement (dans « Éléments supprimés », ou création annulée), appelé avant la
/// suppression des lignes : `DELETE` de ses sous-dossiers puis de lui-même (le serveur ne supprime pas les
/// sous-dossiers avec leur parent, RFC 3501 § 6.3.4). Une création encore en attente est simplement retirée.
pub(crate) fn folder_removed(conn: &Connection, folder: i64) -> Result<()> {
    let (source, old): (i64, Option<String>) =
        conn.query_row("SELECT source_id, imap_name FROM folder WHERE id = ?1", [folder], |r| Ok((r.get(0)?, r.get(1)?))).map_err(err)?;
    let (Some(account), Some(old)) = (account_of(conn, source)?, old) else { return Ok(()) };
    let delimiter = delimiter(conn, source, &old)?;
    let mut names: Vec<String> = conn
        .prepare("SELECT imap_name FROM folder WHERE source_id = ?1 AND imap_name IS NOT NULL")
        .map_err(err)?
        .query_map([source], |r| r.get::<_, String>(0))
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?
        .into_iter()
        .filter(|n| under(n, &old, &delimiter).is_some())
        .collect();
    // Les plus profonds d'abord.
    names.sort_by_key(|n| std::cmp::Reverse(n.len()));
    for name in names {
        // Suppressions et drapeaux de ses messages : sans objet, la boîte part avec eux.
        conn.execute("DELETE FROM imap_pending WHERE account_id = ?1 AND mailbox = ?2 AND kind IN ('flags', 'expunge')", params![account, name])
            .map_err(err)?;
        let creating = conn
            .execute("DELETE FROM imap_pending WHERE account_id = ?1 AND kind = 'mkdir' AND mailbox = ?2", params![account, name])
            .map_err(err)?;
        if creating == 0 {
            push(conn, account, "rmdir", &name, None, None, "{}".into())?;
        }
    }
    Ok(())
}

/// Nom d'un dossier tel qu'il est créé sur le serveur : le séparateur IMAP n'y peut pas figurer, sinon
/// « [electromagnetisme.fr] » devient « [electromagnetisme » › « fr] ». Il est remplacé par un caractère d'aspect
/// identique (U+2024 pour le point, U+2215 pour la barre oblique), que `fold` ramène au caractère d'origine.
pub(crate) fn server_leaf(name: &str, delimiter: &str) -> String {
    let substitute = match delimiter {
        "." => "\u{2024}",
        "/" => "\u{2215}",
        _ => "_",
    };
    if delimiter.is_empty() { name.to_string() } else { name.replace(delimiter, substitute) }
}

/// Classe un message dans un dossier en dehors des opérations de l'interface (envoi : Boîte d'envoi vers Éléments
/// envoyés) ; le changement est reporté sur le serveur comme un déplacement.
pub fn file_message(conn: &Connection, id: i64, folder: i64) -> Result<()> {
    let tx = conn.unchecked_transaction().map_err(err)?;
    let from: i64 = tx.query_row("SELECT folder_id FROM message WHERE id = ?1", [id], |r| r.get(0)).map_err(err)?;
    queue(&tx, Change::Moved { id, from, to: folder })?;
    tx.execute("UPDATE message SET folder_id = ?2 WHERE id = ?1", params![id, folder]).map_err(err)?;
    tx.commit().map_err(err)
}
