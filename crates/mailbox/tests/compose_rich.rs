//! Rédaction enrichie (lot AS) sur un dossier de données synthétique : importance, confirmation de lecture, lien et
//! image insérés dans le corps ; brouillon enregistré, rouvert puis réenregistré sans les options.

use std::{fs, path::PathBuf};

use mail_parser::{MessageParser, MimeHeaders};
use thunderlook_mailbox::Mailbox;
use thunderlook_model::{ComposeKind, Importance};
use thunderlook_store::rusqlite::{Connection, params};

struct Fixture {
    dir: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn folder(c: &Connection, parent: Option<i64>, name: &str, role: &str) -> i64 {
    c.execute(
        "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position) VALUES (1, ?1, 0, ?2, ?3, NULL, 0, 0)",
        params![parent, name, role],
    )
    .unwrap();
    c.last_insert_rowid()
}

const SENT: &str = "From: Paul Adam <paul@example.fr>\r\nTo: x@example.org\r\nSubject: s\r\n\r\ncorps\r\n";

fn setup() -> (Fixture, Mailbox) {
    let dir = std::env::temp_dir().join(format!(
        "tl-compose-rich-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let c = thunderlook_store::open(&dir).unwrap();
    c.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (1, 'pst', 'x', 'x', 'Test', 0, 0)", [])
        .unwrap();
    let root = folder(&c, None, "", "root");
    let ipm = folder(&c, Some(root), "IPM", "ipm");
    folder(&c, Some(ipm), "Boîte de réception", "inbox");
    let sent = folder(&c, Some(ipm), "Éléments envoyés", "sent");
    folder(&c, Some(ipm), "Brouillons", "drafts");
    // Un message envoyé : il donne l'adresse d'envoi de la source.
    c.execute(
        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, received_at, sent_at,
            message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
         VALUES (?1, 0, 'IPM.Note', 's', 'Paul Adam', 'paul@example.fr', '', '', 1791490440000, 1791490440000, 1, 1, 0, 1, 0, 100, 0,
            'html', '', '', x'')",
        [sent],
    )
    .unwrap();
    let id = c.last_insert_rowid();
    let rel = thunderlook_store::eml_relative_path(1, id);
    let path = thunderlook_store::resolve(&dir, &rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, SENT).unwrap();
    c.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, rel]).unwrap();
    drop(c);
    let sig = dir.join("signatures");
    fs::create_dir_all(&sig).unwrap();
    // SAFETY : tests de ce fichier seulement, avant toute lecture de la variable.
    unsafe { std::env::set_var("THUNDERLOOK_SIGNATURES", &sig) };
    let mb = Mailbox::open(&dir).unwrap();
    (Fixture { dir }, mb)
}

fn importance_column(dir: &PathBuf, id: &str) -> i64 {
    let c = thunderlook_store::open(dir).unwrap();
    c.query_row("SELECT importance FROM message WHERE id = ?1", [id.trim_start_matches('m').parse::<i64>().unwrap()], |r| r.get(0)).unwrap()
}

fn eml(dir: &PathBuf, id: &str) -> Vec<u8> {
    let c = thunderlook_store::open(dir).unwrap();
    let rel: String =
        c.query_row("SELECT eml_path FROM message WHERE id = ?1", [id.trim_start_matches('m').parse::<i64>().unwrap()], |r| r.get(0)).unwrap();
    fs::read(thunderlook_store::resolve(dir, &rel)).unwrap()
}

#[test]
fn importance_receipt_link_and_image() {
    let (fx, mut mb) = setup();
    let mut d = mb.compose(ComposeKind::New, None).unwrap();
    assert_eq!((d.importance, d.read_receipt), (Importance::Normal, false), "par défaut : normale, sans confirmation");
    d.to = "Dominique <d@example.org>".into();
    d.subject = "Devis".into();
    d.importance = Importance::High;
    d.read_receipt = true;
    d.html = "<p>Voir <a href=\"https://example.org/devis\">le devis</a> <font face=\"Arial\" color=\"#c00000\">en rouge</font></p>\
              <p><img src=\"data:image/png;base64,iVBORw0KGgo=\"></p>"
        .into();
    let saved = mb.save_draft(&d).unwrap();
    let id = saved.id.clone().unwrap();
    assert_eq!(importance_column(&fx.dir, &id), 2, "colonne importance : haute");

    let bytes = eml(&fx.dir, &id);
    let m = MessageParser::default().parse(&bytes).unwrap();
    assert_eq!(m.header_raw("Importance").map(str::trim), Some("High"));
    assert_eq!(m.header_raw("X-Priority").map(str::trim), Some("1"));
    let dnt = m.header_raw("Disposition-Notification-To").unwrap();
    assert!(dnt.contains("paul@example.fr"), "{dnt}");
    let html = m.body_html(0).unwrap();
    assert!(html.contains("href=\"https://example.org/devis\""), "{html}");
    assert!(html.contains("color=\"#c00000\""), "{html}");
    assert!(html.contains("cid:image001."), "l'image insérée part en pièce intégrée : {html}");
    assert!(m.attachments().any(|p| p.content_id().is_some_and(|c| c.starts_with("image001."))));

    // Rouvert : options conservées, image revenue en data:.
    let back = mb.open_draft(&id).unwrap();
    assert_eq!((back.importance, back.read_receipt), (Importance::High, true));
    assert!(back.html.contains("data:image/png;base64,iVBORw0KGgo="));

    // Sans les options : en-têtes retirés, colonne revenue à normale.
    let mut plain = back.clone();
    plain.importance = Importance::Normal;
    plain.read_receipt = false;
    mb.save_draft(&plain).unwrap();
    assert_eq!(importance_column(&fx.dir, &id), 1);
    let bytes = eml(&fx.dir, &id);
    let m = MessageParser::default().parse(&bytes).unwrap();
    assert!(m.header_raw("Importance").is_none() && m.header_raw("X-Priority").is_none());
    assert!(m.header_raw("Disposition-Notification-To").is_none());
    let back = mb.open_draft(&id).unwrap();
    assert_eq!((back.importance, back.read_receipt), (Importance::Normal, false));

    // Importance faible.
    let mut low = back;
    low.importance = Importance::Low;
    mb.save_draft(&low).unwrap();
    assert_eq!(importance_column(&fx.dir, &id), 0);
    let bytes = eml(&fx.dir, &id);
    let m = MessageParser::default().parse(&bytes).unwrap();
    assert_eq!(m.header_raw("X-Priority").map(str::trim), Some("5"));
    assert_eq!(mb.open_draft(&id).unwrap().importance, Importance::Low);
}
