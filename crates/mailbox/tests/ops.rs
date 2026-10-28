//! Opérations du J3 sur un dossier de données synthétique (aucune donnée réelle).

use std::{fs, path::PathBuf};

use thunderlook_mailbox::Mailbox;
use thunderlook_model::{FlagAction, FlagState, FolderOp, ListQuery, MessageOp, Selection, SortField};
use thunderlook_store::rusqlite::{Connection, params};

struct Fixture {
    dir: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn folder(c: &Connection, parent: Option<i64>, name: &str, role: &str, pos: i64) -> i64 {
    c.execute(
        "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position) VALUES (1, ?1, 0, ?2, ?3, NULL, 0, ?4)",
        params![parent, name, role, pos],
    )
    .unwrap();
    c.last_insert_rowid()
}

fn message(c: &Connection, dir: &PathBuf, folder: i64, subject: &str, read: bool) -> i64 {
    c.execute(
        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, received_at,
            message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
         VALUES (?1, 0, 'IPM.Note', ?2, 'Camille', 'c@example.org', 'p@example.org', '', 1760000000000, 0, ?3, 0, 1, 0, 100, 0, 'text', '', '', x'')",
        params![folder, subject, read],
    )
    .unwrap();
    let id = c.last_insert_rowid();
    let rel = thunderlook_store::eml_relative_path(1, id);
    let path = thunderlook_store::resolve(dir, &rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, format!("Subject: {subject}\r\nFrom: c@example.org\r\n\r\nCorps\r\n")).unwrap();
    c.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, rel]).unwrap();
    id
}

