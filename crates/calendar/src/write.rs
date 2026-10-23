//! Écriture des rendez-vous dans la base de ThunderLook (sans Outlook) : création, modification, suppression.
//! Les propriétés sont écrites dans le sac MAPI de la ligne, comme celles qu'Outlook y aurait mises, pour que la lecture
//! (`load`), l'export et les vues n'aient rien à savoir de l'origine du rendez-vous. La description va dans un `.eml`
//! minimal (`eml/<source>/tl/<id>.eml`).

use std::path::Path;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use thunderlook_store::rusqlite::{Connection, OptionalExtension, params};

use crate::{
    bag::{self, Value},
    expand,
    recur::{End, Exception, Frequency, Pattern, Recurrence, local_to_minutes, minutes_to_local},
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
        raw: Default::default(),
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
    let midnight = local_to_minutes(date.and_hms_opt(0, 0, 0).unwrap_or_default());
    let mut rec = Recurrence::parse(&blob)?;
    // Une occurrence déplacée (exception) : on retire l'exception ; la date d'origine reste dans les supprimées.
    let patched = match rec.exceptions.iter().position(|x| minutes_to_local(x.original_start).date() == date) {
        Some(i) => {
            let x = rec.exceptions.remove(i);
            drop_modified(&mut rec, &x);
            rec.encode()
        }
        None => Recurrence::with_deleted_instance(&blob, midnight)?,
    };
    let props = bag::merge(&old, &[(id, Some(Value::Binary(patched)))])?;
    conn.execute("UPDATE message SET props = ?2, modified_at = ?3, indexed = 0 WHERE id = ?1", params![message_id, props, now_ms()]).map_err(e)?;
    Ok(())
}

/// Retire de `modified` la date (minuit local) du nouveau début de l'exception `x`.
fn drop_modified(rec: &mut Recurrence, x: &Exception) {
    let at = local_to_minutes(minutes_to_local(x.start).date().and_hms_opt(0, 0, 0).unwrap_or_default());
    if let Some(i) = rec.modified.iter().position(|m| *m == at) {
        rec.modified.remove(i);
    }
}

fn parse_local(t: &str) -> Result<NaiveDateTime> {
    NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M").map_err(|_| format!("date invalide : {t}"))
}

/// Série lue pour modification : sac, table des noms et périodicité.
struct Series {
    source_id: i64,
    old: Vec<u8>,
    named: Named,
    blob_id: u16,
    rec: Recurrence,
    subject: String,
}

