//! Banc d'essai de l'échange par fichiers (étape 5 de `notes/mpe-plan.md`, critères du § 7) : le va-et-vient avec un
//! téléphone, simulé en réécrivant les fichiers `.vcf` / `.ics` / `.vnt` comme le ferait MPE. Après chaque scénario :
//! 0 doublon, 0 perte. Les écarts constatés sont des tests `#[ignore]` qui échouent tant que le défaut subsiste.

use thunderlook_calendar::{recur::Frequency, write::{Edit, Repeat, RepeatEnd}};
use thunderlook_exchange::{Change, ChangeKind, ImportReport, ItemKind, Origin, Payload, Resolution, apply, export_all, import_changes, undo};
use thunderlook_model::{contacts::ContactForm, items::{NoteForm, TaskForm}};

mod common;
use common::*;

/// Effectifs en base : contacts, tâches, notes, rendez-vous.
fn counts(w: &World) -> [usize; 4] {
    [
        contacts_of(w).len(),
        thunderlook_items::tasks(&w.conn).unwrap().rows.len(),
        thunderlook_items::notes(&w.conn, &w.dir).unwrap().rows.len(),
        thunderlook_calendar::load(&w.conn).unwrap().appointments.len(),
    ]
}

/// Import complet : tout ce qui est coché d'office, plus les suppressions si `with_deletions`, puis ré-export.
fn sync(w: &mut World, with_deletions: bool) -> ImportReport {
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    let accepted: Vec<_> = rep.changes.iter().filter(|c| with_deletions || c.default_selected()).cloned().collect();
    let dir = w.dir.clone();
    let done = apply(&mut w.conn, &dir, &accepted).unwrap();
    assert!(done.errors.is_empty(), "{:?}", done.errors);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    rep
}

/// Ce qu'écrit MPE en réécrivant `name` : même contenu, mais les lignes pliées à 75 octets et des fins de ligne `\n`.
fn rewrite_like_phone(w: &World, name: &str) {
    let text = read(w, name).replace("\r\n", "\n");
    write(w, name, &text);
}

fn jpeg(seed: u8) -> String {
    let mut b = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0];
    b.extend((0..2000).map(|i| (i as u8).wrapping_mul(seed).wrapping_add(seed)));
    b.extend([0xFF, 0xD9]);
    thunderlook_contacts::photo_uri(&b)
}

fn weekly(count: u32) -> Option<Repeat> {
    Some(Repeat { frequency: Frequency::Weekly, interval: 1, end: RepeatEnd::Count(count) })
}

fn ids_in(text: &str, prop: &str) -> Vec<String> {
    let mut v: Vec<String> = text.replace("\r\n ", "").lines().filter_map(|l| l.strip_prefix(prop)).map(str::to_string).collect();
    v.sort();
    v
}

