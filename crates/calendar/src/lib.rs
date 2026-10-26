//! Calendrier (J9) : rendez-vous lus depuis les éléments `IPM.Appointment` importés, dont le sac MAPI complet est
//! conservé dans `message.props`. Option A du plan (`notes/j9-plan.md`) : Outlook fait foi, lecture seule ici.

pub mod bag;
pub mod csv;
pub mod expand;
pub mod ics;
pub mod ics_series;
pub mod imported;
pub mod itip;
pub mod named;
pub mod recur;
pub mod tz;
pub mod write;
pub mod view;

use std::collections::HashMap;

use thunderlook_store::rusqlite::Connection;

use crate::{
    bag::Bag,
    named::{Named, PSETID_APPOINTMENT, PSETID_COMMON, PSETID_MEETING, PS_PUBLIC_STRINGS},
    recur::Recurrence,
    tz::TimeZone,
};

pub type Result<T> = std::result::Result<T, String>;

/// Disponibilité affichée (`PidLidBusyStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Busy {
    Free,
    Tentative,
    Busy,
    OutOfOffice,
    WorkingElsewhere,
}

#[derive(Clone, Debug)]
pub struct Appointment {
    pub id: i64,
    /// `PidLidCleanGlobalObjectId` en hexadécimal majuscule : l'`UID` iCalendar qu'Outlook exporte.
    pub uid: Option<String>,
    pub source_id: i64,
    pub folder_id: i64,
    pub subject: String,
    pub location: Option<String>,
    /// Début et fin en millisecondes Unix (UTC) ; pour une série, ceux de la première occurrence.
    pub start_ms: i64,
    pub end_ms: i64,
    pub all_day: bool,
    pub busy: Busy,
    pub recurrence: Option<Recurrence>,
    /// Fuseau de la série (`PidLidTimeZoneStruct`) ; Europe/Paris s'il manque.
    pub time_zone: Option<TimeZone>,
    pub time_zone_name: Option<String>,
    /// Rappel en minutes avant le début, s'il est actif.
    pub reminder_minutes: Option<i64>,
    /// Délai du rappel même inactif (`PidLidReminderDelta`) : l'export CSV d'Outlook en déduit la date de rappel.
    pub reminder_delta: Option<i64>,
    pub categories: Vec<String>,
    /// Réunion (`asfMeeting` de `PidLidAppointmentStateFlags`) ; l'organisateur est l'expéditeur.
    pub is_meeting: bool,
    pub organizer: String,
    pub attendees: String,
    /// `PidLidResponseStatus` : 0 aucune, 1 organisée, 2 provisoire, 3 acceptée, 4 refusée, 5 non répondu.
    pub response_status: i64,
    pub private: bool,
    /// `PidTagSensitivity` : 0 normal, 1 personnel, 2 privé, 3 confidentiel.
    pub sensitivity: i64,
    /// 0 basse, 1 normale, 2 haute.
    pub importance: i64,
    /// `.eml` de l'élément (relatif au dossier de données), d'où se lit sa description.
    pub eml_path: Option<String>,
    /// Description déjà connue (calendrier importé) ; sinon lue dans le `.eml`.
    pub description: Option<String>,
}

/// Identifiants locaux des propriétés nommées utiles, pour une source.
struct Ids {
    busy: Option<u16>,
    location: Option<u16>,
    start: Option<u16>,
    end: Option<u16>,
    sub_type: Option<u16>,
    recur: Option<u16>,
    state: Option<u16>,
    response: Option<u16>,
    recurring: Option<u16>,
    tz: Option<u16>,
    tz_name: Option<u16>,
    reminder_delta: Option<u16>,
    reminder_set: Option<u16>,
    clean_goid: Option<u16>,
    keywords: Option<u16>,
}