fn load_series(conn: &Connection, message_id: i64) -> Result<Series> {
    let row: Option<(i64, Vec<u8>, String)> = conn
        .query_row("SELECT f.source_id, m.props, m.subject FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1", [message_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()
        .map_err(e)?;
    let (source_id, old, subject) = row.ok_or("rendez-vous introuvable")?;
    let named = named_of(conn, source_id)?;
    let blob_id = named.id(PSETID_APPOINTMENT, 0x8216).ok_or("source sans périodicité")?;
    let blob = bag::Bag::parse(&old)?.binary(blob_id).map(<[u8]>::to_vec).ok_or("ce rendez-vous n'est pas une série")?;
    let rec = Recurrence::parse(&blob)?;
    Ok(Series { source_id, old, named, blob_id, rec, subject })
}

fn save(conn: &Connection, data_dir: &Path, message_id: i64, source_id: i64, edit: &Edit, props: &[u8]) -> Result<()> {
    let eml = write_eml(data_dir, source_id, message_id, &edit.subject, &edit.body)?;
    conn.execute(
        "UPDATE message SET subject = ?2, sensitivity = ?3, modified_at = ?4, preview = ?5, eml_path = ?6, props = ?7, indexed = 0 WHERE id = ?1",
        params![message_id, edit.subject, if edit.private { 2 } else { 0 }, now_ms(), preview(&edit.body), eml, props],
    )
    .map_err(e)?;
    conn.execute("DELETE FROM message_fts WHERE rowid = ?1", [message_id]).map_err(e)?;
    Ok(())
}

/// Ce que `update_series` a abandonné en changeant la périodicité.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeriesChange {
    /// Occurrences modifiées (exceptions) perdues.
    pub lost_exceptions: usize,
    /// Occurrences supprimées (sans exception) rétablies.
    pub lost_deleted: usize,
}

/// Modification d'une série existante : objet, lieu, etc. comme `update`, plus les heures (heure de début et durée de
/// `edit.start`/`edit.end`) et, avec `rule`, la périodicité (la date de `edit.start` est alors le premier jour de la
/// série). Sans `rule`, ou avec la même périodicité, la date de `edit.start` est ignorée et les exceptions comme les
/// occurrences supprimées sont conservées ; si la périodicité change, elles sont abandonnées (nombre dans le retour).
pub fn update_series(conn: &mut Connection, data_dir: &Path, message_id: i64, edit: &Edit, rule: Option<Repeat>) -> Result<SeriesChange> {
    let s = load_series(conn, message_id)?;
    let (start, end) = (parse_local(&edit.start)?, parse_local(&edit.end)?);
    if end < start {
        return Err("La fin est avant le début.".into());
    }
    let mut rec = s.rec.clone();
    let mut change = SeriesChange::default();
    if let Some(rule) = &rule {
        let fresh = recurrence_of(edit, rule)?;
        let same = (fresh.frequency, fresh.pattern, fresh.period, fresh.end, fresh.start_date, fresh.end_date)
            == (rec.frequency, rec.pattern, rec.period, rec.end, rec.start_date, rec.end_date);
        if !same {
            change = SeriesChange { lost_exceptions: rec.exceptions.len(), lost_deleted: rec.deleted.len().saturating_sub(rec.exceptions.len()) };
            rec = fresh;
        }
    }
    rec.start_offset = start.hour() * 60 + start.minute();
    rec.end_offset = rec.start_offset + (end - start).num_minutes() as u32;
    // Début et fin du rendez-vous : ceux de la première occurrence.
    let first_day = minutes_to_local(rec.start_date).date();
    let first = expand::dates(&rec, first_day, first_day + Duration::days(366 * 5)).first().copied().unwrap_or(first_day);
    let at = |offset: u32| first.and_hms_opt(0, 0, 0).unwrap_or_default() + Duration::minutes(i64::from(offset));
    let (first_start, first_end) = (TimeZone::PARIS.to_utc(at(rec.start_offset)).and_utc().timestamp_millis(), TimeZone::PARIS.to_utc(at(rec.end_offset)).and_utc().timestamp_millis());
    let a = |lid| s.named.id(PSETID_APPOINTMENT, lid);
    let mut set = props_of(&s.named, edit, false)?;
    set.push((s.blob_id, Some(Value::Binary(rec.encode()))));
    for (id, v) in [
        (a(0x820D), Value::time_ms(first_start)),
        (a(0x820E), Value::time_ms(first_end)),
        (a(0x8215), Value::Bool(edit.all_day)),
        (Some(0x0060), Value::time_ms(first_start)),
        (Some(0x0061), Value::time_ms(first_end)),
    ] {
        if let Some(id) = id {
            set.push((id, Some(v)));
        }
    }
    let props = bag::merge(&s.old, &set)?;
    save(conn, data_dir, message_id, s.source_id, edit, &props)?;
    Ok(change)
}

/// Modifie UNE occurrence d'une série (`date` : son jour d'origine `AAAA-MM-JJ`) : exception portant les champs de
/// `edit` qui diffèrent de la série (objet, lieu, début, fin, disponibilité, rappel, journée entière) ; le jour
/// d'origine rejoint les occurrences supprimées et le jour du nouveau début les modifiées. Une exception déjà présente
/// pour ce jour est remplacée. Description, catégories et caractère privé de `edit` ne s'appliquent qu'à la série
/// (ThunderLook n'écrit pas de message intégré par exception).
pub fn update_occurrence(conn: &mut Connection, _data_dir: &Path, message_id: i64, date: &str, edit: &Edit) -> Result<()> {
    let s = load_series(conn, message_id)?;
    let day = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| format!("date invalide : {date}"))?;
    let midnight = local_to_minutes(day.and_hms_opt(0, 0, 0).unwrap_or_default());
    let (start, end) = (parse_local(&edit.start)?, parse_local(&edit.end)?);
    if end < start {
        return Err("La fin est avant le début.".into());
    }
    let mut rec = s.rec;
    let previous = rec.exceptions.iter().position(|x| minutes_to_local(x.original_start).date() == day);
    let original_start = match previous {
        Some(i) => {
            let x = rec.exceptions.remove(i);
            drop_modified(&mut rec, &x);
            x.original_start
        }
        None => {
            if rec.deleted.contains(&midnight) || !expand::dates(&rec, day, day).contains(&day) {
                return Err(format!("{date} n'est pas une occurrence de cette série"));
            }
            midnight + rec.start_offset
        }
    };
    let bag = bag::Bag::parse(&s.old)?;
    let a = |lid| s.named.id(PSETID_APPOINTMENT, lid);
    let c = |lid| s.named.id(PSETID_COMMON, lid);
    let master_location = a(0x8208).and_then(|i| bag.text(i)).unwrap_or_default();
    let master_busy = a(0x8205).and_then(|i| bag.int(i)).unwrap_or(2);
    let master_all_day = a(0x8215).and_then(|i| bag.bool(i)).unwrap_or(false);
    let master_reminder = c(0x8503).and_then(|i| bag.bool(i)).unwrap_or(false);
    let master_delta = c(0x8501).and_then(|i| bag.int(i)).unwrap_or(15);
    let x = Exception {
        start: local_to_minutes(start),
        end: local_to_minutes(end),
        original_start,
        subject: Some(edit.subject.clone()).filter(|v| *v != s.subject),
        location: Some(edit.location.clone()).filter(|v| *v != master_location),
        busy_status: Some(edit.busy).filter(|v| i64::from(*v) != master_busy).map(|v| v as u32),
        reminder_set: Some(edit.reminder.is_some()).filter(|v| *v != master_reminder),
        reminder_delta: edit.reminder.filter(|m| i64::from(*m) != master_delta).map(|m| m as u32),
        all_day: Some(edit.all_day).filter(|v| *v != master_all_day),
        ..Default::default()
    };
    if !rec.deleted.contains(&midnight) {
        rec.deleted.push(midnight);
    }
    rec.modified.push(local_to_minutes(start.date().and_hms_opt(0, 0, 0).unwrap_or_default()));
    rec.exceptions.push(x);
    rec.deleted.sort_unstable();
    rec.modified.sort_unstable();
    rec.exceptions.sort_by_key(|x| x.original_start);
    let props = bag::merge(&s.old, &[(s.blob_id, Some(Value::Binary(rec.encode())))])?;
    conn.execute("UPDATE message SET props = ?2, modified_at = ?3, indexed = 0 WHERE id = ?1", params![message_id, props, now_ms()]).map_err(e)?;
    Ok(())
}