/// Un jeu riche : accents, texte long replié, photo, catégories, rappel, tâche avec catégories, série avec exception.
fn rich(w: &mut World) {
    let (dir, conn) = (w.dir.clone(), &mut w.conn);
    let long = "Réunion « très » longue : éléments à préparer — café, thé, crème brûlée, œuf, ñandú, Zoë ; ".repeat(4);
    let c = ContactForm {
        given_name: "Hélène".into(),
        surname: "Œuvre-Ñandú".into(),
        company: "Café & Cie; SARL".into(),
        email1: "helene@exemple.fr".into(),
        mobile: "06 12 34 56 78".into(),
        notes: format!("Ligne 1\nLigne 2, avec virgule ; et point-virgule\n{long}"),
        photo: Some(jpeg(3)),
        categories: "Clients, VIP, Été".into(),
        work_street: "12 rue de l'Église\nBâtiment B".into(),
        work_city: "Fenioux".into(),
        ..Default::default()
    };
    thunderlook_contacts::write::create(conn, &dir, 1, &c).unwrap();
    thunderlook_contacts::write::create(conn, &dir, 1, &ContactForm { given_name: "Simple".into(), surname: "Contact".into(), email1: "s@exemple.fr".into(), ..Default::default() }).unwrap();
    let t = TaskForm { folder_id: "f2".into(), subject: "Déclarer la TVA — échéance".into(), due: "2026-11-20".into(), importance: 2, categories: "Compta, Urgent".into(), body: format!("Détail\n{long}"), reminder: Some("2026-11-19T09:00".into()), ..Default::default() };
    thunderlook_items::task_save(conn, &dir, &t).unwrap();
    thunderlook_items::task_save(conn, &dir, &TaskForm { folder_id: "f2".into(), subject: "Tâche simple".into(), ..Default::default() }).unwrap();
    thunderlook_items::note_save(conn, &dir, &NoteForm { folder_id: "f3".into(), body: format!("Note accentuée : été, naïf, Noël\n{long}"), color: 2, id: None }).unwrap();
    thunderlook_items::note_save(conn, &dir, &NoteForm { folder_id: "f3".into(), body: "Autre note".into(), color: 0, id: None }).unwrap();
    let ev = |s: &str, start: &str, end: &str, reminder| Edit { subject: s.into(), location: "Mairie, salle n°2".into(), start: start.into(), end: end.into(), busy: 2, reminder, body: format!("Ordre du jour\n{long}"), categories: "Mairie, Été".into(), ..Default::default() };
    thunderlook_calendar::write::create(conn, &dir, 4, &ev("Conseil municipal", "2026-11-05T18:00", "2026-11-05T20:00", Some(30))).unwrap();
    thunderlook_calendar::write::create(conn, &dir, 4, &Edit { all_day: true, ..ev("Fermé : Toussaint", "2026-11-01T00:00", "2026-11-02T00:00", None) }).unwrap();
    let series = thunderlook_calendar::write::create(conn, &dir, 4, &Edit { repeat: weekly(5), ..ev("Yoga du lundi", "2026-10-26T18:00", "2026-10-26T19:00", Some(15)) }).unwrap();
    thunderlook_calendar::write::update_occurrence(conn, &dir, series, "2026-11-02", &ev("Yoga du lundi (déplacé)", "2026-11-03T18:30", "2026-11-03T19:30", Some(15))).unwrap();
    thunderlook_calendar::write::delete_occurrence(conn, series, "2026-11-09").unwrap();
}

/// Contenu vu par le téléphone : tous les champs, pour comparer avant/après.
fn snapshot(w: &World) -> serde_json::Value {
    let mut contacts: Vec<_> = contacts_of(w).into_iter().map(|mut f| { f.id = None; f }).collect();
    contacts.sort_by(|a, b| a.surname.cmp(&b.surname));
    let tasks: Vec<_> = thunderlook_items::tasks(&w.conn).unwrap().rows.iter().map(|r| thunderlook_items::task_form(&w.conn, &w.dir, r.id[1..].parse().unwrap()).unwrap()).map(|mut f| { f.id = None; f }).collect();
    let notes: Vec<_> = thunderlook_items::notes(&w.conn, &w.dir).unwrap().rows.iter().map(|r| thunderlook_items::note_form(&w.conn, &w.dir, r.id[1..].parse().unwrap()).unwrap()).map(|mut f| { f.id = None; f }).collect();
    let cal = thunderlook_calendar::load(&w.conn).unwrap();
    let mut events: Vec<_> = cal.appointments.iter().map(|a| format!("{}|{}|{}|{:?}|{:?}|{:?}|{}", a.subject, a.start_ms, a.end_ms, a.reminder_minutes, a.categories, a.location, a.recurrence.is_some())).collect();
    events.sort();
    serde_json::json!({ "contacts": contacts, "tasks": tasks, "notes": notes, "events": events })
}

// --- Aller-retour sans doublon ---------------------------------------------------------------------------------------

#[test]
fn import_export_reimport_sans_doublon_ni_perte() {
    let mut w = world("aller-retour");
    rich(&mut w);
    let (before_counts, before) = (counts(&w), snapshot(&w));
    export_all(&w.conn, &w.dir, &w.out).unwrap();

    // Trois tours complets : le téléphone relit les fichiers et les réécrit (fins de ligne différentes).
    for tour in 0..3 {
        for f in ["contacts.vcf", "taches.ics", "notes.vnt", "calendrier.ics"] {
            rewrite_like_phone(&w, f);
        }
        let rep = sync(&mut w, true);
        assert!(rep.changes.is_empty(), "tour {tour} : {:?}", rep.changes);
        assert_eq!(counts(&w), before_counts, "tour {tour}");
        assert_eq!(snapshot(&w), before, "tour {tour}");
    }
}

