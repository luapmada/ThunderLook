//! « Créer une règle » depuis un message (clic droit) : la règle va dans le jeu du compte du message, vise le dossier
//! du compte par son nom IMAP, apparaît dans « Règles et alertes » et s'exécute (données synthétiques, sans réseau).

use std::{fs, path::Path};

use thunderlook_mailbox::{FolderMatch, FolderResolver, Mailbox};
use thunderlook_model::{NewRule, RuleRunRequest, RunFilter};
use thunderlook_store::rusqlite::{Connection, params};

fn source(c: &Connection, id: i64, kind: &str, name: &str) {
    c.execute(
        "INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (?1, ?2, ?3, '', ?3, 0, 0)",
        params![id, kind, name],
    )
    .unwrap();
}

fn folder(c: &Connection, source: i64, parent: Option<i64>, name: &str, role: &str, imap: Option<&str>, entry: Option<&[u8]>) -> i64 {
    let props: Option<Vec<u8>> =
        entry.map(|e| [0xFFu8, 0x0F, 0x02, 0x01].into_iter().chain((e.len() as u32).to_le_bytes()).chain(e.iter().copied()).collect());
    c.execute(
        "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position, imap_name, props)
         VALUES (?1, ?2, 0, ?3, ?4, 'IPF.Note', 0, 0, ?5, ?6)",
        params![source, parent, name, role, imap, props],
    )
    .unwrap();
    c.last_insert_rowid()
}

fn message(c: &Connection, dir: &Path, source: i64, folder: i64, from: (&str, &str), subject: &str) -> i64 {
    c.execute(
        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, received_at,
            message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
         VALUES (?1, 0, 'IPM.Note', ?2, ?3, ?4, 'Paul', '', 1760000000000, 0, 0, 0, 1, 0, 100, 0, 'text', '', '', x'')",
        params![folder, subject, from.0, from.1],
    )
    .unwrap();
    let id = c.last_insert_rowid();
    let rel = thunderlook_store::eml_relative_path(source, id);
    let path = thunderlook_store::resolve(dir, &rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, format!("Subject: {subject}\r\nFrom: {} <{}>\r\nTo: Paul <paul@example.org>\r\n\r\ncorps\r\n", from.0, from.1)).unwrap();
    c.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, rel]).unwrap();
    id
}

fn folder_of(dir: &Path, id: i64) -> i64 {
    thunderlook_store::open(dir).unwrap().query_row("SELECT folder_id FROM message WHERE id = ?1", [id], |r| r.get(0)).unwrap()
}

