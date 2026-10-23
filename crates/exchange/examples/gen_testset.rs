//! Lot AD : jeu de test SYNTHÉTIQUE (aucune donnée réelle) pour le S20 FE, lisible par MPE et relisible par ThunderLook.
//! Usage : `cargo run -p thunderlook-exchange --example gen_testset -- <dossier>`
//!
//! Écrit deux variantes complètes, chacune avec les quatre fichiers d'échange (noms de `docs/mpe-fichiers.md`) :
//! `<dossier>/vcard30/` (contacts en vCard 3.0, UTF-8) et `<dossier>/vcard21/` (contacts en vCard 2.1, quoted-printable
//! UTF-8). Le calendrier (iCalendar 2.0), les tâches (iCalendar 2.0) et les notes (vNote 1.1) sont identiques dans les deux.
//! Tous les noms commencent par « ZZ-TEST » ; les identifiants sont `ZZ-TEST-…`. Pas de `manifest.json` : c'est ThunderLook
//! qui l'écrit à son premier export.

use std::{env, fs, path::Path, process::ExitCode};

const CRLF: &str = "\r\n";
/// JPEG 1×1 pixel (134 octets) : la « photo minuscule ».
const TINY_JPEG_B64: &str = "/9j/4AAQSkZJRgABAQEASABIAAD/2wBDAP//////////////////////////////////////////////////////////////////////////////////////wgALCAABAAEBAREA/8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQABPxA=";
const PARIS: &str = "TZID=Europe/Paris";

struct Person {
    given: &'static str,
    surname: &'static str,
    company: &'static str,
    mobile: &'static str,
    second: (&'static str, &'static str),
    street: &'static str,
    city: &'static str,
    postal: &'static str,
}

