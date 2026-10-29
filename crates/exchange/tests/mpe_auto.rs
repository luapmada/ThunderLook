//! Synchronisation automatique d'un dossier écrit par MPE 2.3.0 seul (cible fichier du S23 de Paul) : pas de
//! `manifest.json`, pas de `taches.ics`, identifiants = GUID aléatoires de MPE. Les fichiers de test reprennent le relevé
//! statistique du format réel : vCard 2.1 (UID, REV, X-ACCOUNT, N/FN parfois en CHARSET=UTF-8, EMAIL;OTHER, TEL sans type,
//! ADR et NOTE en quoted-printable, PHOTO, X-ANNIVERSARY), iCalendar 2.0 `-//FJ Software//MyPhoneExplorer 2.3.0//EN`
//! (`TZID=Romance`, journées entières en `VALUE=DATE`, RRULE, EXDATE).
//!
//! Chaque test pose les fichiers comme MPE, lance des passages comme la tâche de fond, et vérifie ce que ThunderLook
//! devient ; le dossier d'échange n'est jamais écrit par ThunderLook (empreinte vérifiée).

use std::{collections::BTreeMap, fs, time::Duration};

use thunderlook_exchange::auto::{self, Outcome, Params, PassReport};
use thunderlook_model::contacts::ContactForm;

mod common;
use common::*;

fn quick() -> Params {
    Params { stable_gap: Duration::from_millis(20), min_age: Duration::ZERO, ..Params::default() }
}

/// Passage comme la tâche de fond : consigné (signature des fichiers, journal).
fn tick(w: &mut World, p: &Params) -> PassReport {
    let (dir, out) = (w.dir.clone(), w.out.clone());
    auto::run_and_record(&mut w.conn, &dir, &out, p).0
}

/// Nom → contenu de chaque fichier du dossier d'échange.
fn folder(w: &World) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(&w.out).unwrap().map(|e| e.unwrap()).map(|e| (e.file_name().to_string_lossy().into_owned(), fs::read(e.path()).unwrap())).collect()
}

const GUID: [&str; 6] = [
    "{6F1D2C3A-0001-4B7E-9A10-00000000A001}",
    "{6F1D2C3A-0002-4B7E-9A10-00000000A002}",
    "{6F1D2C3A-0003-4B7E-9A10-00000000A003}",
    "{6F1D2C3A-0004-4B7E-9A10-00000000A004}",
    "{6F1D2C3A-0005-4B7E-9A10-00000000A005}",
    "{6F1D2C3A-0006-4B7E-9A10-00000000A006}",
];

fn card(guid: &str, given: &str, surname: &str, cell: &str) -> String {
    format!(
        "BEGIN:VCARD\r\nVERSION:2.1\r\nUID:{guid}\r\nREV:20261024T101500Z\r\nX-ACCOUNT:vnd.sec.contact.phone\r\n\
         N;CHARSET=UTF-8:{surname};{given};;;\r\nFN;CHARSET=UTF-8:{given} {surname}\r\nCATEGORIES:Clients\r\n\
         TEL;CELL:{cell}\r\nORG:ADI\r\nEND:VCARD\r\n"
    )
}

