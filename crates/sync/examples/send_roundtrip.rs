//! Envoi réel depuis ThunderLook vers le compte de test lui-même, puis réception (dossier de données de test).
//! Usage : `cargo run -p thunderlook-sync --example send_roundtrip -- <dossier de test> <source>`

use std::{path::Path, time::Instant};

use thunderlook_mailbox::Mailbox;
use thunderlook_model::ComposeKind;
use thunderlook_store::rusqlite::params;
use thunderlook_sync::{account, sync_account_blocking};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, source) = (Path::new(&args[0]), args[1].parse::<i64>().map_err(|e| e.to_string())?);
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    let address = account::account(&conn, source)?.address;
    let subject = format!("Envoi J7 {}", chrono_like_stamp());

    let mut mb = Mailbox::open(dir)?;
    let mut d = mb.compose(ComposeKind::New, None)?;
    d.to = address.clone();
    d.bcc = format!("ThunderLook <{address}>");
    d.subject = subject.clone();
    d.html = "<p>Envoi réel depuis ThunderLook (J7), avec une copie cachée.</p>".into();
    let started = Instant::now();
    let id = mb.send_draft(&d)?;
    drop(mb);
    let state = |id: &str| -> Result<String, String> {
        conn.query_row(
            "SELECT f.name || ' (uid ' || COALESCE(m.imap_uid, '-') || ')' FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1",
            [id.trim_start_matches('m').parse::<i64>().unwrap_or(0)],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())
    };
    println!("« {subject} » mis dans : {}", state(&id)?);

    let mut received = None;
    for pass in 1..=10 {
        let r = sync_account_blocking(dir, source)?;
        println!(
            "relève {pass} : {} envoyé(s) {:?}, {} nouveau(x), report {:?}, {:.1} s",
            r.sent,
            r.send_error,
            r.new_messages,
            r.push_error,
            started.elapsed().as_secs_f32()
        );
        received = conn
            .query_row(
                "SELECT m.eml_path FROM message m JOIN folder f ON f.id = m.folder_id WHERE f.source_id = ?1 AND f.role = 'inbox' AND m.subject = ?2",
                params![source, subject],
                |r| r.get::<_, String>(0),
            )
            .ok();
        if received.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
    let rel = received.ok_or("message non reçu après 10 relèves")?;
    let bytes = std::fs::read(thunderlook_store::resolve(dir, &rel)).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    println!("reçu après {:.1} s ; en-tête Bcc présent : {} ; X-Unsent présent : {}", started.elapsed().as_secs_f32(), text.contains("\nBcc:"), text.contains("X-Unsent"));
    println!("message envoyé maintenant dans : {}", state(&id)?);
    let outbox: i64 = conn.query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    let pending: i64 = conn.query_row("SELECT COUNT(*) FROM imap_pending", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    println!("file d'envoi : {outbox} ; opérations en attente : {pending}");
    println!("objet : {subject}");
    Ok(())
}

fn chrono_like_stamp() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("{s}")
}
