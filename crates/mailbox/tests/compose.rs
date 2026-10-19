//! Rédaction (J5) sur un dossier de données synthétique (aucune donnée réelle) : réponse à tous fidèle,
//! transfert avec pièces jointes, brouillon enregistré puis rouvert.

use std::{fs, path::PathBuf};

use mail_parser::{MessageParser, MimeHeaders};
use thunderlook_mailbox::{Mailbox, parse_addresses};
use thunderlook_model::{ComposeKind, DraftAttachmentSource, ListQuery, SortField};
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

fn message(c: &Connection, dir: &PathBuf, folder: i64, from: (&str, &str), subject: &str, eml: &str, index: Option<Vec<u8>>) -> i64 {
    c.execute(
        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, received_at, sent_at,
            message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props,
            internet_message_id, conversation_topic, conversation_index, conversation_id)
         VALUES (?1, 0, 'IPM.Note', ?2, ?3, ?4, '', '', 1791490440000, 1791490440000, 1, 1, 0, 1, 0, 100, 0, 'html', '', '', x'',
            ?5, ?6, ?7, ?8)",
        params![
            folder,
            subject,
            from.0,
            from.1,
            format!("orig-{folder}-{subject}@example.org"),
            subject.trim_start_matches("RE: "),
            index.clone(),
            index.as_ref().map(|i| i[6..22].to_vec()).unwrap_or_default()
        ],
    )
    .unwrap();
    let id = c.last_insert_rowid();
    let rel = thunderlook_store::eml_relative_path(1, id);
    let path = thunderlook_store::resolve(dir, &rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, eml).unwrap();
    c.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, rel]).unwrap();
    id
}

const ORIGINAL: &str = "From: Roland MULLER <roland@example.org>\r\nTo: Paul Adam - Adam-declic <paul@example.fr>, Lou <lou@example.org>\r\n\
Cc: Camille <camille@example.org>, paul@example.fr\r\nSubject: vos réponses\r\nMessage-ID: <orig@example.org>\r\nReferences: <first@example.org>\r\n\
MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"M\"\r\n\r\n--M\r\nContent-Type: multipart/related; boundary=\"R\"\r\n\r\n\
--R\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><head><style>p.x{color:red}</style></head><body><p class=x>Bonjour <img src=\"cid:logo@x\"></p></body></html>\r\n\
--R\r\nContent-Type: image/png\r\nContent-ID: <logo@x>\r\nContent-Transfer-Encoding: base64\r\n\r\niVBORw0KGgo=\r\n--R--\r\n\
--M\r\nContent-Type: application/pdf; name=\"devis.pdf\"\r\nContent-Disposition: attachment; filename=\"devis.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQK\r\n--M--\r\n";

const SENT: &str = "From: Paul Adam - Adam-declic <paul@example.fr>\r\nTo: x@example.org\r\nSubject: s\r\n\r\ncorps\r\n";

