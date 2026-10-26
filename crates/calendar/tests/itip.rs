//! Lot BH : cœur iTIP. Messages types (forme Outlook et Gmail), aller-retour REQUEST/REPLY/CANCEL.

use thunderlook_calendar::itip::*;

// 2026-11-03 14:00 Paris (CET = UTC+1) = 13:00 UTC.
const MS_1300Z: i64 = 1_793_710_800_000;
const HOUR: i64 = 3_600_000;

const OUTLOOK_REQUEST: &str = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nPRODID:Microsoft Exchange Server 2010\r\nVERSION:2.0\r\n\
BEGIN:VTIMEZONE\r\nTZID:Romance Standard Time\r\nBEGIN:STANDARD\r\nDTSTART:16010101T030000\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
BEGIN:VEVENT\r\nORGANIZER;CN=Paul Durand:MAILTO:Paul@Example.org\r\n\
ATTENDEE;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION;RSVP=TRUE;CN=\"Luap, Madada\":MAILTO:luap@example.org\r\n\
ATTENDEE;ROLE=OPT-PARTICIPANT;PARTSTAT=ACCEPTED;CN=Zoé:mailto:zoe@example.org\r\n\
DESCRIPTION;LANGUAGE=fr-FR:Ordre du jour\\n- point 1\\, point 2\r\n\
UID:040000008200E00074C5B7101A82E008000000001234\r\nSUMMARY;LANGUAGE=fr-FR:Réunion de suivi\r\n\
DTSTART;TZID=Romance Standard Time:20261103T140000\r\nDTEND;TZID=Romance Standard Time:20261103T150000\r\n\
SEQUENCE:2\r\nLOCATION:Salle 4\r\nDTSTAMP:20261020T080000Z\r\nCLASS:PUBLIC\r\n\
BEGIN:VALARM\r\nTRIGGER:-PT15M\r\nACTION:DISPLAY\r\nDESCRIPTION:Reminder\r\nEND:VALARM\r\n\
END:VEVENT\r\nEND:VCALENDAR\r\n";

#[test]
fn reads_outlook_request() {
    let v = parse(OUTLOOK_REQUEST).unwrap();
    assert_eq!(v.len(), 1);
    let e = &v[0];
    assert_eq!(e.method, Method::Request);
    assert_eq!(e.uid, "040000008200E00074C5B7101A82E008000000001234");
    assert_eq!(e.sequence, 2);
    assert_eq!(e.summary, "Réunion de suivi");
    assert_eq!(e.location.as_deref(), Some("Salle 4"));
    assert_eq!(e.description.as_deref(), Some("Ordre du jour\n- point 1, point 2"));
    assert_eq!((e.start_ms, e.end_ms), (MS_1300Z, MS_1300Z + HOUR));
    assert!(!e.all_day && e.warnings.is_empty());
    let org = e.organizer.as_ref().unwrap();
    assert_eq!((org.email.as_str(), org.name.as_str()), ("paul@example.org", "Paul Durand"));
    assert_eq!(e.attendees.len(), 2);
    assert_eq!(e.attendees[0].name, "Luap, Madada");
    assert!(e.attendees[0].rsvp);
    assert_eq!(e.attendees[1].partstat, PartStat::Accepted);
    // Les propriétés du VALARM (DESCRIPTION, TRIGGER) ne remplacent pas celles de l'événement.
    assert_ne!(e.description.as_deref(), Some("Reminder"));
}

#[test]
fn utc_times_and_all_day_and_duration() {
    let t = "BEGIN:VCALENDAR\nMETHOD:PUBLISH\nBEGIN:VEVENT\nUID:a\nDTSTART:20261103T130000Z\nDURATION:PT90M\nSUMMARY:x\nEND:VEVENT\n\
BEGIN:VEVENT\nUID:b\nDTSTART;VALUE=DATE:20261103\nSUMMARY:y\nEND:VEVENT\nEND:VCALENDAR\n";
    let v = parse(t).unwrap();
    assert_eq!(v[0].method, Method::Publish);
    assert_eq!((v[0].start_ms, v[0].end_ms), (MS_1300Z, MS_1300Z + 90 * 60_000));
    assert!(v[1].all_day);
    assert_eq!(v[1].end_ms - v[1].start_ms, 24 * HOUR);
}

#[test]
fn unknown_tzid_is_flagged_not_hidden() {
    let t = "BEGIN:VCALENDAR\nMETHOD:REQUEST\nBEGIN:VEVENT\nUID:a\nDTSTART;TZID=Pacific Standard Time:20261103T090000\nSUMMARY:x\nEND:VEVENT\nEND:VCALENDAR\n";
    let v = parse(t).unwrap();
    assert_eq!(v[0].warnings.len(), 1);
    assert!(v[0].warnings[0].contains("Pacific Standard Time"));
}

