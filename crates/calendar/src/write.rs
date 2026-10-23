//! Écriture des rendez-vous dans la base de ThunderLook (sans Outlook) : création, modification, suppression.
//! Les propriétés sont écrites dans le sac MAPI de la ligne, comme celles qu'Outlook y aurait mises, pour que la lecture
//! (`load`), l'export et les vues n'aient rien à savoir de l'origine du rendez-vous. La description va dans un `.eml`
//! minimal (`eml/<source>/tl/<id>.eml`).

use std::path::Path;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use thunderlook_store::rusqlite::{Connection, OptionalExtension, params};

use crate::{
    bag::{self, Value},
    expand,
    recur::{End, Frequency, Pattern, Recurrence, local_to_minutes, minutes_to_local},
    named::{Named, PS_PUBLIC_STRINGS, PSETID_APPOINTMENT, PSETID_COMMON, PSETID_MEETING},
    tz::TimeZone,
};

type Result<T> = std::result::Result<T, String>;

/// Répétition d'un nouveau rendez-vous : « tous les `interval` jours, semaines, mois ou ans » (même jour de la semaine,
/// du mois ou de l'année que le début), jusqu'à une fin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repeat {
    pub frequency: Frequency,
    pub interval: u32,
    pub end: RepeatEnd,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepeatEnd {
    Never,
    Count(u32),
    /// Dernier jour (`AAAA-MM-JJ`, inclus).
    Until(String),
}

/// Rendez-vous saisi. Dates locales (heure de Paris) `AAAA-MM-JJTHH:MM`.
#[derive(Clone, Debug, Default)]
pub struct Edit {
    pub subject: String,
    pub location: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
    /// Minutes avant le début ; `None` : pas de rappel.
    pub reminder: Option<i32>,
    /// 0 disponible, 1 provisoire, 2 occupé(e), 3 absent(e), 4 travail ailleurs.
    pub busy: i32,
    pub body: String,
    /// Catégories séparées par des virgules.
    pub categories: String,
    pub private: bool,
    /// Nouveau rendez-vous périodique (création seulement : une série existante garde sa périodicité).
    pub repeat: Option<Repeat>,
}

fn e(x: thunderlook_store::rusqlite::Error) -> String {
    x.to_string()
}

fn utc_ms(local: &str) -> Result<i64> {
    let t = NaiveDateTime::parse_from_str(local, "%Y-%m-%dT%H:%M").map_err(|_| format!("date invalide : {local}"))?;
    Ok(TimeZone::PARIS.to_utc(t).and_utc().timestamp_millis())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

fn named_of(conn: &Connection, source_id: i64) -> Result<Named> {
    let raw: Vec<u8> = conn.query_row("SELECT props FROM source WHERE id = ?1", [source_id], |r| r.get::<_, Option<Vec<u8>>>(0)).map_err(e)?.unwrap_or_default();
    Named::parse(&raw)
}

fn preview(body: &str) -> String {
    body.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect()
}

fn write_eml(data_dir: &Path, source_id: i64, id: i64, subject: &str, body: &str) -> Result<String> {
    let rel = format!("eml/{source_id}/tl/{id}.eml");
    let path = thunderlook_store::resolve(data_dir, &rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|x| x.to_string())?;
    }
    let subject = subject.replace(['\r', '\n'], " ");
    let text = format!("MIME-Version: 1.0\r\nSubject: {subject}\r\nContent-Type: text/plain; charset=\"utf-8\"\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{body}");
    std::fs::write(&path, text).map_err(|x| x.to_string())?;
    Ok(rel)
}

const NEVER_END: u32 = 0x5AE9_80DF;

