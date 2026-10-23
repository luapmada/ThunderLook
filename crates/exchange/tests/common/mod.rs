//! Banc commun : une vraie base de test (une source, un dossier par type) et des aides de lecture/écriture.
#![allow(dead_code)]

use std::{fs, path::PathBuf};

use thunderlook_calendar::write::Edit;
use thunderlook_exchange::{ChangeKind, ItemKind};
use thunderlook_model::{contacts::ContactForm, items::{NoteForm, TaskForm}};
use thunderlook_store::rusqlite::Connection;

const APPOINTMENT: &str = "00062002-0000-0000-C000-000000000046";
const COMMON: &str = "00062008-0000-0000-C000-000000000046";
const TASK: &str = "00062003-0000-0000-C000-000000000046";
const NOTE: &str = "0006200E-0000-0000-C000-000000000046";
const ADDRESS: &str = "00062004-0000-0000-C000-000000000046";
const PUBLIC_STRINGS: &str = "00020329-0000-0000-C000-000000000046";

pub struct World {
    pub dir: PathBuf,
    pub out: PathBuf,
    pub conn: Connection,
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
        let _ = fs::remove_dir_all(&self.out);
    }
}

/// Base de test : une source dont la table de noms couvre contacts, tâches, notes et rendez-vous ; un dossier par type.
pub fn world(name: &str) -> World {
    let dir = std::env::temp_dir().join(format!("tl-exchange-{name}-{}", std::process::id()));
    let out = std::env::temp_dir().join(format!("tl-exchange-out-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&out);
    let conn = thunderlook_store::open(&dir).unwrap();
    let mut names = Vec::new();
    let mut next = 0x8000u16;
    let mut lid = |names: &mut Vec<u8>, set: &str, lid: u32| {
        names.extend_from_slice(&next.to_le_bytes());
        names.extend_from_slice(set.as_bytes());
        names.push(0);
        names.extend_from_slice(&lid.to_le_bytes());
        next += 1;
    };
    for l in [0x820D, 0x820E, 0x8215, 0x8208, 0x8205, 0x8223, 0x8216, 0x8217, 0x8218] {
        lid(&mut names, APPOINTMENT, l);
    }
    for l in [0x8501, 0x8503, 0x8502] {
        lid(&mut names, COMMON, l);
    }
    for l in [0x8101, 0x8102, 0x8104, 0x8105, 0x810F, 0x811C, 0x8116, 0x8126, 0x8109] {
        lid(&mut names, TASK, l);
    }
    lid(&mut names, NOTE, 0x8B00);
    lid(&mut names, ADDRESS, 0x8015);
    names.extend_from_slice(&next.to_le_bytes());
    names.extend_from_slice(PUBLIC_STRINGS.as_bytes());
    names.push(1);
    names.extend_from_slice(&8u32.to_le_bytes());
    names.extend_from_slice(b"Keywords");
    conn.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at, props) VALUES (1, 'tl', '', '', 't', 0, 0, ?1)", [names]).unwrap();
    for (id, name, class) in [(1, "Contacts", "IPF.Contact"), (2, "Tâches", "IPF.Task"), (3, "Notes", "IPF.StickyNote"), (4, "Calendrier", "IPF.Appointment")] {
        conn.execute("INSERT INTO folder (id, source_id, nid, name, role, hidden, position, container_class) VALUES (?1, 1, ?1, ?2, 'folder', 0, ?1, ?3)", thunderlook_store::rusqlite::params![id, name, class]).unwrap();
    }
    World { dir, out, conn }
}

pub fn contact(given: &str, surname: &str, mobile: &str) -> ContactForm {
    ContactForm { given_name: given.into(), surname: surname.into(), email1: format!("{}@exemple.fr", given.to_lowercase()), mobile: mobile.into(), company: "ADI".into(), ..Default::default() }
}

/// Deux éléments de chaque sorte, dont une série de rendez-vous (qui ne se relit pas).
pub fn fill(w: &mut World) {
    let (dir, conn) = (w.dir.clone(), &mut w.conn);
    thunderlook_contacts::write::create(conn, &dir, 1, &contact("Léa", "Martin", "0612345678")).unwrap();
    thunderlook_contacts::write::create(conn, &dir, 1, &contact("Paul", "Durand", "0687654321")).unwrap();
    for (s, due) in [("Appeler le comptable", "2026-11-03"), ("Facturer octobre", "2026-10-31")] {
        let f = TaskForm { folder_id: "f2".into(), subject: s.into(), due: due.into(), importance: 1, ..Default::default() };
        thunderlook_items::task_save(conn, &dir, &f).unwrap();
    }
    for body in ["Courses\npain, lait", "Code du portail\n1234"] {
        thunderlook_items::note_save(conn, &dir, &NoteForm { folder_id: "f3".into(), body: body.into(), color: 3, id: None }).unwrap();
    }
    let edit = |s: &str, start: &str, end: &str| Edit { subject: s.into(), location: "Salle A".into(), start: start.into(), end: end.into(), busy: 2, ..Default::default() };
    thunderlook_calendar::write::create(conn, &dir, 4, &edit("Réunion client", "2026-10-27T14:00", "2026-10-27T15:00")).unwrap();
    thunderlook_calendar::write::create(conn, &dir, 4, &edit("Dentiste", "2026-10-28T09:00", "2026-10-28T09:30")).unwrap();
    let series = Edit { repeat: Some(thunderlook_calendar::write::Repeat { frequency: thunderlook_calendar::recur::Frequency::Weekly, interval: 1, end: thunderlook_calendar::write::RepeatEnd::Count(4) }), ..edit("Point d'équipe", "2026-10-26T10:00", "2026-10-26T10:30") };
    thunderlook_calendar::write::create(conn, &dir, 4, &series).unwrap();
}

pub fn read(w: &World, name: &str) -> String {
    fs::read_to_string(w.out.join(name)).unwrap()
}

pub fn write(w: &World, name: &str, text: &str) {
    fs::write(w.out.join(name), text).unwrap();
}

pub fn kinds(rep: &thunderlook_exchange::ImportReport) -> Vec<(ChangeKind, ItemKind)> {
    rep.changes.iter().map(|c| (c.kind, c.item)).collect()
}

pub fn contacts_of(w: &World) -> Vec<ContactForm> {
    let all = thunderlook_contacts::load(&w.conn, None, None).unwrap();
    all.contacts.iter().map(|c| thunderlook_contacts::write::form_from(c, &w.dir)).collect()
}

