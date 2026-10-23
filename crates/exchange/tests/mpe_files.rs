//! Lot AI : lecture en lecture seule de `Phonebook.dat`, `Organizer.dat` et `Notes.dat` de MPE. Fichiers SYNTHÉTIQUES au
//! format décrit dans `docs/mpe-fichiers.md` § 1.5 : aucune donnée réelle.

use std::{collections::BTreeMap, fs, path::{Path, PathBuf}, time::Duration};

use thunderlook_exchange::{ChangeKind, ImportReport, ItemKind, apply, last_undo, mpe_files::{self, MpeOptions}, undo};

mod common;
use common::*;

/// 2026-10-23 12:00 UTC.
const NOW_MS: i64 = 1_792_756_800_000;

fn opts() -> MpeOptions {
    MpeOptions { settle: Duration::ZERO, now_ms: Some(NOW_MS), ..Default::default() }
}

fn device(w: &World) -> PathBuf {
    let d = w.out.join("SM-G781B");
    fs::create_dir_all(&d).unwrap();
    d
}

fn put(d: &Path, name: &str, text: &str) {
    fs::write(d.join(name), text).unwrap();
}

fn vcard(luid: u32, given: &str, surname: &str, mobile: &str) -> String {
    format!(
        "BEGIN:VCARD\r\nVERSION:2.1\r\nN:{surname};{given};;;\r\nFN:{given} {surname}\r\nTEL;CELL:{mobile}\r\nX-IRMC-LUID:{luid}\r\nOUTLOOK-UID:00000000AA{luid:04X}\r\nEND:VCARD\r\n",
    )
}

fn vnote(luid: u32, body: &str) -> String {
    format!("BEGIN:VNOTE\r\nVERSION:1.1\r\nBODY;CHARSET=UTF-8:{body}\r\nX-IRMC-LUID:{luid}\r\nX-OUTLOOK-UID:00000000BB{luid:04X}\r\nEND:VNOTE\r\n")
}

fn vevent(luid: u32, subject: &str, day: &str) -> String {
    format!(
        "BEGIN:VEVENT\r\nX-IRMC-LUID:{luid}\r\nX-OUTLOOK-UID:00000000CC{luid:04X}\r\nDTSTART:{day}T090000Z\r\nDTEND:{day}T100000Z\r\nSUMMARY:{subject}\r\nEND:VEVENT\r\n"
    )
}

fn organizer(events: &[String]) -> String {
    format!("BEGIN:VCALENDAR\r\nVERSION:1.0\r\nPRODID:-//Synthetique//FR\r\n{}END:VCALENDAR\r\n", events.concat())
}

struct Phone {
    contacts: Vec<String>,
    notes: Vec<String>,
    events: Vec<String>,
}

impl Phone {
    fn initial() -> Phone {
        Phone {
            contacts: vec![vcard(101, "Léa", "Martin", "0612345678"), vcard(102, "Paul", "Durand", "0687654321")],
            notes: vec![vnote(201, "Courses pain lait"), vnote(202, "Code portail 1234")],
            events: vec![vevent(301, "Réunion client", "20261027"), vevent(302, "Dentiste", "20261028"), vevent(303, "Salon lointain", "20270615")],
        }
    }

    fn write(&self, d: &Path) {
        put(d, "Phonebook.dat", &self.contacts.concat());
        put(d, "Notes.dat", &self.notes.concat());
        put(d, "Organizer.dat", &organizer(&self.events));
    }
}

/// Contenu (octets) de tous les fichiers du dossier : doit rester identique, MPE ne doit rien voir.
fn snapshot(d: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(d).unwrap().flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), fs::read(e.path()).unwrap())).collect()
}

fn check(w: &World, d: &Path) -> ImportReport {
    mpe_files::import_changes_mpe(&w.conn, &w.dir, d, &opts()).unwrap_or_else(|e| panic!("{e}"))
}

fn all_changes(rep: &ImportReport) -> Vec<&thunderlook_exchange::Change> {
    rep.changes.iter().chain(&rep.conflicts).collect()
}

/// Importe et applique tout ce qui est proposé (conflits : on garde le téléphone), comme le ferait l'utilisateur.
fn import_all(w: &mut World, d: &Path) -> usize {
    let rep = check(w, d);
    let mut chosen = rep.changes.clone();
    for mut c in rep.conflicts.clone() {
        c.resolution = Some(thunderlook_exchange::Resolution::KeepPhone);
        chosen.push(c);
    }
    let dir = w.dir.clone();
    apply(&mut w.conn, &dir, &chosen).unwrap().applied
}

