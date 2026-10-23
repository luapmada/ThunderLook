//! Banc d'essai du lot F : fabrique une base réaliste (4 200 rendez-vous dont 400 séries, 10 000 contacts, 440 notes,
//! plus du courrier de remplissage qui rend le parcours de `message` coûteux comme sur une vraie base) puis chronomètre
//! `load`, `view::build` (semaine, mois), `contacts::load` (dossier, fiche) et `items::notes`.
//! Usage : `cargo run --release -p thunderlook-calendar --example bench -- [dossier] [courriers de remplissage]`
//! Les empreintes (`digest`) doivent être identiques avant et après une optimisation.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::Instant,
};

use chrono::NaiveDate;
use thunderlook_calendar::{
    bag::{self, Value},
    view,
    write::{self, Edit, Repeat, RepeatEnd},
};
use thunderlook_model::calendar::{CalendarQuery, CalendarViewKind};
use thunderlook_store::rusqlite::{Connection, params};

const APPOINTMENTS: usize = 4_200;
const SERIES: usize = 400;
const CONTACTS: usize = 10_000;
const NOTES: usize = 440;
const APPT: &str = "00062002-0000-0000-C000-000000000046";
const COMMON: &str = "00062008-0000-0000-C000-000000000046";
const MEETING: &str = "6ED8DA90-450B-101B-98DA-00AA003F1305";
const PUBLIC: &str = "00020329-0000-0000-C000-000000000046";
const ADDRESS: &str = "00062004-0000-0000-C000-000000000046";

/// Générateur congruentiel : mêmes données à chaque exécution.
struct Rng(u64);
impl Rng {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % n
    }
}

/// Table des propriétés nommées de la source (format de `named.rs`) et identifiants locaux attribués.
fn named_table() -> (Vec<u8>, impl Fn(&str, u32) -> u16, u16) {
    let mut entries: Vec<(&str, u32)> = Vec::new();
    for l in [0x8205, 0x8208, 0x820D, 0x820E, 0x8215, 0x8216, 0x8217, 0x8218, 0x8223] {
        entries.push((APPT, l));
    }
    entries.extend([(COMMON, 0x8501), (COMMON, 0x8503), (MEETING, 0x0023)]);
    for l in [0x8005, 0x8006, 0x8022, 0x802B, 0x8062, 0x8015, 0x8045, 0x8046, 0x8047, 0x8048, 0x8049, 0x8080, 0x8082, 0x8083, 0x8084] {
        entries.push((ADDRESS, l));
    }
    let mut raw = Vec::new();
    for (i, (guid, lid)) in entries.iter().enumerate() {
        raw.extend_from_slice(&(0x8000 + i as u16).to_le_bytes());
        raw.extend_from_slice(guid.as_bytes());
        raw.push(0);
        raw.extend_from_slice(&lid.to_le_bytes());
    }
    let keywords = 0x8000 + entries.len() as u16;
    raw.extend_from_slice(&keywords.to_le_bytes());
    raw.extend_from_slice(PUBLIC.as_bytes());
    raw.push(1);
    raw.extend_from_slice(&8u32.to_le_bytes());
    raw.extend_from_slice(b"Keywords");
    let lookup = move |g: &str, l: u32| 0x8000 + entries.iter().position(|e| *e == (g, l)).expect("propriété nommée") as u16;
    (raw, lookup, keywords)
}

fn folder(conn: &Connection, source: i64, nid: i64, name: &str, class: &str) -> i64 {
    conn.execute(
        "INSERT INTO folder (source_id, nid, name, role, container_class, hidden, position) VALUES (?1, ?2, ?3, 'folder', ?4, 0, ?2)",
        params![source, nid, name, class],
    )
    .unwrap();
    conn.last_insert_rowid()
}