/// La fiche la plus chargée du relevé : QP + CHARSET, adresse, note, URL, anniversaires, photo, téléphones sans type / FAX.
fn rich_card(guid: &str) -> String {
    format!(
        "BEGIN:VCARD\r\nVERSION:2.1\r\nUID:{guid}\r\nREV:20261024T101500Z\r\nX-ACCOUNT:vnd.sec.contact.phone\r\n\
         N;CHARSET=UTF-8:Étienne;Zoé;;;\r\nFN;CHARSET=UTF-8:Zoé Étienne\r\nCATEGORIES:Amis,Voisins\r\nEMAIL;HOME:zoe@exemple.fr\r\n\
         TEL;CELL:0611223344\r\nTEL;WORK:0546000001\r\nTEL;FAX;WORK:0546000002\r\nTEL:0546000003\r\n\
         ADR;WORK;ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8:;;1 rue des =C3=A9coles;Saintes;;17100;France\r\n\
         ORG:Café Étienne\r\nTITLE:Gérante\r\nNOTE;ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8:Ligne un=0D=0ALigne =C3=A0 deux\r\n\
         URL:http://exemple.fr\r\nBDAY:1980-02-03\r\nX-ANNIVERSARY:2005-06-07\r\n\
         PHOTO;ENCODING=BASE64;TYPE=JPEG:/9j/4AAQSkZJRgABAQEASABIAAD/2wBDAP//////////////////////////////////////////////////\r\n \
         ////////////////////////////////////////////////////wgALCAABAAEBAREA/8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQABPxA=\r\n\r\n\
         END:VCARD\r\n"
    )
}

fn calendar(events: &[String]) -> String {
    format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//FJ Software//MyPhoneExplorer 2.3.0//EN\r\nCALSCALE:GREGORIAN\r\n{}END:VCALENDAR\r\n", events.concat())
}

fn event(guid: &str, summary: &str, start: &str, end: &str, more: &str) -> String {
    format!("BEGIN:VEVENT\r\nUID:{guid}\r\nDTSTAMP:20261024T101500Z\r\nSUMMARY:{summary}\r\nDTSTART;TZID=Romance:{start}\r\nDTEND;TZID=Romance:{end}\r\n{more}END:VEVENT\r\n")
}

fn all_day(guid: &str, summary: &str, day: &str, next: &str, more: &str) -> String {
    format!("BEGIN:VEVENT\r\nUID:{guid}\r\nDTSTAMP:20261024T101500Z\r\nSUMMARY:{summary}\r\nDTSTART;VALUE=DATE:{day}\r\nDTEND;VALUE=DATE:{next}\r\n{more}END:VEVENT\r\n")
}

fn note(guid: &str, body_qp: &str) -> String {
    format!("BEGIN:VNOTE\r\nVERSION:1.1\r\nBODY;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:{body_qp}\r\nX-OUTLOOK-UID:{guid}\r\nEND:VNOTE\r\n")
}

fn appointments(w: &World) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = thunderlook_calendar::load(&w.conn)
        .unwrap()
        .appointments
        .iter()
        .map(|a| (a.subject.clone(), thunderlook_calendar::view::local_now(a.start_ms).format("%Y-%m-%dT%H:%M").to_string()))
        .collect();
    v.sort();
    v
}

fn names(w: &World) -> Vec<String> {
    let mut v: Vec<String> = contacts_of(w).iter().map(|c| format!("{} {} {}", c.given_name, c.surname, c.mobile)).collect();
    v.sort();
    v
}

// --- Lecture du format réel ------------------------------------------------------------------------------------------

