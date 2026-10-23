//! Recherche globale : contacts, rendez-vous (objet, lieu, description), tâches et notes. Insensible à la casse et aux
//! accents (même clé que le tri des contacts) ; tous les mots saisis doivent se retrouver dans l'élément.
//!
//! Le courrier a son propre index plein texte (`thunderlook_mailbox::search`) et n'est pas concerné ici.

use std::{collections::HashMap, path::Path};

use serde::{Deserialize, Serialize};
use thunderlook_calendar::tz::TimeZone;
use thunderlook_contacts::view::sort_key;
use thunderlook_store::rusqlite::Connection;

/// Nombre de résultats rendus par [`query`].
pub const DEFAULT_LIMIT: usize = 100;

/// Étendue de la recherche.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    #[default]
    All,
    Contacts,
    Appointments,
    Tasks,
    Notes,
}

impl Scope {
    fn has(self, kind: Kind) -> bool {
        self == Scope::All || self.kind() == Some(kind)
    }

    fn kind(self) -> Option<Kind> {
        match self {
            Scope::All => None,
            Scope::Contacts => Some(Kind::Contact),
            Scope::Appointments => Some(Kind::Appointment),
            Scope::Tasks => Some(Kind::Task),
            Scope::Notes => Some(Kind::Note),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Contact,
    Appointment,
    Task,
    Note,
}

/// Un résultat. `id` suit les identifiants de l'interface : `p<message>` contact, `a<message>` rendez-vous,
/// `t<message>` tâche, `n<message>` note.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub kind: Kind,
    pub id: String,
    pub title: String,
    /// Extrait autour du mot trouvé (ou première information utile de l'élément).
    pub snippet: String,
    /// Rendez-vous : `AAAA-MM-JJ HH:MM` (heure locale, `AAAA-MM-JJ` si journée entière) ; tâche : échéance ; note :
    /// dernière modification ; contact : vide.
    pub when: String,
}

/// Champ d'un élément : libellé pour l'extrait, texte, et poids (un mot trouvé dans le titre classe mieux).
struct Field {
    text: String,
    title: bool,
}

struct Item {
    hit: Hit,
    fields: Vec<Field>,
}

/// [`query`] avec [`DEFAULT_LIMIT`] résultats au plus. Un type d'élément illisible est ignoré, pas fatal.
pub fn query(conn: &Connection, data_dir: &Path, text: &str, scope: Scope) -> Vec<Hit> {
    query_limit(conn, data_dir, text, scope, DEFAULT_LIMIT)
}

/// Recherche des mots de `text` (tous requis) ; les résultats dont le titre contient les mots passent en premier, puis
/// l'ordre est celui des types (contacts, rendez-vous, tâches, notes) et du titre.
pub fn query_limit(conn: &Connection, data_dir: &Path, text: &str, scope: Scope, limit: usize) -> Vec<Hit> {
    let terms: Vec<String> = sort_key(text).split_whitespace().map(str::to_string).collect();
    if terms.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut found: Vec<(u8, usize, String, Hit)> = Vec::new();
    let mut add = |items: Vec<Item>, order: usize| {
        for it in items {
            let folded: Vec<String> = it.fields.iter().map(|f| sort_key(&f.text)).collect();
            let all = folded.join("\n");
            if !terms.iter().all(|t| all.contains(t.as_str())) {
                continue;
            }
            let in_title = it.fields.iter().zip(&folded).any(|(f, s)| f.title && terms.iter().all(|t| s.contains(t.as_str())));
            let mut hit = it.hit;
            hit.snippet = snippet(&it.fields, &folded, &terms);
            found.push((u8::from(!in_title), order, sort_key(&hit.title), hit));
        }
    };
    if scope.has(Kind::Contact) {
        add(contacts(conn, data_dir).unwrap_or_default(), 0);
    }
    if scope.has(Kind::Appointment) {
        add(appointments(conn, data_dir).unwrap_or_default(), 1);
    }
    if scope.has(Kind::Task) {
        add(tasks(conn, data_dir).unwrap_or_default(), 2);
    }
    if scope.has(Kind::Note) {
        add(notes(conn, data_dir).unwrap_or_default(), 3);
    }
    found.sort_by(|a, b| (a.0, a.1, &a.2).cmp(&(b.0, b.1, &b.2)));
    found.into_iter().take(limit).map(|f| f.3).collect()
}

/// Extrait (une ligne, 80 caractères environ) : autour du premier mot trouvé dans un champ qui n'est pas le titre,
/// sinon le premier champ non vide qui n'est pas le titre.
fn snippet(fields: &[Field], folded: &[String], terms: &[String]) -> String {
    let body = fields.iter().zip(folded).filter(|(f, _)| !f.title);
    for (f, s) in body.clone() {
        for t in terms {
            if let Some(at) = s.find(t.as_str()) {
                return around(&f.text, s, at, t.len());
            }
        }
    }
    body.map(|(f, _)| flat(&f.text)).find(|t| !t.is_empty()).map(|t| t.chars().take(80).collect()).unwrap_or_default()
}

fn flat(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Fenêtre de texte original autour d'une position de la forme repliée (le repliement peut changer la longueur en
/// octets, pas le nombre de caractères sauf cas rares : on repasse par un tableau de positions).
fn around(orig: &str, folded: &str, at: usize, len: usize) -> String {
    let mut starts = Vec::new(); // octet replié où commence chaque caractère original
    let mut pos = 0;
    for c in orig.chars() {
        starts.push(pos);
        pos += sort_key(c.encode_utf8(&mut [0; 4])).len();
    }
    debug_assert_eq!(pos, folded.len());
    let chars: Vec<char> = orig.chars().collect();
    let first = starts.partition_point(|&p| p <= at).saturating_sub(1);
    let last = starts.partition_point(|&p| p < at + len).max(first + 1).min(chars.len());
    let (from, to) = (first.saturating_sub(30), (last + 50).min(chars.len()));
    let mut out = flat(&chars[from..to].iter().collect::<String>());
    if from > 0 {
        out.insert(0, '…');
    }
    if to < chars.len() {
        out.push('…');
    }
    out
}

/// Texte (corps) d'un `.eml` de la base.
fn body(data_dir: &Path, rel: &str) -> String {
    if rel.is_empty() {
        return String::new();
    }
    std::fs::read(thunderlook_store::resolve(data_dir, rel))
        .ok()
        .and_then(|b| mail_parser::MessageParser::default().parse(&b).and_then(|m| m.body_text(0).map(|t| t.into_owned())))
        .unwrap_or_default()
}

fn field(text: impl Into<String>, title: bool) -> Field {
    Field { text: text.into(), title }
}

/// Chemins `.eml` des éléments d'une classe : identifiant → chemin relatif.
fn eml_paths(conn: &Connection, class_like: &str) -> Result<HashMap<i64, String>, String> {
    let e = |x: thunderlook_store::rusqlite::Error| x.to_string();
    let mut st = conn.prepare("SELECT id, eml_path FROM message WHERE message_class LIKE ?1").map_err(e)?;
    st.query_map([class_like], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).map_err(e)?.collect::<Result<_, _>>().map_err(e)
}

fn digits(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

fn contacts(conn: &Connection, data_dir: &Path) -> Result<Vec<Item>, String> {
    let all = thunderlook_contacts::load(conn, None, None)?;
    let mut out = Vec::new();
    for c in all.contacts {
        let title = c.title();
        let mut fields = vec![field(title.clone(), true), field(c.display_name.clone(), true), field(format!("{} {}", c.given_name, c.surname), true)];
        fields.push(field(c.nickname.clone(), true));
        fields.push(field(c.company.clone(), false));
        fields.push(field([&c.job_title, &c.department].map(String::as_str).join(" "), false));
        for e in &c.emails {
            fields.push(field(e.address.clone(), false));
        }
        for (label, value) in &c.phones {
            fields.push(field(format!("{label} : {value} {}", digits(value)), false));
        }
        for a in [&c.work, &c.home, &c.other] {
            fields.push(field(a.lines().join(", "), false));
        }
        fields.push(field(c.web_page.clone(), false));
        fields.push(field(body(data_dir, c.eml_path.as_deref().unwrap_or("")), false));
        let subtitle = if !c.company.is_empty() && c.company != title { c.company.clone() } else { c.emails.first().map(|e| e.address.clone()).unwrap_or_default() };
        out.push(Item { hit: Hit { kind: Kind::Contact, id: format!("p{}", c.id), title, snippet: subtitle, when: String::new() }, fields });
    }
    Ok(out)
}

fn appointments(conn: &Connection, data_dir: &Path) -> Result<Vec<Item>, String> {
    let cal = thunderlook_calendar::load(conn)?;
    let mut out = Vec::new();
    for a in &cal.appointments {
        let tz = a.time_zone.unwrap_or(TimeZone::PARIS);
        let start = chrono::DateTime::from_timestamp_millis(a.start_ms).map(|t| tz.to_local(t.naive_utc()));
        // Un rendez-vous d'une journée entière est enregistré à minuit local.
        let when = start.map(|t| t.format(if a.all_day { "%Y-%m-%d" } else { "%Y-%m-%d %H:%M" }).to_string()).unwrap_or_default();
        let fields = vec![
            field(a.subject.clone(), true),
            field(a.location.clone().unwrap_or_default(), false),
            field(thunderlook_calendar::description(a, data_dir).unwrap_or_default(), false),
        ];
        out.push(Item { hit: Hit { kind: Kind::Appointment, id: format!("a{}", a.id), title: a.subject.clone(), snippet: String::new(), when }, fields });
    }
    Ok(out)
}

fn tasks(conn: &Connection, data_dir: &Path) -> Result<Vec<Item>, String> {
    let list = thunderlook_items::tasks(conn)?;
    let paths = eml_paths(conn, "IPM.Task%")?;
    Ok(list
        .rows
        .into_iter()
        .map(|t| {
            let path = t.id.strip_prefix('t').and_then(|n| n.parse::<i64>().ok()).and_then(|n| paths.get(&n)).cloned().unwrap_or_default();
            let fields = vec![field(t.title.clone(), true), field(body(data_dir, &path), false), field(t.categories.join(", "), false)];
            Item { hit: Hit { kind: Kind::Task, id: t.id, title: t.title, snippet: String::new(), when: t.due }, fields }
        })
        .collect())
}

fn notes(conn: &Connection, data_dir: &Path) -> Result<Vec<Item>, String> {
    let list = thunderlook_items::notes(conn, data_dir)?;
    let paths = eml_paths(conn, "IPM.StickyNote%")?;
    Ok(list
        .rows
        .into_iter()
        .map(|n| {
            let path = n.id.strip_prefix('n').and_then(|x| x.parse::<i64>().ok()).and_then(|x| paths.get(&x)).cloned().unwrap_or_default();
            let fields = vec![field(n.title.clone(), true), field(body(data_dir, &path), false)];
            Item { hit: Hit { kind: Kind::Note, id: n.id, title: n.title, snippet: String::new(), when: n.date }, fields }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use thunderlook_calendar::write::Edit;
    use thunderlook_model::{contacts::ContactForm, items::{NoteForm, TaskForm}};

    const PSETID_APPOINTMENT: &str = "00062002-0000-0000-C000-000000000046";
    const PSETID_TASK: &str = "00062003-0000-0000-C000-000000000046";

    /// Base de test : une source dont la table de noms a les propriétés des rendez-vous et des tâches, un dossier par type.
    fn fixture(name: &str) -> (std::path::PathBuf, Connection) {
        let dir = std::env::temp_dir().join(format!("tl-search-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let conn = thunderlook_store::open(&dir).unwrap();
        let mut names = Vec::new();
        let appointment = [0x820Du32, 0x820E, 0x8215, 0x8208, 0x8205, 0x8223, 0x8216, 0x8217, 0x8218].map(|l| (PSETID_APPOINTMENT, l));
        let task = [0x8101u32, 0x8102, 0x8104, 0x8105, 0x810F, 0x811C].map(|l| (PSETID_TASK, l));
        for (i, (set, lid)) in appointment.into_iter().chain(task).enumerate() {
            names.extend_from_slice(&(0x8000 + i as u16).to_le_bytes());
            names.extend_from_slice(set.as_bytes());
            names.push(0);
            names.extend_from_slice(&lid.to_le_bytes());
        }
        conn.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at, props) VALUES (1, 'tl', '', '', 't', 0, 0, ?1)", [names]).unwrap();
        for (id, name) in [(1, "Calendrier"), (2, "Contacts"), (3, "Tâches"), (4, "Notes")] {
            conn.execute("INSERT INTO folder (id, source_id, nid, name, role, hidden, position) VALUES (?1, 1, ?1, ?2, 'folder', 0, 0)", (id, name)).unwrap();
        }
        (dir, conn)
    }

    fn filled(name: &str) -> (std::path::PathBuf, Connection) {
        let (dir, mut conn) = fixture(name);
        let contact = |given: &str, surname: &str, company: &str, mobile: &str, notes: &str| ContactForm {
            folder_id: "k2".into(),
            given_name: given.into(),
            surname: surname.into(),
            company: company.into(),
            mobile: mobile.into(),
            notes: notes.into(),
            ..Default::default()
        };
        thunderlook_contacts::write::create(&mut conn, &dir, 2, &contact("Élodie", "Lefèvre", "Boulangerie Soleil", "06 12 34 56 78", "Allergique aux noisettes")).unwrap();
        thunderlook_contacts::write::create(&mut conn, &dir, 2, &contact("Paul", "Durand", "ADI", "", "")).unwrap();
        let appt = |subject: &str, location: &str, body: &str, start: &str, end: &str| Edit {
            subject: subject.into(),
            location: location.into(),
            body: body.into(),
            start: start.into(),
            end: end.into(),
            ..Default::default()
        };
        thunderlook_calendar::write::create(&mut conn, &dir, 1, &appt("Réunion chantier", "Fenioux", "Apporter les plans de l'étage", "2026-11-03T09:00", "2026-11-03T10:00")).unwrap();
        thunderlook_calendar::write::create(&mut conn, &dir, 1, &appt("Dentiste", "Cabinet Soleil", "", "2026-11-04T14:30", "2026-11-04T15:00")).unwrap();
        let task = |subject: &str, due: &str, body: &str| TaskForm { folder_id: "f3".into(), subject: subject.into(), due: due.into(), body: body.into(), ..Default::default() };
        thunderlook_items::task_save(&mut conn, &dir, &task("Renouveler l'assurance", "2026-12-01", "Contrat habitation, voir Élodie")).unwrap();
        thunderlook_items::note_save(&mut conn, &dir, &NoteForm { folder_id: "f4".into(), body: "Code du portail\nLe chantier commence lundi".into(), color: 3, ..Default::default() }).unwrap();
        (dir, conn)
    }

    fn titles(hits: &[Hit]) -> Vec<&str> {
        hits.iter().map(|h| h.title.as_str()).collect()
    }

    #[test]
    fn case_and_accents_are_ignored() {
        let (dir, conn) = filled("accents");
        for q in ["elodie", "ÉLODIE", "lefevre", "Lefèvre élodie"] {
            let hits = query(&conn, &dir, q, Scope::Contacts);
            assert_eq!(hits.len(), 1, "{q}");
            assert_eq!(hits[0].kind, Kind::Contact);
            assert!(hits[0].id.starts_with('p'));
        }
        assert!(query(&conn, &dir, "inconnu", Scope::All).is_empty());
        assert!(query(&conn, &dir, "   ", Scope::All).is_empty());
    }

    #[test]
    fn every_kind_is_searched() {
        let (dir, conn) = filled("kinds");
        let kinds = |q: &str| query(&conn, &dir, q, Scope::All).iter().map(|h| h.kind).collect::<Vec<_>>();
        assert_eq!(kinds("chantier"), [Kind::Appointment, Kind::Note]); // objet du rendez-vous, texte de la note
        assert_eq!(kinds("plans etage"), [Kind::Appointment]); // description
        assert_eq!(kinds("fenioux"), [Kind::Appointment]); // lieu
        assert_eq!(kinds("noisettes"), [Kind::Contact]); // notes du contact
        assert_eq!(kinds("assurance"), [Kind::Task]);
        assert_eq!(kinds("soleil"), [Kind::Contact, Kind::Appointment]); // société et lieu
        assert_eq!(kinds("elodie"), [Kind::Contact, Kind::Task]); // nom, puis description de la tâche
    }

    #[test]
    fn scope_restricts_the_search() {
        let (dir, conn) = filled("scope");
        assert_eq!(titles(&query(&conn, &dir, "chantier", Scope::Notes)), ["Code du portail"]);
        assert_eq!(titles(&query(&conn, &dir, "chantier", Scope::Appointments)), ["Réunion chantier"]);
        assert!(query(&conn, &dir, "chantier", Scope::Tasks).is_empty());
    }

    #[test]
    fn hits_carry_id_when_and_snippet() {
        let (dir, conn) = filled("fields");
        let a = &query(&conn, &dir, "plans", Scope::All)[0];
        assert_eq!((a.when.as_str(), a.title.as_str()), ("2026-11-03 09:00", "Réunion chantier"));
        assert!(a.id.starts_with('a') && a.snippet.contains("plans de l'étage"), "{a:?}");
        let t = &query(&conn, &dir, "assurance", Scope::All)[0];
        assert_eq!(t.when, "2026-12-01");
        assert!(t.id.starts_with('t'));
        let n = &query(&conn, &dir, "lundi", Scope::All)[0];
        assert!(n.id.starts_with('n') && n.snippet.contains("commence lundi"), "{n:?}");
        let c = &query(&conn, &dir, "0612", Scope::All)[0]; // numéro saisi sans espaces
        assert_eq!(c.title, "Élodie Lefèvre"); // pas de « Classer sous » dans la base de test
        assert!(c.snippet.contains("06 12 34 56 78"), "{c:?}");
    }

    #[test]
    fn title_matches_come_first_and_limit_applies() {
        let (dir, conn) = filled("rank");
        // « soleil » est dans la société du contact (pas le titre) et dans le lieu du rendez-vous : ordre des types.
        // « chantier » est dans le titre du rendez-vous et dans le texte de la note : le titre passe avant.
        let hits = query(&conn, &dir, "chantier", Scope::All);
        assert_eq!((hits[0].kind, hits[1].kind), (Kind::Appointment, Kind::Note));
        assert_eq!(query_limit(&conn, &dir, "e", Scope::All, 2).len(), 2);
        assert!(query_limit(&conn, &dir, "e", Scope::All, 0).is_empty());
    }

    #[test]
    fn snippet_window_keeps_original_text() {
        let long = format!("{} Éléphant ÉCRU {}", "x".repeat(100), "y".repeat(100));
        let fields = [field("titre", true), field(long.clone(), false)];
        let folded: Vec<String> = fields.iter().map(|f| sort_key(&f.text)).collect();
        let s = snippet(&fields, &folded, &["ecru".to_string()]);
        assert!(s.starts_with('…') && s.ends_with('…') && s.contains("ÉCRU"), "{s}");
        assert!(s.chars().count() < 100);
    }
}