const PEOPLE: [Person; 20] = [
    Person { given: "Léa", surname: "Æbelø", company: "Boulangerie du Rocher", mobile: "06 12 00 00 01", second: ("HOME", "05 46 00 00 01"), street: "1 rue des Écoles", city: "Fenioux", postal: "17350" },
    Person { given: "Zoë", surname: "Œuvre", company: "Atelier Çà et Là", mobile: "06 12 00 00 02", second: ("WORK", "05 46 00 00 02"), street: "2 place de l'Église", city: "Saint-Jean-d'Angély", postal: "17400" },
    Person { given: "Íñigo", surname: "Núñez", company: "Cantina Ñandú", mobile: "06 12 00 00 03", second: ("HOME", "05 46 00 00 03"), street: "3 chemin du Moulin", city: "Taillebourg", postal: "17350" },
    Person { given: "Noël", surname: "Dupré-Lefèvre", company: "Fromagerie d'Été", mobile: "06 12 00 00 04", second: ("WORK", "05 46 00 00 04"), street: "4 impasse des Lilas", city: "Saintes", postal: "17100" },
    Person { given: "Åsa", surname: "Öberg", company: "Scierie Nordique", mobile: "06 12 00 00 05", second: ("HOME", "05 46 00 00 05"), street: "5 rue du Port", city: "La Rochelle", postal: "17000" },
    Person { given: "Jean-Pierre", surname: "O'Brien", company: "Pub l'Écluse", mobile: "06 12 00 00 06", second: ("WORK", "05 46 00 00 06"), street: "6 quai Lhomme", city: "Rochefort", postal: "17300" },
    Person { given: "Élodie", surname: "Garçon", company: "Mairie de Fenioux", mobile: "06 12 00 00 07", second: ("HOME", "05 46 00 00 07"), street: "7 route de Saintes", city: "Fenioux", postal: "17350" },
    Person { given: "Amélie", surname: "Brûlé", company: "Crèmerie Brûlée", mobile: "06 12 00 00 08", second: ("WORK", "05 46 00 00 08"), street: "8 rue Pasteur", city: "Surgères", postal: "17700" },
    Person { given: "Mathéo", surname: "Pérez", company: "Garage du Pont", mobile: "06 12 00 00 09", second: ("HOME", "05 46 00 00 09"), street: "9 avenue de la Gare", city: "Saint-Savinien", postal: "17350" },
    Person { given: "Chloé", surname: "Vidal", company: "Cabinet Vidal & Fils", mobile: "06 12 00 00 10", second: ("WORK", "05 46 00 00 10"), street: "10 rue Gambetta", city: "Royan", postal: "17200" },
    Person { given: "François", surname: "Müller", company: "Brasserie Rhénane", mobile: "06 12 00 00 11", second: ("HOME", "05 46 00 00 11"), street: "11 rue Saint-Michel", city: "Jonzac", postal: "17500" },
    Person { given: "Océane", surname: "Lemaître", company: "Salon Océane", mobile: "06 12 00 00 12", second: ("WORK", "05 46 00 00 12"), street: "12 place du Marché", city: "Marennes", postal: "17320" },
    Person { given: "Hélène", surname: "Ferrand", company: "Pharmacie Centrale", mobile: "06 12 00 00 13", second: ("HOME", "05 46 00 00 13"), street: "13 rue du Château", city: "Pons", postal: "17800" },
    Person { given: "Gaël", surname: "Mesnard", company: "Mesnard Informatique", mobile: "06 12 00 00 14", second: ("WORK", "05 46 00 00 14"), street: "14 zone artisanale", city: "Saintes", postal: "17100" },
    Person { given: "Inès", surname: "Barbier", company: "Barbier Paysagiste", mobile: "06 12 00 00 15", second: ("HOME", "05 46 00 00 15"), street: "15 chemin des Vignes", city: "Cognac", postal: "16100" },
    Person { given: "Ludovic", surname: "Thibault", company: "Thibault Menuiserie", mobile: "06 12 00 00 16", second: ("WORK", "05 46 00 00 16"), street: "16 rue des Tonneliers", city: "Cognac", postal: "16100" },
    Person { given: "Maëlle", surname: "Quéré", company: "Quéré Conseil", mobile: "06 12 00 00 17", second: ("HOME", "05 46 00 00 17"), street: "17 allée des Tilleuls", city: "Saintes", postal: "17100" },
    Person { given: "Rémi", surname: "Boisselier", company: "Ferme de la Vallée", mobile: "06 12 00 00 18", second: ("WORK", "05 46 00 00 18"), street: "18 la Vallée", city: "Fenioux", postal: "17350" },
    Person { given: "Sœur", surname: "Anaïs", company: "Association Écho", mobile: "06 12 00 00 19", second: ("HOME", "05 46 00 00 19"), street: "19 rue de l'Abbaye", city: "Saintes", postal: "17100" },
    Person { given: "Yann", surname: "Kerviel", company: "Voilerie Kerviel", mobile: "06 12 00 00 20", second: ("WORK", "05 46 00 00 20"), street: "20 quai des Chartrons", city: "La Rochelle", postal: "17000" },
];

/// Fiches avec photo : 1 sur 7.
fn has_photo(i: usize) -> bool {
    i.is_multiple_of(7)
}

/// Quoted-printable UTF-8 ; `\n` devient `=0A`. Les lignes longues sont coupées par `=` + saut de ligne.
fn qp(s: &str) -> String {
    let mut out = String::new();
    let mut line = 0;
    for &b in s.as_bytes() {
        let piece = match b {
            b'\n' => "=0A".to_string(),
            b'\r' => continue,
            b'=' => "=3D".to_string(),
            b' '..=b'~' => (b as char).to_string(),
            _ => format!("={b:02X}"),
        };
        if line + piece.len() > 70 {
            out.push_str("=\r\n");
            line = 0;
        }
        line += piece.len();
        out.push_str(&piece);
    }
    out
}

/// Pliage RFC (75 octets, suite après un espace), sans couper un caractère UTF-8.
fn fold(line: &str) -> String {
    let mut out = String::new();
    let mut len = 0;
    for ch in line.chars() {
        let n = ch.len_utf8();
        if len + n > 75 {
            out.push_str("\r\n ");
            len = 1;
        }
        out.push(ch);
        len += n;
    }
    out + CRLF
}

/// Échappement des valeurs texte iCalendar / vCard 3.0.
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace('\n', "\\n")
}