#[allow(clippy::too_many_arguments)]
fn message(conn: &Connection, folder: i64, nid: i64, class: &str, subject: &str, from: &str, to: &str, props: &[u8], eml: &str) -> i64 {
    conn.execute(
        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, created_at, modified_at, received_at,
            message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
         VALUES (?1, ?2, ?3, ?4, ?5, '', ?6, '', 1, 1, ?2, 0, 1, 0, 1, 0, 0, 0, 'text', ?4, ?7, ?8)",
        params![folder, nid, class, subject, from, to, eml, props],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn main() -> Result<(), String> {
    let dir: PathBuf = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("thunderlook-bench"));
    let filler: usize = std::env::args().nth(2).and_then(|n| n.parse().ok()).unwrap_or(60_000);
    let _ = std::fs::remove_dir_all(&dir);
    let t = Instant::now();
    let (folders, contact_folder, cal_folder) = build(&dir, filler)?;
    let size = std::fs::metadata(dir.join(thunderlook_store::DB_FILE)).map_or(0, |m| m.len());
    println!("base fabriquée en {:.1} s : {} Mo, {filler} courriers de remplissage, dossiers {folders:?}", t.elapsed().as_secs_f64(), size >> 20);

    let conn = thunderlook_store::open_read_only(&dir).map_err(|e| e.to_string())?;
    let mut calendars = view::calendars(&conn)?;
    calendars.iter_mut().for_each(|c| c.shown = c.id == format!("c{cal_folder}"));
    let now = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap().and_hms_opt(10, 0, 0).unwrap();
    let query = |kind| CalendarQuery { kind, anchor: "2026-10-21".into(), start: None };

    let mut digest = DefaultHasher::new();
    let cal = bench("calendar::load", 3, || thunderlook_calendar::load(&conn).unwrap());
    println!("  rendez-vous {} ; illisibles {}", cal.appointments.len(), cal.issues.len());
    for a in &cal.appointments {
        (a.id, a.start_ms, a.end_ms, &a.subject, &a.location, &a.categories, a.reminder_minutes, a.recurrence.is_some()).hash(&mut digest);
    }
    for (name, kind) in [("view::build semaine", CalendarViewKind::Week), ("view::build mois", CalendarViewKind::Month)] {
        let v = bench(name, 5, || view::build(&cal, calendars.clone(), &query(kind), now));
        format!("{v:?}").hash(&mut digest);
        println!("  {} jours, {} rendez-vous", v.days.len(), v.days.iter().map(|d| d.all_day.len() + d.timed.len()).sum::<usize>());
    }
    let list = bench("contacts::load dossier", 3, || thunderlook_contacts::load(&conn, Some(contact_folder), None).unwrap());
    println!("  contacts {} ; illisibles {}", list.contacts.len(), list.issues.len());
    let shown = bench("contacts::view::list", 3, || thunderlook_contacts::view::list(&list, contact_folder));
    format!("{:?}", shown.rows.iter().map(|r| &r.id).collect::<Vec<_>>()).hash(&mut digest);
    for c in &list.contacts {
        format!("{c:?}").hash(&mut digest);
    }
    let id = list.contacts[list.contacts.len() / 2].id;
    let one = bench("contacts::load fiche (dossier + id)", 5, || thunderlook_contacts::load(&conn, Some(contact_folder), Some(id)).unwrap());
    let alone = bench("contacts::load fiche (id seul)", 5, || thunderlook_contacts::load(&conn, None, Some(id)).unwrap());
    assert_eq!(format!("{:?}", one.contacts), format!("{:?}", alone.contacts));
    format!("{:?}", alone.contacts).hash(&mut digest);
    let notes = bench("items::notes", 3, || thunderlook_items::notes(&conn, &dir).unwrap());
    println!("  notes {}", notes.rows.len());
    notes.rows.iter().for_each(|n| (&n.id, &n.title, &n.text).hash(&mut digest));
    println!("digest {:016x}", digest.finish());
    Ok(())
}

/// Meilleur de `n` passages, en millisecondes.
fn bench<T>(name: &str, n: usize, mut f: impl FnMut() -> T) -> T {
    let mut best = f64::MAX;
    let mut last = None;
    for _ in 0..n {
        let t = Instant::now();
        let r = f();
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
        last = Some(r);
    }
    println!("{name:<40} {best:>9.1} ms (meilleur de {n})");
    last.unwrap()
}

fn build(dir: &Path, filler: usize) -> Result<((i64, i64, i64, i64), i64, i64), String> {
    let mut conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA synchronous = OFF;").map_err(|e| e.to_string())?;
    let (raw, id, keywords) = named_table();
    conn.execute(
        "INSERT INTO source (kind, origin_path, work_path, display_name, size, imported_at, props) VALUES ('pst', 'bench.pst', 'bench.pst', 'Bench', 0, 1, ?1)",
        [&raw],
    )
    .map_err(|e| e.to_string())?;
    let source = conn.last_insert_rowid();
    let (mail, cal, contacts, notes) = {
        let c = &conn;
        (
            folder(c, source, 1, "Boîte de réception", "IPF.Note"),
            folder(c, source, 2, "Calendrier", "IPF.Appointment"),
            folder(c, source, 3, "Contacts", "IPF.Contact"),
            folder(c, source, 4, "Notes", "IPF.StickyNote"),
        )
    };
    let mut rng = Rng(42);
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    // Courrier de remplissage : sacs d'environ 1,5 Ko, comme les en-têtes et propriétés d'un vrai courriel.
    for i in 0..filler {
        let pad: String = (0..700).map(|_| char::from(b'a' + rng.next(26) as u8)).collect();
        let props = bag::merge(&[], &[(0x0037, Some(Value::Unicode(format!("Courriel {i}")))), (0x1000, Some(Value::Unicode(pad)))])?;
        message(&tx, mail, 100 + i as i64, "IPM.Note", &format!("Courriel {i}"), "Expéditeur", "dest@exemple.fr", &props, "");
    }

    // Rendez-vous simples (sacs construits avec `bag::merge`).
    let categories = ["Client", "Perso", "Atelier", "Déplacement"];
    for i in 0..APPOINTMENTS - SERIES {
        let day = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap() + chrono::Duration::days(rng.next(1_100) as i64);
        let hour = 8 + rng.next(10) as i64;
        let start = day.and_hms_opt(hour as u32, 0, 0).unwrap().and_utc().timestamp_millis() - 3_600_000;
        let end = start + (30 + 30 * rng.next(4) as i64) * 60_000;
        let mut set = vec![
            (id(APPT, 0x8205), Some(Value::Long(2))),
            (id(APPT, 0x8208), Some(Value::Unicode(format!("Salle {}", rng.next(20))))),
            (id(APPT, 0x820D), Some(Value::time_ms(start))),
            (id(APPT, 0x820E), Some(Value::time_ms(end))),
            (id(APPT, 0x8215), Some(Value::Bool(false))),
            (id(APPT, 0x8223), Some(Value::Bool(false))),
            (id(APPT, 0x8217), Some(Value::Long(i64::from(rng.next(2) == 0) as i32))),
            (id(APPT, 0x8218), Some(Value::Long(0))),
            (id(COMMON, 0x8503), Some(Value::Bool(true))),
            (id(COMMON, 0x8501), Some(Value::Long(15))),
            (id(MEETING, 0x0023), Some(Value::Binary((0..48).map(|k| (i + k) as u8).collect()))),
            (keywords, Some(Value::Strings(vec![categories[rng.next(4) as usize].to_string()]))),
            (0x0060, Some(Value::time_ms(start))),
            (0x0061, Some(Value::time_ms(end))),
        ];
        set.sort_by_key(|(k, _)| *k);
        let props = bag::merge(&[], &set)?;
        message(&tx, cal, 1_000 + i as i64, "IPM.Appointment", &format!("Rendez-vous {i}"), "Organisateur", "invité@exemple.fr", &props, "");
    }
    tx.commit().map_err(|e| e.to_string())?;

    // Séries : seule `write::create` sait encoder la périodicité ([MS-OXOCAL]) ; elle passe par `bag::merge` aussi.
    let freqs = [thunderlook_calendar::recur::Frequency::Daily, thunderlook_calendar::recur::Frequency::Weekly, thunderlook_calendar::recur::Frequency::Monthly];
    for i in 0..SERIES {
        let day = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap() + chrono::Duration::days(rng.next(900) as i64);
        let edit = Edit {
            subject: format!("Série {i}"),
            location: "Bureau".into(),
            start: format!("{day}T09:00"),
            end: format!("{day}T10:00"),
            reminder: Some(15),
            busy: 2,
            categories: "Client".into(),
            repeat: Some(Repeat { frequency: freqs[i % 3], interval: 1 + (i % 2) as u32, end: if i % 4 == 0 { RepeatEnd::Never } else { RepeatEnd::Count(30) } }),
            ..Edit::default()
        };
        write::create(&mut conn, dir, cal, &edit)?;
    }

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for i in 0..CONTACTS {
        let (given, sur) = (format!("Prénom{}", rng.next(900)), format!("Nom{i}"));
        let set = vec![
            (0x3001, Some(Value::Unicode(format!("{given} {sur}")))),
            (0x3A06, Some(Value::Unicode(given.clone()))),
            (0x3A11, Some(Value::Unicode(sur.clone()))),
            (0x3A16, Some(Value::Unicode(format!("Société {}", rng.next(800))))),
            (0x3A17, Some(Value::Unicode("Gérant".into()))),
            (0x3A08, Some(Value::Unicode(format!("05 46 {:02} {:02} {:02}", rng.next(100), rng.next(100), rng.next(100))))),
            (0x3A1C, Some(Value::Unicode(format!("06 {:02} {:02} {:02} {:02}", rng.next(100), rng.next(100), rng.next(100), rng.next(100))))),
            (id(ADDRESS, 0x8005), Some(Value::Unicode(format!("{sur}, {given}")))),
            (id(ADDRESS, 0x8083), Some(Value::Unicode(format!("{given}.{sur}@exemple.fr").to_lowercase()))),
            (id(ADDRESS, 0x8082), Some(Value::Unicode("SMTP".into()))),
            (id(ADDRESS, 0x8080), Some(Value::Unicode(format!("{given} {sur}")))),
            (id(ADDRESS, 0x8045), Some(Value::Unicode(format!("{} rue des Lilas", rng.next(99) + 1)))),
            (id(ADDRESS, 0x8046), Some(Value::Unicode("Saintes".into()))),
            (id(ADDRESS, 0x8048), Some(Value::Unicode("17100".into()))),
            (keywords, Some(Value::Strings(vec!["Client".into()]))),
        ];
        let mut set = set;
        set.sort_by_key(|(k, _)| *k);
        let props = bag::merge(&[], &set)?;
        message(&tx, contacts, 10_000 + i as i64, "IPM.Contact", &format!("{given} {sur}"), "", "", &props, "");
    }
    for i in 0..NOTES {
        let rel = format!("eml/{source}/n/{i}.eml");
        let path = thunderlook_store::resolve(dir, &rel);
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&path, format!("MIME-Version: 1.0\r\nSubject: Note {i}\r\nContent-Type: text/plain; charset=\"utf-8\"\r\n\r\nTexte de la note {i}\r\nsur deux lignes\r\n")).map_err(|e| e.to_string())?;
        let props = bag::merge(&[], &[(0x0037, Some(Value::Unicode(format!("Note {i}"))))])?;
        message(&tx, notes, 20_000 + i as i64, "IPM.StickyNote", &format!("Note {i}"), "", "", &props, &rel);
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(((mail, cal, contacts, notes), contacts, cal))
}