#[test]
fn rejects_garbage_and_incomplete_events() {
    assert!(parse("hello").is_err());
    assert!(parse("BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:sans uid\nEND:VEVENT\nEND:VCALENDAR\n").is_err());
}

#[test]
fn recurrence_id_and_rrule_kept() {
    let t = "BEGIN:VCALENDAR\nMETHOD:REQUEST\nBEGIN:VEVENT\nUID:s\nDTSTART:20261103T130000Z\nRRULE:FREQ=WEEKLY;BYDAY=TU\nSUMMARY:série\nEND:VEVENT\n\
BEGIN:VEVENT\nUID:s\nRECURRENCE-ID:20261110T130000Z\nDTSTART:20261110T140000Z\nSUMMARY:déplacée\nEND:VEVENT\nEND:VCALENDAR\n";
    let v = parse(t).unwrap();
    assert_eq!(v[0].rrule.as_deref(), Some("FREQ=WEEKLY;BYDAY=TU"));
    assert!(v[0].recurrence_id_ms.is_none());
    assert!(v[1].recurrence_id_ms.is_some());
    assert!(!same_event(&v[0], &v[1]));
}

fn me() -> Person {
    Person::new("LUAP@example.org", "Luap Madada")
}

fn organized() -> Invitation {
    let mut e = Invitation::new("uid-1@thunderlook", "Point; équipe, 1", MS_1300Z, MS_1300Z + HOUR);
    e.organizer = Some(Person::new("luap@example.org", "Luap Madada"));
    e.attendees = vec![Person::new("Paul@Example.org", "Paul Durand"), Person::new("zoe@example.org", "")];
    e.location = Some("Bureau".into());
    e.description = Some("ligne 1\nligne 2 \\ fin".into());
    e
}

#[test]
fn request_roundtrip() {
    let e = organized();
    let text = build_request(&e, MS_1300Z - 24 * HOUR).unwrap();
    assert!(text.starts_with("BEGIN:VCALENDAR\r\n") && text.contains("METHOD:REQUEST\r\n"));
    assert!(text.contains("DTSTART:20261103T130000Z\r\n"));
    assert!(text.lines().all(|l| l.len() <= 75), "lignes repliées");
    let back = &parse(&text).unwrap()[0];
    assert_eq!(back.method, Method::Request);
    assert_eq!(back.uid, e.uid);
    assert_eq!(back.summary, e.summary);
    assert_eq!(back.description, e.description);
    assert_eq!((back.start_ms, back.end_ms), (e.start_ms, e.end_ms));
    assert_eq!(back.attendees.len(), 2);
    assert_eq!(back.attendees[0].email, "paul@example.org");
    assert_eq!(back.attendees[0].partstat, PartStat::NeedsAction);
    assert!(back.attendees[0].rsvp);
}

#[test]
fn request_all_day_and_series_roundtrip() {
    let mut e = organized();
    e.all_day = true;
    e.start_ms = MS_1300Z - 14 * HOUR; // 2026-11-03 00:00 Paris (= 02/11 23:00 UTC)
    e.end_ms = e.start_ms + 24 * HOUR;
    e.rrule = Some("FREQ=DAILY;COUNT=3".into());
    let text = build_request(&e, 0).unwrap();
    assert!(text.contains("DTSTART;VALUE=DATE:20261103\r\n") && text.contains("DTEND;VALUE=DATE:20261104\r\n"));
    let back = &parse(&text).unwrap()[0];
    assert!(back.all_day);
    assert_eq!((back.start_ms, back.end_ms), (e.start_ms, e.end_ms));
    assert_eq!(back.rrule.as_deref(), Some("FREQ=DAILY;COUNT=3"));
}

#[test]
fn request_validation() {
    let mut e = organized();
    e.organizer = None;
    assert!(build_request(&e, 0).is_err());
    let mut e = organized();
    e.attendees.clear();
    assert!(build_request(&e, 0).is_err());
    let mut e = organized();
    e.end_ms = e.start_ms - 1;
    assert!(build_request(&e, 0).is_err());
}

#[test]
fn reply_accept_decline_tentative() {
    let req = parse(OUTLOOK_REQUEST).unwrap().remove(0);
    for (ans, name) in [(PartStat::Accepted, "ACCEPTED"), (PartStat::Declined, "DECLINED"), (PartStat::Tentative, "TENTATIVE")] {
        let (to, text) = build_reply(&req, &me(), ans, MS_1300Z).unwrap();
        assert_eq!(to, "paul@example.org");
        assert!(text.contains("METHOD:REPLY\r\n") && text.contains(&format!("PARTSTAT={name}")));
        assert!(!text.contains("DTSTART"), "un REPLY ne répète pas l'horaire");
        assert!(!text.contains("STATUS:"));
        let r = &parse_reply(&text);
        assert_eq!(r.method, Method::Reply);
        assert_eq!(r.uid, req.uid);
        assert_eq!(r.sequence, req.sequence);
        assert_eq!(r.attendees.len(), 1, "seul le répondant figure dans le REPLY");
        assert_eq!(r.attendees[0].email, "luap@example.org");
        assert_eq!(r.attendees[0].partstat, ans);
    }
}

