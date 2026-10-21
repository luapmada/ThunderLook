//! Envoi (J7) : la file `outbox` est transmise en SMTP. Le message envoyé passe de la Boîte d'envoi à Éléments
//! envoyés ; si le compte garde une copie sur le serveur (`save_sent`), ce classement est reporté comme un dépôt
//! (APPEND) au report suivant. En échec, le message reste dans la Boîte d'envoi avec l'erreur (3 tentatives).

use std::path::Path;

use lettre::{
    AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
    address::{Address as LAddress, Envelope},
    transport::smtp::authentication::Credentials,
};
use mail_parser::{Address, MessageParser};
use thunderlook_store::rusqlite::{Connection, params};

use crate::{Result, Security, account::Account};

const MAX_ATTEMPTS: i64 = 3;

fn e(x: impl std::fmt::Display) -> String {
    x.to_string()
}

/// Octets transmis : sans les en-têtes `Bcc` (avec leurs lignes de continuation) ni `X-Unsent`.
pub fn transport_bytes(eml: &[u8]) -> Vec<u8> {
    let end = eml.windows(4).position(|w| w == b"\r\n\r\n").map_or(eml.len(), |p| p + 2);
    let (head, body) = eml.split_at(end);
    let mut out = Vec::with_capacity(eml.len());
    let mut skipping = false;
    for line in head.split_inclusive(|b| *b == b'\n') {
        let continuation = line.first().is_some_and(|b| *b == b' ' || *b == b'\t');
        if !continuation {
            let lower = line.to_ascii_lowercase();
            skipping = lower.starts_with(b"bcc:") || lower.starts_with(b"x-unsent:");
        }
        if !skipping {
            out.extend_from_slice(line);
        }
    }
    out.extend_from_slice(body);
    out
}

fn recipients(m: &mail_parser::Message) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for a in [m.to(), m.cc(), m.bcc()].into_iter().flatten() {
        let a: &Address = a;
        for x in a.iter() {
            if let Some(addr) = x.address() {
                if !out.iter().any(|o| o.eq_ignore_ascii_case(addr)) {
                    out.push(addr.to_string());
                }
            }
        }
    }
    out
}

fn transport(account: &Account, password: &str) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
    let s = &account.endpoints.smtp;
    let builder = match s.security {
        Security::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&s.host),
        Security::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&s.host),
    }
    .map_err(|x| format!("{} : {x}", s.host))?;
    Ok(builder.port(s.port).credentials(Credentials::new(account.endpoints.username.clone(), password.to_string())).build())
}

/// Transmet les messages en attente du compte ; renvoie le nombre envoyé et la dernière erreur.
pub async fn send_outbox(conn: &Connection, data_dir: &Path, account: &Account, password: &str) -> Result<(u32, Option<String>)> {
    let queued: Vec<(i64, String)> = conn
        .prepare(
            "SELECT o.message_id, m.eml_path FROM outbox o JOIN message m ON m.id = o.message_id
             WHERE o.account_id = ?1 AND o.attempts < ?2 ORDER BY o.queued_at",
        )
        .map_err(e)?
        .query_map(params![account.id, MAX_ATTEMPTS], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(e)?
        .collect::<std::result::Result<_, _>>()
        .map_err(e)?;
    if queued.is_empty() {
        return Ok((0, None));
    }
    let smtp = transport(account, password)?;
    let sent_folder: Option<i64> = conn
        .query_row("SELECT id FROM folder WHERE source_id = ?1 AND role = 'sent' ORDER BY imap_name IS NULL, id LIMIT 1", [account.source_id], |r| {
            r.get(0)
        })
        .ok();
    let (mut sent, mut last_error) = (0, None);
    for (id, rel) in queued {
        let result: Result<()> = async {
            let bytes = std::fs::read(thunderlook_store::resolve(data_dir, &rel)).map_err(|x| format!("{rel} : {x}"))?;
            let parsed = MessageParser::default().parse(&bytes).ok_or("message illisible")?;
            let to: Vec<LAddress> = recipients(&parsed).iter().filter_map(|a| a.parse().ok()).collect();
            if to.is_empty() {
                return Err("aucun destinataire valide".into());
            }
            let from: LAddress = account.address.parse().map_err(|x| format!("{} : {x}", account.address))?;
            let envelope = Envelope::new(Some(from), to).map_err(e)?;
            smtp.send_raw(&envelope, &transport_bytes(&bytes)).await.map_err(|x| format!("SMTP : {x}"))?;
            Ok(())
        }
        .await;
        match result {
            Ok(()) => {
                conn.execute("DELETE FROM outbox WHERE message_id = ?1", [id]).map_err(e)?;
                if let Some(folder) = sent_folder {
                    if account.save_sent {
                        thunderlook_mailbox::file_message(conn, id, folder)?;
                    } else {
                        conn.execute("UPDATE message SET folder_id = ?2 WHERE id = ?1", params![id, folder]).map_err(e)?;
                    }
                }
                sent += 1;
            }
            Err(err) => {
                conn.execute("UPDATE outbox SET attempts = attempts + 1, last_error = ?2 WHERE message_id = ?1", params![id, err]).map_err(e)?;
                last_error = Some(err);
            }
        }
    }
    Ok((sent, last_error))
}

#[cfg(test)]
mod tests {
    use super::transport_bytes;

    #[test]
    fn bcc_and_unsent_are_not_transmitted() {
        let eml = b"From: a@x.fr\r\nBcc: secret@x.fr,\r\n autre@x.fr\r\nX-Unsent: 1\r\nSubject: Test\r\n\r\nBcc: dans le corps\r\n";
        let out = String::from_utf8(transport_bytes(eml)).unwrap();
        assert_eq!(out, "From: a@x.fr\r\nSubject: Test\r\n\r\nBcc: dans le corps\r\n");
    }
}
