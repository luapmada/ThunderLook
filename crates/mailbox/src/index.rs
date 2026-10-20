//! Indexation plein texte (J4), en tâche de fond : objet, expéditeur, destinataires (noms et adresses
//! des en-têtes), corps en texte brut et noms des pièces jointes. Les messages sont repris là où
//! l'indexation s'est arrêtée (`message.indexed = 0`), y compris ceux créés par une copie.
//!
//! Le texte n'est pas stocké : l'index est sans contenu (`content=''`) et le corps reste dans les `.eml`.

use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    thread,
};

use mail_parser::{Address, MessageParser};
use thunderlook_store::rusqlite::{params, params_from_iter};

use crate::{Result, err};

/// Messages traités par transaction.
const BATCH: usize = 1000;
/// Taille maximale du corps indexé (caractères) : au-delà, le gain de rappel ne vaut pas la taille de l'index.
const BODY_MAX: usize = 200_000;

struct Pending {
    id: i64,
    subject: String,
    sender: String,
    to_cc: String,
    eml: String,
}

struct Doc {
    id: i64,
    subject: String,
    sender: String,
    recipients: String,
    body: String,
}

fn addresses(a: Option<&Address>, out: &mut String) {
    let Some(a) = a else { return };
    for addr in a.iter() {
        if let Some(n) = addr.name() {
            out.push_str(n);
            out.push(' ');
        }
        if let Some(e) = addr.address() {
            out.push_str(e);
            out.push(' ');
        }
    }
}

fn truncate(mut s: String, max: usize) -> String {
    if s.len() > max {
        let mut cut = max;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
    s
}

/// Corps en texte brut : la partie texte, sinon la partie HTML convertie.
pub(crate) fn body_text(m: &mail_parser::Message) -> String {
    let text_part = m.text_part(0).filter(|t| t.is_text() && !t.is_text_html());
    match (text_part.and_then(|t| t.text_contents()), m.html_part(0).filter(|h| h.is_text_html())) {
        (Some(t), _) => t.to_string(),
        (None, Some(h)) => h.text_contents().map(thunderlook_mime::html_to_text).unwrap_or_default(),
        (None, None) => String::new(),
    }
}

/// Lit le `.eml` : corps en texte brut et destinataires des en-têtes.
fn parse(data_dir: &Path, p: Pending) -> Doc {
    let mut recipients = p.to_cc;
    let mut body = String::new();
    if let Ok(bytes) = fs::read(thunderlook_store::resolve(data_dir, &p.eml))
        && let Some(m) = MessageParser::default().parse(&bytes)
    {
        recipients.push(' ');
        addresses(m.to(), &mut recipients);
        addresses(m.cc(), &mut recipients);
        addresses(m.bcc(), &mut recipients);
        body = truncate(body_text(&m), BODY_MAX);
    }
    Doc { id: p.id, subject: p.subject, sender: p.sender, recipients, body }
}

/// Indexe les messages en attente. Renvoie le nombre indexé ; s'arrête proprement si `stop` passe à vrai.
pub fn index_pending(data_dir: &Path, stop: &AtomicBool, progress: &mut dyn FnMut(u64, u64)) -> Result<u64> {
    let mut conn = thunderlook_store::open(data_dir).map_err(err)?;
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM message WHERE indexed = 0", [], |r| r.get(0)).map_err(err)?;
    let total = total as u64;
    let workers = thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16);
    let mut done = 0u64;
    progress(done, total);
    while !stop.load(Ordering::Relaxed) {
        let batch: Vec<Pending> = {
            let mut stmt = conn
                .prepare_cached(
                    "SELECT id, subject, from_name || ' ' || from_addr, to_line || ' ' || cc_line, eml_path
                     FROM message WHERE indexed = 0 ORDER BY id LIMIT ?1",
                )
                .map_err(err)?;
            stmt.query_map([BATCH as i64], |r| {
                Ok(Pending { id: r.get(0)?, subject: r.get(1)?, sender: r.get(2)?, to_cc: r.get(3)?, eml: r.get(4)? })
            })
            .map_err(err)?
            .collect::<std::result::Result<_, _>>()
            .map_err(err)?
        };
        if batch.is_empty() {
            break;
        }
        let ids: Vec<i64> = batch.iter().map(|p| p.id).collect();
        let mut attachments: HashMap<i64, String> = HashMap::new();
        {
            let marks = vec!["?"; ids.len()].join(",");
            let mut stmt = conn
                .prepare(&format!("SELECT message_id, filename FROM attachment WHERE message_id IN ({marks})"))
                .map_err(err)?;
            let rows = stmt.query_map(params_from_iter(&ids), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).map_err(err)?;
            for row in rows {
                let (m, name) = row.map_err(err)?;
                let e = attachments.entry(m).or_default();
                e.push_str(&name);
                e.push(' ');
            }
        }

        // Lecture des .eml en parallèle, écriture dans une seule transaction.
        let chunk = batch.len().div_ceil(workers);
        let mut parts: Vec<Vec<Pending>> = Vec::new();
        let mut it = batch.into_iter().peekable();
        while it.peek().is_some() {
            parts.push(it.by_ref().take(chunk).collect());
        }
        let docs: Vec<Doc> = thread::scope(|s| {
            let handles: Vec<_> = parts
                .into_iter()
                .map(|part| s.spawn(move || part.into_iter().map(|p| parse(data_dir, p)).collect::<Vec<_>>()))
                .collect();
            handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
        });

        let tx = conn.transaction().map_err(err)?;
        {
            let mut del = tx.prepare_cached("DELETE FROM message_fts WHERE rowid = ?1").map_err(err)?;
            let mut ins = tx
                .prepare_cached("INSERT INTO message_fts (rowid, subject, sender, recipients, body, attachments) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")
                .map_err(err)?;
            let mut mark = tx.prepare_cached("UPDATE message SET indexed = 1 WHERE id = ?1").map_err(err)?;
            for d in &docs {
                del.execute([d.id]).map_err(err)?;
                let att = attachments.get(&d.id).map(String::as_str).unwrap_or("");
                ins.execute(params![d.id, d.subject, d.sender, d.recipients, d.body, att]).map_err(err)?;
                mark.execute([d.id]).map_err(err)?;
            }
        }
        tx.commit().map_err(err)?;
        done += docs.len() as u64;
        progress(done, total.max(done));
    }
    Ok(done)
}
