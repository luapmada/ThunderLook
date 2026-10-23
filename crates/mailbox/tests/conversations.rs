//! Vue en conversations (J3b) sur un dossier de données synthétique (aucune donnée réelle).

use std::{fs, path::PathBuf};

use thunderlook_mailbox::Mailbox;
use thunderlook_model::{FlagState, ListQuery, MessageOp, Selection, SortField};
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

/// Message de la conversation `conv` (octet répété dans le GUID de l'index), ou sans index si `None`.
/// `subject` porte le marqueur de préfixe du PST quand il commence par `\u{1}`.
#[allow(clippy::too_many_arguments)]
fn message(c: &Connection, dir: &PathBuf, folder: i64, subject: &str, from: &str, minute: i64, read: bool, conv: Option<u8>) -> i64 {
    let index = conv.map(|g| {
        let mut v = vec![0x01, 0, 0, 0, 0, 0];
        v.extend([g; 16]);
        v
    });
    // Sac MAPI minimal : `PidTagConversationIndexTracking` à vrai.
    let props: Vec<u8> = [0x16, 0x30, 0x0B, 0x00, 0x01].to_vec();
    c.execute(
        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, received_at,
            message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path,
            props, conversation_topic, conversation_index)
         VALUES (?1, 0, 'IPM.Note', ?2, ?3, '', 'p@example.org', '', ?4, 0, ?5, 0, 1, 0, 100, 0, 'text', '', '', ?6, ?7, ?8)",
        params![folder, subject, from, 1_760_000_000_000i64 + minute * 60_000, read, props, subject.trim_start_matches(['\u{1}', '\u{5}']), index],
    )
    .unwrap();
    let id = c.last_insert_rowid();
    let rel = thunderlook_store::eml_relative_path(1, id);
    let path = thunderlook_store::resolve(dir, &rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, format!("Subject: {subject}\r\n\r\nCorps\r\n")).unwrap();
    c.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, rel]).unwrap();
    id
}

fn query(folder: i64, conversations: bool) -> ListQuery {
    ListQuery { folder_id: format!("f{folder}"), unread_only: false, sort: SortField::Date, descending: true, conversations, then_by: None, then_descending: false }
}

