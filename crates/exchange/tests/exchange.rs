//! Échange avec le téléphone, de bout en bout sur une vraie base : export, relecture, changements, application, annulation.

use std::{fs, path::PathBuf};

use thunderlook_calendar::write::Edit;
use thunderlook_exchange::{ChangeKind, ItemKind, Payload, apply, export_all, import_changes, last_undo, modified_files, read_manifest, undo};
use thunderlook_model::{contacts::ContactForm, items::{NoteForm, TaskForm}};
use thunderlook_store::rusqlite::Connection;

const APPOINTMENT: &str = "00062002-0000-0000-C000-000000000046";
const COMMON: &str = "00062008-0000-0000-C000-000000000046";
const TASK: &str = "00062003-0000-0000-C000-000000000046";
const NOTE: &str = "0006200E-0000-0000-C000-000000000046";
const ADDRESS: &str = "00062004-0000-0000-C000-000000000046";
const PUBLIC_STRINGS: &str = "00020329-0000-0000-C000-000000000046";

struct World {
    dir: PathBuf,
    out: PathBuf,
    conn: Connection,
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
        let _ = fs::remove_dir_all(&self.out);
    }
}

/// Base de test : une source dont la table de noms couvre contacts, tâches, notes et rendez-vous ; un dossier par type.
fn world(name: &str) -> World {
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

fn contact(given: &str, surname: &str, mobile: &str) -> ContactForm {
    ContactForm { given_name: given.into(), surname: surname.into(), email1: format!("{}@exemple.fr", given.to_lowercase()), mobile: mobile.into(), company: "ADI".into(), ..Default::default() }
}

/// Deux éléments de chaque sorte, dont une série de rendez-vous (qui ne se relit pas).
fn fill(w: &mut World) {
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

fn read(w: &World, name: &str) -> String {
    fs::read_to_string(w.out.join(name)).unwrap()
}

fn write(w: &World, name: &str, text: &str) {
    fs::write(w.out.join(name), text).unwrap();
}

fn kinds(rep: &thunderlook_exchange::ImportReport) -> Vec<(ChangeKind, ItemKind)> {
    rep.changes.iter().map(|c| (c.kind, c.item)).collect()
}

fn contacts_of(w: &World) -> Vec<ContactForm> {
    let all = thunderlook_contacts::load(&w.conn, None, None).unwrap();
    all.contacts.iter().map(|c| thunderlook_contacts::write::form_from(c, &w.dir)).collect()
}

#[test]
fn export_ecrit_les_quatre_fichiers_et_un_manifeste_exact() {
    let mut w = world("export");
    fill(&mut w);
    let rep = export_all(&w.conn, &w.dir, &w.out).unwrap();
    let names: Vec<&str> = rep.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["contacts.vcf", "taches.ics", "notes.vnt", "calendrier.ics"]);
    assert_eq!(rep.files.iter().map(|f| f.count).collect::<Vec<_>>(), [2, 2, 2, 3]);
    assert_eq!(read_manifest(&w.out).unwrap(), rep);
    assert!(read(&w, "contacts.vcf").contains("VERSION:3.0"));
    // Écriture atomique : aucun fichier temporaire ne traîne.
    assert!(fs::read_dir(&w.out).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
    // Les séries ne sont jamais proposées à la suppression.
    assert_eq!(rep.file("calendrier.ics").unwrap().ids.len(), 2);
}

#[test]
fn export_puis_import_sans_modification_ne_donne_aucun_changement() {
    let mut w = world("noop");
    fill(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.changes.is_empty(), "{:?}", rep.changes);

    // Même contenu, mais sans le raccourci du manifeste : la comparaison élément par élément donne aussi zéro.
    fs::remove_file(w.out.join("manifest.json")).unwrap();
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.changes.is_empty(), "{:?}", rep.changes);
    assert_eq!(rep.unchanged, 2 + 2 + 2 + 2);
    assert_eq!(rep.warnings.len(), 1, "la série est signalée : {:?}", rep.warnings);
}

#[test]
fn un_contact_modifie_dans_le_vcf_donne_un_seul_changement() {
    let mut w = world("modif");
    fill(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let vcf = read(&w, "contacts.vcf");
    assert!(vcf.contains("0612345678"));
    write(&w, "contacts.vcf", &vcf.replace("0612345678", "0699999999"));

    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert_eq!(kinds(&rep), [(ChangeKind::Modified, ItemKind::Contact)]);
    let c = &rep.changes[0];
    assert!(c.summary.contains("Léa Martin") && c.summary.contains("mobile"), "{}", c.summary);
    assert!(c.default_selected());
    // Import sans rien écrire.
    assert!(contacts_of(&w).iter().any(|f| f.mobile == "0612345678"));
    assert!(last_undo(&w.dir).is_none());

    let mut w = w;
    let done = apply(&mut w.conn, &w.dir.clone(), &rep.changes).unwrap();
    assert_eq!((done.applied, done.errors.len()), (1, 0));
    let after = contacts_of(&w);
    assert!(after.iter().any(|f| f.mobile == "0699999999") && after.iter().all(|f| f.mobile != "0612345678"));
    // Appliqué : plus rien à proposer.
    assert!(import_changes(&w.conn, &w.dir, &w.out).unwrap().changes.is_empty());

    // Annulation : l'ancien numéro revient.
    let ts = last_undo(&w.dir).unwrap();
    assert_eq!(done.undo.as_deref(), Some(ts.as_str()));
    assert_eq!(undo(&mut w.conn, &w.dir.clone(), &ts).unwrap(), 1);
    assert!(contacts_of(&w).iter().any(|f| f.mobile == "0612345678"));
    assert!(last_undo(&w.dir).is_none());
}

#[test]
fn une_suppression_est_proposee_jamais_appliquee_d_office() {
    let mut w = world("suppr");
    fill(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    // Paul disparaît du fichier.
    let vcf = read(&w, "contacts.vcf");
    let kept: Vec<&str> = vcf.split("BEGIN:VCARD\r\n").filter(|b| !b.is_empty() && !b.contains("Durand")).collect();
    write(&w, "contacts.vcf", &kept.iter().map(|b| format!("BEGIN:VCARD\r\n{b}")).collect::<String>());

    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert_eq!(kinds(&rep), [(ChangeKind::Deleted, ItemKind::Contact)]);
    assert!(!rep.changes[0].default_selected());
    assert!(rep.changes[0].summary.contains("Durand"));

    // L'utilisateur n'accepte rien : rien ne bouge, rien n'est consigné.
    let mut w = w;
    let dir = w.dir.clone();
    let none = apply(&mut w.conn, &dir, &[]).unwrap();
    assert_eq!((none.applied, none.undo), (0, None));
    assert_eq!(contacts_of(&w).len(), 2);

    // Acceptée explicitement : supprimée, puis rétablie par l'annulation.
    let done = apply(&mut w.conn, &dir, &rep.changes).unwrap();
    assert_eq!(done.applied, 1);
    assert_eq!(contacts_of(&w).len(), 1);
    assert_eq!(undo(&mut w.conn, &dir, &done.undo.unwrap()).unwrap(), 1);
    let back = contacts_of(&w);
    assert_eq!(back.len(), 2);
    assert!(back.iter().any(|f| f.surname == "Durand" && f.mobile == "0687654321"));
}

#[test]
fn un_element_cree_depuis_l_export_n_est_pas_une_suppression() {
    let mut w = world("recent");
    fill(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let dir = w.dir.clone();
    thunderlook_contacts::write::create(&mut w.conn, &dir, 1, &contact("Nina", "Roche", "0600000000")).unwrap();
    // Le fichier a changé (autre contact modifié côté téléphone) mais ne connaît pas Nina : pas de suppression proposée.
    write(&w, "contacts.vcf", &read(&w, "contacts.vcf").replace("0612345678", "0611111111"));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert_eq!(kinds(&rep), [(ChangeKind::Modified, ItemKind::Contact)]);
}

#[test]
fn un_nouveau_contact_du_telephone_est_reconnu_ensuite_par_son_identifiant() {
    let mut w = world("nouveau");
    fill(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let card = "BEGIN:VCARD\r\nVERSION:3.0\r\nN:Blanc;Zoé;;;\r\nFN:Zoé Blanc\r\nTEL;TYPE=CELL,VOICE:0655555555\r\nX-OUTLOOK-UID:000000AB12CD\r\nEND:VCARD\r\n";
    write(&w, "contacts.vcf", &format!("{}{card}", read(&w, "contacts.vcf")));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert_eq!(kinds(&rep), [(ChangeKind::New, ItemKind::Contact)]);
    assert_eq!(rep.changes[0].ext_uid.as_deref(), Some("000000AB12CD"));

    let mut w = w;
    let dir = w.dir.clone();
    let done = apply(&mut w.conn, &dir, &rep.changes).unwrap();
    assert_eq!((done.applied, done.errors.len()), (1, 0));
    assert!(contacts_of(&w).iter().any(|f| f.surname == "Blanc" && f.mobile == "0655555555"));
    // Le même fichier ne propose plus rien : l'identifiant de MPE est retenu.
    assert!(import_changes(&w.conn, &w.dir, &w.out).unwrap().changes.is_empty());
    // Annuler supprime le contact créé.
    undo(&mut w.conn, &dir, &done.undo.unwrap()).unwrap();
    assert_eq!(contacts_of(&w).len(), 2);
}

#[test]
fn taches_notes_et_rendez_vous_modifies() {
    let mut w = world("autres");
    fill(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    write(&w, "taches.ics", &read(&w, "taches.ics").replace("SUMMARY:Facturer octobre", "SUMMARY:Facturer octobre et novembre"));
    write(&w, "notes.vnt", &read(&w, "notes.vnt").replace("1234", "9876"));
    write(&w, "calendrier.ics", &read(&w, "calendrier.ics").replace("SUMMARY;LANGUAGE=fr:Dentiste", "SUMMARY;LANGUAGE=fr:Dentiste (déplacé)"));

    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    let mut got = kinds(&rep);
    got.sort_by_key(|(_, i)| *i as u8);
    assert_eq!(got, [(ChangeKind::Modified, ItemKind::Task), (ChangeKind::Modified, ItemKind::Note), (ChangeKind::Modified, ItemKind::Event)], "{:?}", rep.changes);

    let mut w = w;
    let dir = w.dir.clone();
    let done = apply(&mut w.conn, &dir, &rep.changes).unwrap();
    assert_eq!((done.applied, done.errors.len()), (3, 0), "{:?}", done.errors);
    assert!(thunderlook_items::tasks(&w.conn).unwrap().rows.iter().any(|r| r.title == "Facturer octobre et novembre"));
    assert!(thunderlook_items::notes(&w.conn, &dir).unwrap().rows.iter().any(|r| r.text.contains("9876")));
    let cal = thunderlook_calendar::load(&w.conn).unwrap();
    assert!(cal.appointments.iter().any(|a| a.subject == "Dentiste (déplacé)"));
    assert!(import_changes(&w.conn, &w.dir, &w.out).unwrap().changes.is_empty());

    assert_eq!(undo(&mut w.conn, &dir, &done.undo.unwrap()).unwrap(), 3);
    assert!(thunderlook_items::tasks(&w.conn).unwrap().rows.iter().any(|r| r.title == "Facturer octobre"));
    assert!(thunderlook_items::notes(&w.conn, &dir).unwrap().rows.iter().any(|r| r.text.contains("1234")));
    assert!(thunderlook_calendar::load(&w.conn).unwrap().appointments.iter().any(|a| a.subject == "Dentiste"));
}

#[test]
fn les_charges_utiles_portent_l_identifiant_local() {
    let mut w = world("payload");
    fill(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    write(&w, "contacts.vcf", &read(&w, "contacts.vcf").replace("0612345678", "0611111111"));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    let Payload::Contact(f) = &rep.changes[0].payload else { panic!("pas un contact") };
    assert_eq!((f.id.as_deref(), f.mobile.as_str(), f.company.as_str()), (Some(rep.changes[0].id.as_str()), "0611111111", "ADI"));
}

#[test]
fn fichiers_modifies_depuis_l_export() {
    let mut w = world("modifies");
    fill(&mut w);
    assert!(modified_files(&w.out).is_empty(), "pas de manifeste : rien à protéger");
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    assert!(modified_files(&w.out).is_empty());
    write(&w, "notes.vnt", &read(&w, "notes.vnt").replace("1234", "9876"));
    assert_eq!(modified_files(&w.out), ["notes.vnt"]);
    fs::remove_file(w.out.join("taches.ics")).unwrap();
    assert_eq!(modified_files(&w.out), ["taches.ics", "notes.vnt"]);
}