/// Périodicité d'un nouveau rendez-vous, au format des séries qu'Outlook écrit (FirstDateTime, Period… relevés sur les
/// séries de la base).
fn recurrence_of(edit: &Edit, rep: &Repeat) -> Result<Recurrence> {
    let parse = |t: &str| NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M").map_err(|_| format!("date invalide : {t}"));
    let (start, end) = (parse(&edit.start)?, parse(&edit.end)?);
    let date = start.date();
    let interval = rep.interval.clamp(1, 999);
    let midnight = |d: NaiveDate| local_to_minutes(d.and_hms_opt(0, 0, 0).unwrap_or_default());
    let month_index = (date.year() - 1601) * 12 + date.month0() as i32;
    let first_of_month = |idx: u32| {
        let (y, m) = (1601 + (idx / 12) as i32, idx % 12 + 1);
        midnight(NaiveDate::from_ymd_opt(y, m, 1).unwrap_or_default())
    };
    let (pattern, period, first) = match rep.frequency {
        Frequency::Daily => (Pattern::Day, 1_440 * interval, midnight(date) % (1_440 * interval)),
        Frequency::Weekly => {
            let monday = date - Duration::days(i64::from(date.weekday().num_days_from_monday()));
            (Pattern::Week { days: 1 << date.weekday().num_days_from_sunday() }, interval, midnight(monday) % (10_080 * interval))
        }
        Frequency::Monthly => (Pattern::Month { day: date.day() }, interval, first_of_month(month_index as u32 % interval)),
        Frequency::Yearly => (Pattern::Month { day: date.day() }, 12 * interval, first_of_month(date.month0())),
    };
    let start_offset = start.time().format("%H").to_string().parse::<u32>().unwrap_or(0) * 60 + start.time().format("%M").to_string().parse::<u32>().unwrap_or(0);
    let duration = (end - start).num_minutes().max(0) as u32;
    let mut r = Recurrence {
        frequency: rep.frequency,
        pattern,
        calendar_type: 0,
        first_date_time: first,
        period,
        end: End::Never,
        occurrences: 0,
        first_day_of_week: 1,
        deleted: vec![],
        modified: vec![],
        start_date: midnight(date),
        end_date: NEVER_END,
        start_offset,
        end_offset: start_offset + duration,
        exceptions: vec![],
    };
    let horizon = date + Duration::days(366 * 120);
    match &rep.end {
        RepeatEnd::Never => {}
        RepeatEnd::Count(n) => {
            let n = (*n).clamp(1, 9_999) as usize;
            let all = expand::dates(&r, date, horizon);
            let last = all.get(n - 1).or(all.last()).copied().unwrap_or(date);
            r.end = End::Count(n.min(all.len().max(1)) as u32);
            r.occurrences = n as u32;
            r.end_date = midnight(last);
        }
        RepeatEnd::Until(day) => {
            let until = NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| format!("date de fin invalide : {day}"))?;
            if until < date {
                return Err("La fin de la répétition est avant le début.".into());
            }
            r.end = End::Date;
            r.end_date = midnight(until);
            r.occurrences = expand::dates(&r, date, until).len() as u32;
        }
    }
    let _ = minutes_to_local(0);
    Ok(r)
}

/// Propriétés du rendez-vous : (identifiant local dans la source, valeur).
fn props_of(n: &Named, edit: &Edit, times: bool) -> Result<Vec<(u16, Option<Value>)>> {
    let a = |lid| n.id(PSETID_APPOINTMENT, lid);
    let c = |lid| n.id(PSETID_COMMON, lid);
    let mut v: Vec<(u16, Option<Value>)> = Vec::new();
    fn put(v: &mut Vec<(u16, Option<Value>)>, id: Option<u16>, val: Value) {
        if let Some(id) = id {
            v.push((id, Some(val)));
        }
    }
    put(&mut v, a(0x8208), Value::Unicode(edit.location.clone()));
    put(&mut v, a(0x8205), Value::Long(edit.busy));
    put(&mut v, c(0x8503), Value::Bool(edit.reminder.is_some()));
    if let Some(m) = edit.reminder {
        put(&mut v, c(0x8501), Value::Long(m));
    }
    let cats: Vec<String> = edit.categories.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    if cats.is_empty() {
        if let Some(id) = n.name(PS_PUBLIC_STRINGS, "Keywords") {
            v.push((id, None));
        }
    } else {
        put(&mut v, n.name(PS_PUBLIC_STRINGS, "Keywords"), Value::Strings(cats));
    }
    if times {
        let (start, end) = (utc_ms(&edit.start)?, utc_ms(&edit.end)?);
        put(&mut v, a(0x820D), Value::time_ms(start));
        put(&mut v, a(0x820E), Value::time_ms(end));
        put(&mut v, a(0x8215), Value::Bool(edit.all_day));
        v.push((0x0060, Some(Value::time_ms(start))));
        v.push((0x0061, Some(Value::time_ms(end))));
    }
    Ok(v)
}

