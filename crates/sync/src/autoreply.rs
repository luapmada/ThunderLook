//! Réponse automatique locale (J7) : tant qu'elle est active (et dans sa période), chaque nouveau message de la Boîte
//! de réception reçoit une réponse, mise dans la Boîte d'envoi du compte. Une réponse par expéditeur et par période ;
//! jamais à soi-même, aux listes de diffusion, aux réponses automatiques ni aux adresses techniques.

use std::path::Path;

use mail_builder::{
    MessageBuilder,
    headers::{address::Address as BAddress, raw::Raw},
};
use mail_parser::{HeaderValue, MessageParser};
use thunderlook_model::AutoReply;
use thunderlook_store::rusqlite::{Connection, params};

use crate::{Result, account::Account};

fn e(x: impl std::fmt::Display) -> String {
    x.to_string()
}

/// Expéditeur à qui l'on ne répond pas.
fn technical(addr: &str) -> bool {
    let local = addr.split('@').next().unwrap_or("").to_ascii_lowercase();
    ["noreply", "no-reply", "no_reply", "donotreply", "do-not-reply", "mailer-daemon", "postmaster", "bounce", "notification", "notifications"]
        .iter()
        .any(|p| local == *p || local.starts_with(&format!("{p}-")) || local.starts_with(&format!("{p}+")) || local.starts_with(&format!("{p}.")))
}

/// Le message appelle-t-il une réponse automatique ? (`None` : oui, avec l'adresse et l'objet ; sinon la raison.)
pub fn eligible(eml: &[u8], own: &str) -> std::result::Result<(String, String, Option<String>), &'static str> {
    let m = MessageParser::default().parse(eml).ok_or("message illisible")?;
    let header = |name: &str| m.header_raw(name).map(|v| v.trim().to_ascii_lowercase());
    if header("List-Id").is_some() || header("List-Unsubscribe").is_some() {
        return Err("liste de diffusion");
    }
    if header("Precedence").is_some_and(|p| ["bulk", "list", "junk"].contains(&p.as_str())) {
        return Err("envoi en nombre");
    }
    if header("Auto-Submitted").is_some_and(|a| a != "no") || header("X-Autoreply").is_some() || header("X-Autorespond").is_some() {
        return Err("réponse automatique");
    }
    let from = m.reply_to().or(m.from()).and_then(|a| a.first()).and_then(|a| a.address()).ok_or("sans expéditeur")?.to_string();
    if from.eq_ignore_ascii_case(own) {
        return Err("message de soi-même");
    }
    if technical(&from) {
        return Err("adresse technique");
    }
    let id = match m.message_id() {
        Some(id) => Some(id.to_string()),
        None => match m.header("Message-ID") {
            Some(HeaderValue::Text(t)) => Some(t.to_string()),
            _ => None,
        },
    };
    Ok((from, m.subject().unwrap_or_default().to_string(), id))
}

/// Période en cours : la réponse automatique est-elle active aujourd'hui (dates `AAAA-MM-JJ`, incluses) ?
pub fn active(cfg: &AutoReply, today: &str) -> bool {
    cfg.enabled && cfg.start.as_deref().is_none_or(|s| s.is_empty() || today >= s) && cfg.end.as_deref().is_none_or(|e| e.is_empty() || today <= e)
}