#[test]
fn uid_stables_d_un_export_a_l_autre() {
    let mut w = world("uid");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let first: Vec<String> = ["contacts.vcf", "taches.ics", "notes.vnt", "calendrier.ics"].iter().map(|f| read(&w, f)).collect();
    // Une modification n'en change pas l'UID ; un ajout ne change pas les autres.
    let dir = w.dir.clone();
    thunderlook_contacts::write::create(&mut w.conn, &dir, 1, &contact("Nina", "Roche", "0600000000")).unwrap();
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let uids = |t: &str| [ids_in(t, "UID:"), ids_in(t, "X-OUTLOOK-UID:")].concat();
    let again: Vec<String> = ["contacts.vcf", "taches.ics", "notes.vnt", "calendrier.ics"].iter().map(|f| read(&w, f)).collect();
    let (old, new) = (uids(&first[0]), uids(&again[0]));
    assert_eq!(old.len(), 2);
    assert!(old.iter().all(|u| new.contains(u)) && new.len() == 3, "{old:?} → {new:?}");
    for i in 1..4 {
        assert_eq!(uids(&first[i]), uids(&again[i]), "{}", ["", "tâches", "notes", "calendrier"][i]);
    }
    let cal = &first[3];
    let mut distinct = ids_in(cal, "UID:");
    distinct.dedup();
    assert_eq!(distinct.len(), ids_in(cal, "BEGIN:VEVENT").len() - 1, "l'exception partage l'UID de sa série : {cal}");
}

#[test]
fn le_contenu_riche_survit_a_l_export() {
    let mut w = world("riche");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let vcf = read(&w, "contacts.vcf");
    // Lignes repliées : jamais plus de 75 octets, jamais de coupe au milieu d'un caractère accentué.
    for t in ["contacts.vcf", "taches.ics", "notes.vnt", "calendrier.ics"] {
        let text = read(&w, t);
        assert!(String::from_utf8(text.clone().into_bytes()).is_ok());
        assert!(text.split("\r\n").all(|l| l.len() <= 76), "{t} : ligne trop longue");
    }
    assert!(vcf.contains("PHOTO;ENCODING=b;TYPE=JPEG:"), "photo exportée");
    assert!(vcf.contains("CATEGORIES:Clients,VIP,Été"), "{vcf}");
    let ics = read(&w, "calendrier.ics");
    assert!(ics.contains("BEGIN:VALARM") && ics.contains("RRULE:") && ics.contains("RECURRENCE-ID") && ics.contains("EXDATE"), "{ics}");
    assert!(read(&w, "taches.ics").contains("CATEGORIES:Compta,Urgent"));
}

// --- Modifié d'un côté -----------------------------------------------------------------------------------------------

#[test]
fn modifie_sur_le_telephone_puis_reexport_sans_doublon() {
    let mut w = world("modif-tel");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let n = counts(&w);
    write(&w, "contacts.vcf", &read(&w, "contacts.vcf").replace("Simple", "Modifié"));
    write(&w, "taches.ics", &read(&w, "taches.ics").replace("Tâche simple", "Tâche changée"));
    write(&w, "notes.vnt", &read(&w, "notes.vnt").replace("Autre note", "Autre note modifiée"));
    write(&w, "calendrier.ics", &read(&w, "calendrier.ics").replace("Conseil municipal", "Conseil extraordinaire"));
    let rep = sync(&mut w, true);
    assert_eq!(rep.changes.len(), 4, "{:?}", rep.changes);
    assert!(rep.changes.iter().all(|c| c.kind == ChangeKind::Modified));
    assert_eq!(counts(&w), n);
    // Aucun reste : ni changement, ni perte de la photo, des catégories, du rappel.
    assert!(import_changes(&w.conn, &w.dir, &w.out).unwrap().changes.is_empty());
    let c = contacts_of(&w);
    assert!(c.iter().any(|f| f.given_name == "Modifié") && c.iter().any(|f| f.photo.is_some() && f.categories.contains("VIP")));
    let cal = thunderlook_calendar::load(&w.conn).unwrap();
    let ev = cal.appointments.iter().find(|a| a.subject == "Conseil extraordinaire").unwrap();
    assert_eq!(ev.reminder_minutes, Some(30));
}