#[test]
fn rule_from_sender_moves_into_account_folder() {
    let dir = std::env::temp_dir().join(format!("tl-create-rule-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let c = thunderlook_store::open(&dir).unwrap();
    source(&c, 3, "imap", "paul@example.org");
    c.execute(
        "INSERT INTO account (source_id, name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, save_sent, created_at)
         VALUES (3, '', 'paul@example.org', 'paul@example.org', 'imap.example.org', 993, 'tls', 'smtp.example.org', 465, 'tls', 1, 0)",
        [],
    )
    .unwrap();
    let root = folder(&c, 3, None, "", "root", None, None);
    let top = folder(&c, 3, Some(root), "paul@example.org", "ipm", None, None);
    let inbox = folder(&c, 3, Some(top), "Boîte de réception", "inbox", Some("INBOX"), None);
    let target = folder(&c, 3, Some(inbox), "Factures", "folder", Some("INBOX.Factures"), None);
    let from_x = message(&c, &dir, 3, inbox, ("Camille", "x@example.org"), "Facture 12");
    let other = message(&c, &dir, 3, inbox, ("Dominique", "y@example.org"), "Facture 13");
    drop(c);

    let mut mb = Mailbox::open(&dir).unwrap();
    let draft = mb.rule_draft(&format!("m{from_x}")).unwrap();
    assert_eq!((draft.from_name.as_str(), draft.from_addr.as_str()), ("Camille", "x@example.org"));
    assert_eq!((draft.to_name.as_str(), draft.to_addr.as_str()), ("Paul", "paul@example.org"));
    assert_eq!((draft.folder_id.as_str(), draft.rule_set.as_str()), (format!("f{inbox}").as_str(), "paul@example.org"));
    assert!(draft.note.is_some(), "règles à l'arrivée désactivées : signalé");
    assert_eq!(draft.account_section.as_deref(), Some("s3"), "section du compte : avertissement d'une cible hors du compte");

    let mut req = NewRule {
        message_id: format!("m{from_x}"),
        name: "Camille".into(),
        from: false,
        subject: None,
        sent_to: false,
        folder_id: format!("f{target}"),
    };
    assert!(mb.create_rule(&req).is_err(), "sans condition : refusé");
    req.from = true;
    let id = mb.create_rule(&req).unwrap();

    // « Règles et alertes » : dans le jeu du compte, avec la description d'Outlook, sans avertissement.
    let listed = mb.rules().unwrap();
    let r = listed.iter().find(|r| r.id == id).unwrap();
    assert_eq!((r.origin.as_str(), r.origin_label.as_str(), r.active, r.error.as_deref()), ("account:3", "paul@example.org", true, None));
    assert_eq!(
        r.description,
        ["Appliquer cette règle après l'arrivée du message", "de Camille <x@example.org>", "le déplacer dans le dossier Factures"]
    );
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);

    // Cible désignée par le nom IMAP du compte, comme les règles copiées.
    let c = thunderlook_store::open(&dir).unwrap();
    let json: String = c.query_row("SELECT rule_json FROM rule WHERE origin = 'account:3'", [], |r| r.get(0)).unwrap();
    let rule: thunderlook_rules::Rule = serde_json::from_str(&json).unwrap();
    assert_eq!(rule.elements.iter().map(|e| e.id).collect::<Vec<_>>(), [0x190, 0x64, 0xCB, 0x12C]);
    let thunderlook_rules::Value::Folder { folder_entry_id, name, .. } = &rule.elements[3].values[0] else { panic!() };
    assert_eq!(folder_entry_id.as_slice(), b"imap:3:INBOX.Factures");
    assert_eq!(FolderResolver::load(&c).unwrap().resolve(folder_entry_id, name), FolderMatch::EntryId(target));
    drop(c);

    // « Exécuter cette règle maintenant » sur la Boîte de réception : seul le message de x@example.org part.
    let report = mb
        .run_rules(&RuleRunRequest {
            rule_ids: vec![id],
            folder_id: format!("f{inbox}"),
            subfolders: false,
            filter: RunFilter::All,
            apply: true,
            only: Vec::new(),
            confirm_moves: None,
        })
        .unwrap();
    assert_eq!((report.scanned, report.changed, report.applied), (2, 1, true), "{:?}", report.errors);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(folder_of(&dir, from_x), target);
    assert_eq!(folder_of(&dir, other), inbox);
    drop(mb);
    let _ = fs::remove_dir_all(&dir);
}

/// Message d'un PST sans compte : la règle va dans le jeu de ce PST et vise le dossier par son EntryID.
#[test]
fn rule_from_pst_message_goes_to_pst_set() {
    let dir = std::env::temp_dir().join(format!("tl-create-rule-pst-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let c = thunderlook_store::open(&dir).unwrap();
    source(&c, 1, "pst", "Archives");
    let root = folder(&c, 1, None, "", "root", None, None);
    let ipm = folder(&c, 1, Some(root), "IPM", "ipm", None, None);
    let inbox = folder(&c, 1, Some(ipm), "Boîte de réception", "inbox", None, None);
    let entry: Vec<u8> = [0u8; 4].into_iter().chain([7u8; 16]).chain(0x8022u32.to_le_bytes()).collect();
    let target = folder(&c, 1, Some(inbox), "Lettres", "folder", None, Some(&entry));
    let m = message(&c, &dir, 1, inbox, ("Lettre info", "news@example.org"), "Nouvelles d'octobre");
    drop(c);

    let mut mb = Mailbox::open(&dir).unwrap();
    let req = NewRule {
        message_id: format!("m{m}"),
        name: "Nouvelles".into(),
        from: false,
        subject: Some("Nouvelles".into()),
        sent_to: true,
        folder_id: format!("f{target}"),
    };
    let id = mb.create_rule(&req).unwrap();
    let r = mb.rules().unwrap().into_iter().find(|r| r.id == id).unwrap();
    assert_eq!((r.origin.as_str(), r.origin_label.as_str()), ("pst:s1", "Archives"));
    assert_eq!(
        r.description,
        [
            "Appliquer cette règle après l'arrivée du message",
            "envoyé à Paul <paul@example.org>",
            "contenant \"Nouvelles\" dans l'objet",
            "le déplacer dans le dossier Lettres"
        ]
    );
    // Une seconde règle rejoint le même jeu, à la suite.
    let id2 = mb.create_rule(&NewRule { name: "Bis".into(), ..req }).unwrap();
    let r2 = mb.rules().unwrap().into_iter().find(|r| r.id == id2).unwrap();
    assert_eq!((r2.origin.as_str(), r2.position), ("pst:s1", 1));
    let report = mb
        .run_rules(&RuleRunRequest {
            rule_ids: vec![id],
            folder_id: format!("f{inbox}"),
            subfolders: false,
            filter: RunFilter::All,
            apply: true,
            only: Vec::new(),
            confirm_moves: None,
        })
        .unwrap();
    assert_eq!(report.changed, 1, "{report:?}");
    assert_eq!(folder_of(&dir, m), target);
    drop(mb);
    let _ = fs::remove_dir_all(&dir);
}