/// Prépare les réponses aux nouveaux messages `ids` de la Boîte de réception ; renvoie les adresses qui en reçoivent
/// une (à ajouter à `cfg.replied`).
pub fn queue_replies(conn: &Connection, data_dir: &Path, account: &Account, ids: &[i64], cfg: &AutoReply) -> Result<Vec<String>> {
    let outbox: i64 = conn
        .query_row("SELECT id FROM folder WHERE source_id = ?1 AND role = 'outbox' ORDER BY id LIMIT 1", [account.source_id], |r| r.get(0))
        .map_err(e)?;
    let mut replied = Vec::new();
    for id in ids {
        let rel: String = conn.query_row("SELECT eml_path FROM message WHERE id = ?1", [id], |r| r.get(0)).map_err(e)?;
        let Ok(bytes) = std::fs::read(thunderlook_store::resolve(data_dir, &rel)) else { continue };
        let Ok((to, subject, message_id)) = eligible(&bytes, &account.address) else { continue };
        if cfg.replied.iter().chain(&replied).any(|r: &String| r.eq_ignore_ascii_case(&to)) {
            continue;
        }
        let reply_subject = format!("Réponse automatique : {subject}");
        let mut builder = MessageBuilder::new()
            .from(BAddress::new_address(Some(account.sender_name.clone()).filter(|n| !n.is_empty()), account.address.clone()))
            .to(to.clone())
            .subject(reply_subject.clone())
            .header("Auto-Submitted", Raw::new("auto-replied"))
            .header("X-Auto-Response-Suppress", Raw::new("All"))
            .text_body(cfg.text.clone());
        if let Some(mid) = &message_id {
            let bare = mid.trim_matches(['<', '>']).to_string();
            builder = builder.in_reply_to(bare.clone()).references(bare);
        }
        let eml = builder.write_to_vec().map_err(e)?;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
        conn.execute(
            "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, sent_at, received_at,
                created_at, modified_at, message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count,
                body_kind, preview, conversation_topic, eml_path, props)
             VALUES (?1, 0, 'IPM.Note', ?2, ?3, ?4, ?5, '', ?6, ?6, ?6, ?6, 0, 1, 0, 1, 0, ?7, 0, 'text', ?8, ?9, '', x'')",
            params![
                outbox,
                reply_subject,
                account.sender_name,
                account.address,
                to,
                now,
                eml.len() as i64,
                thunderlook_mime::preview_from_text(&cfg.text, 255),
                thunderlook_mailbox::normalized_subject(&subject)
            ],
        )
        .map_err(e)?;
        let new_id = conn.last_insert_rowid();
        let rel = thunderlook_store::eml_relative_path(account.source_id, new_id);
        let path = thunderlook_store::resolve(data_dir, &rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(e)?;
        }
        std::fs::write(&path, &eml).map_err(e)?;
        conn.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![new_id, rel]).map_err(e)?;
        conn.execute("INSERT INTO outbox (message_id, account_id, queued_at) VALUES (?1, ?2, ?3)", params![new_id, account.id, now]).map_err(e)?;
        replied.push(to);
    }
    Ok(replied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eml(headers: &str) -> Vec<u8> {
        format!("From: Camille <camille@example.org>\r\nTo: moi@example.fr\r\nSubject: Devis\r\nMessage-ID: <a@b>\r\n{headers}\r\nCorps\r\n").into_bytes()
    }

    #[test]
    fn who_gets_an_automatic_reply() {
        assert_eq!(eligible(&eml(""), "moi@example.fr").unwrap(), ("camille@example.org".into(), "Devis".into(), Some("a@b".into())));
        assert_eq!(eligible(&eml("List-Id: <x.example.org>\r\n"), "moi@example.fr"), Err("liste de diffusion"));
        assert_eq!(eligible(&eml("Precedence: bulk\r\n"), "moi@example.fr"), Err("envoi en nombre"));
        assert_eq!(eligible(&eml("Auto-Submitted: auto-replied\r\n"), "moi@example.fr"), Err("réponse automatique"));
        assert!(eligible(&eml("Auto-Submitted: no\r\n"), "moi@example.fr").is_ok());
        assert_eq!(eligible(&eml(""), "CAMILLE@example.org"), Err("message de soi-même"));
        let noreply = b"From: no-reply@shop.fr\r\nSubject: x\r\n\r\nc\r\n";
        assert_eq!(eligible(noreply, "moi@example.fr"), Err("adresse technique"));
    }

    #[test]
    fn period_bounds_are_inclusive() {
        let cfg = AutoReply { enabled: true, start: Some("2026-10-20".into()), end: Some("2026-10-31".into()), ..Default::default() };
        assert!(active(&cfg, "2026-10-20") && active(&cfg, "2026-10-31"));
        assert!(!active(&cfg, "2026-11-01") && !active(&cfg, "2026-10-19"));
        assert!(!active(&AutoReply { enabled: false, ..cfg.clone() }, "2026-10-25"));
        assert!(active(&AutoReply { enabled: true, ..Default::default() }, "2030-01-01"));
    }
}
