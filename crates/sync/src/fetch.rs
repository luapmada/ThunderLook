//! Relève IMAP (J7) : dossiers, nouveaux messages, drapeaux et suppressions faites ailleurs.
//!
//! Chaque message relevé est stocké comme ceux d'un PST : un `.eml` (octets reçus, inchangés) et une ligne `message`
//! (expéditeur, destinataires, objet, dates, aperçu, pièces jointes, conversation), avec `imap_uid`. L'indexation plein
//! texte du J4 les reprend ensuite (`indexed = 0`).

use std::{collections::HashSet, path::Path, time::Instant};

use async_imap::types::{Flag, NameAttribute};
use futures::TryStreamExt;
use mail_parser::{Address, MessageParser, MimeHeaders};
use thunderlook_store::rusqlite::{Connection, OptionalExtension, params};

use crate::{ImapSession, Result, account::Account};

/// Messages relevés par commande `UID FETCH`.
const BATCH: usize = 50;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub folders: u32,
    pub new_messages: u32,
    pub updated_flags: u32,
    pub removed: u32,
    pub elapsed_ms: u32,
    /// Opération locale qui n'a pas pu être reportée sur le serveur (gardée pour la relève suivante).
    pub push_error: Option<String>,
    /// Messages envoyés depuis la Boîte d'envoi, et dernière erreur d'envoi.
    pub sent: u32,
    pub send_error: Option<String>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

fn e(x: impl std::fmt::Display) -> String {
    x.to_string()
}

/// Dossier du serveur et sa place dans ThunderLook.
struct Remote {
    name: String,
    role: &'static str,
    display: String,
    /// Nom IMAP du parent dans l'arborescence locale (`None` : haut du compte).
    parent: Option<String>,
    selectable: bool,
}

