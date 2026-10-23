//! Lot AE : vCalendar 1.0 (`Organizer.dat` de MPE) et vCard 2.1 en quoted-printable, lus par l'import. Échantillons
//! SYNTHÉTIQUES (aucune donnée réelle) au format décrit dans `docs/mpe-fichiers.md` § 1.5.

use thunderlook_exchange::{Change, ChangeKind, EventData, ImportReport, Payload, import_changes, vcal10};

mod common;
use common::*;

fn import(name: &str, file: &str, text: &str) -> ImportReport {
    let w = world(name);
    std::fs::create_dir_all(&w.out).unwrap();
    write(&w, file, text);
    import_changes(&w.conn, &w.dir, &w.out).unwrap()
}

fn event_of(c: &Change) -> &EventData {
    match &c.payload {
        Payload::Event(e) => e,
        p => panic!("pas un rendez-vous : {p:?}"),
    }
}

fn find<'a>(rep: &'a ImportReport, subject: &str) -> &'a EventData {
    rep.changes.iter().filter_map(|c| match &c.payload { Payload::Event(e) => Some(e), _ => None }).find(|e| e.subject == subject).unwrap_or_else(|| panic!("« {subject} » absent : {:?} {:?}", rep.changes.iter().map(|c| &c.summary).collect::<Vec<_>>(), rep.warnings))
}

/// Un VEVENT 1.0 à la MPE : UTC, QP UTF-8 avec coupure douce, identifiants X-IRMC-LUID / X-OUTLOOK-UID.
fn vevent(extra: &str) -> String {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("BEGIN:VEVENT\r\nX-IRMC-LUID:{}\r\nX-OUTLOOK-UID:000000007A74605D{n:02X}\r\n{extra}END:VEVENT\r\n", 1200 + n)
}

fn calendar(events: &[String]) -> String {
    format!("BEGIN:VCALENDAR\r\nVERSION:1.0\r\nPRODID:-//Synthetique//FR\r\n{}END:VCALENDAR\r\n", events.concat())
}

const SIMPLE: &str = "DTSTART:20261104T090000Z\r\nDTEND:20261104T100000Z\r\n\
SUMMARY;ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8:R=C3=A9union =C3=A9quipe\r\n\
LOCATION;ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8:Salle des f=C3=AAtes\r\n\
DESCRIPTION;ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8:Ordre du jour=0D=0A- bilan =C3=A0 faire=0D=0A- budget tr=\r\n=C3=A8s long\r\n\
CATEGORIES;ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8:Mairie;=C3=89t=C3=A9\r\n\
AALARM:20261104T084500Z;;;\r\n";

#[test]
fn detection_de_version() {
    assert!(vcal10::is_vcal10("BEGIN:VCALENDAR\r\nVERSION:1.0\r\nBEGIN:VEVENT\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"));
    assert!(!vcal10::is_vcal10("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nVERSION:1.0\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"));
}

#[test]
fn rendez_vous_simple_qp_utf8_rappel_et_identifiants() {
    let rep = import("vcal-simple", "calendrier.ics", &calendar(&[vevent(SIMPLE)]));
    assert!(rep.warnings.is_empty(), "{:?}", rep.warnings);
    assert_eq!(rep.changes.len(), 1);
    let c = &rep.changes[0];
    assert_eq!(c.kind, ChangeKind::New);
    assert!(c.ext_uid.as_deref().is_some_and(|u| u.starts_with("000000007A74605D")), "{:?}", c.ext_uid);
    let e = event_of(c);
    assert_eq!((e.subject.as_str(), e.location.as_str()), ("Réunion équipe", "Salle des fêtes"));
    // 09:00Z = 10:00 à Paris en novembre.
    assert_eq!((e.start.as_str(), e.end.as_str(), e.all_day), ("2026-11-04T10:00", "2026-11-04T11:00", false));
    assert_eq!(e.body, "Ordre du jour\n- bilan à faire\n- budget très long");
    assert_eq!(e.categories, "Mairie, Été");
    assert_eq!(e.reminder, Some(15));
    assert!(e.series.is_none());
}