#[test]
fn premiere_lecture_du_format_mpe_sans_manifeste_et_sans_rien_ecrire_dans_le_dossier() {
    let mut w = world("mpe-premiere");
    fs::create_dir_all(&w.out).unwrap();
    write(&w, "contacts.vcf", &format!("{}{}", rich_card(GUID[0]), card(GUID[1], "Léa", "Martin", "0612345678")));
    let weekly = event(GUID[2], "Point hebdo", "20261026T100000", "20261026T110000", "RRULE:FREQ=WEEKLY;BYDAY=MO;COUNT=6\r\nEXDATE;TZID=Romance:20261102T100000\r\n");
    write(
        &w,
        "calendrier.ics",
        &calendar(&[
            event(GUID[3], "Dentiste", "20261028T090000", "20261028T093000", "BEGIN:VALARM\r\nTRIGGER:-PT15M\r\nACTION:DISPLAY\r\nEND:VALARM\r\n"),
            all_day(GUID[4], "Férié", "20261101", "20261102", ""),
            all_day(GUID[5], "Anniversaire de Paul", "20261115", "20261116", "RRULE:FREQ=YEARLY\r\n"),
            weekly,
        ]),
    );
    write(&w, "notes.vnt", &note("N1", "Courses=0D=0Apain, lait =C3=A0 midi"));
    let before = folder(&w);

    // Première lecture : 7 éléments d'un coup sur une base vide dépassent le seuil, confirmé une fois (voir plus bas).
    assert!(matches!(tick(&mut w, &quick()).outcome, Outcome::Blocked(_)));
    assert!(contacts_of(&w).is_empty());
    let rep = tick(&mut w, &Params { ignore_threshold: true, recheck: true, ..quick() });
    assert_eq!(rep.outcome, Outcome::Done, "{rep:?}");
    assert!(rep.files_only, "{rep:?}");
    assert_eq!(rep.created, 2 + 4 + 1, "{rep:?}");
    assert_eq!(rep.aside.total(), 0, "{rep:?}");
    // Le FAX est conservé depuis le lot BB (fidélité des contacts) : il ne reste que le téléphone sans type.
    assert!(rep.losses >= 1, "téléphone sans type : informatif, pas mis de côté ({rep:?})");
    assert_eq!(rep.exported, None, "jamais d'export dans un dossier écrit par MPE");

    // Le dossier de MPE n'a pas bougé d'un octet : ni manifeste, ni export.
    assert_eq!(folder(&w), before);
    assert!(!w.out.join("manifest.json").exists());
    // C'est le dossier de MPE : l'export manuel et l'export programmé le refusent.
    assert_eq!(thunderlook_exchange::foreign_files(&w.out), ["contacts.vcf", "notes.vnt", "calendrier.ics"]);

    let zoe = contacts_of(&w).into_iter().find(|c| c.surname == "Étienne").unwrap();
    // (Les adresses et e-mails ne se rangent que dans une vraie base : la base de test n'a pas ces propriétés nommées.)
    assert_eq!((zoe.given_name.as_str(), zoe.company.as_str(), zoe.mobile.as_str()), ("Zoé", "Café Étienne", "0611223344"));
    assert_eq!(zoe.notes, "Ligne un\nLigne à deux");
    assert_eq!(zoe.categories, "Amis, Voisins");
    assert_eq!(zoe.birthday, "1980-02-03");
    let appts = appointments(&w);
    assert_eq!(appts.len(), 4, "{appts:?}");
    assert!(appts.contains(&("Dentiste".into(), "2026-10-28T09:00".into())));
    assert!(appts.contains(&("Point hebdo".into(), "2026-10-26T10:00".into())), "série TZID=Romance acceptée");
    assert_eq!(thunderlook_items::notes(&w.conn, &w.dir).unwrap().rows.len(), 1);
}

#[test]
fn passage_suivant_sur_les_memes_fichiers_ne_relit_rien_et_ne_journalise_pas() {
    let mut w = world("mpe-inchange");
    fs::create_dir_all(&w.out).unwrap();
    write(&w, "contacts.vcf", &card(GUID[0], "Léa", "Martin", "0612345678"));
    write(&w, "calendrier.ics", &calendar(&[event(GUID[3], "Dentiste", "20261028T090000", "20261028T093000", "")]));
    assert_eq!(tick(&mut w, &quick()).created, 2);
    let journal = auto::read_journal(&w.dir).len();
    let again = tick(&mut w, &quick());
    assert!(matches!(again.outcome, Outcome::Skipped(ref s) if s.starts_with(auto::UNCHANGED)), "{again:?}");
    assert_eq!(auto::read_journal(&w.dir).len(), journal, "un passage inchangé n'est pas journalisé");
    assert!(auto::read_state(&w.dir).last_checked.is_some());
    // « Relire maintenant » relit quand même : toujours rien à faire, aucun doublon.
    let forced = tick(&mut w, &Params { recheck: true, ..quick() });
    assert_eq!((forced.outcome.clone(), forced.created, forced.modified, forced.aside.total()), (Outcome::Done, 0, 0, 0), "{forced:?}");
    assert_eq!((contacts_of(&w).len(), appointments(&w).len()), (1, 1));
}