/// Rôles d'après `INBOX` et les attributs SPECIAL-USE (RFC 6154) ; les dossiers spéciaux vont au haut du compte sous
/// leur nom français, comme dans Outlook, même quand le serveur les range sous `INBOX.` (Hostinger).
fn remote_folders(list: &[(String, Option<String>, Vec<NameAttribute<'static>>)]) -> Vec<Remote> {
    let special = |attrs: &[NameAttribute]| {
        attrs.iter().find_map(|a| match a {
            NameAttribute::Sent => Some(("sent", "Éléments envoyés")),
            NameAttribute::Drafts => Some(("drafts", "Brouillons")),
            NameAttribute::Junk => Some(("junk", "Courrier indésirable")),
            NameAttribute::Trash => Some(("deleted", "Éléments supprimés")),
            NameAttribute::Archive => Some(("archive", "Archives")),
            _ => None,
        })
    };
    list.iter()
        .map(|(name, delim, attrs)| {
            let selectable = !attrs.iter().any(|a| matches!(a, NameAttribute::NoSelect));
            if name.eq_ignore_ascii_case("INBOX") {
                return Remote { name: name.clone(), role: "inbox", display: "Boîte de réception".into(), parent: None, selectable };
            }
            if let Some((role, label)) = special(attrs) {
                return Remote { name: name.clone(), role, display: label.into(), parent: None, selectable };
            }
            let (parent, leaf) = match delim.as_deref().and_then(|d| name.rsplit_once(d)) {
                Some((p, l)) => (Some(p.to_string()), l.to_string()),
                None => (None, name.clone()),
            };
            Remote { name: name.clone(), role: "folder", display: leaf, parent, selectable }
        })
        .collect()
}

/// Crée, renomme ou retire les dossiers locaux pour refléter le serveur ; renvoie les dossiers à relever.
fn reconcile_folders(conn: &Connection, data_dir: &Path, source: i64, remotes: &[Remote]) -> Result<Vec<(i64, String)>> {
    let top: i64 = conn
        .query_row("SELECT id FROM folder WHERE source_id = ?1 AND role = 'ipm'", [source], |r| r.get(0))
        .map_err(e)?;
    let mut ids: Vec<(String, i64)> = Vec::new();
    // Parents avant enfants : tri par profondeur du nom.
    let mut ordered: Vec<&Remote> = remotes.iter().collect();
    ordered.sort_by_key(|r| r.name.matches(['.', '/']).count());
    for (pos, r) in ordered.iter().enumerate() {
        let parent = r.parent.as_ref().and_then(|p| ids.iter().find(|(n, _)| n == p).map(|x| x.1)).unwrap_or(top);
        let existing: Option<i64> = conn
            .query_row("SELECT id FROM folder WHERE source_id = ?1 AND imap_name = ?2", params![source, r.name], |x| x.get(0))
            .optional()
            .map_err(e)?;
        let id = match existing {
            Some(id) => {
                conn.execute("UPDATE folder SET name = ?2, role = ?3, parent_id = ?4 WHERE id = ?1", params![id, r.display, r.role, parent])
                    .map_err(e)?;
                id
            }
            None => {
                conn.execute(
                    "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position, imap_name)
                     VALUES (?1, ?2, 0, ?3, ?4, 'IPF.Note', 0, ?5, ?6)",
                    params![source, parent, r.display, r.role, pos as i64, r.name],
                )
                .map_err(e)?;
                conn.last_insert_rowid()
            }
        };
        ids.push((r.name.clone(), id));
    }
    // Dossiers disparus du serveur : retirés avec leurs messages (et leurs `.eml`).
    let gone: Vec<i64> = conn
        .prepare("SELECT id FROM folder WHERE source_id = ?1 AND imap_name IS NOT NULL")
        .map_err(e)?
        .query_map([source], |r| r.get(0))
        .map_err(e)?
        .collect::<std::result::Result<Vec<i64>, _>>()
        .map_err(e)?
        .into_iter()
        .filter(|id| !ids.iter().any(|(_, x)| x == id))
        .collect();
    for id in gone {
        purge_messages(conn, data_dir, &format!("folder_id = {id}"))?;
        conn.execute("DELETE FROM folder WHERE id = ?1", [id]).map_err(e)?;
    }
    Ok(ids
        .into_iter()
        .filter(|(name, _)| remotes.iter().any(|r| &r.name == name && r.selectable))
        .map(|(n, id)| (id, n))
        .collect())
}

/// Supprime des messages locaux (lignes, index plein texte, `.eml`) selon une condition SQL sur `message`.
fn purge_messages(conn: &Connection, data_dir: &Path, cond: &str) -> Result<u32> {
    let rows: Vec<(i64, String)> = conn
        .prepare(&format!("SELECT id, eml_path FROM message WHERE {cond}"))
        .map_err(e)?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(e)?
        .collect::<std::result::Result<_, _>>()
        .map_err(e)?;
    for (id, rel) in &rows {
        conn.execute("DELETE FROM message_fts WHERE rowid = ?1", [id]).map_err(e)?;
        conn.execute("DELETE FROM message WHERE id = ?1", [id]).map_err(e)?;
        if !rel.is_empty() {
            let _ = std::fs::remove_file(thunderlook_store::resolve(data_dir, rel));
        }
    }
    Ok(rows.len() as u32)
}

fn first_address(a: Option<&Address>) -> (String, String) {
    a.and_then(|a| a.first()).map_or((String::new(), String::new()), |x| {
        (x.name().unwrap_or_default().to_string(), x.address().unwrap_or_default().to_string())
    })
}

/// Ligne À ou Cc telle que l'import PST l'écrit : noms affichés (sinon adresses), séparés par « ; ».
fn display_line(a: Option<&Address>) -> String {
    a.map(|a| a.iter().map(|x| x.name().or(x.address()).unwrap_or_default().to_string()).collect::<Vec<_>>().join("; ")).unwrap_or_default()
}

/// Importance d'après `Importance` ou `X-Priority` : 0 basse, 1 normale, 2 haute.
fn importance(m: &mail_parser::Message) -> i64 {
    let h = |name: &str| m.header_raw(name).map(|v| v.trim().to_ascii_lowercase());
    match (h("Importance").as_deref(), h("X-Priority").as_deref().and_then(|p| p.chars().next())) {
        (Some("high"), _) | (_, Some('1' | '2')) => 2,
        (Some("low"), _) | (_, Some('4' | '5')) => 0,
        _ => 1,
    }
}

struct Incoming {
    uid: u32,
    seen: bool,
    flagged: bool,
    internal_ms: Option<i64>,
    bytes: Vec<u8>,
}

/// Enregistre un message relevé : ligne `message`, `.eml`, pièces jointes.
fn store_message(conn: &Connection, data_dir: &Path, source: i64, folder: i64, m: &Incoming) -> Result<()> {
    let parsed = MessageParser::default().parse(&m.bytes);
    let (subject, from, to, cc, sent, mid, irt, imp, body, kind, atts) = match &parsed {
        Some(p) => {
            let body = thunderlook_mailbox::body_text(p);
            let kind = if p.html_body_count() > 0 { "html" } else if p.text_body_count() > 0 { "text" } else { "none" };
            let atts: Vec<(String, String, i64, Option<String>, bool)> = p
                .attachments()
                .enumerate()
                .map(|(i, part)| {
                    let name = part.attachment_name().map(str::to_string).unwrap_or_else(|| format!("pièce jointe {}", i + 1));
                    let mime = part
                        .content_type()
                        .map(|c| format!("{}/{}", c.ctype(), c.subtype().unwrap_or("octet-stream")))
                        .unwrap_or_else(|| "application/octet-stream".into());
                    let cid = part.content_id().map(|c| c.trim_matches(['<', '>']).to_string());
                    let inline = part.content_disposition().is_some_and(|d| d.ctype() == "inline");
                    (name, mime, part.len() as i64, cid, inline)
                })
                .collect();
            (
                p.subject().unwrap_or_default().to_string(),
                first_address(p.from()),
                display_line(p.to()),
                display_line(p.cc()),
                p.date().map(|d| d.to_timestamp() * 1000),
                p.message_id().map(str::to_string),
                p.in_reply_to().as_text().map(str::to_string),
                importance(p),
                body,
                kind,
                atts,
            )
        }
        None => (String::new(), (String::new(), String::new()), String::new(), String::new(), None, None, None, 1, String::new(), "none", vec![]),
    };
    let topic = thunderlook_mailbox::normalized_subject(&subject).to_string();
    let conv = thunderlook_store::conversation_id(None, false, Some(&topic)).map(|c| c.to_vec()).unwrap_or_default();
    let received = m.internal_ms.or(sent).unwrap_or_else(now_ms);
    let preview = thunderlook_mime::preview_from_text(&body, 255);
    let attachment_count = atts.iter().filter(|a| !a.4).count() as i64;
    conn.execute(
        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, sent_at, received_at,
            created_at, modified_at, message_flags, is_read, flag_status, flag_request, importance, sensitivity, size, attachment_count,
            body_kind, preview, internet_message_id, in_reply_to, conversation_topic, eml_path, props, conversation_id, imap_uid)
         VALUES (?1, 0, 'IPM.Note', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?8, 0, ?9, ?10, ?11, ?12, 0, ?13, ?14, ?15, ?16, ?17, ?18, ?19, '', x'', ?20, ?21)",
        params![
            folder,
            subject,
            from.0,
            from.1,
            to,
            cc,
            sent,
            received,
            m.seen,
            if m.flagged { 2 } else { 0 },
            m.flagged.then_some("Assurer un suivi"),
            imp,
            m.bytes.len() as i64,
            attachment_count,
            kind,
            preview,
            mid,
            irt,
            topic,
            conv,
            m.uid
        ],
    )
    .map_err(e)?;
    let id = conn.last_insert_rowid();
    let rel = thunderlook_store::eml_relative_path(source, id);
    let path = thunderlook_store::resolve(data_dir, &rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(e)?;
    }
    std::fs::write(&path, &m.bytes).map_err(|x| format!("{} : {x}", path.display()))?;
    conn.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, rel]).map_err(e)?;
    for (pos, (name, mime, size, cid, inline)) in atts.iter().enumerate() {
        conn.execute(
            "INSERT INTO attachment (message_id, position, filename, mime_type, size, method, content_id, inline, hidden, props)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, 0, x'')",
            params![id, pos as i64, name, mime, size, cid, inline],
        )
        .map_err(e)?;
    }
    Ok(())
}

