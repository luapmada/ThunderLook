//! Tris Lu/non lu, Type, Compte et deuxième critère, sur des données synthétiques.

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

fn add(c: &Connection, subject: &str, class: &str, read: i64, importance: i64, at: i64) {
    c.execute(
        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, received_at,
            message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
         VALUES (3, 0, ?1, ?2, 'Camille', 'c@example.org', '', '', ?3, 0, ?4, 0, ?5, 0, 100, 0, 'text', '', '', x'')",
        params![class, subject, at, read, importance],
    )
    .unwrap();
}

fn subjects(mb: &mut Mailbox, q: &ListQuery) -> Vec<String> {
    let view = mb.open_list(q).unwrap();
    (0..view.count).map(|i| mb.rows(view.token, i, 1).unwrap()[0].subject.clone()).collect()
}

fn setup(name: &str) -> (Fixture, Mailbox) {
    let dir = std::env::temp_dir().join(format!("tl-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let fx = Fixture(dir.clone());
    let c = thunderlook_store::open(&dir).unwrap();
    c.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (1, 'pst', 'x', 'x', 'Boîte A', 0, 0)", []).unwrap();
    for (n, role, parent) in [("", "root", None), ("IPM", "ipm", Some(1)), ("Boîte de réception", "inbox", Some(2))] {
        c.execute(
            "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position) VALUES (1, ?1, 0, ?2, ?3, NULL, 0, 0)",
            params![parent, n, role],
        )
        .unwrap();
    }
    add(&c, "a", "IPM.Note", 1, 1, 100);
    add(&c, "b", "IPM.Schedule.Meeting.Request", 0, 2, 200);
    add(&c, "c", "IPM.Note", 0, 0, 300);
    add(&c, "d", "REPORT.IPM.Note.NDR", 1, 2, 400);
    drop(c);
    (fx, Mailbox::open(&dir).unwrap())
}

fn query(sort: SortField, descending: bool, then_by: Option<SortField>, then_descending: bool) -> ListQuery {
    ListQuery { folder_id: "f3".into(), unread_only: false, sort, descending, conversations: false, then_by, then_descending }
}

#[test]
fn read_type_account() {
    let (_fx, mut mb) = setup("sort-more");
    let groups = |mb: &mut Mailbox, s| mb.open_list(&query(s, false, None, false)).unwrap().groups.into_iter().map(|g| (g.label, g.count)).collect::<Vec<_>>();
    assert_eq!(groups(&mut mb, SortField::Read), [("Non lu".to_string(), 2), ("Lu".to_string(), 2)]);
    assert_eq!(
        groups(&mut mb, SortField::Type),
        [("Message".to_string(), 2), ("Invitation à une réunion".to_string(), 1), ("Accusé ou rapport".to_string(), 1)]
    );
    assert_eq!(groups(&mut mb, SortField::Account), [("Boîte A".to_string(), 4)]);
}

#[test]
fn second_criterion() {
    let (_fx, mut mb) = setup("sort-then");
    // Lu/non lu, puis importance haute d'abord ; sans 2e critère : du plus récent au plus ancien.
    assert_eq!(subjects(&mut mb, &query(SortField::Read, false, None, false)), ["c", "b", "d", "a"]);
    assert_eq!(subjects(&mut mb, &query(SortField::Read, false, Some(SortField::Importance), false)), ["b", "c", "d", "a"]);
    assert_eq!(subjects(&mut mb, &query(SortField::Read, false, Some(SortField::Importance), true)), ["c", "b", "a", "d"]);
    // Deuxième critère = date croissante.
    assert_eq!(subjects(&mut mb, &query(SortField::Read, false, Some(SortField::Date), false)), ["b", "c", "a", "d"]);
}