// --- Rester à jour --------------------------------------------------------------------------------------------------

#[test]
fn rendez_vous_cree_puis_modifie_sur_le_telephone_apparait_apres_la_synchro_suivante() {
    let mut w = world("mpe-rdv");
    fs::create_dir_all(&w.out).unwrap();
    write(&w, "calendrier.ics", &calendar(&[event(GUID[0], "Dentiste", "20261028T090000", "20261028T093000", "")]));
    write(&w, "contacts.vcf", &card(GUID[1], "Léa", "Martin", "0612345678"));
    assert_eq!(tick(&mut w, &quick()).created, 2);
    assert_eq!(appointments(&w), [("Dentiste".to_string(), "2026-10-28T09:00".to_string())]);

    // MPE (une heure plus tard) : un rendez-vous créé sur le téléphone, le premier déplacé et renommé.
    write(
        &w,
        "calendrier.ics",
        &calendar(&[
            event(GUID[0], "Dentiste (déplacé)", "20261029T140000", "20261029T143000", "LOCATION:Cabinet\\, Saintes\r\n"),
            event(GUID[2], "Garage", "20261030T080000", "20261030T090000", ""),
        ]),
    );
    let rep = tick(&mut w, &quick());
    assert_eq!((rep.outcome.clone(), rep.created, rep.modified, rep.aside.total()), (Outcome::Done, 1, 1, 0), "{rep:?}");
    assert_eq!(appointments(&w), [("Dentiste (déplacé)".to_string(), "2026-10-29T14:00".to_string()), ("Garage".to_string(), "2026-10-30T08:00".to_string())]);

    // Troisième heure : rien de nouveau côté téléphone, rien ne bouge (pas de doublon).
    write(&w, "calendrier.ics", &read(&w, "calendrier.ics").replace("DTSTAMP:20261024T101500Z", "DTSTAMP:20261024T111500Z"));
    let rep = tick(&mut w, &quick());
    assert_eq!((rep.outcome.clone(), rep.created, rep.modified), (Outcome::Done, 0, 0), "{rep:?}");
    assert_eq!(appointments(&w).len(), 2);
}

#[test]
fn serie_modifiee_sur_le_telephone_est_remplacee() {
    let mut w = world("mpe-serie");
    fs::create_dir_all(&w.out).unwrap();
    let series = |count: u32, hour: &str| calendar(&[event(GUID[0], "Hebdo", &format!("20261026T{hour}0000"), &format!("20261026T{}0000", hour.parse::<u32>().unwrap() + 1), &format!("RRULE:FREQ=WEEKLY;BYDAY=MO;COUNT={count}\r\n"))]);
    write(&w, "calendrier.ics", &series(4, "10"));
    assert_eq!(tick(&mut w, &quick()).created, 1);
    write(&w, "calendrier.ics", &series(8, "14"));
    let rep = tick(&mut w, &quick());
    assert_eq!((rep.created, rep.modified, rep.aside.total()), (0, 1, 0), "{rep:?}");
    assert_eq!(appointments(&w), [("Hebdo".to_string(), "2026-10-26T14:00".to_string())]);
}

#[test]
fn contact_modifie_sur_le_telephone_est_applique_et_le_second_passage_ne_double_rien() {
    let mut w = world("mpe-contact");
    fs::create_dir_all(&w.out).unwrap();
    write(&w, "contacts.vcf", &format!("{}{}", card(GUID[0], "Léa", "Martin", "0612345678"), card(GUID[1], "Paul", "Durand", "0687654321")));
    assert_eq!(tick(&mut w, &quick()).created, 2);
    write(&w, "contacts.vcf", &read(&w, "contacts.vcf").replace("0612345678", "0699999999"));
    let rep = tick(&mut w, &quick());
    assert_eq!((rep.created, rep.modified, rep.aside.total()), (0, 1, 0), "{rep:?}");
    assert_eq!(names(&w), ["Léa Martin 0699999999", "Paul Durand 0687654321"]);
    assert_eq!(tick(&mut w, &Params { recheck: true, ..quick() }).modified, 0);
}