#[test]
fn jour_entier_et_heures_locales() {
    let rep = import(
        "vcal-jour",
        "calendrier.ics",
        &calendar(&[
            vevent("DTSTART:20261110\r\nDTEND:20261110\r\nSUMMARY:Férié\r\n"),
            vevent("DTSTART:20261112\r\nDTEND:20261115\r\nSUMMARY:Séminaire\r\n"),
            vevent("DTSTART:20261120T140000\r\nDTEND:20261120T150000\r\nSUMMARY:Local\r\n"),
        ]),
    );
    let a = find(&rep, "Férié");
    assert!(a.all_day);
    assert_eq!(a.start[..10], *"2026-11-10");
    let b = find(&rep, "Séminaire");
    assert!(b.all_day && b.start[..10] == *"2026-11-12");
    let c = find(&rep, "Local");
    assert_eq!((c.start.as_str(), c.end.as_str()), ("2026-11-20T14:00", "2026-11-20T15:00"));
}

#[test]
fn series_1_0_converties_exdate_compris() {
    let rep = import(
        "vcal-series",
        "calendrier.ics",
        &calendar(&[
            vevent("DTSTART:20261102T090000Z\r\nDTEND:20261102T100000Z\r\nSUMMARY:Équipe\r\nRRULE:W1 MO TU #10\r\nEXDATE:20261103T090000Z\r\n"),
            vevent("DTSTART:20261105T090000Z\r\nDTEND:20261105T100000Z\r\nSUMMARY:Quotidien\r\nRRULE:D2 #5\r\n"),
            vevent("DTSTART:20261115T090000Z\r\nDTEND:20261115T100000Z\r\nSUMMARY:Loyer\r\nRRULE:MD1 15 #4\r\n"),
            vevent("DTSTART:20261110T090000Z\r\nDTEND:20261110T100000Z\r\nSUMMARY:Comité\r\nRRULE:MP1 2+ TU #3\r\n"),
            vevent("DTSTART:20261127T090000Z\r\nDTEND:20261127T100000Z\r\nSUMMARY:Dernier vendredi\r\nRRULE:MP1 1- FR 20270331T000000Z\r\n"),
            vevent("DTSTART:20261110T090000Z\r\nDTEND:20261110T100000Z\r\nSUMMARY:Anniversaire\r\nRRULE:YM1 11 #3\r\n"),
            vevent("DTSTART:20261110T090000Z\r\nDTEND:20261110T100000Z\r\nSUMMARY:Annuel jour\r\nRRULE:YD1 314 #3\r\n"),
        ]),
    );
    assert!(rep.warnings.is_empty(), "{:?}", rep.warnings);
    assert_eq!(rep.changes.len(), 7);
    for s in ["Équipe", "Quotidien", "Loyer", "Comité", "Dernier vendredi", "Anniversaire", "Annuel jour"] {
        assert!(find(&rep, s).series.is_some(), "{s} doit être une série");
    }
}

#[test]
fn conversion_des_regles() {
    let rule = |r: &str| {
        let c = vcal10::to_ical(&calendar(&[vevent(&format!("DTSTART:20261110T090000Z\r\nDTEND:20261110T100000Z\r\nSUMMARY:x\r\nRRULE:{r}\r\n"))]));
        let l = c.ics.lines().find(|l| l.starts_with("RRULE:")).map(|l| l[6..].to_string());
        (l, c.warnings)
    };
    assert_eq!(rule("D1 #0").0.as_deref(), Some("FREQ=DAILY"));
    assert_eq!(rule("D3 #6").0.as_deref(), Some("FREQ=DAILY;INTERVAL=3;COUNT=6"));
    assert_eq!(rule("W1 MO TU #10").0.as_deref(), Some("FREQ=WEEKLY;BYDAY=MO,TU;COUNT=10"));
    assert_eq!(rule("W2 TU #0").0.as_deref(), Some("FREQ=WEEKLY;INTERVAL=2;BYDAY=TU"));
    assert_eq!(rule("MD1 15 #0").0.as_deref(), Some("FREQ=MONTHLY;BYMONTHDAY=15"));
    assert_eq!(rule("MD1 1- #0").0.as_deref(), Some("FREQ=MONTHLY;BYMONTHDAY=-1"));
    assert_eq!(rule("MD1 LD #0").0.as_deref(), Some("FREQ=MONTHLY;BYMONTHDAY=-1"));
    assert_eq!(rule("MP1 1+ MO #0").0.as_deref(), Some("FREQ=MONTHLY;BYDAY=1MO"));
    assert_eq!(rule("MP1 1- FR #0").0.as_deref(), Some("FREQ=MONTHLY;BYDAY=-1FR"));
    assert_eq!(rule("YM1 10 #0").0.as_deref(), Some("FREQ=YEARLY;BYMONTH=10"));
    assert_eq!(rule("YD1 #2").0.as_deref(), Some("FREQ=YEARLY;COUNT=2"));
    assert_eq!(rule("D1 20261231T000000Z").0.as_deref(), Some("FREQ=DAILY;UNTIL=20261231T000000Z"));
    // Sans fin explicite, la norme 1.0 compte 2 occurrences.
    assert_eq!(rule("D1").0.as_deref(), Some("FREQ=DAILY;COUNT=2"));
}