fn vcard30() -> String {
    let mut out = String::new();
    for (k, p) in PEOPLE.iter().enumerate() {
        let n = k + 1;
        let name = format!("ZZ-TEST {}", p.surname);
        let mut l = vec![
            "BEGIN:VCARD".to_string(),
            "VERSION:3.0".into(),
            format!("N:{};{};;;", esc(&name), esc(p.given)),
            format!("FN:{} {}", esc(p.given), esc(&name)),
            format!("ORG:{}", esc(p.company)),
            format!("TEL;TYPE=CELL,VOICE:{}", p.mobile),
            format!("TEL;TYPE={},VOICE:{}", p.second.0, p.second.1),
            format!("EMAIL;TYPE=INTERNET:zz-test{n:02}@exemple.invalid"),
            format!("ADR;TYPE=HOME:;;{};{};;{};France", esc(p.street), esc(p.city), p.postal),
            "CATEGORIES:ZZ-TEST".into(),
            format!("UID:ZZ-TEST-C{n:02}"),
        ];
        if has_photo(k) {
            l.push(format!("PHOTO;ENCODING=b;TYPE=JPEG:{TINY_JPEG_B64}"));
        }
        l.push("END:VCARD".into());
        l.iter().for_each(|x| out.push_str(&fold(x)));
    }
    out
}

fn vcard21() -> String {
    let mut out = String::new();
    for (k, p) in PEOPLE.iter().enumerate() {
        let n = k + 1;
        let name = format!("ZZ-TEST {}", p.surname);
        let q = ";CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE";
        let mut l = vec![
            "BEGIN:VCARD".to_string(),
            "VERSION:2.1".into(),
            format!("N{q}:{};{};;;", qp(&name), qp(p.given)),
            format!("FN{q}:{} {}", qp(p.given), qp(&name)),
            format!("ORG{q}:{}", qp(p.company)),
            format!("TEL;CELL;VOICE:{}", p.mobile),
            format!("TEL;{};VOICE:{}", p.second.0, p.second.1),
            format!("EMAIL;INTERNET:zz-test{n:02}@exemple.invalid"),
            format!("ADR;HOME{q}:;;{};{};;{};France", qp(p.street), qp(p.city), p.postal),
            "CATEGORIES:ZZ-TEST".into(),
            format!("X-OUTLOOK-UID:ZZ-TEST-C{n:02}"),
        ];
        if has_photo(k) {
            l.push(format!("PHOTO;ENCODING=BASE64;JPEG:\r\n {TINY_JPEG_B64}\r\n"));
        }
        l.push("END:VCARD".into());
        l.iter().for_each(|x| {
            out.push_str(x);
            out.push_str(CRLF);
        });
    }
    out
}

/// `(sujet, lieu, début, fin)` : `AAAAMMJJTHHMMSS`, ou `AAAAMMJJ` pour une journée entière (fin exclue).
const SIMPLE: [(&str, &str, &str, &str); 16] = [
    ("Réunion Æbelø", "Salle des fêtes", "20261102T140000", "20261102T153000"),
    ("Dentiste : contrôle, détartrage", "Cabinet Dr Núñez", "20261103T091500", "20261103T094500"),
    ("Déjeuner d'équipe", "Café de la Poste", "20261104T120000", "20261104T140000"),
    ("Visite chez Œuvre", "Atelier Çà et Là", "20261105T100000", "20261105T110000"),
    ("Appel fournisseur", "Téléphone", "20261106T160000", "20261106T163000"),
    ("Marché de Noël", "Place de l'Église", "20261107T090000", "20261107T120000"),
    ("Révision de la voiture", "Garage du Pont", "20261109T083000", "20261109T100000"),
    ("Cours de piano", "Conservatoire", "20261110T181500", "20261110T191500"),
    ("Rendez-vous banque", "Agence de Saintes", "20261112T143000", "20261112T153000"),
    ("Jour férié (armistice)", "", "20261111", "20261112"),
    ("Congé : voyage à Séville", "Espagne", "20261116", "20261119"),
    ("Contrôle technique", "Centre de Saintes", "20261120T110000", "20261120T120000"),
    ("Dîner chez les Dupré-Lefèvre", "Saintes", "20261121T193000", "20261121T223000"),
    ("Atelier « photo & nature »", "Médiathèque", "20261124T150000", "20261124T170000"),
    ("Vœux du maire", "Mairie de Fenioux", "20261203T180000", "20261203T193000"),
    ("Réveillon : préparatifs", "Chez Ludovic", "20261231T100000", "20261231T120000"),
];

