//! Lot AA : séries (RRULE), EXDATE et occurrences modifiées venues du téléphone. Chaque test applique vraiment les
//! changements dans une base de test, ré-exporte, relit : 0 changement, 0 doublon, 0 perte.

use chrono::{DateTime, NaiveDate};
use thunderlook_calendar::{
    Appointment,
    recur::{Frequency, minutes_to_local},
    tz::TimeZone,
    write::{Edit, Repeat, RepeatEnd},
};
use thunderlook_exchange::{ChangeKind, ImportReport, ItemKind, apply, export_all, import_changes, undo};

mod common;
use common::*;

fn ev(subject: &str, start: &str, end: &str) -> Edit {
    Edit { subject: subject.into(), location: "Salle 3".into(), start: start.into(), end: end.into(), busy: 2, reminder: Some(20), categories: "Mairie, Été".into(), body: "Ordre du jour".into(), ..Default::default() }
}

fn calendar(w: &World) -> Vec<Appointment> {
    thunderlook_calendar::load(&w.conn).unwrap().appointments
}

fn series_of(w: &World, subject: &str) -> Appointment {
    calendar(w).into_iter().find(|a| a.subject == subject && a.recurrence.is_some()).unwrap_or_else(|| panic!("série « {subject} » absente"))
}

/// Occurrences (début local `AAAA-MM-JJ HH:MM`, exception ?) sur toute la durée de la série.
fn occurrences(a: &Appointment) -> Vec<String> {
    thunderlook_calendar::expand::occurrences(a, 0, 4_102_444_800_000)
        .into_iter()
        .map(|o| {
            let utc = DateTime::from_timestamp_millis(o.start_ms).unwrap().naive_utc();
            format!("{}{}", TimeZone::PARIS.to_local(utc).format("%Y-%m-%d %H:%M"), if o.exception { "*" } else { "" })
        })
        .collect()
}

fn apply_all(w: &mut World, with_deletions: bool) -> (ImportReport, Option<String>) {
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    let accepted: Vec<_> = rep.changes.iter().filter(|c| with_deletions || c.default_selected()).cloned().collect();
    let dir = w.dir.clone();
    let done = apply(&mut w.conn, &dir, &accepted).unwrap();
    assert!(done.errors.is_empty(), "{:?}", done.errors);
    (rep, done.undo)
}

/// Ré-exporte puis relit : plus rien à proposer, et pas d'avertissement.
fn assert_stable(w: &mut World) {
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    for _ in 0..2 {
        let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
        assert!(rep.changes.is_empty() && rep.warnings.is_empty(), "{:?} {:?}", rep.changes, rep.warnings);
        // Le téléphone réécrit le fichier (fins de ligne).
        write(w, "calendrier.ics", &read(w, "calendrier.ics").replace("\r\n", "\n"));
        let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
        assert!(rep.changes.is_empty() && rep.warnings.is_empty(), "{:?} {:?}", rep.changes, rep.warnings);
    }
}

fn weekly(count: u32) -> Option<Repeat> {
    Some(Repeat { frequency: Frequency::Weekly, interval: 1, end: RepeatEnd::Count(count) })
}

/// Série « Yoga » hebdomadaire de 6 séances avec une exception (2/11 déplacée au 3/11) et une séance supprimée (9/11).
fn yoga(w: &mut World) -> i64 {
    let dir = w.dir.clone();
    let id = thunderlook_calendar::write::create(&mut w.conn, &dir, 4, &Edit { repeat: weekly(6), ..ev("Yoga", "2026-10-26T18:00", "2026-10-26T19:00") }).unwrap();
    thunderlook_calendar::write::update_occurrence(&mut w.conn, &dir, id, "2026-11-02", &ev("Yoga (déplacé)", "2026-11-03T18:30", "2026-11-03T19:30")).unwrap();
    thunderlook_calendar::write::delete_occurrence(&mut w.conn, id, "2026-11-09").unwrap();
    id
}

#[test]
fn aller_retour_serie_avec_exception_et_exdate_sans_changement() {
    let mut w = world("aa-aller-retour");
    yoga(&mut w);
    let before = occurrences(&series_of(&w, "Yoga"));
    assert_eq!(before, ["2026-10-26 18:00", "2026-11-03 18:30*", "2026-11-16 18:00", "2026-11-23 18:00", "2026-11-30 18:00"]);
    assert_stable(&mut w);
    assert_eq!(occurrences(&series_of(&w, "Yoga")), before);
}