#[test]
fn ce_que_le_modele_ne_sait_pas_est_refuse_avec_avertissement() {
    let base = "DTSTART:20261110T090000Z\r\nDTEND:20261110T100000Z\r\n";
    let rep = import(
        "vcal-refus",
        "calendrier.ics",
        &calendar(&[
            vevent(&format!("{base}SUMMARY:Autre jour\r\nRRULE:YD1 100 #3\r\n")),
            vevent(&format!("{base}SUMMARY:Ajouts\r\nRDATE:20261201T090000Z\r\n")),
            vevent(&format!("{base}SUMMARY:Deux règles\r\nRRULE:D1 #3\r\nRRULE:W1 MO #3\r\n")),
            vevent(&format!("{base}SUMMARY:Rang deux\r\nRRULE:MP1 2- FR #3\r\n")),
            vevent(&format!("{base}SUMMARY:Illisible\r\nRRULE:ZZ9 #3\r\n")),
            vevent("DTSTART:pas une date\r\nSUMMARY:Date cassée\r\n"),
            vevent(&format!("{base}SUMMARY:Sans fin, heure fixe\r\nRRULE:W1 TU #0\r\n")),
            vevent(&format!("{base}SUMMARY:Bon\r\n")),
        ]),
    );
    assert_eq!(rep.changes.len(), 1, "{:?}", rep.changes.iter().map(|c| &c.summary).collect::<Vec<_>>());
    assert_eq!(event_of(&rep.changes[0]).subject, "Bon");
    for s in ["Autre jour", "Ajouts", "Deux règles", "Rang deux", "Illisible", "Date cassée"] {
        assert!(rep.warnings.iter().any(|w| w.contains(s)), "pas d'avertissement pour {s} : {:?}", rep.warnings);
    }
    // Série à heure fixe UTC sans fin : franchirait un changement d'heure.
    assert!(rep.warnings.iter().any(|w| w.contains("Sans fin, heure fixe") && w.contains("changement d'heure")), "{:?}", rep.warnings);
    assert!(rep.warnings.iter().all(|w| w.starts_with("calendrier.ics : vCalendar 1.0") || w.starts_with("calendrier.ics : série")), "{:?}", rep.warnings);
}

#[test]
fn pertes_signalees_taches_participants_fuseau_rappel() {
    let rep = import(
        "vcal-pertes",
        "calendrier.ics",
        &format!(
            "BEGIN:VCALENDAR\r\nVERSION:1.0\r\nTZ:+01:00\r\nDAYLIGHT:TRUE;+02:00;20260329T020000;20261025T030000;CEST;CET\r\n{}\
BEGIN:VTODO\r\nSUMMARY:À faire\r\nEND:VTODO\r\nEND:VCALENDAR\r\n",
            vevent("DTSTART:20261120T140000\r\nDTEND:20261120T150000\r\nSUMMARY:Local\r\nATTENDEE:mailto:a@example.fr\r\nAALARM:20261120T150000;;;\r\n")
        ),
    );
    assert_eq!(rep.changes.len(), 1);
    assert_eq!(event_of(&rep.changes[0]).reminder, None);
    for needle in ["rappel non importé", "tâche", "participants", "TZ/DAYLIGHT"] {
        assert!(rep.warnings.iter().any(|w| w.contains(needle)), "{needle} : {:?}", rep.warnings);
    }
}