#[test]
fn contact_deja_dans_thunderlook_est_reconnu_sans_doublon_puis_suit_le_telephone() {
    // Le contact existe déjà (importé d'Outlook, identifiant local) ; MPE le connaît sous un GUID : reconnu par nom + e-mail.
    let mut w = world("mpe-reconnu");
    let dir = w.dir.clone();
    let lea = ContactForm { email1: String::new(), categories: "Clients".into(), ..contact("Léa", "Martin", "0612345678") };
    thunderlook_contacts::write::create(&mut w.conn, &dir, 1, &lea).unwrap();
    fs::create_dir_all(&w.out).unwrap();
    write(&w, "contacts.vcf", &card(GUID[0], "Léa", "Martin", "0612345678"));
    let rep = tick(&mut w, &quick());
    assert_eq!((rep.created, rep.modified, rep.aside.total()), (0, 0, 0), "{rep:?}");
    assert_eq!(contacts_of(&w).len(), 1);
    // Ensuite le GUID suffit, même si le téléphone change le nom.
    write(&w, "contacts.vcf", &card(GUID[0], "Léa", "Martin-Roux", "0612345678"));
    let rep = tick(&mut w, &quick());
    assert_eq!((rep.created, rep.modified), (0, 1), "{rep:?}");
    assert_eq!(contacts_of(&w).len(), 1);
    assert_eq!(contacts_of(&w)[0].surname, "Martin-Roux");
}

// --- Mis de côté et garde-fous ---------------------------------------------------------------------------------------

#[test]
fn suppression_du_telephone_reste_de_cote_et_un_element_supprime_ici_n_est_pas_ressuscite() {
    let mut w = world("mpe-suppression");
    fs::create_dir_all(&w.out).unwrap();
    let both = format!("{}{}", card(GUID[0], "Léa", "Martin", "0612345678"), card(GUID[1], "Paul", "Durand", "0687654321"));
    write(&w, "contacts.vcf", &both);
    assert_eq!(tick(&mut w, &quick()).created, 2);

    // Supprimé sur le téléphone : proposé, jamais appliqué seul.
    write(&w, "contacts.vcf", &card(GUID[0], "Léa", "Martin", "0612345678"));
    let rep = tick(&mut w, &quick());
    assert_eq!((rep.aside.deletions, rep.created, rep.modified), (1, 0, 0), "{rep:?}");
    assert_eq!(contacts_of(&w).len(), 2);
    let again = tick(&mut w, &Params { recheck: true, ..quick() });
    assert_eq!(again.aside.deletions, 1, "le compteur reste tant que rien n'est tranché");

    // Supprimé ICI (Léa) : le téléphone qui la garde ne la ressuscite pas.
    let lea = contacts_of(&w).into_iter().find(|c| c.surname == "Martin").unwrap();
    let n: i64 = lea.id.unwrap()[1..].parse().unwrap();
    thunderlook_contacts::write::delete(&mut w.conn, n).unwrap();
    write(&w, "contacts.vcf", &both);
    let rep = tick(&mut w, &quick());
    assert_eq!((rep.created, rep.modified), (0, 0), "{rep:?}");
    assert_eq!(names(&w), ["Paul Durand 0687654321"]);
}

