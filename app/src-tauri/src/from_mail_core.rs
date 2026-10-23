//! Créer un rendez-vous, une tâche, une note ou un contact à partir d'un message (lot Q) : fonctions pures, sans Tauri
//! ni base de données, pour être testées sous Linux. Rien n'est écrit ici : on ne fabrique que des formulaires.

use chrono::{Duration, NaiveDateTime, Timelike};
use thunderlook_model::{
    calendar::AppointmentForm,
    contacts::ContactForm,
    items::{NoteForm, TaskForm},
};

/// Longueur maximale (en caractères) du corps recopié dans un formulaire.
pub const BODY_MAX: usize = 4_000;

/// Expéditeur d'un message : nom (peut être vide) et adresse.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sender {
    pub name: String,
    pub address: String,
}

impl Sender {
    /// « Nom <adresse> », ou l'adresse seule, ou le nom seul : la ligne « De : » des formulaires.
    pub fn line(&self) -> String {
        match (self.name.is_empty(), self.address.is_empty()) {
            (false, false) => format!("{} <{}>", self.name, self.address),
            (true, false) => self.address.clone(),
            _ => self.name.clone(),
        }
    }
}

fn unquote(s: &str) -> &str {
    let s = s.trim();
    let s = s.strip_prefix('"').and_then(|t| t.strip_suffix('"')).unwrap_or(s);
    s.trim()
}

/// Analyse un expéditeur : `Nom <adresse>`, `"Nom, Prénom" <adresse>`, `adresse`, `adresse (Nom)`.
/// Un « nom » identique à l'adresse est écarté.
pub fn parse_sender(raw: &str) -> Sender {
    let raw = raw.trim();
    let angle = raw.rfind('<').zip(raw.rfind('>')).filter(|(lt, gt)| lt < gt);
    let (name, address) = if let Some((lt, gt)) = angle {
        (unquote(&raw[..lt]).to_string(), raw[lt + 1..gt].trim().to_string())
    } else if let Some((addr, rest)) = raw.split_once('(').filter(|(a, _)| a.contains('@')) {
        (rest.trim_end().trim_end_matches(')').trim().to_string(), addr.trim().to_string())
    } else if raw.contains('@') && !raw.contains(char::is_whitespace) {
        (String::new(), raw.to_string())
    } else {
        // Pas d'adresse reconnaissable : tout le texte est le nom.
        (unquote(raw).to_string(), String::new())
    };
    let address = address.trim_matches(|c: char| c == '<' || c == '>' || c == '"' || c.is_whitespace()).to_string();
    let name = if name.eq_ignore_ascii_case(&address) { String::new() } else { name };
    Sender { name, address }
}

/// Prénom et nom d'un nom affiché : « Dupont, Jean » → (Jean, Dupont) ; « Jean Marie Dupont » → (Jean Marie, Dupont) ;
/// un seul mot → nom de famille.
pub fn split_name(name: &str) -> (String, String) {
    let name = name.trim();
    if let Some((surname, given)) = name.split_once(',') {
        return (given.trim().to_string(), surname.trim().to_string());
    }
    match name.rsplit_once(char::is_whitespace) {
        Some((given, surname)) => (given.trim().to_string(), surname.trim().to_string()),
        None => (String::new(), name.to_string()),
    }
}

/// Texte tronqué à `max` caractères (« … » ajouté s'il a fallu couper), fins de ligne normalisées, lignes vides en excès retirées.
pub fn clip(text: &str, max: usize) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for line in text.replace("\r\n", "\n").replace('\r', "\n").lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 || out.is_empty() {
                continue;
            }
        } else {
            blank = 0;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    }
    let out = out.trim_end().to_string();
    if out.chars().count() <= max {
        return out;
    }
    let cut: String = out.chars().take(max).collect();
    format!("{}…", cut.trim_end())
}