#[test]
fn serie_modifiee_sur_le_telephone_remplace_la_serie_en_gardant_categorie_et_rappel() {
    let mut w = world("aa-modif");
    let id = yoga(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    // Le téléphone renomme la série, supprime la séance du 16/11 (EXDATE) et ne renvoie ni catégories ni rappel.
    let text = read(&w, "calendrier.ics")
        .replace("SUMMARY;LANGUAGE=fr:Yoga\r\n", "SUMMARY;LANGUAGE=fr:Yoga du soir\r\n")
        .replace("CATEGORIES:Mairie,Été\r\n", "")
        .replace("BEGIN:VALARM\r\nTRIGGER:-PT20M\r\nACTION:DISPLAY\r\nDESCRIPTION:Reminder\r\nEND:VALARM\r\n", "");
    let first_uid = text.lines().find_map(|l| l.strip_prefix("UID:")).unwrap().to_string();
    let exdate = format!("EXDATE;TZID=\"Romance Standard Time\":20261116T180000\r\nUID:{first_uid}\r\n");
    let text = text.replacen(&format!("UID:{first_uid}\r\n"), &exdate, 1);
    write(&w, "calendrier.ics", &text);
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    let modified: Vec<_> = rep.changes.iter().filter(|c| c.item == ItemKind::Event).collect();
    assert_eq!(modified.len(), 1, "{:?}", rep.changes);
    assert_eq!(modified[0].kind, ChangeKind::Modified);
    assert_eq!(modified[0].id, format!("a{id}"));
    let (_, undo_stamp) = apply_all(&mut w, false);

    let calendar = calendar(&w);
    assert_eq!(calendar.len(), 1, "pas de doublon");
    let a = &calendar[0];
    assert_eq!(a.id, id, "même série, même identifiant");
    assert_eq!(a.subject, "Yoga du soir");
    assert_eq!(a.categories, ["Mairie", "Été"], "catégories conservées");
    assert_eq!(a.reminder_minutes, Some(20), "rappel conservé");
    assert_eq!(occurrences(a), ["2026-10-26 18:00", "2026-11-03 18:30*", "2026-11-23 18:00", "2026-11-30 18:00"]);
    assert_stable(&mut w);

    // Annuler le dernier import rétablit la série d'avant.
    let dir = w.dir.clone();
    undo(&mut w.conn, &dir, &undo_stamp.unwrap()).unwrap();
    let a = series_of(&w, "Yoga");
    assert_eq!(occurrences(&a), ["2026-10-26 18:00", "2026-11-03 18:30*", "2026-11-16 18:00", "2026-11-23 18:00", "2026-11-30 18:00"]);
    assert_eq!(calendar_len(&w), 1);
}

fn calendar_len(w: &World) -> usize {
    calendar(w).len()
}

#[test]
fn serie_modifiee_a_une_exception_changee_sur_le_telephone() {
    let mut w = world("aa-exception");
    yoga(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    // L'occurrence déplacée change d'heure et de lieu sur le téléphone.
    let text = read(&w, "calendrier.ics").replace("DTSTART;TZID=\"Romance Standard Time\":20261103T183000", "DTSTART;TZID=\"Romance Standard Time\":20261103T170000").replace("LOCATION:Salle 3\r\nPRIORITY:5\r\nSUMMARY;LANGUAGE=fr:Yoga (déplacé)", "LOCATION:Salle 9\r\nPRIORITY:5\r\nSUMMARY;LANGUAGE=fr:Yoga (déplacé)");
    write(&w, "calendrier.ics", &text);
    let (rep, _) = apply_all(&mut w, false);
    assert_eq!(rep.changes.len(), 1, "{:?}", rep.changes);
    assert_eq!(occurrences(&series_of(&w, "Yoga"))[1], "2026-11-03 17:00*");
    assert_stable(&mut w);
}

const PARIS: &str = "DTSTART;TZID=Europe/Paris:";

fn phone_event(extra: &str) -> String {
    format!("BEGIN:VEVENT\r\nUID:mpe-1\r\nSUMMARY:Marché\r\nLOCATION:Place\r\n{extra}END:VEVENT\r\n")
}

fn add_to_phone(w: &World, events: &str) {
    write(w, "calendrier.ics", &read(w, "calendrier.ics").replace("END:VCALENDAR", &format!("{events}END:VCALENDAR")));
}

/// Le téléphone ajoute `events` ; une seule nouvelle série « Marché » est créée, sans avertissement, et plus rien ne bouge.
fn import_new_series(name: &str, events: &str) -> Vec<String> {
    let mut w = world(name);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    add_to_phone(&w, events);
    let (rep, _) = apply_all(&mut w, false);
    assert!(rep.warnings.is_empty(), "{:?}", rep.warnings);
    assert_eq!(rep.changes.len(), 1, "{:?}", rep.changes);
    assert_eq!(rep.changes[0].kind, ChangeKind::New);
    assert_eq!(calendar(&w).len(), 1);
    let occ = occurrences(&series_of(&w, "Marché"));
    assert_stable(&mut w);
    assert_eq!(occurrences(&series_of(&w, "Marché")), occ);
    occ
}

#[test]
fn serie_creee_sur_le_telephone_avec_exdate_et_exception() {
    let events = format!(
        "{}BEGIN:VEVENT\r\nUID:mpe-1\r\nSUMMARY:Marché de Noël\r\nRECURRENCE-ID:20261128T080000Z\r\nDTSTART:20261128T090000Z\r\nDTEND:20261128T150000Z\r\nEND:VEVENT\r\n",
        phone_event("DTSTART:20261107T080000Z\r\nDTEND:20261107T120000Z\r\nRRULE:FREQ=WEEKLY;COUNT=5\r\nEXDATE:20261121T080000Z\r\n")
    );
    assert_eq!(import_new_series("aa-new", &events), ["2026-11-07 09:00", "2026-11-14 09:00", "2026-11-28 10:00*", "2026-12-05 09:00"]);
}

#[test]
fn fin_de_mois_depuis_le_telephone() {
    let e = phone_event(&format!("{PARIS}20261031T180000\r\nDTEND;TZID=Europe/Paris:20261031T190000\r\nRRULE:FREQ=MONTHLY;BYMONTHDAY=-1;COUNT=4\r\n"));
    assert_eq!(import_new_series("aa-fin-de-mois", &e), ["2026-10-31 18:00", "2026-11-30 18:00", "2026-12-31 18:00", "2027-01-31 18:00"]);
}

#[test]
fn until_en_utc_depuis_le_telephone() {
    // 09:00Z = 10:00 à Paris en décembre : le 1/12 est inclus, le 8/12 non.
    let e = phone_event(&format!("{PARIS}20261103T100000\r\nDTEND;TZID=Europe/Paris:20261103T110000\r\nRRULE:FREQ=WEEKLY;UNTIL=20261201T090000Z\r\n"));
    assert_eq!(import_new_series("aa-until", &e), ["2026-11-03 10:00", "2026-11-10 10:00", "2026-11-17 10:00", "2026-11-24 10:00", "2026-12-01 10:00"]);
}

#[test]
fn changement_d_heure_garde_l_heure_locale() {
    let e = phone_event(&format!("{PARIS}20261019T100000\r\nDTEND;TZID=Europe/Paris:20261019T110000\r\nRRULE:FREQ=WEEKLY;COUNT=3\r\n"));
    let mut w = world("aa-dst");
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    add_to_phone(&w, &e);
    apply_all(&mut w, false);
    let a = series_of(&w, "Marché");
    assert_eq!(occurrences(&a), ["2026-10-19 10:00", "2026-10-26 10:00", "2026-11-02 10:00"]);
    // Heure d'été jusqu'au 25/10 : 08:00Z, puis 09:00Z.
    let utc: Vec<String> = thunderlook_calendar::expand::occurrences(&a, 0, 4_102_444_800_000).iter().map(|o| DateTime::from_timestamp_millis(o.start_ms).unwrap().format("%m-%d %H:%MZ").to_string()).collect();
    assert_eq!(utc, ["10-19 08:00Z", "10-26 09:00Z", "11-02 09:00Z"]);
    assert_stable(&mut w);
}

#[test]
fn serie_utc_qui_traverse_le_changement_d_heure_est_refusee_avec_avertissement() {
    let w = world("aa-utc-dst");
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    add_to_phone(&w, &phone_event("DTSTART:20261019T090000Z\r\nDTEND:20261019T100000Z\r\nRRULE:FREQ=WEEKLY;COUNT=4\r\n"));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.changes.is_empty(), "{:?}", rep.changes);
    assert_eq!(rep.warnings.len(), 1, "{:?}", rep.warnings);
    assert!(rep.warnings[0].contains("Marché") && rep.warnings[0].contains("changement d'heure"), "{:?}", rep.warnings);
}

#[test]
fn vingt_neuf_fevrier_refuse_depuis_le_telephone_mais_stable_depuis_thunderlook() {
    let mut w = world("aa-29-fevrier");
    // Créée dans ThunderLook : exportée, relue à l'identique, aucun avertissement.
    let dir = w.dir.clone();
    let rule = Some(Repeat { frequency: Frequency::Yearly, interval: 1, end: RepeatEnd::Count(3) });
    thunderlook_calendar::write::create(&mut w.conn, &dir, 4, &Edit { repeat: rule, ..ev("Anniversaire", "2028-02-29T09:00", "2028-02-29T10:00") }).unwrap();
    assert_stable(&mut w);
    assert_eq!(occurrences(&series_of(&w, "Anniversaire")), ["2028-02-29 09:00", "2029-02-28 09:00", "2030-02-28 09:00"]);
    // Créée sur le téléphone : la RFC saute les années non bissextiles, ThunderLook non → refusée, dite, rien d'écrit.
    add_to_phone(&w, &phone_event(&format!("{PARIS}20320229T090000\r\nDTEND;TZID=Europe/Paris:20320229T100000\r\nRRULE:FREQ=YEARLY;COUNT=3\r\n")));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.changes.is_empty(), "{:?}", rep.changes);
    assert_eq!(rep.warnings.len(), 1);
    assert!(rep.warnings[0].contains("29 février") && rep.warnings[0].contains("Marché"), "{:?}", rep.warnings);
    assert_eq!(calendar(&w).len(), 1);
}