#[test]
fn conflit_modifie_des_deux_cotes_reste_de_cote() {
    let mut w = world("mpe-conflit");
    let dir = w.dir.clone();
    fs::create_dir_all(&w.out).unwrap();
    write(&w, "contacts.vcf", &card(GUID[0], "Léa", "Martin", "0612345678"));
    assert_eq!(tick(&mut w, &quick()).created, 1);
    let id = contacts_of(&w)[0].id.clone().unwrap();
    let n: i64 = id[1..].parse().unwrap();
    let mut f = thunderlook_contacts::write::form_of(&w.conn, &dir, n).unwrap();
    f.mobile = "0700000000".into();
    thunderlook_contacts::write::update(&mut w.conn, &dir, n, &f).unwrap();
    write(&w, "contacts.vcf", &read(&w, "contacts.vcf").replace("0612345678", "0688888888"));
    let rep = tick(&mut w, &quick());
    assert_eq!((rep.aside.conflicts, rep.modified), (1, 0), "{rep:?}");
    assert_eq!(names(&w), ["Léa Martin 0700000000"]);
    // Le téléphone n'a pas changé de nouveau : ThunderLook modifié seul ne produit rien.
    write(&w, "contacts.vcf", &read(&w, "contacts.vcf").replace("20261024T101500Z", "20261024T111500Z"));
    let rep = tick(&mut w, &quick());
    assert_eq!(rep.aside.conflicts, 1, "toujours en attente : {rep:?}");
}

#[test]
fn modification_faite_ici_part_dans_le_fichier_de_mpe_fiche_par_fiche() {
    let mut w = world("mpe-retour");
    let dir = w.dir.clone();
    fs::create_dir_all(&w.out).unwrap();
    let both = format!("{}{}", card(GUID[0], "Léa", "Martin", "0612345678"), card(GUID[1], "Paul", "Durand", "0687654321"));
    write(&w, "contacts.vcf", &both);
    assert_eq!(tick(&mut w, &quick()).created, 2);
    let paul_before = read(&w, "contacts.vcf").split("BEGIN:VCARD").find(|c| c.contains("Durand")).unwrap().to_string();
    let lea = contacts_of(&w).into_iter().find(|c| c.surname == "Martin").unwrap();
    let n: i64 = lea.id.unwrap()[1..].parse().unwrap();
    let mut f = thunderlook_contacts::write::form_of(&w.conn, &dir, n).unwrap();
    f.mobile = "0700000000".into();
    thunderlook_contacts::write::update(&mut w.conn, &dir, n, &f).unwrap();

    // Fichiers inchangés : la modification part quand même ; l'autre fiche reste octet pour octet.
    let rep = tick(&mut w, &quick());
    assert_eq!(rep.exported, Some(true), "{rep:?}");
    let text = read(&w, "contacts.vcf");
    assert!(text.contains("0700000000") && !text.contains("0612345678"), "{text}");
    assert!(text.contains(&format!("UID:{}", GUID[0])), "même identifiant pour MPE : {text}");
    assert!(text.contains(&paul_before), "fiche non touchée réécrite : {text}");
    // Relu ensuite : rien ne revient comme « modifié par le téléphone », rien à renvoyer.
    let again = tick(&mut w, &Params { recheck: true, ..quick() });
    assert_eq!((again.created, again.modified, again.aside.conflicts, again.exported), (0, 0, 0, None), "{again:?}");
    assert_eq!(names(&w), ["Léa Martin 0700000000", "Paul Durand 0687654321"]);

    // Supprimée ici : retirée du fichier.
    thunderlook_contacts::write::delete(&mut w.conn, n).unwrap();
    let rep = tick(&mut w, &quick());
    assert_eq!(rep.exported, Some(true), "{rep:?}");
    assert!(!read(&w, "contacts.vcf").contains(GUID[0]));
}

#[test]
fn modification_faite_ici_seulement_n_est_pas_ecrasee_par_le_fichier_inchange() {
    let mut w = world("mpe-ici");
    let dir = w.dir.clone();
    fs::create_dir_all(&w.out).unwrap();
    write(&w, "contacts.vcf", &card(GUID[0], "Léa", "Martin", "0612345678"));
    tick(&mut w, &quick());
    let n: i64 = contacts_of(&w)[0].id.clone().unwrap()[1..].parse().unwrap();
    let mut f = thunderlook_contacts::write::form_of(&w.conn, &dir, n).unwrap();
    f.mobile = "0700000000".into();
    thunderlook_contacts::write::update(&mut w.conn, &dir, n, &f).unwrap();
    let rep = tick(&mut w, &Params { recheck: true, ..quick() });
    assert_eq!((rep.modified, rep.aside.total()), (0, 0), "{rep:?}");
    assert_eq!(names(&w), ["Léa Martin 0700000000"]);
}