fn dt(prop: &str, v: &str) -> String {
    if v.len() == 8 { format!("{prop};VALUE=DATE:{v}") } else { format!("{prop};{PARIS}:{v}") }
}

fn event(uid: &str, subject: &str, location: &str, start: &str, end: &str, extra: &[String]) -> Vec<String> {
    let mut l = vec!["BEGIN:VEVENT".to_string(), format!("UID:{uid}"), "DTSTAMP:20261023T080000Z".into(), format!("SUMMARY:{}", esc(subject))];
    if !location.is_empty() {
        l.push(format!("LOCATION:{}", esc(location)));
    }
    l.push(dt("DTSTART", start));
    l.push(dt("DTEND", end));
    l.extend(extra.iter().cloned());
    l.push("END:VEVENT".into());
    l
}

fn alarm(minutes: u32) -> Vec<String> {
    vec!["BEGIN:VALARM".into(), format!("TRIGGER:-PT{minutes}M"), "ACTION:DISPLAY".into(), "DESCRIPTION:Rappel".into(), "END:VALARM".into()]
}

fn calendar() -> String {
    let mut lines: Vec<String> = vec!["BEGIN:VCALENDAR".into(), "VERSION:2.0".into(), "PRODID:-//ThunderLook//gen_testset//FR".into(), "CALSCALE:GREGORIAN".into()];
    for (k, (s, loc, st, en)) in SIMPLE.iter().enumerate() {
        let mut extra = Vec::new();
        if k % 4 == 1 {
            extra.push(format!("DESCRIPTION:{}", esc("Penser à apporter la carte Vitale, l'ordonnance\net les clés ; merci.")));
            extra.extend(alarm(15));
        }
        if k % 5 == 0 {
            extra.push("CATEGORIES:ZZ-TEST".into());
        }
        lines.extend(event(&format!("ZZ-TEST-E{:02}", k + 1), &format!("ZZ-TEST {s}"), loc, st, en, &extra));
    }
    // 4 séries (jour / semaine / mois / an), 2 exceptions (occurrence déplacée), 1 suppression (EXDATE).
    let series = |uid: &str, subject: &str, loc: &str, start: &str, end: &str, rule: &str, more: &[String]| {
        let mut extra = vec![format!("RRULE:{rule}")];
        extra.extend(more.iter().cloned());
        event(uid, &format!("ZZ-TEST {subject}"), loc, start, end, &extra)
    };
    lines.extend(series("ZZ-TEST-S1", "Point quotidien", "Visio", "20261102T090000", "20261102T091500", "FREQ=DAILY;COUNT=10", &[]));
    lines.extend(series("ZZ-TEST-S2", "Yoga du mardi", "Dojo", "20261103T183000", "20261103T193000", "FREQ=WEEKLY;BYDAY=TU;COUNT=8", &[format!("EXDATE;{PARIS}:20261124T183000")]));
    lines.extend(series("ZZ-TEST-S3", "Comptable du 5", "Cabinet Brûlé", "20261105T140000", "20261105T150000", "FREQ=MONTHLY;BYMONTHDAY=5;COUNT=6", &[]));
    lines.extend(series("ZZ-TEST-S4", "Anniversaire de l'association", "Salle Écho", "20261120T190000", "20261120T210000", "FREQ=YEARLY;COUNT=5", &[]));
    // Exceptions : l'occurrence du 17/11 du yoga est déplacée à 19:30 ; celle du 5/12 du comptable change de lieu et d'heure.
    let mut moved = event("ZZ-TEST-S2", "ZZ-TEST Yoga du mardi", "Dojo", "20261117T193000", "20261117T203000", &[]);
    moved.insert(4, format!("RECURRENCE-ID;{PARIS}:20261117T183000"));
    lines.extend(moved);
    let mut moved = event("ZZ-TEST-S3", "ZZ-TEST Comptable du 5", "Cabinet Brûlé (annexe)", "20261205T100000", "20261205T110000", &[]);
    moved.insert(4, format!("RECURRENCE-ID;{PARIS}:20261205T140000"));
    lines.extend(moved);
    lines.push("END:VCALENDAR".into());
    lines.iter().map(|l| fold(l)).collect()
}