#[test]
fn modifie_dans_thunderlook_puis_le_telephone_renvoie_l_ancien_fichier() {
    // Le fichier reste celui de l'export précédent (le téléphone n'a rien changé) : l'import ne doit pas défaire la
    // modification faite dans ThunderLook depuis.
    let mut w = world("modif-tl");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let (dir, id) = (w.dir.clone(), contacts_of(&w).into_iter().find(|f| f.surname == "Contact").unwrap().id.unwrap());
    let mut f = thunderlook_contacts::write::form_of(&w.conn, &dir, id[1..].parse().unwrap()).unwrap();
    f.mobile = "0700000000".into();
    thunderlook_contacts::write::update(&mut w.conn, &dir, id[1..].parse().unwrap(), &f).unwrap();
    // Le téléphone modifie un autre élément : le fichier n'est plus « intact », la comparaison se fait élément par élément.
    write(&w, "contacts.vcf", &read(&w, "contacts.vcf").replace("Hélène", "Helena"));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    // Un seul changement proposé (Hélène) : l'élément modifié ici est resté intact sur le téléphone, ThunderLook fait foi.
    let touched: Vec<_> = rep.changes.iter().map(|c| c.summary.clone()).collect();
    assert_eq!(rep.changes.len(), 1, "{touched:?}");
    assert!(rep.conflicts.is_empty());
}

#[test]
fn modification_locale_non_ecrasee_par_un_ancien_fichier() {
    let mut w = world("modif-tl-strict");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let dir = w.dir.clone();
    let id = contacts_of(&w).into_iter().find(|f| f.surname == "Contact").unwrap().id.unwrap();
    let mut f = thunderlook_contacts::write::form_of(&w.conn, &dir, id[1..].parse().unwrap()).unwrap();
    f.mobile = "0700000000".into();
    thunderlook_contacts::write::update(&mut w.conn, &dir, id[1..].parse().unwrap(), &f).unwrap();
    write(&w, "contacts.vcf", &read(&w, "contacts.vcf").replace("Hélène", "Helena"));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert_eq!(rep.changes.len(), 1, "seul Hélène a changé côté téléphone : {:?}", rep.changes.iter().map(|c| &c.summary).collect::<Vec<_>>());
}

// --- Conflits : modifié des deux côtés (lot AC) -----------------------------------------------------------------------

fn contact_id(w: &World, surname: &str) -> i64 {
    contacts_of(w).into_iter().find(|f| f.surname == surname).unwrap().id.unwrap()[1..].parse().unwrap()
}

fn edit_here(w: &mut World, surname: &str, mobile: &str) -> i64 {
    let (dir, n) = (w.dir.clone(), contact_id(w, surname));
    let mut f = thunderlook_contacts::write::form_of(&w.conn, &dir, n).unwrap();
    f.mobile = mobile.into();
    thunderlook_contacts::write::update(&mut w.conn, &dir, n, &f).unwrap();
    n
}

fn mobile_of(w: &World, n: i64) -> String {
    thunderlook_contacts::write::form_of(&w.conn, &w.dir, n).unwrap().mobile
}

#[test]
fn intact_sur_le_telephone_rien_n_est_propose_et_la_valeur_locale_fait_foi() {
    // (a) fichier = valeur exportée : ni changement ni conflit, même si le fichier a été réécrit (autre élément modifié).
    let mut w = world("conflit-a");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let n = edit_here(&mut w, "Contact", "0700000000");
    write(&w, "contacts.vcf", &read(&w, "contacts.vcf").replace("Hélène", "Helena"));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.conflicts.is_empty() && rep.changes.len() == 1, "{rep:?}");
    sync(&mut w, false);
    assert_eq!(mobile_of(&w, n), "0700000000");
}

#[test]
fn modifie_seulement_sur_le_telephone_est_propose_et_applique() {
    // (b) fichier ≠ valeur exportée ET ThunderLook inchangé : modification du téléphone, appliquée.
    let mut w = world("conflit-b");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let n = contact_id(&w, "Œuvre-Ñandú");
    let before = mobile_of(&w, n);
    write(&w, "contacts.vcf", &read(&w, "contacts.vcf").replace(&before, "0611223344"));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.conflicts.is_empty(), "{rep:?}");
    assert_eq!(rep.changes.len(), 1, "{rep:?}");
    sync(&mut w, false);
    assert_eq!(mobile_of(&w, n), "0611223344");
}