#[test]
fn fichier_tronque_ou_vide_n_est_pas_lu() {
    let mut w = world("mpe-tronque");
    fs::create_dir_all(&w.out).unwrap();
    let both = format!("{}{}", card(GUID[0], "Léa", "Martin", "0612345678"), card(GUID[1], "Paul", "Durand", "0687654321"));
    write(&w, "contacts.vcf", &both);
    assert_eq!(tick(&mut w, &quick()).created, 2);
    // MPE interrompue au milieu de la deuxième fiche.
    write(&w, "contacts.vcf", &format!("{}BEGIN:VCARD\r\nVERSION:2.1\r\nUID:{}\r\nN:Dur", card(GUID[0], "Léa", "Martin", "0612345678"), GUID[1]));
    let rep = tick(&mut w, &quick());
    assert!(matches!(rep.outcome, Outcome::Skipped(ref s) if s.contains("tronqué")), "{rep:?}");
    write(&w, "contacts.vcf", "");
    let rep = tick(&mut w, &quick());
    assert!(matches!(rep.outcome, Outcome::Skipped(ref s) if s.contains("vide")), "{rep:?}");
    assert_eq!(contacts_of(&w).len(), 2);
}

#[test]
fn arrivee_massive_est_bloquee_puis_appliquee_sur_demande() {
    let mut w = world("mpe-seuil");
    let dir = w.dir.clone();
    for i in 0..10 {
        thunderlook_contacts::write::create(&mut w.conn, &dir, 1, &contact(&format!("Ami{i}"), "Local", &format!("06000000{i:02}"))).unwrap();
    }
    fs::create_dir_all(&w.out).unwrap();
    let many: String = (0..60).map(|i| card(&format!("{{GUID-{i:04}}}"), &format!("Tel{i}"), "Phone", &format!("07000000{i:02}"))).collect();
    write(&w, "contacts.vcf", &many);
    let rep = tick(&mut w, &quick());
    assert!(matches!(rep.outcome, Outcome::Blocked(_)), "{rep:?}");
    assert_eq!(contacts_of(&w).len(), 10);
    // Même fichier au tour suivant : pas relu, pas re-journalisé.
    let journal = auto::read_journal(&w.dir).len();
    assert!(matches!(tick(&mut w, &quick()).outcome, Outcome::Skipped(_)));
    assert_eq!(auto::read_journal(&w.dir).len(), journal);
    // « Appliquer malgré le seuil ».
    let rep = tick(&mut w, &Params { ignore_threshold: true, recheck: true, ..quick() });
    assert_eq!((rep.outcome.clone(), rep.created), (Outcome::Done, 60), "{rep:?}");
    assert_eq!(contacts_of(&w).len(), 70);
    assert!(rep.undo.is_some());
}

#[test]
fn passage_annule_ne_ressuscite_pas_au_passage_suivant() {
    let mut w = world("mpe-annulation");
    fs::create_dir_all(&w.out).unwrap();
    write(&w, "contacts.vcf", &card(GUID[0], "Léa", "Martin", "0612345678"));
    let rep = tick(&mut w, &quick());
    let stamp = rep.undo.clone().unwrap();
    let dir = w.dir.clone();
    thunderlook_exchange::undo(&mut w.conn, &dir, &stamp).unwrap();
    assert!(contacts_of(&w).is_empty());
    let rep = tick(&mut w, &Params { recheck: true, ..quick() });
    assert_eq!((rep.created, rep.modified), (0, 0), "{rep:?}");
    assert!(contacts_of(&w).is_empty());
}