impl Ids {
    fn new(n: &Named) -> Self {
        let a = |lid| n.id(PSETID_APPOINTMENT, lid);
        let c = |lid| n.id(PSETID_COMMON, lid);
        Self {
            busy: a(0x8205),
            location: a(0x8208),
            start: a(0x820D),
            end: a(0x820E),
            sub_type: a(0x8215),
            recur: a(0x8216),
            state: a(0x8217),
            response: a(0x8218),
            recurring: a(0x8223),
            tz: a(0x8233),
            tz_name: a(0x8234),
            reminder_delta: c(0x8501),
            reminder_set: c(0x8503),
            clean_goid: n.id(PSETID_MEETING, 0x0023),
            keywords: n.name(PS_PUBLIC_STRINGS, "Keywords"),
        }
    }
}

/// Propriétés MAPI fixes.
const PR_START_DATE: u16 = 0x0060;
const PR_END_DATE: u16 = 0x0061;

/// Un élément qui n'a pas pu être lu entièrement, et pourquoi.
#[derive(Clone, Debug)]
pub struct Issue {
    pub id: i64,
    pub subject: String,
    pub detail: String,
}

pub struct Calendar {
    pub appointments: Vec<Appointment>,
    pub issues: Vec<Issue>,
}

fn read(bag: &Bag, ids: &Ids, row: &Row) -> Result<Appointment> {
    let start_ms = ids.start.and_then(|i| bag.time_ms(i)).or_else(|| bag.time_ms(PR_START_DATE)).ok_or("début absent")?;
    let end_ms = ids.end.and_then(|i| bag.time_ms(i)).or_else(|| bag.time_ms(PR_END_DATE)).ok_or("fin absente")?;
    let recurring = ids.recurring.and_then(|i| bag.bool(i)).unwrap_or(false);
    let recurrence = match (recurring, ids.recur.and_then(|i| bag.binary(i))) {
        (true, Some(b)) => Some(Recurrence::parse(b)?),
        (true, None) => return Err("rendez-vous périodique sans PidLidAppointmentRecur".into()),
        (false, _) => None,
    };
    let time_zone = match ids.tz.and_then(|i| bag.binary(i)) {
        Some(b) => Some(TimeZone::parse(b)?),
        None => None,
    };
    let reminder_set = ids.reminder_set.and_then(|i| bag.bool(i)).unwrap_or(false);
    let busy = match ids.busy.and_then(|i| bag.int(i)).unwrap_or(2) {
        0 => Busy::Free,
        1 => Busy::Tentative,
        3 => Busy::OutOfOffice,
        4 => Busy::WorkingElsewhere,
        _ => Busy::Busy,
    };
    Ok(Appointment {
        id: row.id,
        uid: ids.clean_goid.and_then(|i| bag.binary(i)).map(|b| b.iter().map(|x| format!("{x:02X}")).collect()),
        source_id: row.source_id,
        folder_id: row.folder_id,
        subject: row.subject.clone(),
        location: ids.location.and_then(|i| bag.text(i)),
        start_ms,
        end_ms,
        all_day: ids.sub_type.and_then(|i| bag.bool(i)).unwrap_or(false),
        busy,
        recurrence,
        time_zone,
        time_zone_name: ids.tz_name.and_then(|i| bag.text(i)),
        reminder_minutes: if reminder_set { ids.reminder_delta.and_then(|i| bag.int(i)).or(Some(15)) } else { None },
        reminder_delta: ids.reminder_delta.and_then(|i| bag.int(i)),
        // Ordre d'origine des catégories ; à défaut, la table `message_category` (triée par nom).
        categories: ids.keywords.and_then(|i| bag.strings(i)).unwrap_or_default(),
        is_meeting: ids.state.and_then(|i| bag.int(i)).is_some_and(|s| s & 1 != 0),
        organizer: row.from_name.clone(),
        attendees: row.to_line.clone(),
        response_status: ids.response.and_then(|i| bag.int(i)).unwrap_or(0),
        private: row.sensitivity == 2,
        sensitivity: row.sensitivity,
        importance: row.importance,
        eml_path: Some(row.eml_path.clone()).filter(|p| !p.is_empty()),
        description: None,
    })
}