#[test]
fn modifie_des_deux_cotes_est_un_conflit_a_part_jamais_applique_sans_choix() {
    // (c) les deux ont changé : proposé séparément, non coché, ignoré par `apply` sans choix.
    let mut w = world("conflit-c");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let n = edit_here(&mut w, "Œuvre-Ñandú", "0700000000");
    let exported = contacts_of(&w).len();
    let old = read(&w, "contacts.vcf");
    let before_phone = thunderlook_contacts::write::form_of(&w.conn, &w.dir, n).unwrap();
    // Le téléphone change le même élément (autre valeur de mobile que l'export et que ThunderLook).
    assert!(old.contains("06 12 34 56 78"));
    write(&w, "contacts.vcf", &old.replace("06 12 34 56 78", "0688888888"));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.changes.is_empty(), "{:?}", rep.changes);
    assert_eq!(rep.conflicts.len(), 1, "{rep:?}");
    let c = rep.conflicts[0].clone();
    assert_eq!((c.conflict, c.kind, c.item, c.default_selected()), (Some(Origin::Both), ChangeKind::Modified, ItemKind::Contact, false));
    assert!(c.key.contains(":conflict:"));

    // Sans choix : rien d'écrit, une erreur explicite.
    let dir = w.dir.clone();
    let done = apply(&mut w.conn, &dir, &[c.clone()]).unwrap();
    assert_eq!((done.applied, done.errors.len(), done.undo), (0, 1, None));
    assert_eq!(mobile_of(&w, n), "0700000000");

    // Garder ThunderLook : rien d'écrit.
    let done = apply(&mut w.conn, &dir, &[Change { resolution: Some(Resolution::KeepThunderlook), ..c.clone() }]).unwrap();
    assert_eq!((done.applied, done.kept, done.errors.len()), (0, 1, 0));
    assert_eq!(mobile_of(&w, n), "0700000000");
    assert_eq!(contacts_of(&w).len(), exported);

    // Garder le téléphone : appliqué, annulable.
    let done = apply(&mut w.conn, &dir, &[Change { resolution: Some(Resolution::KeepPhone), ..c }]).unwrap();
    assert_eq!((done.applied, done.errors.len()), (1, 0));
    assert_eq!(mobile_of(&w, n), "0688888888");
    undo(&mut w.conn, &dir, &done.undo.unwrap()).unwrap();
    assert_eq!(thunderlook_contacts::write::form_of(&w.conn, &dir, n).unwrap().mobile, before_phone.mobile);
}

#[test]
fn ancien_manifeste_sans_empreintes_propose_sans_appliquer_d_office() {
    // Migration : un manifeste d'avant le lot AC n'a pas `values`. Le fichier modifié par le téléphone est proposé comme
    // conflit « inconnu » (jamais appliqué d'office) ; un fichier intact reste muet.
    let mut w = world("conflit-migration");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let path = w.out.join("manifest.json");
    let mut m: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for f in m["files"].as_array_mut().unwrap() {
        f.as_object_mut().unwrap().remove("values");
    }
    std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()).unwrap();

    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.changes.is_empty() && rep.conflicts.is_empty(), "{rep:?}");

    let n = contact_id(&w, "Œuvre-Ñandú");
    let before = mobile_of(&w, n);
    write(&w, "contacts.vcf", &read(&w, "contacts.vcf").replace(&before, "0611223344"));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.changes.is_empty(), "{:?}", rep.changes);
    assert_eq!(rep.conflicts.len(), 1, "{rep:?}");
    assert_eq!(rep.conflicts[0].conflict, Some(Origin::Unknown));
    assert!(!rep.conflicts[0].default_selected());
}

// --- Supprimé d'un côté (propagé) -------------------------------------------------------------------------------------