#[test]
fn serie_non_representable_est_refusee_sans_proposer_la_suppression() {
    let mut w = world("aa-refus");
    yoga(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    // Le téléphone réécrit la série avec une règle que ThunderLook ne sait pas représenter.
    write(&w, "calendrier.ics", &read(&w, "calendrier.ics").replace("RRULE:FREQ=WEEKLY;INTERVAL=1;BYDAY=MO;WKST=MO;COUNT=6", "RRULE:FREQ=WEEKLY;BYWEEKNO=44,45;BYDAY=MO"));
    add_to_phone(&w, &phone_event(&format!("{PARIS}20261026T100000\r\nDTEND;TZID=Europe/Paris:20261026T110000\r\nRRULE:FREQ=HOURLY;COUNT=3\r\n")));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.changes.is_empty(), "ni création, ni modification, ni suppression : {:?}", rep.changes);
    assert_eq!(rep.warnings.len(), 2, "{:?}", rep.warnings);
    assert!(rep.warnings.iter().any(|w| w.contains("Yoga") && w.contains("BYWEEKNO")));
    assert!(rep.warnings.iter().any(|w| w.contains("Marché") && w.contains("HOURLY")));
    let dir = w.dir.clone();
    let before = occurrences(&series_of(&w, "Yoga"));
    let done = apply(&mut w.conn, &dir, &rep.changes).unwrap();
    assert_eq!(done.applied, 0);
    assert_eq!(occurrences(&series_of(&w, "Yoga")), before);
}