struct Row {
    id: i64,
    source_id: i64,
    folder_id: i64,
    subject: String,
    from_name: String,
    to_line: String,
    sensitivity: i64,
    importance: i64,
    eml_path: String,
}

/// Classe d'un rendez-vous : `IPM.Appointment` ou une classe dérivée (`IPM.Appointment.…`). `LIKE` de la requête
/// ratisse un peu plus large (`IPM.AppointmentX`) pour pouvoir passer par l'index `message_class_nocase`.
fn is_appointment_class(class: &str) -> bool {
    class == "IPM.Appointment" || class.get(..16).is_some_and(|p| p.eq_ignore_ascii_case("IPM.Appointment."))
}

/// Tous les rendez-vous de la base (`IPM.Appointment` et classes dérivées), avec les éléments illisibles.
pub fn load(conn: &Connection) -> Result<Calendar> {
    load_where(conn, None)
}

/// Les rendez-vous `ids` seulement (ceux qui n'existent plus, ou ne sont pas des rendez-vous, sont absents).
pub fn load_ids(conn: &Connection, ids: &[i64]) -> Result<Calendar> {
    load_where(conn, Some(ids))
}

/// Empreinte d'un rendez-vous : dossier, date de modification et taille du sac. Toute écriture (`write`) change l'une
/// des trois ; elle sert à ne relire que ce qui a changé.
pub type Stamp = (i64, Option<i64>, i64);

/// Empreintes de tous les rendez-vous de la base, sans lire leurs sacs (`length` ne charge pas la valeur).
pub fn stamps(conn: &Connection) -> Result<HashMap<i64, Stamp>> {
    let e = |x: thunderlook_store::rusqlite::Error| x.to_string();
    let mut stmt = conn
        .prepare("SELECT m.id, m.folder_id, m.modified_at, length(m.props), m.message_class FROM message m WHERE m.message_class LIKE 'IPM.Appointment%'")
        .map_err(e)?;
    let mut rows = stmt.query([]).map_err(e)?;
    let mut out = HashMap::new();
    while let Some(r) = rows.next().map_err(e)? {
        if is_appointment_class(&r.get::<_, String>(4).map_err(e)?) {
            out.insert(r.get(0).map_err(e)?, (r.get(1).map_err(e)?, r.get(2).map_err(e)?, r.get::<_, Option<i64>>(3).map_err(e)?.unwrap_or(0)));
        }
    }
    Ok(out)
}

/// Met à jour `cal` (rendez-vous de la base seulement : ceux des calendriers importés, d'identifiant négatif, sont
/// retirés) d'après les empreintes `old` de sa lecture précédente : seuls les rendez-vous modifiés, ajoutés ou supprimés
/// depuis sont relus. Le résultat est identique à [`load`] (même ordre, par identifiant). Rend aussi les empreintes
/// actuelles, à repasser au prochain appel.
pub fn refresh(mut cal: Calendar, old: &HashMap<i64, Stamp>, conn: &Connection) -> Result<(Calendar, HashMap<i64, Stamp>)> {
    let now = stamps(conn)?;
    let changed: Vec<i64> = now.iter().filter(|(id, s)| old.get(id) != Some(s)).map(|(id, _)| *id).collect();
    let removed = old.keys().filter(|id| !now.contains_key(id)).count();
    // Rien lu avant, ou presque tout changé : une lecture complète coûte moins qu'un va-et-vient.
    if old.is_empty() || changed.len() + removed > now.len() / 2 {
        return Ok((load(conn)?, now));
    }
    let fresh = load_ids(conn, &changed)?;
    let keep = |id: i64| id > 0 && now.contains_key(&id) && !changed.contains(&id);
    cal.appointments.retain(|a| keep(a.id));
    cal.issues.retain(|i| keep(i.id));
    cal.appointments.extend(fresh.appointments);
    cal.issues.extend(fresh.issues);
    cal.appointments.sort_by_key(|a| a.id);
    cal.issues.sort_by_key(|i| i.id);
    Ok((cal, now))
}