#[test]
fn supprime_sur_le_telephone_est_propage_et_seulement_lui() {
    let mut w = world("suppr-tel");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let n = counts(&w);

    let drop_block = |text: String, begin: &str, end: &str, needle: &str| -> String {
        let mut out = String::new();
        let mut block = String::new();
        let mut inside = false;
        for line in text.split_inclusive("\r\n") {
            if line.starts_with(begin) { inside = true; }
            if inside { block.push_str(line); } else { out.push_str(line); }
            if line.starts_with(end) {
                inside = false;
                if !block.contains(needle) { out.push_str(&block); }
                block.clear();
            }
        }
        out
    };
    write(&w, "contacts.vcf", &drop_block(read(&w, "contacts.vcf"), "BEGIN:VCARD", "END:VCARD", "Simple"));
    write(&w, "taches.ics", &drop_block(read(&w, "taches.ics"), "BEGIN:VTODO", "END:VTODO", "Tâche simple"));
    write(&w, "notes.vnt", &drop_block(read(&w, "notes.vnt"), "BEGIN:VNOTE", "END:VNOTE", "Autre note"));
    write(&w, "calendrier.ics", &drop_block(read(&w, "calendrier.ics"), "BEGIN:VEVENT", "END:VEVENT", "Fermé"));

    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    let mut deleted: Vec<_> = rep.changes.iter().filter(|c| c.kind == ChangeKind::Deleted).map(|c| c.item).collect();
    deleted.sort_by_key(|i| *i as u8);
    assert_eq!(deleted, [ItemKind::Contact, ItemKind::Task, ItemKind::Note, ItemKind::Event], "{:?}", rep.changes);
    assert_eq!(rep.changes.len(), 4, "aucune autre proposition : {:?}", rep.changes);
    assert!(rep.changes.iter().all(|c| !c.default_selected()));

    sync(&mut w, true);
    assert_eq!(counts(&w), [n[0] - 1, n[1] - 1, n[2] - 1, n[3] - 1]);
    // Les autres sont intacts, série et exception comprises.
    let cal = thunderlook_calendar::load(&w.conn).unwrap();
    assert!(cal.appointments.iter().any(|a| a.recurrence.is_some()) && cal.appointments.iter().any(|a| a.subject == "Conseil municipal"));
    assert!(contacts_of(&w).iter().any(|f| f.photo.is_some()));
    assert!(import_changes(&w.conn, &w.dir, &w.out).unwrap().changes.is_empty());
}

#[test]
fn supprime_dans_thunderlook_n_est_pas_recree_par_un_ancien_fichier() {
    let mut w = world("suppr-tl");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let dir = w.dir.clone();
    let id = contacts_of(&w).into_iter().find(|f| f.surname == "Contact").unwrap().id.unwrap();
    thunderlook_contacts::write::delete(&mut w.conn, id[1..].parse().unwrap()).unwrap();
    let n = counts(&w);
    // Le téléphone a modifié un autre contact mais garde encore l'ancien : l'ancien ne doit pas revenir en « nouveau ».
    write(&w, "contacts.vcf", &read(&w, "contacts.vcf").replace("Hélène", "Helena"));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.changes.iter().all(|c| c.kind != ChangeKind::New), "{:?}", rep.changes);
    let _ = dir;
    sync(&mut w, true);
    assert_eq!(counts(&w), n);
}

#[test]
fn supprimer_puis_annuler_restitue_tout() {
    let mut w = world("suppr-undo");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let (n, before) = (counts(&w), snapshot(&w));
    let text = read(&w, "contacts.vcf");
    let kept: Vec<&str> = text.split("BEGIN:VCARD\r\n").filter(|b| !b.is_empty() && !b.contains("Œuvre")).collect();
    write(&w, "contacts.vcf", &kept.iter().map(|b| format!("BEGIN:VCARD\r\n{b}")).collect::<String>());
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    let dir = w.dir.clone();
    let done = apply(&mut w.conn, &dir, &rep.changes).unwrap();
    assert_eq!(done.applied, 1);
    assert_eq!(undo(&mut w.conn, &dir, &done.undo.unwrap()).unwrap(), 1);
    assert_eq!(counts(&w), n);
    assert_eq!(snapshot(&w), before, "photo, catégories et notes revenues telles quelles");
}

// --- Créé sur le téléphone -------------------------------------------------------------------------------------------

