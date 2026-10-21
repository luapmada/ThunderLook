//! Étape 5 du J9 : CSV au format de l'export d'Outlook (version française) : UTF-8 avec BOM, 22 colonnes,
//! dates `j/m/aaaa`, heures `HH:MM:SS`, booléens « Vrai » / « Faux ». Comme Outlook, on exporte les occurrences d'une
//! période (pas de périodicité en CSV) ; l'import donne des rendez-vous simples.

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime};

use crate::{Appointment, Busy, expand::Occurrence, tz::TimeZone};

const TZ: TimeZone = TimeZone::PARIS;

pub const HEADER: [&str; 22] = [
    "Objet",
    "Début",
    "Début",
    "Fin",
    "Fin",
    "Journée entière",
    "Rappel actif/inactif",
    "Date de rappel",
    "Heure du rappel",
    "Organisateur d'une réunion",
    "Participants obligatoires",
    "Participants facultatifs",
    "Ressources de la réunion",
    "Afficher la disponibilité",
    "Catégories",
    "Critère de diffusion",
    "Description",
    "Emplacement",
    "Informations facturation",
    "Kilométrage",
    "Priorité",
    "Privé",
];

/// Code de disponibilité de l'export d'Outlook. Établis sur l'exemple du 19/12/2023 (280 lignes rapprochées de la
/// base) : libre → 3, occupé → 2. Provisoire (1), absent (4) et ailleurs (5) : non vérifiés, l'exemple n'en contient pas.
pub fn busy_code(b: Busy) -> &'static str {
    match b {
        Busy::Free => "3",
        Busy::Busy => "2",
        Busy::Tentative => "1",
        Busy::OutOfOffice => "4",
        Busy::WorkingElsewhere => "5",
    }
}

fn busy_of(code: &str) -> Busy {
    match code.trim() {
        "3" => Busy::Free,
        "1" => Busy::Tentative,
        "4" => Busy::OutOfOffice,
        "5" => Busy::WorkingElsewhere,
        _ => Busy::Busy,
    }
}

fn local(ms: i64) -> NaiveDateTime {
    TZ.to_local(DateTime::from_timestamp_millis(ms).unwrap_or_default().naive_utc())
}

fn day(t: NaiveDateTime) -> String {
    t.format("%-d/%-m/%Y").to_string()
}

fn time(t: NaiveDateTime) -> String {
    t.format("%H:%M:%S").to_string()
}

fn yes(b: bool) -> &'static str {
    if b { "Vrai" } else { "Faux" }
}

fn quote(out: &mut String, field: &str) {
    out.push('"');
    out.push_str(&field.replace('"', "\"\""));
    out.push('"');
}

/// Une ligne par occurrence (dans l'ordre donné), avec la description fournie par l'appelant.
pub fn export(rows: &[(&Appointment, Occurrence)], description: &dyn Fn(&Appointment) -> Option<String>) -> String {
    let mut out = String::from("\u{FEFF}");
    for (i, h) in HEADER.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        quote(&mut out, h);
    }
    out.push_str("\r\n");
    for (a, o) in rows {
        let (s, e) = (local(o.start_ms), local(o.end_ms));
        // Rappel inactif : Outlook écrit tout de même la date qu'il aurait, d'après le délai de l'élément.
        let at = s - Duration::minutes(a.reminder_minutes.or(a.reminder_delta).unwrap_or(15));
        let fields: [String; 22] = [
            o.subject.clone(),
            day(s),
            time(s),
            day(e),
            time(e),
            yes(o.all_day).into(),
            yes(a.reminder_minutes.is_some()).into(),
            day(at),
            time(at),
            if a.is_meeting { a.organizer.clone() } else { String::new() },
            if a.is_meeting { a.attendees.clone() } else { String::new() },
            String::new(),
            String::new(),
            busy_code(a.busy).into(),
            a.categories.join(";"),
            match a.sensitivity {
                1 => "Personnel",
                2 => "Privé",
                3 => "Confidentiel",
                _ => "Normale",
            }
            .into(),
            description(a).unwrap_or_default(),
            o.location.clone().unwrap_or_default(),
            String::new(),
            String::new(),
            match a.importance {
                0 => "Faible",
                2 => "Haute",
                _ => "Normale",
            }
            .into(),
            yes(a.sensitivity == 2).into(),
        ];
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            quote(&mut out, f);
        }
        out.push_str("\r\n");
    }
    out
}