/// Déplace un rendez-vous simple (glisser-déposer, poignée de fin) : nouveaux début et fin, heure de Paris
/// `AAAA-MM-JJTHH:MM`. Le reste du rendez-vous est inchangé. Refuse une série (une occurrence se déplace par
/// `update_occurrence`).
pub fn move_appointment(conn: &mut Connection, message_id: i64, start_local: &str, end_local: &str) -> Result<()> {
    let row: Option<(i64, Vec<u8>)> = conn
        .query_row("SELECT f.source_id, m.props FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1", [message_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .map_err(e)?;
    let (source_id, old) = row.ok_or("rendez-vous introuvable")?;
    let named = named_of(conn, source_id)?;
    let a = |lid| named.id(PSETID_APPOINTMENT, lid);
    if a(0x8223).and_then(|id| bag::Bag::parse(&old).ok().and_then(|b| b.bool(id))).unwrap_or(false) {
        return Err("Ce rendez-vous est une série : déplacez une occurrence.".into());
    }
    let (start, end) = (utc_ms(start_local)?, utc_ms(end_local)?);
    if end < start {
        return Err("La fin est avant le début.".into());
    }
    let (Some(start_id), Some(end_id)) = (a(0x820D), a(0x820E)) else {
        return Err("ce calendrier n'a pas les propriétés d'un rendez-vous (source sans table de noms)".into());
    };
    let set = [
        (start_id, Some(Value::time_ms(start))),
        (end_id, Some(Value::time_ms(end))),
        (0x0060, Some(Value::time_ms(start))),
        (0x0061, Some(Value::time_ms(end))),
    ];
    let props = bag::merge(&old, &set)?;
    conn.execute("UPDATE message SET props = ?2, modified_at = ?3, indexed = 0 WHERE id = ?1", params![message_id, props, now_ms()]).map_err(e)?;
    Ok(())
}

/// Suppression d'un rendez-vous (toute la série pour un rendez-vous périodique).
pub fn delete(conn: &mut Connection, message_id: i64) -> Result<()> {
    thunderlook_safety::snapshot_conn(conn, message_id)?;
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

    /// Base de test : une source dont la table de noms a les propriétés d'un rendez-vous, un dossier calendrier.
    fn fixture(name: &str) -> (std::path::PathBuf, Connection, i64) {
        let dir = std::env::temp_dir().join(format!("tl-write-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let conn = thunderlook_store::open(&dir).unwrap();
        let mut names = Vec::new();
        let mut add = |id: u16, set: &str, lid: u32| {
            names.extend_from_slice(&id.to_le_bytes());
            names.extend_from_slice(set.as_bytes());
            names.push(0);
            names.extend_from_slice(&lid.to_le_bytes());
        };
        for (i, lid) in [0x820D, 0x820E, 0x8215, 0x8208, 0x8205, 0x8223, 0x8216, 0x8217, 0x8218].into_iter().enumerate() {
            add(0x8000 + i as u16, PSETID_APPOINTMENT, lid);
        }
        add(0x8010, PSETID_COMMON, 0x8501);
        add(0x8011, PSETID_COMMON, 0x8503);
        conn.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at, props) VALUES (1, 'tl', '', '', 't', 0, 0, ?1)", [names]).unwrap();
        conn.execute("INSERT INTO folder (id, source_id, nid, name, role, hidden, position) VALUES (1, 1, 1, 'Calendrier', 'folder', 0, 0)", []).unwrap();
        (dir, conn, 1)
    }

    fn weekly(conn: &mut Connection, dir: &Path, folder: i64) -> i64 {
        let edit = Edit {
            subject: "Point d'équipe".into(),
            location: "Salle A".into(),
            start: "2026-10-23T14:00".into(),
            end: "2026-10-23T14:30".into(),
            reminder: Some(15),
            busy: 2,
            repeat: Some(Repeat { frequency: Frequency::Weekly, interval: 1, end: RepeatEnd::Count(6) }),
            ..Default::default()
        };
        create(conn, dir, folder, &edit).unwrap()
    }

    /// Occurrences de l'année, sous la forme « jour d'origine début-fin objet » (heure de Paris).
    fn listing(conn: &Connection, id: i64) -> Vec<String> {
        let cal = crate::load(conn).unwrap();
        let a = cal.appointments.iter().find(|a| a.id == id).unwrap();
        let tz = a.time_zone.unwrap_or(TimeZone::PARIS);
        let local = |ms: i64| tz.to_local(chrono::DateTime::from_timestamp_millis(ms).unwrap().naive_utc()).format("%m-%d %H:%M").to_string();
        expand::occurrences(a, utc_ms("2026-01-01T00:00").unwrap(), utc_ms("2027-06-01T00:00").unwrap())
            .iter()
            .map(|o| format!("{} {} → {} {}{}", o.original_date.map(|d| d.format("%m-%d").to_string()).unwrap_or_default(), local(o.start_ms), local(o.end_ms), o.subject, if o.exception { " *" } else { "" }))
            .collect()
    }

    fn recurrence(conn: &Connection, id: i64) -> Recurrence {
        let s = load_series(conn, id).unwrap();
        s.rec
    }

    #[test]
    fn delete_then_undo_restores_the_series() {
        let (dir, mut conn, folder) = fixture("undo");
        let id = weekly(&mut conn, &dir, folder);
        let before = listing(&conn, id);
        delete(&mut conn, id).unwrap();
        assert!(!crate::load(&conn).unwrap().appointments.iter().any(|a| a.id == id));
        assert_eq!(thunderlook_safety::undo(&mut conn, &dir).unwrap(), "Annulé : suppression du rendez-vous « Point d'équipe »");
        assert_eq!(listing(&conn, id), before);

        // Une occurrence supprimée puis annulée : la série retrouve ses six dates.
        thunderlook_safety::snapshot_edit(&conn, &dir, id).unwrap();
        delete_occurrence(&mut conn, id, "2026-10-30").unwrap();
        assert_eq!(listing(&conn, id).len(), 5);
        thunderlook_safety::undo(&mut conn, &dir).unwrap();
        assert_eq!(listing(&conn, id), before);
    }

    #[test]
    fn update_occurrence_moves_one_occurrence() {
        let (dir, mut conn, folder) = fixture("occ");
        let id = weekly(&mut conn, &dir, folder);
        let before = listing(&conn, id);
        assert_eq!(before.len(), 6, "{before:?}");
        let edit = Edit {
            subject: "Point déplacé".into(),
            location: "Salle A".into(),
            start: "2026-10-31T16:00".into(),
            end: "2026-10-31T17:00".into(),
            reminder: Some(15),
            busy: 1,
            ..Default::default()
        };
        update_occurrence(&mut conn, &dir, id, "2026-10-30", &edit).unwrap();
        let after = listing(&conn, id);
        assert_eq!(after.len(), 6, "{after:?}");
        assert!(after.iter().any(|l| l == "10-30 10-31 16:00 → 10-31 17:00 Point déplacé *"), "{after:?}");
        assert!(!after.iter().any(|l| l.contains("10-30 14:00")), "l'ancienne occurrence est restée : {after:?}");
        let others = |v: &[String]| v.iter().filter(|l| !l.starts_with("10-30")).cloned().collect::<Vec<_>>();
        assert_eq!(others(&before), others(&after));
        // Seuls les champs réellement modifiés sont écrits.
        let r = recurrence(&conn, id);
        let x = &r.exceptions[0];
        assert_eq!((x.subject.as_deref(), x.location.as_deref(), x.busy_status, x.reminder_set, x.reminder_delta, x.all_day), (Some("Point déplacé"), None, Some(1), None, None, None));
        assert_eq!((r.deleted.len(), r.modified.len()), (1, 1));
        // Une seconde modification du même jour remplace l'exception.
        let again = Edit { subject: "Point d'équipe".into(), start: "2026-10-30T09:00".into(), end: "2026-10-30T09:30".into(), ..edit.clone() };
        update_occurrence(&mut conn, &dir, id, "2026-10-30", &again).unwrap();
        let r = recurrence(&conn, id);
        assert_eq!((r.exceptions.len(), r.deleted.len(), r.modified.len()), (1, 1, 1));
        assert_eq!(r.exceptions[0].subject, None);
        assert!(listing(&conn, id).iter().any(|l| l == "10-30 10-30 09:00 → 10-30 09:30 Point d'équipe *"));
        assert!(update_occurrence(&mut conn, &dir, id, "2026-10-29", &again).is_err(), "pas une occurrence");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_occurrence_after_exception() {
        let (dir, mut conn, folder) = fixture("del");
        let id = weekly(&mut conn, &dir, folder);
        let edit = Edit { subject: "Autre".into(), start: "2026-10-31T16:00".into(), end: "2026-10-31T17:00".into(), busy: 2, reminder: Some(15), location: "Salle A".into(), ..Default::default() };
        update_occurrence(&mut conn, &dir, id, "2026-10-30", &edit).unwrap();
        // Supprimer une autre occurrence laisse l'exception intacte.
        delete_occurrence(&mut conn, id, "2026-11-06").unwrap();
        let r = recurrence(&conn, id);
        assert_eq!((r.exceptions.len(), r.deleted.len(), r.modified.len()), (1, 2, 1));
        assert_eq!(listing(&conn, id).len(), 5);
        // Supprimer l'occurrence déplacée retire l'exception et son jour de `modified`, sans rétablir l'originale.
        delete_occurrence(&mut conn, id, "2026-10-30").unwrap();
        let r = recurrence(&conn, id);
        assert_eq!((r.exceptions.len(), r.deleted.len(), r.modified.len()), (0, 2, 0));
        let after = listing(&conn, id);
        assert_eq!(after.len(), 4, "{after:?}");
        assert!(after.iter().all(|l| !l.starts_with("10-30") && !l.starts_with("11-06")), "{after:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_series_times_keep_the_rest_and_new_rule_drops_it() {
        let (dir, mut conn, folder) = fixture("ser");
        let id = weekly(&mut conn, &dir, folder);
        let moved = Edit { subject: "Autre".into(), start: "2026-10-31T16:00".into(), end: "2026-10-31T17:00".into(), busy: 2, reminder: Some(15), location: "Salle A".into(), ..Default::default() };
        update_occurrence(&mut conn, &dir, id, "2026-10-30", &moved).unwrap();
        delete_occurrence(&mut conn, id, "2026-11-06").unwrap();
        // Heures seules : 15 h – 16 h ; exception et suppression conservées.
        let times = Edit { subject: "Point d'équipe".into(), location: "Salle A".into(), start: "2026-10-23T15:00".into(), end: "2026-10-23T16:00".into(), busy: 2, reminder: Some(15), ..Default::default() };
        let change = update_series(&mut conn, &dir, id, &times, None).unwrap();
        assert_eq!(change, SeriesChange::default());
        let l = listing(&conn, id);
        assert_eq!(l.len(), 5, "{l:?}");
        assert!(l.contains(&"10-23 10-23 15:00 → 10-23 16:00 Point d'équipe".to_string()), "{l:?}");
        assert!(l.contains(&"10-30 10-31 16:00 → 10-31 17:00 Autre *".to_string()), "{l:?}");
        // La même périodicité n'abandonne rien.
        let same = Repeat { frequency: Frequency::Weekly, interval: 1, end: RepeatEnd::Count(6) };
        assert_eq!(update_series(&mut conn, &dir, id, &times, Some(same)).unwrap(), SeriesChange::default());
        assert_eq!(recurrence(&conn, id).exceptions.len(), 1);
        // Une autre périodicité (toutes les 2 semaines, 3 fois) abandonne l'exception et la suppression.
        let rule = Repeat { frequency: Frequency::Weekly, interval: 2, end: RepeatEnd::Count(3) };
        let change = update_series(&mut conn, &dir, id, &times, Some(rule)).unwrap();
        assert_eq!(change, SeriesChange { lost_exceptions: 1, lost_deleted: 1 });
        let l = listing(&conn, id);
        assert_eq!(l, ["10-23 10-23 15:00 → 10-23 16:00 Point d'équipe", "11-06 11-06 15:00 → 11-06 16:00 Point d'équipe", "11-20 11-20 15:00 → 11-20 16:00 Point d'équipe"]);
        let r = recurrence(&conn, id);
        assert_eq!((r.exceptions.len(), r.deleted.len(), r.modified.len()), (0, 0, 0));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn move_appointment_changes_times_only() {
        let (dir, mut conn, folder) = fixture("move");
        let single = Edit { subject: "Dentiste".into(), location: "Cabinet".into(), start: "2026-10-23T14:00".into(), end: "2026-10-23T15:00".into(), busy: 2, reminder: Some(15), ..Default::default() };
        let id = create(&mut conn, &dir, folder, &single).unwrap();
        move_appointment(&mut conn, id, "2026-10-24T09:15", "2026-10-24T10:15").unwrap();
        let l = listing(&conn, id);
        assert_eq!(l.len(), 1, "{l:?}");
        assert!(l[0].ends_with("10-24 09:15 → 10-24 10:15 Dentiste"), "{l:?}");
        // Le reste est intact : redimensionner garde le début.
        move_appointment(&mut conn, id, "2026-10-24T09:15", "2026-10-24T11:00").unwrap();
        assert!(listing(&conn, id)[0].ends_with("10-24 09:15 → 10-24 11:00 Dentiste"));
        let a = crate::load(&conn).unwrap().appointments.into_iter().find(|a| a.id == id).unwrap();
        assert_eq!((a.location.as_deref(), a.reminder_minutes), (Some("Cabinet"), Some(15)));
        assert!(move_appointment(&mut conn, id, "2026-10-24T12:00", "2026-10-24T11:00").is_err(), "fin avant début");
        // Une série se déplace par occurrence.
        let series = weekly(&mut conn, &dir, folder);
        assert!(move_appointment(&mut conn, series, "2026-10-24T09:00", "2026-10-24T10:00").is_err());
        assert!(move_appointment(&mut conn, 9_999, "2026-10-24T09:00", "2026-10-24T10:00").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    fn series(conn: &mut Connection, dir: &Path, folder: i64, start: &str, end: &str, frequency: Frequency, until: RepeatEnd) -> i64 {
        let edit = Edit {
            subject: "Série".into(),
            start: start.into(),
            end: end.into(),
            reminder: Some(15),
            busy: 2,
            repeat: Some(Repeat { frequency, interval: 1, end: until }),
            ..Default::default()
        };
        create(conn, dir, folder, &edit).unwrap()
    }

    #[test]
    fn month_end_series_with_exception_and_deletion() {
        let (dir, mut conn, folder) = fixture("eom");
        let id = series(&mut conn, &dir, folder, "2026-01-31T10:00", "2026-01-31T11:00", Frequency::Monthly, RepeatEnd::Count(5));
        let days = |conn: &Connection| listing(conn, id).iter().map(|l| l[..5].to_string()).collect::<Vec<_>>();
        // Jour 31 : les mois plus courts donnent leur dernier jour (comme Outlook).
        assert_eq!(days(&conn), ["01-31", "02-28", "03-31", "04-30", "05-31"]);
        let moved = Edit { subject: "Série".into(), start: "2026-03-01T10:00".into(), end: "2026-03-01T11:00".into(), busy: 2, reminder: Some(15), ..Default::default() };
        update_occurrence(&mut conn, &dir, id, "2026-02-28", &moved).unwrap();
        delete_occurrence(&mut conn, id, "2026-04-30").unwrap();
        // Les heures de la série changent ; l'exception et la suppression survivent.
        let times = Edit { subject: "Série".into(), start: "2026-01-31T09:30".into(), end: "2026-01-31T10:00".into(), busy: 2, reminder: Some(15), ..Default::default() };
        assert_eq!(update_series(&mut conn, &dir, id, &times, None).unwrap(), SeriesChange::default());
        let l = listing(&conn, id);
        assert_eq!(l.len(), 4, "{l:?}");
        assert!(l.contains(&"02-28 03-01 10:00 → 03-01 11:00 Série *".to_string()), "{l:?}");
        assert!(l.contains(&"03-31 03-31 09:30 → 03-31 10:00 Série".to_string()), "{l:?}");
        assert!(l.iter().all(|x| !x.starts_with("04-30")), "{l:?}");
        // Aller-retour de l'encodage.
        let r = recurrence(&conn, id);
        assert_eq!(Recurrence::parse(&r.encode()).unwrap().encode(), r.encode());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn leap_day_yearly_series() {
        let (dir, mut conn, folder) = fixture("leap");
        let id = series(&mut conn, &dir, folder, "2024-02-29T08:00", "2024-02-29T09:00", Frequency::Yearly, RepeatEnd::Never);
        let cal = crate::load(&conn).unwrap();
        let a = cal.appointments.iter().find(|a| a.id == id).unwrap();
        let from = utc_ms("2024-01-01T00:00").unwrap();
        let to = utc_ms("2029-01-01T00:00").unwrap();
        let years: Vec<String> = expand::occurrences(a, from, to).iter().filter_map(|o| o.original_date).map(|d| d.to_string()).collect();
        // Pas d'invention : la série ne tombe que sur un 29 février réel (2024 et 2028) ou sur le 28 (années communes),
        // mais toujours une occurrence par an et jamais un 1er mars.
        assert_eq!(years.len(), 5, "{years:?}");
        assert!(years.iter().all(|d| d.ends_with("-02-29") || d.ends_with("-02-28")), "{years:?}");
        let r = recurrence(&conn, id);
        assert_eq!(Recurrence::parse(&r.encode()).unwrap(), r);
        // Modifier l'occurrence de 2028 puis la supprimer.
        let edit = Edit { subject: "Série".into(), start: "2028-02-29T12:00".into(), end: "2028-02-29T13:00".into(), busy: 2, reminder: Some(15), ..Default::default() };
        update_occurrence(&mut conn, &dir, id, "2028-02-29", &edit).unwrap();
        let cal = crate::load(&conn).unwrap();
        let a = cal.appointments.iter().find(|a| a.id == id).unwrap();
        assert!(expand::occurrences(a, from, to).iter().any(|o| o.exception && o.original_date.map(|d| d.to_string()) == Some("2028-02-29".into())));
        delete_occurrence(&mut conn, id, "2028-02-29").unwrap();
        assert!(recurrence(&conn, id).exceptions.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn local_time_is_stable_across_the_dst_change() {
        let (dir, mut conn, folder) = fixture("dst");
        // Changement d'heure le 2026-10-25 : l'heure locale (14 h) ne bouge pas, l'instant UTC si.
        let id = series(&mut conn, &dir, folder, "2026-10-24T14:00", "2026-10-24T15:00", Frequency::Daily, RepeatEnd::Count(3));
        assert_eq!(listing(&conn, id), ["10-24 10-24 14:00 → 10-24 15:00 Série", "10-25 10-25 14:00 → 10-25 15:00 Série", "10-26 10-26 14:00 → 10-26 15:00 Série"]);
        let cal = crate::load(&conn).unwrap();
        let a = cal.appointments.iter().find(|a| a.id == id).unwrap();
        let o = expand::occurrences(a, utc_ms("2026-10-01T00:00").unwrap(), utc_ms("2026-11-30T00:00").unwrap());
        assert_eq!(o[0].start_ms, utc_ms("2026-10-24T14:00").unwrap());
        assert_eq!(o[1].start_ms - o[0].start_ms, 25 * 3_600_000, "le 25 octobre dure 25 h");
        // Une exception de l'autre côté du changement d'heure garde son heure locale.
        let edit = Edit { subject: "Série".into(), start: "2026-10-25T16:00".into(), end: "2026-10-25T17:00".into(), busy: 2, reminder: Some(15), ..Default::default() };
        update_occurrence(&mut conn, &dir, id, "2026-10-24", &edit).unwrap();
        let l = listing(&conn, id);
        assert!(l.contains(&"10-24 10-25 16:00 → 10-25 17:00 Série *".to_string()), "{l:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn paris_to_utc() {
        let utc = |t: &str| NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M").unwrap().and_utc().timestamp_millis();
        assert_eq!(utc_ms("2026-01-15T10:00").unwrap(), utc("2026-01-15 09:00"));
        assert_eq!(utc_ms("2026-07-15T10:00").unwrap(), utc("2026-07-15 08:00"));
    }
}