// Un REPLY n'a pas de DTSTART : `parse` l'écarte (événement inexploitable). On relit donc ses lignes avec un DTSTART
// ajouté, ce qui est ce que fera l'interface (le REPLY est rapproché de l'événement par UID).
fn parse_reply(text: &str) -> Invitation {
    let patched = text.replace("SUMMARY:", "DTSTART:20261103T130000Z\r\nSUMMARY:");
    parse(&patched).unwrap().remove(0)
}

#[test]
fn reply_refusals() {
    let req = parse(OUTLOOK_REQUEST).unwrap().remove(0);
    assert!(build_reply(&req, &Person::new("intrus@example.org", ""), PartStat::Accepted, 0).is_err());
    assert!(build_reply(&req, &me(), PartStat::NeedsAction, 0).is_err());
    let mut no_org = req.clone();
    no_org.organizer = None;
    assert!(build_reply(&no_org, &me(), PartStat::Accepted, 0).is_err());
    let mut cancel = req.clone();
    cancel.method = Method::Cancel;
    assert!(build_reply(&cancel, &me(), PartStat::Accepted, 0).is_err());
}

#[test]
fn cancel_bumps_sequence() {
    let mut e = organized();
    e.sequence = 3;
    let text = build_cancel(&e, 0).unwrap();
    assert!(text.contains("METHOD:CANCEL\r\n") && text.contains("STATUS:CANCELLED\r\n") && text.contains("SEQUENCE:4\r\n"));
    let back = &parse(&text).unwrap()[0];
    assert_eq!(back.method, Method::Cancel);
    assert_eq!(back.sequence, 4);
    assert_eq!(back.uid, e.uid);
}

#[test]
fn classify_new_update_stale() {
    let mut known = organized();
    known.dtstamp_ms = Some(1000);
    let mut newer = known.clone();
    newer.sequence = 1;
    assert_eq!(classify(None, &newer), Incoming::New);
    assert_eq!(classify(Some(&known), &newer), Incoming::Update);
    assert_eq!(classify(Some(&newer), &known), Incoming::Stale);
    let mut later_stamp = known.clone();
    later_stamp.dtstamp_ms = Some(2000);
    assert_eq!(classify(Some(&known), &later_stamp), Incoming::Update);
    assert_eq!(classify(Some(&known), &known), Incoming::Stale);
    assert!(!reschedules(&known, &later_stamp));
    let mut moved = known.clone();
    moved.start_ms += HOUR;
    assert!(reschedules(&known, &moved));
}

#[test]
fn apply_reply_updates_partstat() {
    let mut e = organized();
    let mut reply = e.clone();
    reply.method = Method::Reply;
    reply.attendees = vec![Person { partstat: PartStat::Declined, rsvp: false, ..Person::new("PAUL@example.org", "") }];
    assert_eq!(apply_reply(&mut e, &reply), Ok(true));
    assert_eq!(e.attendees[0].partstat, PartStat::Declined);
    assert!(!e.attendees[0].rsvp);
    assert_eq!(e.attendees[1].partstat, PartStat::NeedsAction, "les autres participants ne bougent pas");
    assert_eq!(apply_reply(&mut e, &reply), Ok(false), "deuxième application : rien à changer");

    // Inconnu : refusé, rien n'est ajouté.
    let mut stranger = reply.clone();
    stranger.attendees = vec![Person::new("intrus@example.org", "")];
    assert!(apply_reply(&mut e, &stranger).is_err());
    assert_eq!(e.attendees.len(), 2);

    // Autre événement, autre occurrence, mauvaise méthode.
    let mut other = reply.clone();
    other.uid = "autre".into();
    assert!(apply_reply(&mut e, &other).is_err());
    let mut occ = reply.clone();
    occ.recurrence_id_ms = Some(1);
    assert!(apply_reply(&mut e, &occ).is_err());
    assert!(apply_reply(&mut e, &organized()).is_err());

    // Réponse portant sur une version plus ancienne : ignorée.
    e.sequence = 5;
    let mut old = reply.clone();
    old.attendees = vec![Person { partstat: PartStat::Accepted, ..Person::new("paul@example.org", "") }];
    assert_eq!(apply_reply(&mut e, &old), Ok(false));
    assert_eq!(e.attendees[0].partstat, PartStat::Declined);
}