#[test]
fn serie_supprimee_sur_le_telephone_est_proposee_puis_supprimee_et_annulable() {
    let mut w = world("aa-suppr");
    let id = yoga(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let text = read(&w, "calendrier.ics");
    let (a, b) = (text.find("BEGIN:VEVENT").unwrap(), text.rfind("END:VEVENT").unwrap() + "END:VEVENT\r\n".len());
    write(&w, "calendrier.ics", &format!("{}{}", &text[..a], &text[b..]));
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert_eq!(rep.changes.len(), 1, "{:?}", rep.changes);
    assert_eq!((rep.changes[0].kind, rep.changes[0].id.as_str()), (ChangeKind::Deleted, format!("a{id}").as_str()));
    assert!(!rep.changes[0].default_selected());
    // Sans accord, rien ne bouge.
    let (_, none) = apply_all(&mut w, false);
    assert!(none.is_none());
    assert_eq!(calendar(&w).len(), 1);
    let (_, stamp) = apply_all(&mut w, true);
    assert!(calendar(&w).is_empty());
    let dir = w.dir.clone();
    undo(&mut w.conn, &dir, &stamp.unwrap()).unwrap();
    let a = series_of(&w, "Yoga");
    assert_eq!(occurrences(&a), ["2026-10-26 18:00", "2026-11-03 18:30*", "2026-11-16 18:00", "2026-11-23 18:00", "2026-11-30 18:00"]);
    assert_eq!(a.categories, ["Mairie", "Été"]);
    assert_eq!(a.reminder_minutes, Some(20));
}

#[test]
fn rendez_vous_simple_devenu_serie_sur_le_telephone() {
    let mut w = world("aa-simple-serie");
    let dir = w.dir.clone();
    thunderlook_calendar::write::create(&mut w.conn, &dir, 4, &ev("Marché", "2026-11-07T09:00", "2026-11-07T12:00")).unwrap();
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    let text = read(&w, "calendrier.ics").replace("DTSTAMP:", "RRULE:FREQ=WEEKLY;COUNT=3\r\nDTSTAMP:");
    write(&w, "calendrier.ics", &text);
    let (rep, _) = apply_all(&mut w, false);
    assert_eq!(rep.changes.len(), 1, "{:?}", rep.changes);
    assert_eq!(calendar(&w).len(), 1);
    let a = series_of(&w, "Marché");
    assert_eq!(occurrences(&a), ["2026-11-07 09:00", "2026-11-14 09:00", "2026-11-21 09:00"]);
    assert_eq!((a.categories.clone(), a.reminder_minutes), (vec!["Mairie".to_string(), "Été".to_string()], Some(20)));
    assert_stable(&mut w);
}

#[allow(dead_code)]
fn unused(_: NaiveDate) -> String {
    minutes_to_local(0).to_string()
}