fn first_import(name: &str) -> (World, PathBuf, Phone) {
    let mut w = world(name);
    let d = device(&w);
    let phone = Phone::initial();
    phone.write(&d);
    let rep = check(&w, &d);
    assert_eq!(rep.changes.iter().filter(|c| c.kind == ChangeKind::New && c.item == ItemKind::Contact).count(), 2, "{:?}", rep.warnings);
    assert_eq!(rep.changes.iter().filter(|c| c.kind == ChangeKind::New && c.item == ItemKind::Note).count(), 2);
    // « Salon lointain » (juin 2027) est hors de la plage −90 / +90 jours : ni proposé ni appliqué.
    let events: Vec<_> = rep.changes.iter().filter(|c| c.item == ItemKind::Event).map(|c| c.summary.clone()).collect();
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(rep.warnings.iter().any(|m| m.contains("hors de la plage")), "{:?}", rep.warnings);
    assert!(import_all(&mut w, &d) >= 6);
    (w, d, phone)
}

#[test]
fn liste_les_appareils_sans_rien_ecrire() {
    let w = world("mpe-list");
    let d = device(&w);
    Phone::initial().write(&d);
    put(&w.out, "autre.txt", "pas un appareil");
    fs::create_dir_all(w.out.join("Vide")).unwrap();
    let before = snapshot(&d);
    let devices = mpe_files::list_devices(&w.out).unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].name, "SM-G781B");
    assert_eq!(devices[0].files, ["Phonebook.dat", "Organizer.dat", "Notes.dat"]);
    // Le dossier d'un appareil choisi directement est reconnu aussi.
    assert_eq!(mpe_files::list_devices(&d).unwrap().len(), 1);
    assert_eq!(snapshot(&d), before);
}

#[test]
fn deuxieme_lecture_sans_changement_et_dossier_intact() {
    let (w, d, _) = first_import("mpe-deuxieme_lectu");
    let before = snapshot(&d);
    let rep = check(&w, &d);
    assert!(all_changes(&rep).is_empty(), "{:?}", all_changes(&rep).iter().map(|c| (&c.kind, &c.summary)).collect::<Vec<_>>());
    assert!(rep.unchanged >= 6);
    assert_eq!(snapshot(&d), before, "le dossier de MPE a été touché");
    // L'identifiant du téléphone est mémorisé sous un nom propre à l'appareil.
    let map = fs::read_to_string(w.dir.join("exchange-map.json")).unwrap();
    assert!(map.contains("mpe-SM_G781B-101") && map.contains("mpe-SM_G781B-201") && map.contains("mpe-SM_G781B-301"), "{map}");
}

#[test]
fn element_ajoute_sur_le_telephone() {
    let (w, d, mut phone) = first_import("mpe-element_ajoute");
    phone.contacts.push(vcard(103, "Zoé", "Petit", "0699887766"));
    phone.notes.push(vnote(203, "Idée du soir"));
    phone.events.push(vevent(304, "Garage", "20261105"));
    phone.write(&d);
    let rep = check(&w, &d);
    assert_eq!(kinds(&rep).len(), 3, "{:?}", rep.changes.iter().map(|c| &c.summary).collect::<Vec<_>>());
    assert!(rep.changes.iter().all(|c| c.kind == ChangeKind::New && c.default_selected() || c.item == ItemKind::Note));
    assert!(rep.conflicts.is_empty());
}

#[test]
fn element_modifie_sur_le_telephone() {
    let (mut w, d, mut phone) = first_import("mpe-element_modifi");
    phone.contacts[0] = vcard(101, "Léa", "Martin", "0600000000");
    phone.notes[0] = vnote(201, "Courses pain lait beurre");
    phone.events[0] = vevent(301, "Réunion client (reportée)", "20261027");
    phone.write(&d);
    let rep = check(&w, &d);
    // Note : le téléphone est la source, modification cochée d'office.
    let note: Vec<_> = rep.changes.iter().filter(|c| c.item == ItemKind::Note).collect();
    assert_eq!((note.len(), note[0].kind, note[0].default_selected()), (1, ChangeKind::Modified, true), "{:?}", rep.changes.iter().map(|c| &c.summary).collect::<Vec<_>>());
    // Contact et rendez-vous : valeur exportée inconnue, donc conflit à trancher, jamais coché ni appliqué d'office.
    assert_eq!(rep.conflicts.iter().filter(|c| c.item == ItemKind::Contact).count(), 1);
    assert!(rep.conflicts.iter().all(|c| !c.default_selected()));
    assert!(rep.changes.iter().all(|c| c.item != ItemKind::Contact));
    // Appliquer le téléphone, puis annuler.
    assert_eq!(import_all(&mut w, &d), 3);
    assert!(contacts_of(&w).iter().any(|c| c.given_name == "Léa" && c.mobile == "0600000000"));
    let stamp = last_undo(&w.dir).unwrap();
    let dir = w.dir.clone();
    undo(&mut w.conn, &dir, &stamp).unwrap();
    assert!(contacts_of(&w).iter().any(|c| c.given_name == "Léa" && c.mobile == "0612345678"));
}