fn load_where(conn: &Connection, ids: Option<&[i64]>) -> Result<Calendar> {
    let e = |x: thunderlook_store::rusqlite::Error| x.to_string();
    let mut named: HashMap<i64, Ids> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT id, props FROM source WHERE props IS NOT NULL").map_err(e)?;
        let mut rows = stmt.query([]).map_err(e)?;
        while let Some(r) = rows.next().map_err(e)? {
            let raw: Vec<u8> = r.get(1).map_err(e)?;
            named.insert(r.get(0).map_err(e)?, Ids::new(&Named::parse(&raw)?));
        }
    }
    // Entiers formatés par nos soins : pas d'injection possible.
    let only = ids.map(|ids| format!(" AND m.id IN ({})", ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")));
    let only = only.as_deref().unwrap_or("");
    let mut categories: HashMap<i64, Vec<String>> = HashMap::new();
    {
        // `CROSS JOIN` : on part des rendez-vous (index de classe), pas de toute la table des catégories.
        let mut stmt = conn
            .prepare(&format!(
                "SELECT c.message_id, c.name FROM message m CROSS JOIN message_category c ON c.message_id = m.id WHERE m.message_class LIKE 'IPM.Appointment%'{only}"
            ))
            .map_err(e)?;
        let mut rows = stmt.query([]).map_err(e)?;
        while let Some(r) = rows.next().map_err(e)? {
            categories.entry(r.get(0).map_err(e)?).or_default().push(r.get(1).map_err(e)?);
        }
    }
    let mut stmt = conn
        .prepare(&format!(
            "SELECT m.id, f.source_id, m.folder_id, m.subject, m.from_name, m.to_line, m.sensitivity, m.props, m.importance, m.eml_path, m.message_class
             FROM message m CROSS JOIN folder f ON f.id = m.folder_id
             WHERE m.message_class LIKE 'IPM.Appointment%'{only} ORDER BY m.id"
        ))
        .map_err(e)?;
    let mut rows = stmt.query([]).map_err(e)?;
    let (mut appointments, mut issues) = (Vec::new(), Vec::new());
    let none = Ids::new(&Named::default());
    while let Some(r) = rows.next().map_err(e)? {
        if !is_appointment_class(&r.get::<_, String>(10).map_err(e)?) {
            continue;
        }
        let row = Row {
            id: r.get(0).map_err(e)?,
            source_id: r.get(1).map_err(e)?,
            folder_id: r.get(2).map_err(e)?,
            subject: r.get(3).map_err(e)?,
            from_name: r.get(4).map_err(e)?,
            to_line: r.get(5).map_err(e)?,
            sensitivity: r.get(6).map_err(e)?,
            importance: r.get(8).map_err(e)?,
            eml_path: r.get(9).map_err(e)?,
        };
        let props: Vec<u8> = r.get(7).map_err(e)?;
        let result = Bag::parse(&props).and_then(|bag| read(&bag, named.get(&row.source_id).unwrap_or(&none), &row));
        match result {
            Ok(mut a) => {
                let listed = categories.remove(&a.id).unwrap_or_default();
                if a.categories.is_empty() {
                    a.categories = listed;
                }
                appointments.push(a);
            }
            Err(detail) => issues.push(Issue { id: row.id, subject: row.subject, detail }),
        }
    }
    Ok(Calendar { appointments, issues })
}

/// Description (corps en texte) d'un rendez-vous : celle d'un calendrier importé, sinon le texte de son `.eml`.
pub fn description(a: &Appointment, data_dir: &std::path::Path) -> Option<String> {
    if a.description.is_some() {
        return a.description.clone();
    }
    let bytes = std::fs::read(thunderlook_store::resolve(data_dir, a.eml_path.as_deref()?)).ok()?;
    let msg = mail_parser::MessageParser::default().parse(&bytes)?;
    let text = msg.body_text(0)?.into_owned();
    (!text.trim().is_empty()).then_some(text)
}
