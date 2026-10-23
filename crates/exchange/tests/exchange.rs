//! Échange avec le téléphone, de bout en bout sur une vraie base : export, relecture, changements, application, annulation.

use std::fs;

use thunderlook_exchange::{ChangeKind, ItemKind, Payload, apply, export_all, import_changes, last_undo, modified_files, read_manifest, undo};

mod common;
use common::*;

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
    // Les séries sont relues : une série retirée du fichier est proposée à la suppression comme un autre rendez-vous.
    assert_eq!(rep.file("calendrier.ics").unwrap().ids.len(), 3);
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
    assert_eq!(rep.unchanged, 2 + 2 + 2 + 3);
    assert!(rep.warnings.is_empty(), "{:?}", rep.warnings);
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