#[test]
fn element_supprime_sur_le_telephone() {
    let (mut w, d, mut phone) = first_import("mpe-element_suppri");
    phone.contacts.remove(1);
    phone.notes.remove(1);
    phone.events.remove(1); // Dentiste, dans la plage.
    phone.write(&d);
    let rep = check(&w, &d);
    let deleted: Vec<_> = rep.changes.iter().filter(|c| c.kind == ChangeKind::Deleted).collect();
    assert_eq!(deleted.len(), 3, "{:?}", rep.changes.iter().map(|c| (&c.kind, &c.summary)).collect::<Vec<_>>());
    assert!(deleted.iter().all(|c| !c.default_selected()), "une suppression ne doit jamais être cochée d'office");
    assert!(rep.changes.iter().all(|c| c.kind == ChangeKind::Deleted));
    // Accepter la suppression du contact seulement : le reste est intact, et c'est annulable.
    let one: Vec<_> = deleted.iter().filter(|c| c.item == ItemKind::Contact).map(|c| (*c).clone()).collect();
    let dir = w.dir.clone();
    apply(&mut w.conn, &dir, &one).unwrap();
    assert_eq!(contacts_of(&w).len(), 1);
    let stamp = last_undo(&w.dir).unwrap();
    undo(&mut w.conn, &dir, &stamp).unwrap();
    assert_eq!(contacts_of(&w).len(), 2);
}

#[test]
fn rendez_vous_hors_plage_jamais_supprime() {
    let mut w = world("mpe-range");
    let d = device(&w);
    // Premier import avec une plage large : le rendez-vous de juin 2027 est relié.
    let mut phone = Phone::initial();
    phone.write(&d);
    let wide = MpeOptions { range_days: 400, ..opts() };
    let rep = mpe_files::import_changes_mpe(&w.conn, &w.dir, &d, &wide).unwrap();
    assert_eq!(rep.changes.iter().filter(|c| c.item == ItemKind::Event).count(), 3);
    let dir = w.dir.clone();
    apply(&mut w.conn, &dir, &rep.changes).unwrap();
    // MPE ne l'a plus dans son fichier (hors de sa plage) : avec la plage normale, aucune suppression n'est proposée.
    phone.events.remove(2);
    phone.write(&d);
    let rep = check(&w, &d);
    assert!(rep.changes.iter().all(|c| !(c.item == ItemKind::Event && c.kind == ChangeKind::Deleted)), "{:?}", rep.changes.iter().map(|c| &c.summary).collect::<Vec<_>>());
}

#[test]
fn fichier_absent() {
    let (w, d, phone) = first_import("mpe-fichier_absent");
    fs::remove_file(d.join("Notes.dat")).unwrap();
    let rep = check(&w, &d);
    assert!(rep.warnings.iter().any(|m| m.contains("Notes.dat absent")), "{:?}", rep.warnings);
    // Un fichier absent ne fait rien supprimer.
    assert!(rep.changes.iter().all(|c| c.item != ItemKind::Note));
    for f in ["Phonebook.dat", "Organizer.dat"] {
        fs::remove_file(d.join(f)).unwrap();
    }
    let err = mpe_files::import_changes_mpe(&w.conn, &w.dir, &d, &opts()).unwrap_err();
    assert!(err.contains("Aucun fichier de MPE"), "{err}");
    let _ = phone;
}

#[test]
fn fichier_tronque_ou_en_cours_d_ecriture_refuse() {
    let (w, d, phone) = first_import("mpe-fichier_tronqu");
    // Coupé au milieu d'une fiche.
    let full = phone.contacts.concat();
    put(&d, "Phonebook.dat", &full[..full.len() - 30]);
    let before = snapshot(&d);
    let err = mpe_files::import_changes_mpe(&w.conn, &w.dir, &d, &opts()).unwrap_err();
    assert!(err.contains("Phonebook.dat refusé") && err.contains("tronqué"), "{err}");
    // Calendrier coupé avant le « END:VCALENDAR ».
    phone.write(&d);
    let cal = organizer(&phone.events);
    put(&d, "Organizer.dat", &cal[..cal.len() - "END:VCALENDAR\r\n".len()]);
    assert!(mpe_files::import_changes_mpe(&w.conn, &w.dir, &d, &opts()).unwrap_err().contains("Organizer.dat refusé"));
    // Fichier vide (MPE vient de l'ouvrir en écriture).
    phone.write(&d);
    put(&d, "Notes.dat", "");
    assert!(mpe_files::import_changes_mpe(&w.conn, &w.dir, &d, &opts()).unwrap_err().contains("Notes.dat refusé"));
    // Fichier modifié à l'instant : refusé avec le délai par défaut, accepté une fois calme.
    phone.write(&d);
    let strict = MpeOptions { settle: Duration::from_secs(3600), now_ms: Some(NOW_MS), ..Default::default() };
    let err = mpe_files::import_changes_mpe(&w.conn, &w.dir, &d, &strict).unwrap_err();
    assert!(err.contains("modifié à l'instant"), "{err}");
    // Dans tous les cas, rien n'est écrit dans le dossier de MPE (hors nos propres réécritures de test ci-dessus).
    phone.write(&d);
    assert_eq!(snapshot(&d).keys().collect::<Vec<_>>(), before.keys().collect::<Vec<_>>());
    // Et la base n'a pas bougé.
    assert_eq!(contacts_of(&w).len(), 2);
}
