//! Rappels (lot D) : liste des rappels à échéance pour les rendez-vous (une échéance par occurrence d'une série) et les
//! tâches, avec l'état « ignoré » ou « reporté » gardé dans `reminders.json` (dossier de données).

use std::{collections::HashMap, path::Path};

use chrono::NaiveDate;
use thunderlook_calendar::{
    bag::Bag,
    expand,
    named::{Named, PSETID_COMMON},
    tz::TimeZone,
};
use thunderlook_store::rusqlite::Connection;

pub mod state;

pub use state::{dismiss, mail_clear, mail_get, mail_set, snooze};

type Result<T> = std::result::Result<T, String>;

const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;
const PSETID_TASK: &str = "00062003-0000-0000-C000-000000000046";

/// Report maximal pris en compte pour retrouver un rappel reporté dont l'heure d'origine est ancienne (Outlook
/// plafonne le report à deux semaines).
const SNOOZE_REACH_MS: i64 = 14 * DAY_MS;

/// Plage des rappels à rendre, autour de l'instant courant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub lookback_ms: i64,
    pub lookahead_ms: i64,
}

impl Default for Window {
    /// 24 h en arrière : plafond proposé pour les rappels manqués (ThunderLook fermé), voir `REA-LIMITES.md`.
    fn default() -> Self {
        Self { lookback_ms: DAY_MS, lookahead_ms: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Appointment,
    Task,
    /// Message suivi (indicateur) avec rappel posé dans ThunderLook.
    Mail,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Reminder {
    /// Stable : `a<rendez-vous>@<AAAA-MM-JJ>` (jour local de l'occurrence), `t<tâche>` ou `m<message>`.
    pub id: String,
    pub kind: Kind,
    pub title: String,
    /// Heure du rappel (UTC, ms).
    pub when_ms: i64,
    /// Début du rendez-vous (UTC, ms) ; échéance de la tâche à 8 h si elle n'a pas d'heure ; `when_ms` sinon.
    pub event_start_ms: i64,
    pub location: Option<String>,
}

fn e(x: thunderlook_store::rusqlite::Error) -> String {
    x.to_string()
}

fn local_day(tz: &TimeZone, ms: i64) -> NaiveDate {
    tz.to_local(chrono::DateTime::from_timestamp_millis(ms).map(|t| t.naive_utc()).unwrap_or_default()).date()
}

/// Rappels dont l'heure est dans `[now - lookback, now + lookahead]`, hors rappels ignorés ou reportés plus tard,
/// triés par heure. Un rappel reporté revient à l'heure du report (même si son heure d'origine est plus ancienne que
/// `lookback`, dans la limite de 14 jours).
pub fn due(conn: &Connection, data_dir: &Path, now_ms: i64, window: Window) -> Result<Vec<Reminder>> {
    let st = state::load(data_dir);
    let (from, to) = (now_ms - window.lookback_ms, now_ms + window.lookahead_ms);
    let reach = from.min(now_ms - SNOOZE_REACH_MS);
    let mut out: Vec<Reminder> = Vec::new();
    let mut keep = |r: Reminder| {
        let until = st.dismissed.get(&r.id).copied();
        // Un rappel ignoré (`i64::MAX`) ou reporté plus tard reste caché ; reporté puis échu, il compte à l'heure du report.
        let eff = match until {
            Some(u) if u > now_ms => return,
            Some(u) => r.when_ms.max(u),
            None => r.when_ms,
        };
        if (from..=to).contains(&eff) {
            out.push(r);
        }
    };

    let mut cal = thunderlook_calendar::load(conn)?;
    let (imported, _) = thunderlook_calendar::imported::load(data_dir);
    cal.appointments.extend(imported.into_iter().flat_map(|(_, items)| items));
    for a in cal.appointments.iter() {
        let Some(minutes) = a.reminder_minutes.filter(|m| *m >= 0) else { continue };
        let lead = minutes * MINUTE_MS;
        let tz = a.time_zone.unwrap_or(TimeZone::local());
        for o in expand::occurrences(a, reach + lead, to + lead + 1) {
            let when_ms = o.start_ms - lead;
            let day = o.original_date.unwrap_or_else(|| local_day(&tz, o.start_ms));
            keep(Reminder {
                id: format!("a{}@{}", a.id, day.format("%Y-%m-%d")),
                kind: Kind::Appointment,
                title: o.subject,
                when_ms,
                event_start_ms: o.start_ms,
                location: o.location,
            });
        }
    }
    for r in task_reminders(conn)? {
        keep(r);
    }
    for r in mail_reminders(conn, &st.mail)? {
        keep(r);
    }
    out.sort_by(|a, b| (a.when_ms, &a.id).cmp(&(b.when_ms, &b.id)));
    Ok(out)
}

/// Rappels de suivi des messages : seulement ceux dont l'heure a été posée (dialogue « Personnalisé… ») et dont
/// l'indicateur est toujours posé (`flag_status = 2`) ; terminé, effacé ou message supprimé : plus de rappel.
/// Début affiché = échéance de l'indicateur, sinon l'heure du rappel.
fn mail_reminders(conn: &Connection, mail: &std::collections::BTreeMap<String, i64>) -> Result<Vec<Reminder>> {
    let mut st = conn
        .prepare("SELECT m.subject, m.from_name, m.flag_status, m.flag_due, f.name FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1")
        .map_err(e)?;
    let mut out = Vec::new();
    for (id, when_ms) in mail {
        let Some(n) = id.strip_prefix('m').and_then(|n| n.parse::<i64>().ok()) else { continue };
        let row = st
            .query_row([n], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, String>(4)?)))
            .ok();
        let Some((subject, from, status, due, folder)) = row else { continue };
        if status != 2 || is_deleted(&folder) {
            continue;
        }
        let location = Some(from.trim().to_string()).filter(|f| !f.is_empty()).map(|f| format!("De {f}"));
        out.push(Reminder { id: id.clone(), kind: Kind::Mail, title: subject, when_ms: *when_ms, event_start_ms: due.unwrap_or(*when_ms), location });
    }
    Ok(out)
}

fn is_deleted(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("supprim") || n == "deleted items"
}

fn is_trash(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("supprim") || n == "deleted items" || n.contains("sent") || n.contains("envoy")
}

/// Rappels des tâches non terminées : `PidLidReminderTime` (0x8502) si `PidLidReminderSet` (0x8503), sinon rien ; sans
/// heure, l'échéance (`PidLidTaskDueDate`, date flottante) à 8 h, fuseau de l'utilisateur.
fn task_reminders(conn: &Connection) -> Result<Vec<Reminder>> {
    let mut named: HashMap<i64, Named> = HashMap::new();
    {
        let mut st = conn.prepare("SELECT id, props FROM source WHERE props IS NOT NULL").map_err(e)?;
        let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))).map_err(e)?;
        for r in rows {
            let (id, raw) = r.map_err(e)?;
            named.insert(id, Named::parse(&raw)?);
        }
    }
    let mut st = conn
        .prepare(
            "SELECT m.id, f.source_id, f.name, m.subject, m.props FROM message m JOIN folder f ON f.id = m.folder_id
             WHERE m.message_class = 'IPM.Task' OR m.message_class LIKE 'IPM.Task.%'",
        )
        .map_err(e)?;
    let mut rows = st.query([]).map_err(e)?;
    let mut out = Vec::new();
    while let Some(r) = rows.next().map_err(e)? {
        let (id, source, folder): (i64, i64, String) = (r.get(0).map_err(e)?, r.get(1).map_err(e)?, r.get(2).map_err(e)?);
        if is_trash(&folder) {
            continue;
        }
        let subject: String = r.get(3).map_err(e)?;
        let props: Vec<u8> = r.get(4).map_err(e)?;
        let (Some(n), Ok(bag)) = (named.get(&source), Bag::parse(&props)) else { continue };
        let task = |lid| n.id(PSETID_TASK, lid);
        let common = |lid| n.id(PSETID_COMMON, lid);
        let done = task(0x811C).and_then(|i| bag.bool(i)).unwrap_or(false) || task(0x8101).and_then(|i| bag.int(i)) == Some(2);
        if done || !common(0x8503).and_then(|i| bag.bool(i)).unwrap_or(false) {
            continue;
        }
        let due_ms = task(0x8105).and_then(|i| bag.time_ms(i));
        let at_8 = due_ms.and_then(|d| {
            let day = chrono::DateTime::from_timestamp_millis(d)?.naive_utc().date();
            Some(TimeZone::local().to_utc(day.and_hms_opt(8, 0, 0)?).and_utc().timestamp_millis())
        });
        let Some(when_ms) = common(0x8502).and_then(|i| bag.time_ms(i)).or(at_8) else { continue };
        out.push(Reminder { id: format!("t{id}"), kind: Kind::Task, title: subject, when_ms, event_start_ms: at_8.unwrap_or(when_ms), location: None });
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