fn setup() -> (Fixture, Mailbox, [i64; 4]) {
    let dir = std::env::temp_dir().join(format!("tl-ops-{}-{}", std::process::id(), rand_suffix()));
    let _ = fs::remove_dir_all(&dir);
    let c = thunderlook_store::open(&dir).unwrap();
    c.execute(
        "INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (1, 'pst', 'x', 'x', 'Test', 0, 0)",
        [],
    )
    .unwrap();
    let root = folder(&c, None, "", "root", 0);
    let ipm = folder(&c, Some(root), "IPM", "ipm", 0);
    let inbox = folder(&c, Some(ipm), "Boîte de réception", "inbox", 0);
    let deleted = folder(&c, Some(ipm), "Éléments supprimés", "deleted", 1);
    let sent = folder(&c, Some(ipm), "Éléments envoyés", "sent", 2);
    let projets = folder(&c, Some(inbox), "Projets", "folder", 0);
    for (i, s) in ["Devis", "Facture", "Relance"].iter().enumerate() {
        message(&c, &dir, inbox, s, i == 0);
    }
    drop(c);
    let mb = Mailbox::open(&dir).unwrap();
    (Fixture { dir }, mb, [inbox, deleted, sent, projets])
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

fn count(mb: &mut Mailbox, folder: i64) -> (u32, u32) {
    let v = mb
        .open_list(&ListQuery { folder_id: format!("f{folder}"), unread_only: false, sort: SortField::Date, descending: true, conversations: false, then_by: None, then_descending: false })
        .unwrap();
    (v.folder_total, v.folder_unread)
}

fn all(mb: &mut Mailbox, folder: i64) -> Selection {
    let v = mb
        .open_list(&ListQuery { folder_id: format!("f{folder}"), unread_only: false, sort: SortField::Date, descending: true, conversations: false, then_by: None, then_descending: false })
        .unwrap();
    Selection { token: v.token, ranges: vec![(0, v.count)], ids: vec![] }
}

#[test]
fn read_flag_categories_and_undo() {
    let (_fx, mut mb, [inbox, ..]) = setup();
    assert_eq!(count(&mut mb, inbox), (3, 2));
    let sel = all(&mut mb, inbox);
    let r = mb.apply(&sel, &MessageOp::ToggleRead).unwrap();
    assert_eq!(r.affected, 2);
    assert_eq!(count(&mut mb, inbox), (3, 0));
    assert_eq!(mb.undo().unwrap().as_deref(), Some("marquer comme lu"));
    assert_eq!(count(&mut mb, inbox), (3, 2));

    // Marquage automatique : n'entre pas dans l'historique d'annulation.
    let first = mb.rows(sel.token, 0, 3).unwrap().into_iter().find(|m| m.unread).unwrap().id;
    mb.apply(&Selection::ids(vec![first]), &MessageOp::Seen).unwrap();
    assert_eq!(count(&mut mb, inbox), (3, 1));
    assert_eq!(mb.undo_label(), None);

    let sel = all(&mut mb, inbox);
    mb.apply(&sel, &MessageOp::Flag { action: FlagAction::Tomorrow }).unwrap();
    let rows = mb.rows(sel.token, 0, 10).unwrap();
    assert!(rows.iter().all(|m| m.flag == FlagState::Flagged));
    mb.apply(&Selection { ids: vec![rows[0].id.clone()], ..Default::default() }, &MessageOp::Flag { action: FlagAction::Toggle }).unwrap();
    assert_eq!(mb.rows(sel.token, 0, 1).unwrap()[0].flag, FlagState::Done);

    mb.apply(&sel, &MessageOp::Category { name: "Client ADI".into(), on: true }).unwrap();
    assert!(mb.rows(sel.token, 0, 10).unwrap().iter().all(|m| m.categories.iter().any(|c| c.name == "Client ADI")));
    assert!(mb.categories().unwrap().iter().any(|c| c.name == "Client ADI" && c.color.is_none()));
    mb.undo().unwrap();
    assert!(mb.rows(sel.token, 0, 10).unwrap().iter().all(|m| m.categories.is_empty()));
}

#[test]
fn move_copy_delete_and_purge() {
    let (fx, mut mb, [inbox, deleted, _, projets]) = setup();
    let sel = all(&mut mb, inbox);
    let first = mb.rows(sel.token, 0, 1).unwrap()[0].id.clone();
    let one = Selection::ids(vec![first.clone()]);

    mb.apply(&one, &MessageOp::Move { folder_id: format!("f{projets}") }).unwrap();
    assert_eq!(count(&mut mb, projets).0, 1);
    mb.undo().unwrap();
    assert_eq!(count(&mut mb, projets).0, 0);

    let r = mb.apply(&one, &MessageOp::Copy { folder_id: format!("fav-f{projets}") }).unwrap();
    assert_eq!(r.affected, 1);
    let token = all(&mut mb, projets).token;
    let copy = mb.rows(token, 0, 1).unwrap()[0].clone();
    assert_ne!(copy.id, first);
    assert!(mb.message_view(&copy.id, false).is_ok(), "le .eml copié se lit");

    // Suppression : vers « Éléments supprimés », puis définitive depuis ce dossier.
    mb.apply(&one, &MessageOp::Delete).unwrap();
    assert_eq!(count(&mut mb, deleted).0, 1);
    let eml_count = || walk(&fx.dir.join("eml"));
    let before = eml_count();
    let r = mb.apply(&one, &MessageOp::Delete).unwrap();
    assert!(r.message.contains("définitivement"), "{}", r.message);
    assert_eq!(count(&mut mb, deleted).0, 0);
    assert_eq!(eml_count(), before - 1, "le fichier .eml est effacé");

    // Archiver crée « Archives » au premier usage.
    let rest = all(&mut mb, inbox);
    let r = mb.apply(&rest, &MessageOp::Archive).unwrap();
    let archives = r.created_id.expect("dossier Archives créé");
    let tree = mb.folder_tree().unwrap();
    let f = tree.find_folder(&archives).unwrap();
    assert_eq!((f.name.as_str(), f.total), ("Archives", 2));
    mb.undo().unwrap();
    assert_eq!(count(&mut mb, inbox).0, 2);
}

#[test]
fn folders_and_favorites() {
    let (_fx, mut mb, [inbox, deleted, sent, projets]) = setup();
    let r = mb.folder_op(&FolderOp::Create { parent_id: format!("f{inbox}"), name: "Clients".into() }).unwrap();
    let clients = r.created_id.unwrap();
    assert!(mb.folder_op(&FolderOp::Create { parent_id: format!("f{inbox}"), name: " clients ".into() }).is_err(), "doublon refusé");
    mb.folder_op(&FolderOp::Rename { id: clients.clone(), name: "Clients ADI".into() }).unwrap();
    assert!(mb.folder_op(&FolderOp::Rename { id: format!("f{inbox}"), name: "X".into() }).is_err(), "dossier spécial");
    assert!(mb.folder_op(&FolderOp::Move { id: format!("f{projets}"), parent_id: format!("f{projets}") }).is_err(), "dans lui-même");
    mb.folder_op(&FolderOp::Move { id: format!("f{projets}"), parent_id: clients.clone() }).unwrap();
    let tree = mb.folder_tree().unwrap();
    let c = tree.find_folder(&clients).unwrap();
    assert_eq!(c.name, "Clients ADI");
    assert_eq!(c.children.len(), 1);
    mb.undo().unwrap();
    mb.undo().unwrap();
    assert_eq!(mb.folder_tree().unwrap().find_folder(&clients).unwrap().name, "Clients");

    // Supprimer un dossier : il passe dans « Éléments supprimés », puis disparaît de là définitivement.
    mb.folder_op(&FolderOp::Delete { id: clients.clone() }).unwrap();
    let tree = mb.folder_tree().unwrap();
    assert_eq!(tree.find_folder(&format!("f{deleted}")).unwrap().children.len(), 1);
    mb.folder_op(&FolderOp::Delete { id: clients.clone() }).unwrap();
    assert!(mb.folder_tree().unwrap().find_folder(&clients).is_none());

    // Favoris : défaut (réception, envoyés), ajout, retrait, annulation.
    let fav = |mb: &Mailbox| -> Vec<String> {
        mb.folder_tree().unwrap().sections[0].folders.iter().map(|f| f.id.clone()).collect()
    };
    assert_eq!(fav(&mb), [format!("fav-f{inbox}"), format!("fav-f{sent}")]);
    mb.folder_op(&FolderOp::AddFavorite { id: format!("f{projets}") }).unwrap();
    mb.folder_op(&FolderOp::RemoveFavorite { id: format!("fav-f{sent}") }).unwrap();
    assert_eq!(fav(&mb), [format!("fav-f{inbox}"), format!("fav-f{projets}")]);
    mb.undo().unwrap();
    assert_eq!(fav(&mb).len(), 3);

    mb.folder_op(&FolderOp::MarkAllRead { id: format!("f{inbox}") }).unwrap();
    assert_eq!(count(&mut mb, inbox).1, 0);
    mb.folder_op(&FolderOp::Empty { id: format!("f{inbox}") }).unwrap();
    assert_eq!(count(&mut mb, inbox).0, 0);
    assert_eq!(count(&mut mb, deleted).0, 3);
}

fn walk(dir: &PathBuf) -> usize {
    fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .map(|e| if e.file_type().unwrap().is_dir() { walk(&e.path()) } else { 1 })
                .sum()
        })
        .unwrap_or(0)
}