/// Dossier de données vide (source 1) ; supprimé à la fin du test.
fn store() -> (Fixture, Connection) {
    let dir = std::env::temp_dir().join(format!("tl-conv-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let _ = fs::remove_dir_all(&dir);
    let c = thunderlook_store::open(&dir).unwrap();
    (Fixture { dir }, c)
}

#[test]
fn arrangements_categories_flag_attachments() {
    let (fx, c) = store();
    let dir = &fx.dir;
    c.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (1, 'pst', 'x', 'x', 'Test', 0, 0)", [])
        .unwrap();
    let root = folder(&c, None, "", "root");
    let ipm = folder(&c, Some(root), "IPM", "ipm");
    let inbox = folder(&c, Some(ipm), "Boîte de réception", "inbox");
    let both = message(&c, dir, inbox, "Deux catégories", "A", 1, true, None);
    let one = message(&c, dir, inbox, "Une catégorie", "B", 2, true, None);
    let none = message(&c, dir, inbox, "Aucune", "C", 3, true, None);
    for (m, name) in [(both, "Rouge"), (both, "Bleu"), (one, "rouge")] {
        c.execute("INSERT INTO message_category (message_id, name) VALUES (?1, ?2)", params![m, name]).unwrap();
    }
    c.execute("UPDATE message SET flag_status = 2 WHERE id = ?1", [one]).unwrap();
    c.execute("UPDATE message SET flag_status = 1 WHERE id = ?1", [none]).unwrap();
    c.execute(
        "INSERT INTO attachment (message_id, position, filename, mime_type, size, method, inline, hidden, props) VALUES (?1, 0, 'a.pdf', 'application/pdf', 1, 1, 0, 0, x'')",
        [both],
    )
    .unwrap();
    drop(c);
    let mut mb = Mailbox::open(dir).unwrap();
    let open = |mb: &mut Mailbox, sort: SortField| {
        let v = mb.open_list(&ListQuery { folder_id: format!("f{inbox}"), unread_only: false, sort, descending: false, conversations: false, then_by: None, then_descending: false }).unwrap();
        let ids: Vec<String> = mb.rows(v.token, 0, 10).unwrap().into_iter().map(|r| r.id).collect();
        (v.groups.into_iter().map(|g| (g.label, g.count)).collect::<Vec<_>>(), ids, v.token)
    };
    // Catégories : « Rouge » et « rouge » se regroupent ; un message à deux catégories figure deux fois.
    let (groups, ids, token) = open(&mut mb, SortField::Categories);
    assert_eq!(groups, vec![("Bleu".into(), 1), ("Rouge".into(), 2), ("(aucune catégorie)".into(), 1)]);
    assert_eq!(ids, vec![format!("m{both}"), format!("m{one}"), format!("m{both}"), format!("m{none}")]);
    let mut all = mb.resolve(&Selection { token, ranges: vec![(0, 4)], ids: vec![] }).unwrap();
    all.dedup();
    assert_eq!(all.len(), 3, "une sélection ne compte un message qu'une fois");
    let (groups, ids, _) = open(&mut mb, SortField::Flag);
    assert_eq!(groups, vec![("Avec suivi".into(), 1), ("Terminé".into(), 1), ("Sans indicateur".into(), 1)]);
    assert_eq!(ids, vec![format!("m{one}"), format!("m{none}"), format!("m{both}")]);
    let (groups, ids, _) = open(&mut mb, SortField::Attachments);
    assert_eq!(groups, vec![("Avec pièce jointe".into(), 1), ("Sans pièce jointe".into(), 2)]);
    assert_eq!(ids[0], format!("m{both}"));
}

#[test]
fn conversation_view_expand_and_operations() {
    let (fx, c) = store();
    let dir = fx.dir.clone();
    c.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (1, 'pst', 'x', 'x', 'Test', 0, 0)", [])
        .unwrap();
    // Base « antérieure » : on efface les identifiants pour vérifier leur calcul à l'ouverture.
    let root = folder(&c, None, "", "root");
    let ipm = folder(&c, Some(root), "IPM", "ipm");
    let inbox = folder(&c, Some(ipm), "Boîte de réception", "inbox");
    let sent = folder(&c, Some(ipm), "Éléments envoyés", "sent");
    let deleted = folder(&c, Some(ipm), "Éléments supprimés", "deleted");
    // Conversation A : deux reçus et une réponse envoyée ; un élément supprimé reste hors du compte.
    let a1 = message(&c, &dir, inbox, "\u{1}\u{1}Devis", "Camille", 1, true, Some(0xA));
    let a2 = message(&c, &dir, sent, "\u{1}\u{5}RE: Devis", "Paul", 2, true, Some(0xA));
    let a3 = message(&c, &dir, inbox, "\u{1}\u{5}RE: Devis", "Camille", 3, false, Some(0xA));
    message(&c, &dir, deleted, "RE: Devis", "Camille", 4, true, Some(0xA));
    // Conversation B : un seul élément ici et rien ailleurs. C : sans index ni sujet.
    let b = message(&c, &dir, inbox, "Facture", "Dominique", 5, true, Some(0xB));
    c.execute("UPDATE message SET conversation_topic = NULL WHERE id = ?1", [b]).unwrap();
    let lone = message(&c, &dir, inbox, "Note", "Lou", 0, false, None);
    c.execute("UPDATE message SET conversation_topic = NULL WHERE id = ?1", [lone]).unwrap();
    c.execute("UPDATE message SET conversation_id = NULL", []).unwrap();
    c.execute_batch("PRAGMA user_version = 5").unwrap();
    drop(c);

    let mut mb = Mailbox::open(&dir).unwrap();
    // Le marqueur de préfixe du PST a disparu des objets (migration v6).
    let flat = mb.open_list(&query(inbox, false)).unwrap();
    assert_eq!(flat.count, 4);
    let rows = mb.rows(flat.token, 0, 10).unwrap();
    assert!(rows.iter().all(|r| !r.subject.starts_with('\u{1}')), "{:?}", rows.iter().map(|r| &r.subject).collect::<Vec<_>>());

    // En conversations : B (minute 5), A (dernier élément du dossier à la minute 3), puis l'élément sans conversation.
    let v = mb.open_list(&query(inbox, true)).unwrap();
    assert_eq!(v.count, 3);
    assert_eq!((v.folder_total, v.folder_unread), (4, 2));
    let rows = mb.rows(v.token, 0, 10).unwrap();
    assert_eq!(rows[0].id, format!("m{b}"));
    assert_eq!(rows[0].conversation, None, "un seul élément : ligne simple");
    let head = &rows[1];
    assert_eq!(head.id, format!("m{a3}"));
    assert_eq!(head.subject, "Devis");
    assert_eq!(head.from, "Camille; Paul");
    assert!(head.unread);
    let info = head.conversation.expect("en-tête");
    assert_eq!((info.count, info.unread, info.expanded), (3, 1, false));
    assert_eq!(rows[2].id, format!("m{lone}"));

    // Dépliage : les trois éléments, du plus récent au plus ancien, la réponse envoyée avec son dossier.
    let open = mb.toggle_conversation(v.token, 1, None).unwrap();
    assert_eq!((open.token, open.count), (v.token, 6));
    let rows = mb.rows(v.token, 0, 10).unwrap();
    let kids: Vec<(String, bool, Option<String>)> = rows[2..5].iter().map(|r| (r.id.clone(), r.child, r.folder.clone())).collect();
    assert_eq!(
        kids,
        vec![
            (format!("m{a3}"), true, None),
            (format!("m{a2}"), true, Some("Éléments envoyés".into())),
            (format!("m{a1}"), true, None)
        ]
    );
    assert!(rows[1].conversation.unwrap().expanded);
    assert_eq!(rows[5].id, format!("m{lone}"));

    // Une opération sur l'en-tête vise tous les éléments de la conversation dans le dossier.
    let sel = Selection { token: v.token, ranges: vec![(1, 2)], ids: vec![] };
    assert_eq!(mb.resolve(&sel).unwrap(), {
        let mut ids = vec![a1, a3];
        ids.sort_unstable();
        ids
    });
    mb.apply(&sel, &MessageOp::Flag { action: thunderlook_model::FlagAction::Today }).unwrap();

    // Rouverte (après une opération), la conversation reste dépliée ; l'indicateur remonte sur l'en-tête.
    let v2 = mb.open_list(&query(inbox, true)).unwrap();
    assert_eq!(v2.count, 6);
    let rows = mb.rows(v2.token, 1, 1).unwrap();
    assert_eq!(rows[0].flag, FlagState::Flagged);

    // Repli.
    let closed = mb.toggle_conversation(v2.token, 1, Some(false)).unwrap();
    assert_eq!(closed.count, 3);
    assert!(mb.toggle_conversation(v2.token, 0, None).is_ok(), "B est une conversation d'un élément, dépliable sans effet visible");
    drop(mb);
}