/// Texte extrait d'un HTML : suites d'espaces (dont insécables) réduites à une, par ligne.
pub fn collapse_spaces(text: &str) -> String {
    text.lines().map(|l| l.split(|c: char| c.is_whitespace() || c == '\u{a0}').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join("\n")
}

/// Corps recopié (au plus `BODY_MAX` caractères) suivi de « De : … ».
pub fn description(body: &str, from: &Sender) -> String {
    let body = clip(body, BODY_MAX);
    let line = from.line();
    match (body.is_empty(), line.is_empty()) {
        (_, true) => body,
        (true, false) => format!("De : {line}"),
        (false, false) => format!("{body}\n\nDe : {line}"),
    }
}

/// Prochaine heure pleine strictement après `now` (10:00 → 11:00, 10:20 → 11:00).
pub fn next_full_hour(now: NaiveDateTime) -> NaiveDateTime {
    let floor = now.with_minute(0).and_then(|t| t.with_second(0)).and_then(|t| t.with_nanosecond(0)).unwrap_or(now);
    floor + Duration::hours(1)
}

fn stamp(t: NaiveDateTime) -> String {
    t.format("%Y-%m-%dT%H:%M").to_string()
}

/// Rendez-vous pré-rempli : objet du message, description, début à la prochaine heure pleine.
pub fn appointment_form(subject: &str, from: &Sender, body: &str, now: NaiveDateTime, calendar_id: &str, duration_min: u32, reminder: Option<i32>) -> AppointmentForm {
    let start = next_full_hour(now);
    AppointmentForm {
        id: None,
        calendar_id: calendar_id.to_string(),
        subject: subject.trim().to_string(),
        start: stamp(start),
        end: stamp(start + Duration::minutes(i64::from(duration_min.max(1)))),
        reminder,
        busy: 2,
        body: description(body, from),
        repeat_every: 1,
        repeat_count: 10,
        ..Default::default()
    }
}

/// Tâche pré-remplie : objet et description.
pub fn task_form(folder_id: &str, subject: &str, from: &Sender, body: &str) -> TaskForm {
    TaskForm {
        folder_id: folder_id.to_string(),
        subject: subject.trim().to_string(),
        importance: 1,
        body: description(body, from),
        repeat_every: 1,
        repeat_count: 10,
        ..Default::default()
    }
}

/// Note pré-remplie : la première ligne (titre de la note) est l'objet, puis le corps.
pub fn note_form(folder_id: &str, subject: &str, body: &str, color: i32) -> NoteForm {
    let body = clip(body, BODY_MAX);
    let subject = subject.trim();
    let text = match (subject.is_empty(), body.is_empty()) {
        (false, false) => format!("{subject}\n\n{body}"),
        (false, true) => subject.to_string(),
        _ => body,
    };
    NoteForm { id: None, folder_id: folder_id.to_string(), body: text, color }
}

/// Contact pré-rempli avec le nom et l'adresse de l'expéditeur.
pub fn contact_form(folder_id: &str, from: &Sender) -> ContactForm {
    let (given_name, surname) = split_name(&from.name);
    ContactForm { folder_id: folder_id.to_string(), given_name, surname, email1: from.address.clone(), ..Default::default() }
}

/// Contact existant portant cette adresse (sans tenir compte de la casse) : `(identifiant, adresses)` en entrée.
pub fn find_by_address(contacts: &[(i64, Vec<String>)], address: &str) -> Option<i64> {
    let address = address.trim();
    if address.is_empty() {
        return None;
    }
    contacts.iter().find(|(_, emails)| emails.iter().any(|e| e.trim().eq_ignore_ascii_case(address))).map(|(id, _)| *id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn s(name: &str, address: &str) -> Sender {
        Sender { name: name.into(), address: address.into() }
    }

    #[test]
    fn parses_sender_forms() {
        assert_eq!(parse_sender("Jean Dupont <jean@exemple.fr>"), s("Jean Dupont", "jean@exemple.fr"));
        assert_eq!(parse_sender("\"Dupont, Jean\" <jean@exemple.fr>"), s("Dupont, Jean", "jean@exemple.fr"));
        assert_eq!(parse_sender("<jean@exemple.fr>"), s("", "jean@exemple.fr"));
        assert_eq!(parse_sender("  jean@exemple.fr "), s("", "jean@exemple.fr"));
        assert_eq!(parse_sender("jean@exemple.fr (Jean Dupont)"), s("Jean Dupont", "jean@exemple.fr"));
        assert_eq!(parse_sender("jean@exemple.fr <jean@exemple.fr>"), s("", "jean@exemple.fr"));
        assert_eq!(parse_sender("Service interne"), s("Service interne", ""));
        assert_eq!(parse_sender(""), s("", ""));
        assert_eq!(parse_sender("Jean > Dupont <a@b.fr>"), s("Jean > Dupont", "a@b.fr"));
    }

    #[test]
    fn sender_line() {
        assert_eq!(s("Jean", "j@x.fr").line(), "Jean <j@x.fr>");
        assert_eq!(s("", "j@x.fr").line(), "j@x.fr");
        assert_eq!(s("Jean", "").line(), "Jean");
    }

    #[test]
    fn splits_names() {
        assert_eq!(split_name("Jean Dupont"), ("Jean".into(), "Dupont".into()));
        assert_eq!(split_name("Jean Marie Dupont"), ("Jean Marie".into(), "Dupont".into()));
        assert_eq!(split_name("Dupont, Jean"), ("Jean".into(), "Dupont".into()));
        assert_eq!(split_name("Dupont"), (String::new(), "Dupont".into()));
        assert_eq!(split_name(""), (String::new(), String::new()));
    }

    #[test]
    fn collapses_spaces() {
        assert_eq!(collapse_spaces("  Bonjour \u{a0} Jean  \n   \n a   b "), "Bonjour Jean\n\na b");
    }

    #[test]
    fn clips_text() {
        assert_eq!(clip("a\r\n\r\n\r\n\r\nb  \n", 100), "a\n\nb");
        assert_eq!(clip("\n\n  \nbonjour", 100), "bonjour");
        let long = "é".repeat(5_000);
        let c = clip(&long, BODY_MAX);
        assert_eq!(c.chars().count(), BODY_MAX + 1);
        assert!(c.ends_with('…'));
        assert_eq!(clip(&"x".repeat(BODY_MAX), BODY_MAX).chars().count(), BODY_MAX);
    }

    #[test]
    fn description_keeps_sender_after_truncated_body() {
        let d = description(&"x".repeat(9_000), &s("Jean", "j@x.fr"));
        assert!(d.ends_with("\n\nDe : Jean <j@x.fr>"));
        assert_eq!(d.chars().count(), BODY_MAX + 1 + "\n\nDe : Jean <j@x.fr>".chars().count());
        assert_eq!(description("", &s("Jean", "j@x.fr")), "De : Jean <j@x.fr>");
        assert_eq!(description("Bonjour", &s("", "")), "Bonjour");
    }

    fn at(h: u32, m: u32, sec: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 23).unwrap().and_hms_opt(h, m, sec).unwrap()
    }

    #[test]
    fn next_hour_is_strictly_later() {
        assert_eq!(stamp(next_full_hour(at(10, 0, 0))), "2026-10-23T11:00");
        assert_eq!(stamp(next_full_hour(at(10, 20, 33))), "2026-10-23T11:00");
        assert_eq!(stamp(next_full_hour(at(23, 59, 0))), "2026-10-24T00:00");
    }

    #[test]
    fn appointment_prefill() {
        let f = appointment_form("  Réunion budget ", &s("Jean", "j@x.fr"), "Bonjour\r\nà demain", at(9, 41, 0), "c7", 30, Some(15));
        assert_eq!((f.id.as_deref(), f.calendar_id.as_str(), f.subject.as_str()), (None, "c7", "Réunion budget"));
        assert_eq!((f.start.as_str(), f.end.as_str(), f.reminder, f.busy), ("2026-10-23T10:00", "2026-10-23T10:30", Some(15), 2));
        assert_eq!(f.body, "Bonjour\nà demain\n\nDe : Jean <j@x.fr>");
        assert!(!f.all_day && !f.recurring);
    }

    #[test]
    fn task_and_note_prefill() {
        let t = task_form("f3", "Appeler", &s("", "j@x.fr"), "Rappeler Jean");
        assert_eq!((t.id.as_deref(), t.folder_id.as_str(), t.subject.as_str(), t.importance), (None, "f3", "Appeler", 1));
        assert_eq!(t.body, "Rappeler Jean\n\nDe : j@x.fr");
        assert!(t.reminder.is_none() && t.start.is_empty() && t.due.is_empty());
        let n = note_form("f4", "Idée", "Détails", 3);
        assert_eq!((n.id, n.folder_id.as_str(), n.body.as_str(), n.color), (None, "f4", "Idée\n\nDétails", 3));
        assert_eq!(note_form("f4", "Idée", "", 3).body, "Idée");
        assert_eq!(note_form("f4", "", "Corps", 3).body, "Corps");
    }

    #[test]
    fn contact_prefill() {
        let c = contact_form("k2", &s("Dupont, Jean", "jean@x.fr"));
        assert_eq!((c.id, c.folder_id.as_str(), c.given_name.as_str(), c.surname.as_str(), c.email1.as_str()), (None, "k2", "Jean", "Dupont", "jean@x.fr"));
        let c = contact_form("k2", &s("", "jean@x.fr"));
        assert_eq!((c.given_name.as_str(), c.surname.as_str(), c.email1.as_str()), ("", "", "jean@x.fr"));
    }

    #[test]
    fn finds_existing_contact_by_address() {
        let all = vec![(1, vec!["a@x.fr".to_string()]), (2, vec!["b@x.fr".to_string(), "Jean@X.fr".to_string()])];
        assert_eq!(find_by_address(&all, "jean@x.fr"), Some(2));
        assert_eq!(find_by_address(&all, " a@x.fr "), Some(1));
        assert_eq!(find_by_address(&all, "z@x.fr"), None);
        assert_eq!(find_by_address(&all, ""), None);
    }
}