#[test]
fn qp_windows_1252_et_pliage() {
    let rep = import(
        "vcal-1252",
        "calendrier.ics",
        &calendar(&[vevent("DTSTART:20261104T090000Z\r\nDTEND:20261104T100000Z\r\nSUMMARY;ENCODING=QUOTED-PRINTABLE;CHARSET=WINDOWS-1252:Caf=E9 =80 gar=E7on\r\nLOCATION:Salle\r\n  du fond\r\n")]),
    );
    let e = find(&rep, "Café € garçon");
    assert_eq!(e.location, "Salle du fond");
}

#[test]
fn le_lecteur_de_series_voit_le_meme_resultat_qu_en_ical_2_0() {
    // Même série écrite en 1.0 et en 2.0 : mêmes dates, même exception.
    let v1 = import("vcal-eq1", "calendrier.ics", &calendar(&[vevent("DTSTART:20261102T090000Z\r\nDTEND:20261102T100000Z\r\nSUMMARY:Équipe\r\nRRULE:W1 MO #4\r\nEXDATE:20261109T090000Z\r\n")]));
    let v2 = import("vcal-eq2", "calendrier.ics", "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:000000007A74605D01\r\nSUMMARY:Équipe\r\nDTSTART:20261102T090000Z\r\nDTEND:20261102T100000Z\r\nRRULE:FREQ=WEEKLY;BYDAY=MO;COUNT=4\r\nEXDATE:20261109T090000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n");
    let (a, b) = (find(&v1, "Équipe"), find(&v2, "Équipe"));
    assert_eq!((&a.start, &a.end), (&b.start, &b.end));
    assert_eq!(a.series.is_some(), b.series.is_some());
}

// --- vCard 2.1 de MPE -------------------------------------------------------------------------------------------------

#[test]
fn vcard_21_qp_multilignes_telephones_et_photo() {
    let card = "BEGIN:VCARD\r\nVERSION:2.1\r\n\
N;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:Gu=C3=A9rin;Andr=C3=A9e;;;\r\n\
FN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:Andr=C3=A9e Gu=C3=A9rin\r\n\
TEL;CELL:0611223344\r\nTEL;HOME;VOICE:0546111111\r\nTEL;WORK;VOICE:0546222222\r\n\
EMAIL;INTERNET:andree@example.fr\r\n\
NOTE;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:Ligne un=0D=0A=C3=A0 rappeler =E2=82=AC=0D=0Aet une suite tr=\r\n=C3=A8s longue=\r\n et pli=C3=A9e\r\n\
PHOTO;ENCODING=BASE64;JPEG:/9j/4AAQSkZJRgABAQEA\r\n SABIAAD/\r\n\r\n\
X-IRMC-LUID:0000042\r\nX-OUTLOOK-UID:000000007A74605D02\r\nEND:VCARD\r\n";
    let rep = import("vcard21", "contacts.vcf", card);
    assert_eq!(rep.changes.len(), 1, "{:?}", rep.warnings);
    let Payload::Contact(f) = &rep.changes[0].payload else { panic!("pas un contact") };
    assert_eq!((f.given_name.as_str(), f.surname.as_str()), ("Andrée", "Guérin"));
    assert_eq!((f.mobile.as_str(), f.home_phone.as_str(), f.business.as_str()), ("0611223344", "0546111111", "0546222222"));
    assert_eq!(f.notes, "Ligne un\nà rappeler €\net une suite très longue et pliée");
    // L'import d'échange ne reprend jamais les photos (voir `norm_contact`) ; le lecteur de vCard, lui, les lit.
    let parsed = thunderlook_contacts::vcard::parse(card).unwrap();
    assert!(parsed[0].form.photo.as_deref().is_some_and(|p| p.starts_with("data:image/jpeg;base64,")), "{:?}", parsed[0].warnings);
    assert!(parsed[0].warnings.is_empty(), "{:?}", parsed[0].warnings);
    assert_eq!(rep.changes[0].ext_uid.as_deref(), Some("000000007A74605D02"));
    assert!(rep.warnings.iter().all(|w| !w.contains("PHOTO")), "{:?}", rep.warnings);
}