#[test]
fn creations_du_telephone_avec_leurs_propres_identifiants_sans_doublon() {
    let mut w = world("creations");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let n = counts(&w);
    let card = "BEGIN:VCARD\r\nVERSION:2.1\r\nN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:Gu=C3=A9rin;Andr=C3=A9;;;\r\nTEL;CELL:0611223344\r\nCATEGORIES:Amis\r\nX-OUTLOOK-UID:0000AAAA\r\nEND:VCARD\r\n";
    write(&w, "contacts.vcf", &format!("{}{card}", read(&w, "contacts.vcf")));
    write(&w, "taches.ics", &read(&w, "taches.ics").replace("END:VCALENDAR", "BEGIN:VTODO\r\nUID:mpe-todo-1\r\nSUMMARY:Rappeler Zoé\r\nDUE;VALUE=DATE:20261105\r\nEND:VTODO\r\nEND:VCALENDAR"));
    write(&w, "notes.vnt", &format!("{}BEGIN:VNOTE\r\nVERSION:1.1\r\nBODY;CHARSET=UTF-8:Idée du soir\r\nX-OUTLOOK-UID:mpe-note-1\r\nEND:VNOTE\r\n", read(&w, "notes.vnt")));
    write(&w, "calendrier.ics", &read(&w, "calendrier.ics").replace("END:VCALENDAR", "BEGIN:VEVENT\r\nUID:mpe-ev-1\r\nSUMMARY:Dîner\r\nDTSTART:20261106T180000Z\r\nDTEND:20261106T200000Z\r\nBEGIN:VALARM\r\nTRIGGER:-PT45M\r\nACTION:DISPLAY\r\nEND:VALARM\r\nEND:VEVENT\r\nEND:VCALENDAR"));
    let rep = sync(&mut w, true);
    assert_eq!(rep.changes.len(), 4, "{:?} {:?}", rep.changes.iter().map(|c| &c.summary).collect::<Vec<_>>(), rep.warnings);
    assert_eq!(counts(&w), [n[0] + 1, n[1] + 1, n[2] + 1, n[3] + 1]);
    // Le ré-export garde les identifiants du téléphone : MPE ne voit ni « nouvel élément » ni « élément disparu ».
    for (file, uid) in [("contacts.vcf", "0000AAAA"), ("taches.ics", "mpe-todo-1"), ("notes.vnt", "mpe-note-1"), ("calendrier.ics", "mpe-ev-1")] {
        assert!(read(&w, file).contains(uid), "{file} doit garder {uid}");
    }
    // Il modifie ses éléments sous ces identifiants : reconnus, jamais dupliqués.
    write(&w, "contacts.vcf", &read(&w, "contacts.vcf").replace("0611223344", "0600000001"));
    write(&w, "notes.vnt", &read(&w, "notes.vnt").replace("du soir", "du matin"));
    let again = sync(&mut w, true);
    assert_eq!(again.changes.len(), 2, "{:?}", again.changes);
    assert!(again.changes.iter().all(|c| c.kind == ChangeKind::Modified), "{:?}", again.changes);
    assert_eq!(counts(&w), [n[0] + 1, n[1] + 1, n[2] + 1, n[3] + 1]);
    // Supprimé ici : le téléphone, qui garde l'élément sous son identifiant, ne le ressuscite pas.
    let dir = w.dir.clone();
    let id = contacts_of(&w).into_iter().find(|f| f.surname == "Guérin").unwrap().id.unwrap();
    let old = read(&w, "contacts.vcf");
    thunderlook_contacts::write::delete(&mut w.conn, id[1..].parse().unwrap()).unwrap();
    write(&w, "contacts.vcf", &old.replace("Hélène", "Helena"));
    let rep = import_changes(&w.conn, &dir, &w.out).unwrap();
    assert!(rep.changes.iter().all(|c| c.kind != ChangeKind::New), "{:?}", rep.changes);

}

// --- Séries et exceptions --------------------------------------------------------------------------------------------

#[test]
fn serie_et_exceptions_traversent_l_aller_retour_sans_changement() {
    let mut w = world("series");
    rich(&mut w);
    let before = snapshot(&w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    // Le téléphone modifie un rendez-vous simple : la série voisine ne bouge pas.
    write(&w, "calendrier.ics", &read(&w, "calendrier.ics").replace("Conseil municipal", "Conseil"));
    let rep = sync(&mut w, true);
    assert_eq!(rep.changes.len(), 1);
    let s = snapshot(&w);
    let ev = |v: &serde_json::Value| v["events"].as_array().unwrap().iter().filter(|e| e.as_str().unwrap().ends_with("|true")).cloned().collect::<Vec<_>>();
    assert_eq!(ev(&s), ev(&before));
    assert_eq!(ev(&s).len(), 1);
}

#[test]
#[ignore = "ÉCART : une série modifiée sur le téléphone n'est jamais relue (avertissement seulement) ; critère § 7 « périodicités \
            et exceptions identiques » non atteint dans le sens téléphone → ThunderLook. Correction hors périmètre : \
            `ics::parse` rend la série mais l'écriture (`thunderlook_calendar::write`) ne sait pas rejouer des exceptions importées."]
fn serie_modifiee_sur_le_telephone_est_proposee() {
    let mut w = world("serie-modif");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    write(&w, "calendrier.ics", &read(&w, "calendrier.ics").replace("SUMMARY;LANGUAGE=fr:Yoga du lundi\r\n", "SUMMARY;LANGUAGE=fr:Yoga du mardi\r\n"));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.changes.iter().any(|c| c.item == ItemKind::Event && c.summary.contains("Yoga")), "{:?}", rep.changes);
}