/// Enregistrements CSV (RFC 4180 : guillemets doublés, sauts de ligne dans les champs entre guillemets).
pub fn records(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let (mut out, mut row, mut field) = (Vec::new(), Vec::new(), String::new());
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                out.push(std::mem::take(&mut row));
            }
            (c, _) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        out.push(row);
    }
    out
}

fn parse_dt(d: &str, t: &str) -> Option<NaiveDateTime> {
    let date = NaiveDate::parse_from_str(d.trim(), "%d/%m/%Y").ok()?;
    let time = NaiveTime::parse_from_str(t.trim(), "%H:%M:%S").unwrap_or(NaiveTime::MIN);
    Some(date.and_time(time))
}

/// Rendez-vous simples d'un CSV d'Outlook (colonnes repérées par leur en-tête).
pub fn parse(text: &str, folder_id: i64, first_id: i64) -> Result<Vec<Appointment>, String> {
    let rows = records(text);
    let header = rows.first().ok_or("CSV vide")?;
    // « Début » et « Fin » apparaissent deux fois (date puis heure) : on prend les positions dans l'ordre.
    let pos = |name: &str, nth: usize| header.iter().enumerate().filter(|(_, h)| h.trim() == name).map(|(i, _)| i).nth(nth);
    let col = |r: &Vec<String>, i: Option<usize>| i.and_then(|i| r.get(i)).cloned().unwrap_or_default();
    let (subject, sd, st, ed, et) = (pos("Objet", 0), pos("Début", 0), pos("Début", 1), pos("Fin", 0), pos("Fin", 1));
    if subject.is_none() || sd.is_none() {
        return Err("CSV : colonnes « Objet » et « Début » introuvables (format d'export d'Outlook attendu)".into());
    }
    let mut out = Vec::new();
    for (n, r) in rows.iter().skip(1).filter(|r| r.len() > 1).enumerate() {
        let Some(start) = parse_dt(&col(r, sd), &col(r, st)) else { continue };
        let end = parse_dt(&col(r, ed), &col(r, et)).unwrap_or(start);
        let all_day = col(r, pos("Journée entière", 0)) == "Vrai";
        let delta = parse_dt(&col(r, pos("Date de rappel", 0)), &col(r, pos("Heure du rappel", 0))).map(|t| (start - t).num_minutes());
        let reminder = delta.filter(|_| col(r, pos("Rappel actif/inactif", 0)) == "Vrai");
        let sensitivity = match col(r, pos("Critère de diffusion", 0)).as_str() {
            "Personnel" => 1,
            "Privé" => 2,
            "Confidentiel" => 3,
            _ => 0,
        };
        let organizer = col(r, pos("Organisateur d'une réunion", 0));
        out.push(Appointment {
            id: first_id - n as i64,
            uid: None,
            source_id: 0,
            folder_id,
            subject: col(r, subject),
            location: Some(col(r, pos("Emplacement", 0))).filter(|l| !l.trim().is_empty()),
            start_ms: TZ.to_utc(start).and_utc().timestamp_millis(),
            end_ms: TZ.to_utc(end).and_utc().timestamp_millis(),
            all_day,
            busy: busy_of(&col(r, pos("Afficher la disponibilité", 0))),
            recurrence: None,
            time_zone: None,
            time_zone_name: None,
            reminder_minutes: reminder,
            reminder_delta: delta,
            categories: col(r, pos("Catégories", 0)).split(';').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect(),
            is_meeting: !organizer.trim().is_empty(),
            organizer,
            attendees: col(r, pos("Participants obligatoires", 0)),
            response_status: 0,
            private: sensitivity == 2,
            sensitivity,
            importance: match col(r, pos("Priorité", 0)).as_str() {
                "Faible" => 0,
                "Haute" => 2,
                _ => 1,
            },
            eml_path: None,
            description: Some(col(r, pos("Description", 0))).filter(|d| !d.trim().is_empty()),
        });
    }
    Ok(out)
}
