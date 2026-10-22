//! Point ouvert du J7 : après l'envoi d'une réponse ou d'un transfert, l'original est marqué répondu ou transféré
//! (`PidTagLastVerbExecuted`), localement et sur le serveur (`\Answered`, `$Forwarded`) ; données synthétiques.

use std::{fs, path::Path};

use thunderlook_mailbox::Mailbox;
use thunderlook_model::{ComposeKind, LastVerb, ListQuery, SortField};
use thunderlook_store::rusqlite::{Connection, params};

const ORIGINAL: &str = "From: Camille <camille@example.org>\r\nTo: paul@example.org\r\nSubject: Devis\r\nMessage-ID: <orig@example.org>\r\n\
MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nBonjour\r\n";

fn folder(c: &Connection, parent: Option<i64>, name: &str, role: &str, imap: Option<&str>) -> i64 {
    c.execute(
        "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position, imap_name) VALUES (3, ?1, 0, ?2, ?3, 'IPF.Note', 0, 0, ?4)",
        params![parent, name, role, imap],
    )
    .unwrap();
    c.last_insert_rowid()
}

fn setup(dir: &Path) -> (i64, i64) {
    let _ = fs::remove_dir_all(dir);
    let c = thunderlook_store::open(dir).unwrap();
    c.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (3, 'imap', 'p', '', 'p', 0, 0)", []).unwrap();
    c.execute(
        "INSERT INTO account (source_id, name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, save_sent, created_at)
         VALUES (3, 'Paul', 'paul@example.org', 'paul@example.org', 'imap.example.org', 993, 'tls', 'smtp.example.org', 465, 'tls', 1, 0)",
        [],
    )
    .unwrap();
    let root = folder(&c, None, "", "root", None);
    let top = folder(&c, Some(root), "paul@example.org", "ipm", None);
    let inbox = folder(&c, Some(top), "Boîte de réception", "inbox", Some("INBOX"));
    folder(&c, Some(top), "Brouillons", "drafts", Some("INBOX.Drafts"));
    folder(&c, Some(top), "Éléments envoyés", "sent", Some("INBOX.Sent"));
    folder(&c, Some(top), "Boîte d'envoi", "outbox", None);
    c.execute(
        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, received_at, sent_at,
            message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props,
            internet_message_id, conversation_topic, imap_uid)
         VALUES (?1, 0, 'IPM.Note', 'Devis', 'Camille', 'camille@example.org', 'paul@example.org', '', 1791490440000, 1791490440000,
            1, 1, 0, 1, 0, 100, 0, 'text', '', '', x'', 'orig@example.org', 'Devis', 7)",
        [inbox],
    )
    .unwrap();
    let id = c.last_insert_rowid();
    let rel = thunderlook_store::eml_relative_path(3, id);
    let path = thunderlook_store::resolve(dir, &rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, ORIGINAL).unwrap();
    c.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, rel]).unwrap();
    let sig = dir.join("signatures");
    fs::create_dir_all(&sig).unwrap();
    // SAFETY : tests de ce fichier seulement, avant toute lecture de la variable.
    unsafe { std::env::set_var("THUNDERLOOK_SIGNATURES", &sig) };
    (inbox, id)
}

fn state(c: &Connection, id: i64) -> (Option<i64>, bool) {
    c.query_row("SELECT last_verb, last_verb_at IS NOT NULL FROM message WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
}

fn flags_pending(c: &Connection) -> Vec<String> {
    c.prepare("SELECT mailbox || ' ' || uid || ' ' || arg FROM imap_pending WHERE kind = 'flags' ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn reply_then_forward_mark_the_original() {
    let dir = std::env::temp_dir().join(format!("tl-answered-{}", std::process::id()));
    let (inbox, original) = setup(&dir);
    let mut mb = Mailbox::open(&dir).unwrap();
    let c = thunderlook_store::open(&dir).unwrap();

    // Réponse ouverte depuis le message, puis envoyée.
    let r = mb.compose(ComposeKind::Reply, Some(&format!("m{original}"))).unwrap();
    assert_eq!(state(&c, original), (None, false), "rien tant que la réponse n'est pas envoyée");
    mb.send_draft(&r).unwrap();
    assert_eq!(state(&c, original), (Some(102), true));
    assert_eq!(flags_pending(&c), [r#"INBOX 7 {"add":["\\Answered"]}"#]);
    let v = mb.message_view(&format!("m{original}"), false).unwrap();
    assert_eq!(v.last_verb, Some(LastVerb::Replied));
    assert!(v.last_verb_label.as_deref().unwrap().starts_with("Vous avez répondu à ce message le "), "{:?}", v.last_verb_label);
    let list = mb
        .open_list(&ListQuery { folder_id: format!("f{inbox}"), unread_only: false, sort: SortField::Date, descending: true, conversations: false })
        .unwrap();
    assert_eq!(mb.rows(list.token, 0, 1).unwrap()[0].last_verb, Some(LastVerb::Replied));
    // Clic sur une notification : la ligne du message est retrouvée dans la vue.
    assert_eq!(mb.position_in_view(list.token, &format!("m{original}")).unwrap(), Some(0));
    assert_eq!(mb.position_in_view(list.token, "m999999").unwrap(), None);

    // Transfert enregistré, rouvert (la rédaction perd alors sa nature et son original), puis envoyé.
    let mut f = mb.compose(ComposeKind::Forward, Some(&format!("m{original}"))).unwrap();
    f.to = "Dominique <d@example.org>".into();
    let saved = mb.save_draft(&f).unwrap();
    let reopened = mb.open_draft(saved.id.as_deref().unwrap()).unwrap();
    assert_eq!((reopened.kind, reopened.source_id.as_deref()), (ComposeKind::New, None));
    mb.send_draft(&reopened).unwrap();
    assert_eq!(state(&c, original).0, Some(104));
    assert_eq!(flags_pending(&c).last().unwrap(), r#"INBOX 7 {"add":["$Forwarded"]}"#);
    let v = mb.message_view(&format!("m{original}"), false).unwrap();
    assert!(v.last_verb_label.as_deref().unwrap().starts_with("Vous avez transféré ce message le "));

    // Nouveau message sans original : rien n'est marqué.
    let before = flags_pending(&c).len();
    let mut n = mb.compose(ComposeKind::New, None).unwrap();
    n.to = "x@example.org".into();
    n.subject = "RE: sans lien".into();
    mb.send_draft(&n).unwrap();
    assert_eq!(flags_pending(&c).len(), before);
    drop((c, mb));
    let _ = fs::remove_dir_all(&dir);
}
