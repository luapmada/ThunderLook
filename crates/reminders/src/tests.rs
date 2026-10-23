use std::path::{Path, PathBuf};

use thunderlook_calendar::{
    bag::{self, Value},
    named::{PSETID_APPOINTMENT, PSETID_COMMON},
    recur::Frequency,
    write::{self, Edit, Repeat, RepeatEnd},
};
use thunderlook_store::rusqlite::{Connection, params};

use super::*;

const MIN: i64 = MINUTE_MS;

/// Base de test : une source dont la table de noms couvre rendez-vous, rappels et tâches ; un dossier calendrier (1) et
/// un dossier tâches (2).
fn fixture(name: &str) -> (PathBuf, Connection) {
    let dir = std::env::temp_dir().join(format!("tl-reminders-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let conn = thunderlook_store::open(&dir).unwrap();
    let mut names = Vec::new();
    let mut add = |id: u16, set: &str, lid: u32| {
        names.extend_from_slice(&id.to_le_bytes());
        names.extend_from_slice(set.as_bytes());
        names.push(0);
        names.extend_from_slice(&lid.to_le_bytes());
    };
    for (i, lid) in [0x820D, 0x820E, 0x8215, 0x8208, 0x8205, 0x8223, 0x8216, 0x8217, 0x8218].into_iter().enumerate() {
        add(0x8000 + i as u16, PSETID_APPOINTMENT, lid);
    }
    add(0x8010, PSETID_COMMON, 0x8501);
    add(0x8011, PSETID_COMMON, 0x8503);
    add(0x8012, PSETID_COMMON, 0x8502);
    add(0x8020, PSETID_TASK, 0x8101);
    add(0x8021, PSETID_TASK, 0x811C);
    add(0x8022, PSETID_TASK, 0x8105);
    conn.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at, props) VALUES (1, 'tl', '', '', 't', 0, 0, ?1)", [names]).unwrap();
    conn.execute("INSERT INTO folder (id, source_id, nid, name, role, hidden, position) VALUES (1, 1, 1, 'Calendrier', 'folder', 0, 0)", []).unwrap();
    conn.execute("INSERT INTO folder (id, source_id, nid, name, role, hidden, position) VALUES (2, 1, 2, 'Tâches', 'folder', 0, 0)", []).unwrap();
    (dir, conn)
}

fn utc(local: &str) -> i64 {
    let t = chrono::NaiveDateTime::parse_from_str(local, "%Y-%m-%dT%H:%M").unwrap();
    TimeZone::PARIS.to_utc(t).and_utc().timestamp_millis()
}

fn appt(conn: &mut Connection, dir: &Path, subject: &str, start: &str, reminder: Option<i32>, repeat: Option<Repeat>) -> i64 {
    let (date, time) = start.split_once('T').unwrap();
    let end = format!("{date}T{}", if time < "23:00" { format!("{:02}:{}", time[..2].parse::<u32>().unwrap() + 1, &time[3..]) } else { "23:59".into() });
    let edit = Edit { subject: subject.into(), location: "Salle A".into(), start: start.into(), end, reminder, busy: 2, repeat, ..Default::default() };
    write::create(conn, dir, 1, &edit).unwrap()
}

/// Tâche écrite directement (`task_save` n'écrit pas les propriétés de rappel).
fn task(conn: &Connection, subject: &str, status: i64, reminder: Option<i64>, due_day_ms: Option<i64>) -> i64 {
    let mut set = vec![(0x8020, Some(Value::Long(status as i32))), (0x8021, Some(Value::Bool(status == 2)))];
    if let Some(r) = reminder {
        set.push((0x8011, Some(Value::Bool(true))));
        set.push((0x8012, Some(Value::time_ms(r))));
    }
    if reminder.is_none() {
        set.push((0x8011, Some(Value::Bool(true))));
    }
    set.push((0x8022, due_day_ms.map(Value::time_ms)));
    let props = bag::merge(&[], &set.into_iter().filter(|(_, v)| v.is_some()).collect::<Vec<_>>()).unwrap();
    conn.execute(
        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, created_at, modified_at, received_at,
            message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
         VALUES (2, (SELECT COALESCE(MAX(nid), 0) + 1 FROM message), 'IPM.Task', ?1, '', '', '', '', 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 'text', '', '', ?2)",
        params![subject, props],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn ids(v: &[Reminder]) -> Vec<String> {
    v.iter().map(|r| r.id.clone()).collect()
}

fn win() -> Window {
    Window { lookback_ms: 60 * MIN, lookahead_ms: 60 * MIN }
}

#[test]
fn appointment_in_ten_minutes_with_fifteen_minutes_reminder_is_due() {
    let (dir, mut conn) = fixture("single");
    let start = utc("2026-10-23T14:10");
    let now = start - 10 * MIN;
    let id = appt(&mut conn, &dir, "Dentiste", "2026-10-23T14:10", Some(15), None);
    appt(&mut conn, &dir, "Sans rappel", "2026-10-23T14:20", None, None);
    appt(&mut conn, &dir, "Loin", "2026-10-23T18:00", Some(15), None);
    let r = due(&conn, &dir, now, Window { lookback_ms: 60 * MIN, lookahead_ms: 0 }).unwrap();
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!(r[0].id, format!("a{id}@2026-10-23"));
    assert_eq!((r[0].kind, r[0].title.as_str(), r[0].when_ms, r[0].event_start_ms), (Kind::Appointment, "Dentiste", start - 15 * MIN, start));
    assert_eq!(r[0].location.as_deref(), Some("Salle A"));
    // Après l'heure du rappel, hors de la plage arrière : plus dû.
    assert!(due(&conn, &dir, now + 30 * MIN, Window { lookback_ms: 5 * MIN, lookahead_ms: 0 }).unwrap().is_empty());
}

#[test]
fn weekly_series_has_one_reminder_per_occurrence_and_deleted_has_none() {
    let (dir, mut conn) = fixture("series");
    let rep = Repeat { frequency: Frequency::Weekly, interval: 1, end: RepeatEnd::Count(4) };
    let id = appt(&mut conn, &dir, "Point", "2026-11-05T10:00", Some(15), Some(rep));
    let (from, to) = (utc("2026-11-01T00:00"), utc("2026-12-01T00:00"));
    let all = |conn: &Connection| due(conn, &dir, from, Window { lookback_ms: 0, lookahead_ms: to - from }).unwrap();
    assert_eq!(ids(&all(&conn)), ["05", "12", "19", "26"].map(|d| format!("a{id}@2026-11-{d}")));
    write::delete_occurrence(&mut conn, id, "2026-11-12").unwrap();
    assert_eq!(ids(&all(&conn)), ["05", "19", "26"].map(|d| format!("a{id}@2026-11-{d}")));
}

#[test]
fn completed_task_has_no_reminder() {
    let (dir, conn) = fixture("tasks");
    let now = utc("2026-10-23T09:00");
    let open = task(&conn, "À faire", 0, Some(now - 5 * MIN), None);
    task(&conn, "Terminée", 2, Some(now - 5 * MIN), None);
    // Sans heure : l'échéance à 8 h (heure de Paris).
    let due_day = task(&conn, "Échéance", 1, None, Some(chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis()));
    task(&conn, "Sans date", 0, None, None);
    let r = due(&conn, &dir, now, win()).unwrap();
    assert_eq!(ids(&r), [format!("t{due_day}"), format!("t{open}")], "{r:?}");
    assert_eq!(r[0].when_ms, utc("2026-10-23T08:00"));
    assert_eq!(r[1].kind, Kind::Task);
}

#[test]
fn snooze_then_dismiss() {
    let (dir, mut conn) = fixture("state");
    let start = utc("2026-10-23T14:00");
    let id = appt(&mut conn, &dir, "Réunion", "2026-10-23T14:00", Some(15), None);
    let rid = format!("a{id}@2026-10-23");
    let now = start - 10 * MIN;
    assert_eq!(ids(&due(&conn, &dir, now, win()).unwrap()), [rid.clone()]);

    snooze(&dir, &rid, now + 30 * MIN).unwrap();
    assert!(due(&conn, &dir, now, win()).unwrap().is_empty());
    assert!(due(&conn, &dir, now + 29 * MIN, win()).unwrap().is_empty());
    // Échu : revient, même si l'heure d'origine est sortie de la plage arrière.
    let late = Window { lookback_ms: 10 * MIN, lookahead_ms: 0 };
    assert_eq!(ids(&due(&conn, &dir, now + 31 * MIN, late).unwrap()), [rid.clone()]);
    // Le fichier est relu d'un appel à l'autre ; pas de reste de fichier temporaire.
    assert!(dir.join("reminders.json").exists() && !dir.join("reminders.json.tmp").exists());

    dismiss(&dir, &rid).unwrap();
    assert!(due(&conn, &dir, now + 31 * MIN, late).unwrap().is_empty());
    assert!(due(&conn, &dir, now + 3 * 24 * 60 * MIN, Window { lookback_ms: 7 * 24 * 60 * MIN, lookahead_ms: 0 }).unwrap().is_empty());
}

#[test]
fn daylight_saving_change_does_not_shift_reminders() {
    let (dir, mut conn) = fixture("dst");
    // Jeudis 10:00 à Paris : le 22 octobre 2026 (heure d'été), le 29 (heure d'hiver, retour le 25).
    let rep = Repeat { frequency: Frequency::Weekly, interval: 1, end: RepeatEnd::Count(2) };
    let a = appt(&mut conn, &dir, "Automne", "2026-10-22T10:00", Some(15), Some(rep));
    // Jeudis 10:00 : le 25 mars 2027 (heure d'hiver), le 1er avril (heure d'été, passage le 28 mars).
    let rep = Repeat { frequency: Frequency::Weekly, interval: 1, end: RepeatEnd::Count(2) };
    let b = appt(&mut conn, &dir, "Printemps", "2027-03-25T10:00", Some(15), Some(rep));
    let all = |from: &str, to: &str| due(&conn, &dir, utc(from), Window { lookback_ms: 0, lookahead_ms: utc(to) - utc(from) }).unwrap();
    for (r, (id, local)) in all("2026-10-20T00:00", "2026-11-01T00:00").iter().zip([(a, "2026-10-22T09:45"), (a, "2026-10-29T09:45")]) {
        assert_eq!(r.when_ms, utc(local));
        assert!(r.id.starts_with(&format!("a{id}@")));
    }
    let r = all("2027-03-20T00:00", "2027-04-05T00:00");
    assert_eq!(r.iter().map(|x| x.when_ms).collect::<Vec<_>>(), [utc("2027-03-25T09:45"), utc("2027-04-01T09:45")]);
    assert_eq!(ids(&r), [format!("a{b}@2027-03-25"), format!("a{b}@2027-04-01")]);
    // Écart réel entre deux occurrences : 7 jours moins ou plus une heure, pas de glissement de 10:00 local.
    assert_eq!(r[1].when_ms - r[0].when_ms, 7 * DAY_MS - HOUR_MS);
    let r = all("2026-10-20T00:00", "2026-11-01T00:00");
    assert_eq!(r[1].when_ms - r[0].when_ms, 7 * DAY_MS + HOUR_MS);
}