/// Nouveau rendez-vous dans le calendrier `folder_id` ; rend l'identifiant du message.
pub fn create(conn: &mut Connection, data_dir: &Path, folder_id: i64, edit: &Edit) -> Result<i64> {
    let source_id: i64 = conn.query_row("SELECT source_id FROM folder WHERE id = ?1", [folder_id], |r| r.get(0)).map_err(|_| "calendrier inconnu".to_string())?;
    let named = named_of(conn, source_id)?;
    if named.id(PSETID_APPOINTMENT, 0x820D).is_none() {
        return Err("ce calendrier n'a pas les propriétés d'un rendez-vous (source sans table de noms)".into());
    }
    let mut set = props_of(&named, edit, true)?;
    let a = |lid| named.id(PSETID_APPOINTMENT, lid);
    // Un rendez-vous simple : ni périodicité, ni réunion ; identifiant global tiré au hasard (UID de l'export `.ics`).
    for (id, val) in [(a(0x8223), Value::Bool(false)), (a(0x8217), Value::Long(0)), (a(0x8218), Value::Long(0))] {
        if let Some(id) = id {
            set.push((id, Some(val)));
        }
    }
    if let Some(rep) = &edit.repeat {
        let blob = recurrence_of(edit, rep)?.encode();
        for (id, val) in [(a(0x8223), Value::Bool(true)), (a(0x8216), Value::Binary(blob))] {
            if let Some(id) = id {
                set.retain(|(i, _)| *i != id);
                set.push((id, Some(val)));
            }
        }
    }
    if let Some(id) = named.id(PSETID_MEETING, 0x0023) {
        let seed = now_ms() as u128 ^ (u128::from(std::process::id()) << 64);
        let mut uid = seed.to_be_bytes().to_vec();
        uid.extend_from_slice(&(folder_id as u64).to_be_bytes());
        set.push((id, Some(Value::Binary(uid))));
    }
    let props = bag::merge(&[], &set)?;
    let nid: i64 = conn
        .query_row("SELECT MAX(COALESCE(MAX(nid), 0), 1879048192) + 1 FROM message WHERE folder_id = ?1", [folder_id], |r| r.get(0))
        .map_err(e)?;
    let now = now_ms();
    let tx = conn.transaction().map_err(e)?;
    tx.execute(
        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, created_at, modified_at, received_at,
            message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
         VALUES (?1, ?2, 'IPM.Appointment', ?3, '', '', '', '', ?4, ?4, ?4, 0, 1, 0, 1, ?5, 0, 0, 'text', ?6, '', ?7)",
        params![folder_id, nid, edit.subject, now, if edit.private { 2 } else { 0 }, preview(&edit.body), props],
    )
    .map_err(e)?;
    let id = tx.last_insert_rowid();
    let eml = write_eml(data_dir, source_id, id, &edit.subject, &edit.body)?;
    tx.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, eml]).map_err(e)?;
    tx.commit().map_err(e)?;
    Ok(id)
}