fn setup() -> (Fixture, Mailbox, i64, i64) {
    let dir = std::env::temp_dir().join(format!("tl-compose-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let _ = fs::remove_dir_all(&dir);
    let c = thunderlook_store::open(&dir).unwrap();
    c.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (1, 'pst', 'x', 'x', 'Test', 0, 0)", [])
        .unwrap();
    let root = folder(&c, None, "", "root");
    let ipm = folder(&c, Some(root), "IPM", "ipm");
    let inbox = folder(&c, Some(ipm), "Boîte de réception", "inbox");
    let sent = folder(&c, Some(ipm), "Éléments envoyés", "sent");
    let drafts = folder(&c, Some(ipm), "Brouillons", "drafts");
    let mut index = vec![0x01, 1, 2, 3, 4, 5];
    index.extend([0x42; 16]);
    let original = message(&c, &dir, inbox, ("Roland MULLER", "roland@example.org"), "vos réponses", ORIGINAL, Some(index));
    message(&c, &dir, sent, ("Paul Adam - Adam-declic", "paul@example.fr"), "s", SENT, None);
    drop(c);
    // Aucune signature : dossier vide, pour un résultat indépendant de la machine.
    let sig = dir.join("signatures");
    fs::create_dir_all(&sig).unwrap();
    // SAFETY : tests de ce fichier seulement, avant toute lecture de la variable.
    unsafe { std::env::set_var("THUNDERLOOK_SIGNATURES", &sig) };
    let mb = Mailbox::open(&dir).unwrap();
    (Fixture { dir }, mb, original, drafts)
}

#[test]
fn reply_all_is_faithful_and_saved_as_draft() {
    let (fx, mut mb, original, drafts) = setup();
    let d = mb.compose(ComposeKind::ReplyAll, Some(&format!("m{original}"))).unwrap();
    // Destinataires : l'expéditeur et les autres destinataires, sans mes adresses ; copie conservée.
    assert_eq!(d.to, "Roland MULLER <roland@example.org>; Lou <lou@example.org>");
    assert_eq!(d.cc, "Camille <camille@example.org>");
    assert_eq!(d.subject, "RE: vos réponses");
    // Bloc cité comme Outlook 2021 en français, puis le message d'origine avec son image intégrée.
    for part in [
        "border-top:solid #E1E1E1 1.0pt",
        "<b>De&nbsp;:</b> Roland MULLER &lt;roland@example.org&gt;",
        "<b>Envoyé&nbsp;:</b> ",
        "<b>À&nbsp;:</b> Paul Adam - Adam-declic &lt;paul@example.fr&gt;; Lou &lt;lou@example.org&gt;",
        "<b>Cc&nbsp;:</b> Camille &lt;camille@example.org&gt;; paul@example.fr",
        "<b>Objet&nbsp;:</b> vos réponses",
        "<p class=x>Bonjour <img src=\"data:image/png;base64,iVBORw0KGgo=\"></p>",
    ] {
        assert!(d.html.contains(part), "absent : {part}\n{}", d.html);
    }
    assert!(d.styles.contains("p.x{color:red}"));
    assert!(d.attachments.is_empty(), "une réponse ne reprend pas les pièces jointes");

    let saved = mb.save_draft(&d).unwrap();
    let id = saved.id.clone().unwrap();
    // Le brouillon est dans Brouillons, non envoyé, dans la même conversation que l'original.
    let v = mb
        .open_list(&ListQuery { folder_id: format!("f{drafts}"), unread_only: false, sort: SortField::Date, descending: true, conversations: false })
        .unwrap();
    assert_eq!(v.count, 1);
    assert_eq!(mb.rows(v.token, 0, 1).unwrap()[0].id, id);
    assert!(mb.is_draft(&id).unwrap());
    let c = thunderlook_store::open(&fx.dir).unwrap();
    let (conv, orig_conv, index_len, in_reply_to, eml_path): (Vec<u8>, Vec<u8>, i64, String, String) = c
        .query_row(
            "SELECT d.conversation_id, o.conversation_id, length(d.conversation_index), d.in_reply_to, d.eml_path
             FROM message d, message o WHERE d.id = ?1 AND o.id = ?2",
            params![id.trim_start_matches('m').parse::<i64>().unwrap(), original],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(conv, orig_conv);
    assert_eq!(index_len, 27, "en-tête du parent et bloc enfant de 5 octets");
    assert_eq!(in_reply_to, format!("orig-{}-vos réponses@example.org", 3));
    // Le .eml : en-têtes de réponse, marqueur de brouillon, image intégrée en cid:.
    let bytes = fs::read(thunderlook_store::resolve(&fx.dir, &eml_path)).unwrap();
    let m = MessageParser::default().parse(&bytes).unwrap();
    assert_eq!(m.subject(), Some("RE: vos réponses"));
    assert_eq!(m.header_raw("X-Unsent").map(str::trim), Some("1"));
    assert_eq!(m.in_reply_to().as_text(), Some(format!("orig-{}-vos réponses@example.org", 3).as_str()));
    assert!(m.header_raw("References").unwrap().contains("first@example.org"));
    let html = m.body_html(0).unwrap();
    assert!(html.contains("cid:image001."), "{html}");
    assert!(m.attachments().any(|p| p.content_id().is_some_and(|c| c.starts_with("image001."))));
    assert_eq!(parse_addresses(&d.to).len(), 2);

    // Rouvert, il garde destinataires, objet, corps (image revenue en data:) ; réenregistré, il reste unique.
    let reopened = mb.open_draft(&id).unwrap();
    assert_eq!((reopened.to.as_str(), reopened.cc.as_str(), reopened.subject.as_str()), (d.to.as_str(), d.cc.as_str(), d.subject.as_str()));
    assert!(reopened.html.contains("data:image/png;base64,iVBORw0KGgo="));
    let mut edited = reopened.clone();
    edited.html = edited.html.replacen("<o:p>&nbsp;</o:p>", "Merci, je regarde.", 1);
    let again = mb.save_draft(&edited).unwrap();
    assert_eq!(again.id, Some(id.clone()));
    let v = mb
        .open_list(&ListQuery { folder_id: format!("f{drafts}"), unread_only: false, sort: SortField::Date, descending: true, conversations: false })
        .unwrap();
    assert_eq!(v.count, 1);
    let back = mb.open_draft(&id).unwrap();
    assert!(back.html.contains("Merci, je regarde."));
    let m2 = fs::read(thunderlook_store::resolve(&fx.dir, &eml_path)).unwrap();
    let m2 = MessageParser::default().parse(&m2).unwrap();
    assert!(m2.header_raw("References").unwrap().contains("first@example.org"), "le fil est conservé");
}

#[test]
fn reply_and_forward() {
    let (_fx, mut mb, original, _) = setup();
    let id = format!("m{original}");
    let r = mb.compose(ComposeKind::Reply, Some(&id)).unwrap();
    assert_eq!((r.to.as_str(), r.cc.as_str()), ("Roland MULLER <roland@example.org>", ""));
    let f = mb.compose(ComposeKind::Forward, Some(&id)).unwrap();
    assert_eq!((f.to.as_str(), f.subject.as_str()), ("", "TR: vos réponses"));
    assert_eq!(f.attachments.len(), 1, "le PDF suit ; l'image intégrée reste dans le corps");
    assert_eq!(f.attachments[0].name, "devis.pdf");
    assert!(matches!(f.attachments[0].source, DraftAttachmentSource::Message { index: 1, .. }));
    let mut f = f;
    f.to = "Dominique <d@example.org>".into();
    let saved = mb.save_draft(&f).unwrap();
    let back = mb.open_draft(saved.id.as_deref().unwrap()).unwrap();
    assert_eq!(back.attachments.len(), 1);
    assert_eq!(back.attachments[0].name, "devis.pdf");
    assert_eq!(mb.attachment(saved.id.as_deref().unwrap(), match back.attachments[0].source {
        DraftAttachmentSource::Message { index, .. } => index,
        _ => unreachable!(),
    }).unwrap().bytes, b"%PDF-1.4\n");
    // Nouveau message : vide, dans Brouillons de la première source.
    let n = mb.compose(ComposeKind::New, None).unwrap();
    assert!(n.to.is_empty() && n.subject.is_empty());
    assert!(mb.save_draft(&n).unwrap().id.is_some());
}

#[test]
fn addresses() {
    let a = parse_addresses("\"Martin, Lou\" <lou@example.org>; c@example.org, Nom <n@example.org>;; pas-une-adresse");
    let shown: Vec<(Option<&str>, &str)> = a.iter().map(|m| (m.name.as_deref(), m.addr.as_str())).collect();
    assert_eq!(shown, vec![(Some("Martin, Lou"), "lou@example.org"), (None, "c@example.org"), (Some("Nom"), "n@example.org")]);
    assert_eq!(thunderlook_mailbox::format_address(&a[0]), "\"Martin, Lou\" <lou@example.org>");
}