#[test]
fn reglage_manuel_apres_conflit_inconnu_fait_avancer_la_base() {
    // Premier contact avec un fichier MPE sans base : un élément déjà dans ThunderLook et différent est un conflit
    // « inconnu ». Tranché à la main (écran « Échange avec le téléphone »), il ne revient plus.
    use thunderlook_exchange::{Resolution, apply, import_changes, settle};
    let mut w = world("mpe-regle");
    let dir = w.dir.clone();
    thunderlook_contacts::write::create(&mut w.conn, &dir, 1, &ContactForm { email1: String::new(), ..contact("Léa", "Martin", "0700000000") }).unwrap();
    fs::create_dir_all(&w.out).unwrap();
    write(&w, "contacts.vcf", &card(GUID[0], "Léa", "Martin", "0612345678"));
    let rep = tick(&mut w, &quick());
    assert_eq!((rep.aside.conflicts, rep.modified), (1, 0), "{rep:?}");

    let mut rep = import_changes(&w.conn, &dir, &w.out).unwrap();
    assert_eq!(rep.conflicts.len(), 1);
    rep.conflicts[0].resolution = Some(Resolution::KeepPhone);
    let done = apply(&mut w.conn, &dir, &rep.conflicts).unwrap();
    settle(&w.conn, &dir, &rep, &done.done).unwrap();
    assert_eq!(names(&w), ["Léa Martin 0612345678"]);

    let rep = tick(&mut w, &Params { recheck: true, ..quick() });
    assert_eq!((rep.modified, rep.aside.total()), (0, 0), "{rep:?}");
}

// --- Charge réelle ---------------------------------------------------------------------------------------------------

#[test]
fn volume_du_s23_2140_fiches_372_rendez_vous_dont_61_series() {
    let mut w = world("mpe-volume");
    fs::create_dir_all(&w.out).unwrap();
    let cards: String = (0..2140).map(|i| card(&format!("{{6F1D2C3A-{i:04}-4B7E-9A10-0000000B0000}}"), &format!("Prenom{i}"), &format!("Nom{i}"), &format!("06{i:08}"))).collect();
    write(&w, "contacts.vcf", &cards);
    let events: Vec<String> = (0..372)
        .map(|i| {
            let (day, hour) = (1 + i % 28, 8 + i % 10);
            let more = if i % 6 == 0 { "RRULE:FREQ=WEEKLY;COUNT=20\r\n" } else { "" };
            event(&format!("{{E{i:04}}}"), &format!("Rdv {i}"), &format!("202611{day:02}T{hour:02}0000"), &format!("202611{day:02}T{hour:02}3000"), more)
        })
        .collect();
    write(&w, "calendrier.ics", &calendar(&events));
    let t = std::time::Instant::now();
    let first = tick(&mut w, &Params { ignore_threshold: true, ..quick() });
    let (t1, t2) = (t.elapsed(), std::time::Instant::now());
    assert_eq!((first.outcome.clone(), first.created, first.aside.total()), (Outcome::Done, 2140 + 372, 0), "{first:?}");
    // Un rendez-vous déplacé et un contact modifié parmi 2 500.
    write(&w, "contacts.vcf", &read(&w, "contacts.vcf").replace("TEL;CELL:0600000007\r", "TEL;CELL:0600000099\r"));
    write(&w, "calendrier.ics", &read(&w, "calendrier.ics").replace("SUMMARY:Rdv 5\r", "SUMMARY:Rdv 5 modifié\r"));
    let second = tick(&mut w, &quick());
    let t2 = t2.elapsed();
    assert_eq!((second.created, second.modified, second.aside.total()), (0, 2, 0), "{second:?}");
    assert!(names(&w).contains(&"Prenom7 Nom7 0600000099".to_string()));
    eprintln!("première lecture {t1:?}, passage suivant {t2:?}");
}
