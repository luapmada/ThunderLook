//! Tris « À » et « Importance » sur un dossier de données synthétique.

use std::{fs, path::PathBuf};

use thunderlook_mailbox::Mailbox;
use thunderlook_model::{ListQuery, SortField};
use thunderlook_store::rusqlite::{Connection, params};

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn add(c: &Connection, folder: i64, subject: &str, to: &str, importance: i64) {
    c.execute(
        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, received_at,
            message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
         VALUES (?1, 0, 'IPM.Note', ?2, 'Camille', 'c@example.org', ?3, '', 1760000000000, 0, 1, 0, ?4, 0, 100, 0, 'text', '', '', x'')",
        params![folder, subject, to, importance],
    )
    .unwrap();
}

fn open(mb: &mut Mailbox, folder: i64, sort: SortField, descending: bool) -> Vec<(String, u32)> {
    let q = ListQuery { folder_id: format!("f{folder}"), unread_only: false, sort, descending, conversations: false };
    mb.open_list(&q).unwrap().groups.into_iter().map(|g| (g.label, g.count)).collect()
}

#[test]
fn sort_by_recipient_and_importance() {
    let dir = std::env::temp_dir().join(format!("tl-sort-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let _fx = Fixture(dir.clone());
    let c = thunderlook_store::open(&dir).unwrap();
    c.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (1, 'pst', 'x', 'x', 'Test', 0, 0)", []).unwrap();
    for (name, role, parent) in [("", "root", None), ("IPM", "ipm", Some(1)), ("Boîte de réception", "inbox", Some(2))] {
        c.execute(
            "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position) VALUES (1, ?1, 0, ?2, ?3, NULL, 0, 0)",
            params![parent, name, role],
        )
        .unwrap();
    }
    add(&c, 3, "a", "Zoé <z@example.org>", 1);
    add(&c, 3, "b", "Alain <a@example.org>; Zoé <z@example.org>", 2);
    add(&c, 3, "c", "alain <a2@example.org>", 0);
    add(&c, 3, "d", "", 1);
    drop(c);
    let mut mb = Mailbox::open(&dir).unwrap();
    assert_eq!(
        open(&mut mb, 3, SortField::To, false),
        [("(aucun destinataire)".to_string(), 1), ("Alain".to_string(), 2), ("Zoé".to_string(), 1)]
    );
    assert_eq!(
        open(&mut mb, 3, SortField::Importance, false),
        [("Importance haute".to_string(), 1), ("Importance normale".to_string(), 2), ("Importance faible".to_string(), 1)]
    );
    assert_eq!(open(&mut mb, 3, SortField::Importance, true)[0].0, "Importance faible");
}