#[test]
#[ignore = "ÉCART : une série créée sur le téléphone est ignorée (perte silencieuse, simple avertissement). Hors périmètre : \
            l'écriture de séries importées demande une API dans `thunderlook-calendar` (RRULE → `Repeat`, exceptions)."]
fn serie_creee_sur_le_telephone_est_importee() {
    let mut w = world("serie-new");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    write(&w, "calendrier.ics", &read(&w, "calendrier.ics").replace("END:VCALENDAR", "BEGIN:VEVENT\r\nUID:mpe-serie-1\r\nSUMMARY:Marché\r\nDTSTART:20261107T080000Z\r\nDTEND:20261107T120000Z\r\nRRULE:FREQ=WEEKLY;COUNT=4\r\nEND:VEVENT\r\nEND:VCALENDAR"));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.changes.iter().any(|c| c.item == ItemKind::Event && c.summary.contains("Marché")), "{:?}", rep.changes);
}

// --- Listes de distribution ------------------------------------------------------------------------------------------

#[test]
#[ignore = "ÉCART : les listes de distribution (`IPM.DistList`) ne sont pas exportées : `export_all` n'écrit que les contacts \
            (vCard n'a pas d'équivalent sûr ; MPE n'en connaît pas côté téléphone, [supposé]). Aucune perte côté ThunderLook \
            (lecture seule), mais le téléphone ne les voit pas. Décision de Paul : ignorer, ou exporter en `KIND:group` (vCard 4) / contact « Liste : … »."]
fn listes_de_distribution_exportees() {
    let w = world("distlist");
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    assert!(read(&w, "contacts.vcf").contains("X-ADDRESSBOOKSERVER-KIND:group") || read(&w, "contacts.vcf").contains("KIND:group"));
}

#[test]
fn la_photo_n_est_pas_effacee_par_un_fichier_sans_photo() {
    let mut w = world("photo");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    // MPE réécrit la fiche sans la photo (ex. photo trop grande) et change le téléphone.
    let vcf = read(&w, "contacts.vcf");
    let no_photo: String = vcf.replace("\r\n ", "\u{1}").split("\r\n").filter(|l| !l.starts_with("PHOTO")).collect::<Vec<_>>().join("\r\n").replace('\u{1}', "\r\n ");
    write(&w, "contacts.vcf", &no_photo.replace("06 12 34 56 78", "0699999999"));
    sync(&mut w, true);
    let c = contacts_of(&w);
    let h = c.iter().find(|f| f.surname.starts_with("Œuvre")).unwrap();
    assert_eq!(h.mobile, "0699999999");
    assert_eq!(h.photo, Some(jpeg(3)), "la photo existante est conservée");
}

#[test]
fn deux_milliers_de_contacts_synthetiques_sans_doublon() {
    let mut w = world("masse");
    let dir = w.dir.clone();
    for i in 0..2000 {
        let f = ContactForm { given_name: format!("Prénom{i}"), surname: format!("Nom{i:04}"), email1: format!("c{i}@exemple.fr"), mobile: format!("06{i:08}"), categories: "Test".into(), ..Default::default() };
        thunderlook_contacts::write::create(&mut w.conn, &dir, 1, &f).unwrap();
    }
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    rewrite_like_phone(&w, "contacts.vcf");
    let t = std::time::Instant::now();
    let rep = sync(&mut w, true);
    assert!(rep.changes.is_empty(), "{} changements", rep.changes.len());
    assert_eq!(contacts_of(&w).len(), 2000);
    assert!(t.elapsed().as_secs() < 60, "{:?}", t.elapsed());
    let _ = Payload::None;
}

#[test]
#[ignore = "ÉCART : le rappel d'une tâche n'est pas écrit dans `taches.ics` (pas de VALARM dans `thunderlook_items::vtodo::export`, hors \
            périmètre) : le téléphone ne le voit pas. Le rappel n'est pas perdu côté ThunderLook (la comparaison ignore ce que le format ne porte pas)."]
fn rappel_de_tache_exporte() {
    let mut w = world("rappel-tache");
    rich(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    assert!(read(&w, "taches.ics").contains("BEGIN:VALARM"));
}