/// Relève un dossier : nouveaux messages, drapeaux, suppressions faites ailleurs.
async fn sync_folder(s: &mut ImapSession, conn: &Connection, data_dir: &Path, source: i64, folder: i64, name: &str, report: &mut SyncReport) -> Result<()> {
    let mb = s.examine(name).await.map_err(|x| format!("{name} : {x}"))?;
    let (validity, known_validity): (Option<u32>, Option<i64>) = (
        mb.uid_validity,
        conn.query_row("SELECT imap_uidvalidity FROM folder WHERE id = ?1", [folder], |r| r.get(0)).map_err(e)?,
    );
    if validity.map(i64::from) != known_validity && known_validity.is_some() {
        // Les UID ne sont plus valables : la copie locale du dossier est rechargée.
        report.removed += purge_messages(conn, data_dir, &format!("folder_id = {folder} AND imap_uid IS NOT NULL"))?;
    }
    let local: HashSet<u32> = conn
        .prepare("SELECT imap_uid FROM message WHERE folder_id = ?1 AND imap_uid IS NOT NULL")
        .map_err(e)?
        .query_map([folder], |r| r.get::<_, u32>(0))
        .map_err(e)?
        .collect::<std::result::Result<_, _>>()
        .map_err(e)?;
    let remote: HashSet<u32> = if mb.exists == 0 { HashSet::new() } else { s.uid_search("ALL").await.map_err(|x| format!("{name} : {x}"))? };

    // Supprimés ailleurs.
    let gone: Vec<u32> = local.difference(&remote).copied().collect();
    for chunk in gone.chunks(500) {
        let list = chunk.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        report.removed += purge_messages(conn, data_dir, &format!("folder_id = {folder} AND imap_uid IN ({list})"))?;
    }

    // Drapeaux des messages déjà connus (lu, indicateur), sauf ceux qui attendent d'être reportés sur le serveur.
    let kept: Vec<u32> = local.intersection(&remote).copied().collect();
    if !kept.is_empty() {
        let fetches: Vec<_> = s.uid_fetch("1:*", "(UID FLAGS)").await.map_err(|x| format!("{name} : {x}"))?.try_collect().await.map_err(e)?;
        let pending: HashSet<u32> = conn
            .prepare("SELECT uid FROM imap_pending WHERE mailbox = ?1 AND uid IS NOT NULL")
            .map_err(e)?
            .query_map([name], |r| r.get::<_, u32>(0))
            .map_err(e)?
            .collect::<std::result::Result<_, _>>()
            .map_err(e)?;
        for f in fetches {
            let Some(uid) = f.uid else { continue };
            if !local.contains(&uid) || pending.contains(&uid) {
                continue;
            }
            let flags: Vec<Flag> = f.flags().collect();
            let seen = flags.iter().any(|x| matches!(x, Flag::Seen));
            let flagged = flags.iter().any(|x| matches!(x, Flag::Flagged));
            report.updated_flags += conn
                .execute(
                    "UPDATE message SET is_read = ?3,
                        flag_status = CASE WHEN ?4 THEN 2 WHEN flag_status = 2 THEN 0 ELSE flag_status END,
                        flag_request = CASE WHEN ?4 THEN COALESCE(flag_request, 'Assurer un suivi') WHEN flag_status = 2 THEN NULL ELSE flag_request END
                     WHERE folder_id = ?1 AND imap_uid = ?2 AND (is_read <> ?3 OR (flag_status = 2) <> ?4)",
                    params![folder, uid, seen, flagged],
                )
                .map_err(e)? as u32;
        }
    }

    // Nouveaux messages, par lots.
    let mut new: Vec<u32> = remote.difference(&local).copied().collect();
    new.sort_unstable();
    for chunk in new.chunks(BATCH) {
        let list = chunk.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        let fetches: Vec<_> = s
            .uid_fetch(&list, "(UID FLAGS INTERNALDATE RFC822.SIZE BODY.PEEK[])")
            .await
            .map_err(|x| format!("{name} : {x}"))?
            .try_collect()
            .await
            .map_err(e)?;
        let tx = conn.unchecked_transaction().map_err(e)?;
        for f in &fetches {
            let (Some(uid), Some(body)) = (f.uid, f.body()) else { continue };
            let flags: Vec<Flag> = f.flags().collect();
            let m = Incoming {
                uid,
                seen: flags.iter().any(|x| matches!(x, Flag::Seen)),
                flagged: flags.iter().any(|x| matches!(x, Flag::Flagged)),
                internal_ms: f.internal_date().map(|d| d.timestamp_millis()),
                bytes: body.to_vec(),
            };
            store_message(&tx, data_dir, source, folder, &m)?;
            report.new_messages += 1;
        }
        tx.commit().map_err(e)?;
    }
    conn.execute(
        "UPDATE folder SET imap_uidvalidity = ?2, imap_uidnext = ?3, imap_modseq = ?4 WHERE id = ?1",
        params![folder, validity, mb.uid_next, mb.highest_modseq.map(|m| m as i64)],
    )
    .map_err(e)?;
    Ok(())
}

/// Relève complète d'un compte sur une session ouverte.
pub async fn sync(s: &mut ImapSession, conn: &Connection, data_dir: &Path, account: &Account) -> Result<SyncReport> {
    let started = Instant::now();
    let mut report = SyncReport::default();
    let names: Vec<_> = s.list(Some(""), Some("*")).await.map_err(e)?.try_collect().await.map_err(e)?;
    let list: Vec<(String, Option<String>, Vec<NameAttribute<'static>>)> = names
        .iter()
        .map(|n| (n.name().to_string(), n.delimiter().map(str::to_string), n.attributes().iter().map(|a| a.clone().into_owned()).collect()))
        .collect();
    let folders = reconcile_folders(conn, data_dir, account.source_id, &remote_folders(&list))?;
    for (id, name) in &folders {
        sync_folder(s, conn, data_dir, account.source_id, *id, name, &mut report).await?;
        report.folders += 1;
    }
    report.elapsed_ms = started.elapsed().as_millis() as u32;
    Ok(report)
}