#[test]
fn favorites_default_to_the_first_account() {
    // Un PST importé (source 1) et un compte IMAP (source 2) : sans favoris enregistrés, ceux du compte, qui reçoit le courrier.
    let (fx, _mb, [pst_inbox, ..]) = setup();
    let c = thunderlook_store::open(&fx.dir).unwrap();
    c.execute(
        "INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (2, 'imap', '', '', 'a@example.org', 0, 0)",
        [],
    )
    .unwrap();
    let add = |parent: Option<i64>, name: &str, role: &str, pos: i64| -> i64 {
        c.execute(
            "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position) VALUES (2, ?1, 0, ?2, ?3, NULL, 0, ?4)",
            params![parent, name, role, pos],
        )
        .unwrap();
        c.last_insert_rowid()
    };
    let root = add(None, "", "root", 0);
    let ipm = add(Some(root), "IPM", "ipm", 0);
    let inbox = add(Some(ipm), "Boîte de réception", "inbox", 0);
    let sent = add(Some(ipm), "Éléments envoyés", "sent", 1);
    c.execute(
        "INSERT INTO account (id, source_id, name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, save_sent, created_at)
         VALUES (1, 2, '', 'a@example.org', 'a', 'h', 993, 'tls', 'h', 465, 'tls', 1, 0)",
        [],
    )
    .unwrap();
    drop(c);
    let mb = Mailbox::open(&fx.dir).unwrap();
    let tree = mb.folder_tree().unwrap();
    let fav: Vec<String> = tree.sections[0].folders.iter().map(|f| f.id.clone()).collect();
    assert_eq!(fav, [format!("fav-f{inbox}"), format!("fav-f{sent}")]);
    assert_eq!(tree.default_folder().unwrap().id, format!("fav-f{inbox}"), "ouverture sur la réception du compte");
    assert_ne!(tree.default_folder().unwrap().id, format!("fav-f{pst_inbox}"));
}
