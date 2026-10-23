//! Lot F : `refresh` (lecture des seuls rendez-vous changés) donne exactement ce que donne `load`, et `load` ignore
//! les classes voisines (`IPM.AppointmentX`) que le `LIKE` indexé ramène.

use thunderlook_calendar::{
    bag::{self, Value},
    refresh,
    write::{self, Edit},
};
use thunderlook_store::rusqlite::{Connection, params};

const APPT: &str = "00062002-0000-0000-C000-000000000046";
const COMMON: &str = "00062008-0000-0000-C000-000000000046";

/// Source dont la table de noms porte les propriétés de rendez-vous qu'écrit `write::create`.
fn database(dir: &std::path::Path) -> (Connection, i64) {
    let conn = thunderlook_store::open(dir).unwrap();
    let mut raw = Vec::new();
    for (i, (guid, lid)) in [(APPT, 0x820D), (APPT, 0x820E), (APPT, 0x8215), (APPT, 0x8208), (APPT, 0x8205), (APPT, 0x8223), (APPT, 0x8217), (APPT, 0x8218), (COMMON, 0x8503), (COMMON, 0x8501)]
        .into_iter()
        .enumerate()
    {
        raw.extend_from_slice(&(0x8000 + i as u16).to_le_bytes());
        raw.extend_from_slice(guid.as_bytes());
        raw.push(0);
        raw.extend_from_slice(&(lid as u32).to_le_bytes());
    }
    conn.execute("INSERT INTO source (kind, origin_path, work_path, display_name, size, imported_at, props) VALUES ('pst', 'x', 'x', 'X', 0, 1, ?1)", [&raw]).unwrap();
    conn.execute("INSERT INTO folder (source_id, nid, name, role, container_class, hidden, position) VALUES (1, 1, 'Calendrier', 'folder', 'IPF.Appointment', 0, 1)", []).unwrap();
    (conn, 1)
}

fn edit(subject: &str, day: u32) -> Edit {
    Edit { subject: subject.into(), start: format!("2026-10-{day:02}T09:00"), end: format!("2026-10-{day:02}T10:00"), busy: 2, ..Edit::default() }
}

fn dump(cal: &thunderlook_calendar::Calendar) -> String {
    format!("{:?}\n{:?}", cal.appointments, cal.issues.iter().map(|i| (i.id, &i.subject, &i.detail)).collect::<Vec<_>>())
}

#[test]
fn refresh_equals_full_load() {
    let dir = std::env::temp_dir().join(format!("tl-refresh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (mut conn, folder) = database(&dir);
    let ids: Vec<i64> = (1..=12).map(|d| write::create(&mut conn, &dir, folder, &edit(&format!("RDV {d}"), d)).unwrap()).collect();
    // Un voisin de classe que le `LIKE 'IPM.Appointment%'` ramène mais que `load` doit ignorer, et un sac illisible.
    let junk = bag::merge(&[], &[(0x0037, Some(Value::Unicode("x".into())))]).unwrap();
    for (class, props) in [("IPM.AppointmentX", junk.as_slice()), ("IPM.Appointment.Occurrence", &[1u8, 2, 3][..])] {
        conn.execute(
            "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, message_flags, is_read, flag_status, importance,
                sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
             VALUES (?1, ?2, ?3, 's', '', '', '', '', 0, 1, 0, 1, 0, 0, 0, 'text', '', '', ?4)",
            params![folder, 9_000 + class.len() as i64, class, props],
        )
        .unwrap();
    }

    let first = thunderlook_calendar::load(&conn).unwrap();
    assert_eq!(first.appointments.len(), 12);
    assert_eq!(first.issues.len(), 1, "le sac tronqué est signalé, la classe voisine ignorée");
    let old = thunderlook_calendar::stamps(&conn).unwrap();
    assert_eq!(old.len(), 13);

    // Modification, suppression, création : rien d'autre ne doit être relu, et le résultat reste celui d'une lecture complète.
    std::thread::sleep(std::time::Duration::from_millis(5));
    write::update(&mut conn, &dir, ids[3], &edit("RDV 4 modifié", 4)).unwrap();
    write::delete(&mut conn, ids[7]).unwrap();
    let created = write::create(&mut conn, &dir, folder, &edit("Nouveau", 20)).unwrap();
    let (cal, now) = refresh(first, &old, &conn).unwrap();
    assert_eq!(dump(&cal), dump(&thunderlook_calendar::load(&conn).unwrap()));
    assert!(cal.appointments.iter().any(|a| a.id == created));
    assert!(cal.appointments.iter().all(|a| a.id != ids[7]));
    assert!(cal.appointments.iter().any(|a| a.subject == "RDV 4 modifié"));
    assert_eq!(now.len(), 13);

    // Sans changement : identique, et `load_ids` ne rend que ce qu'on lui demande.
    let (again, _) = refresh(cal, &now, &conn).unwrap();
    assert_eq!(dump(&again), dump(&thunderlook_calendar::load(&conn).unwrap()));
    let two = thunderlook_calendar::load_ids(&conn, &[ids[0], ids[1]]).unwrap();
    assert_eq!(two.appointments.iter().map(|a| a.id).collect::<Vec<_>>(), [ids[0], ids[1]]);
    let _ = std::fs::remove_dir_all(&dir);
}