/// Modification d'un rendez-vous. Pour une série, les heures et la périodicité restent celles de la série.
pub fn update(conn: &mut Connection, data_dir: &Path, message_id: i64, edit: &Edit) -> Result<()> {
    let row: Option<(i64, Vec<u8>)> = conn
        .query_row("SELECT f.source_id, m.props FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1", [message_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .map_err(e)?;
    let (source_id, old) = row.ok_or("rendez-vous introuvable")?;
    let named = named_of(conn, source_id)?;
    let recurring = named.id(PSETID_APPOINTMENT, 0x8223).and_then(|id| bag::Bag::parse(&old).ok().and_then(|b| b.bool(id))).unwrap_or(false);
    let set = props_of(&named, edit, !recurring)?;
    let props = bag::merge(&old, &set)?;
    let eml = write_eml(data_dir, source_id, message_id, &edit.subject, &edit.body)?;
    conn.execute(
        "UPDATE message SET subject = ?2, sensitivity = ?3, modified_at = ?4, preview = ?5, eml_path = ?6, props = ?7, indexed = 0 WHERE id = ?1",
        params![message_id, edit.subject, if edit.private { 2 } else { 0 }, now_ms(), preview(&edit.body), eml, props],
    )
    .map_err(e)?;
    conn.execute("DELETE FROM message_fts WHERE rowid = ?1", [message_id]).map_err(e)?;
    Ok(())
}

/// Supprime une seule occurrence d'une série (`day` : sa date d'origine `AAAA-MM-JJ`) : elle rejoint les occurrences
/// supprimées de la périodicité.
pub fn delete_occurrence(conn: &mut Connection, message_id: i64, day: &str) -> Result<()> {
    let row: Option<(i64, Vec<u8>)> = conn
        .query_row("SELECT f.source_id, m.props FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1", [message_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .map_err(e)?;
    let (source_id, old) = row.ok_or("rendez-vous introuvable")?;
    let named = named_of(conn, source_id)?;
    let id = named.id(PSETID_APPOINTMENT, 0x8216).ok_or("source sans périodicité")?;
    let blob = bag::Bag::parse(&old)?.binary(id).map(<[u8]>::to_vec).ok_or("ce rendez-vous n'est pas une série")?;
    let date = NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| format!("date invalide : {day}"))?;
    let patched = Recurrence::with_deleted_instance(&blob, local_to_minutes(date.and_hms_opt(0, 0, 0).unwrap_or_default()))?;
    let props = bag::merge(&old, &[(id, Some(Value::Binary(patched)))])?;
    conn.execute("UPDATE message SET props = ?2, modified_at = ?3, indexed = 0 WHERE id = ?1", params![message_id, props, now_ms()]).map_err(e)?;
    Ok(())
}

/// Suppression d'un rendez-vous (toute la série pour un rendez-vous périodique).
pub fn delete(conn: &mut Connection, message_id: i64) -> Result<()> {
    conn.execute("DELETE FROM message_fts WHERE rowid = ?1", [message_id]).map_err(e)?;
    conn.execute("DELETE FROM message WHERE id = ?1", [message_id]).map_err(e)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_round_trip() {
        let edit = Edit { start: "2026-10-23T14:00".into(), end: "2026-10-23T14:30".into(), ..Default::default() };
        let rep = Repeat { frequency: Frequency::Weekly, interval: 1, end: RepeatEnd::Count(4) };
        let r = recurrence_of(&edit, &rep).unwrap();
        let blob = r.encode();
        let back = Recurrence::parse(&blob).unwrap_or_else(|e| panic!("{e} (blob de {} octets)", blob.len()));
        assert_eq!(back.end, End::Count(4));
        assert_eq!(expand::dates(&back, NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(), NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()).len(), 4);
        let patched = Recurrence::with_deleted_instance(&blob, local_to_minutes(NaiveDate::from_ymd_opt(2026, 10, 30).unwrap().and_hms_opt(0, 0, 0).unwrap())).unwrap();
        assert_eq!(Recurrence::parse(&patched).unwrap().deleted.len(), 1);
    }

    #[test]
    fn paris_to_utc() {
        let utc = |t: &str| NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M").unwrap().and_utc().timestamp_millis();
        assert_eq!(utc_ms("2026-01-15T10:00").unwrap(), utc("2026-01-15 09:00"));
        assert_eq!(utc_ms("2026-07-15T10:00").unwrap(), utc("2026-07-15 08:00"));
    }
}