fn tasks() -> String {
    let mut lines: Vec<String> = vec!["BEGIN:VCALENDAR".into(), "VERSION:2.0".into(), "PRODID:-//ThunderLook//gen_testset//FR".into()];
    let todo = |uid: &str, subject: &str, due: &str, extra: Vec<String>| {
        let mut l = vec!["BEGIN:VTODO".to_string(), format!("UID:{uid}"), "DTSTAMP:20261023T080000Z".into(), format!("SUMMARY:{}", esc(subject)), format!("DUE;VALUE=DATE:{due}"), "STATUS:NEEDS-ACTION".into(), "PERCENT-COMPLETE:0".into(), "PRIORITY:5".into()];
        l.extend(extra);
        l.push("END:VTODO".into());
        l
    };
    lines.extend(todo("ZZ-TEST-T1", "ZZ-TEST Relancer l'électricien", "20261105", vec![]));
    let mut alarm = vec!["BEGIN:VALARM".to_string(), "TRIGGER;VALUE=DATE-TIME:20261104T080000Z".into(), "ACTION:DISPLAY".into(), "DESCRIPTION:Rappel".into(), "END:VALARM".into()];
    lines.append(&mut todo("ZZ-TEST-T2", "ZZ-TEST Déclarer la TVA : octobre", "20261112", std::mem::take(&mut alarm)));
    lines.push("END:VCALENDAR".into());
    lines.iter().map(|l| fold(l)).collect()
}

fn notes() -> String {
    let bodies = [
        "ZZ-TEST Courses\npain, lait, œufs, crème brûlée",
        "ZZ-TEST Code du portail\n1234 (faux, test)",
        "ZZ-TEST Idées cadeau\nÆbelø : livre\nNúñez : café ; thé, sucre",
        "ZZ-TEST Réunion du 2/11\n- budget\n- planning\n- questions diverses = à voir",
        "ZZ-TEST Note longue\nLorem ipsum dolor sit amet, consectetur adipiscing élit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.",
    ];
    bodies.iter().enumerate().map(|(k, b)| format!("BEGIN:VNOTE\r\nVERSION:1.1\r\nBODY;ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8:{}\r\nX-OUTLOOK-UID:ZZ-TEST-N{}\r\nEND:VNOTE\r\n", qp(b), k + 1)).collect()
}

fn write(dir: &Path, name: &str, text: &str) -> Result<(), String> {
    fs::write(dir.join(name), text).map_err(|e| format!("{} : {e}", dir.join(name).display()))
}

fn main() -> ExitCode {
    let Some(root) = env::args().nth(1) else {
        eprintln!("usage : gen_testset <dossier>");
        return ExitCode::from(2);
    };
    let run = || -> Result<(), String> {
        for (sub, contacts) in [("vcard30", vcard30()), ("vcard21", vcard21())] {
            let dir = Path::new(&root).join(sub);
            fs::create_dir_all(&dir).map_err(|e| format!("{} : {e}", dir.display()))?;
            write(&dir, "contacts.vcf", &contacts)?;
            write(&dir, "calendrier.ics", &calendar())?;
            write(&dir, "taches.ics", &tasks())?;
            write(&dir, "notes.vnt", &notes())?;
            println!("{} : 20 contacts ({} avec photo), 20 rendez-vous dont 4 séries, 5 notes, 2 tâches", dir.display(), (0..20).filter(|&i| has_photo(i)).count());
        }
        Ok(())
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erreur : {e}");
            ExitCode::FAILURE
        }
    }
}
