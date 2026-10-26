//! Tâches et notes de ThunderLook : lues dans les sacs MAPI importés ([MS-OXOTASK], [MS-OXONOTE]) et écrites dans la
//! base, sans Outlook. Le texte (description d'une tâche, contenu d'une note) est dans le `.eml` de l'élément.

use std::{collections::HashMap, path::Path};

pub mod filter;
#[allow(dead_code)]
mod textfmt;
mod trecur;
pub mod vnote;
pub mod vtodo;

use thunderlook_calendar::{
    bag::{self, Bag, Value},
    named::{Named, PS_PUBLIC_STRINGS, PSETID_COMMON},
    tz::TimeZone,
};
use thunderlook_model::items::{ItemFolder, NoteForm, NoteList, NoteQuery, NoteRow, TaskForm, TaskList, TaskQuery, TaskRow, task_progress};
use thunderlook_store::rusqlite::{Connection, OptionalExtension, params};

type Result<T> = std::result::Result<T, String>;

const PSETID_TASK: &str = "00062003-0000-0000-C000-000000000046";
const PSETID_NOTE: &str = "0006200E-0000-0000-C000-000000000046";
const DAY_MS: i64 = 86_400_000;

fn e(x: thunderlook_store::rusqlite::Error) -> String {
    x.to_string()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// `AAAA-MM-JJ` d'un instant (UTC).
fn day_text(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).map(|t| t.format("%Y-%m-%d").to_string()).unwrap_or_default()
}

/// Minuit UTC d'un jour `AAAA-MM-JJ` (les dates d'une tâche sont « flottantes » : minuit UTC).
fn day_ms(s: &str) -> Result<Option<i64>> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let d = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| format!("date invalide : {s}"))?;
    Ok(Some(d.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc().timestamp_millis()))
}

struct Sources {
    named: HashMap<i64, Named>,
    none: Named,
}

impl Sources {
    fn load(conn: &Connection) -> Result<Self> {
        let mut named = HashMap::new();
        let mut st = conn.prepare("SELECT id, props FROM source WHERE props IS NOT NULL").map_err(e)?;
        let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))).map_err(e)?;
        for r in rows {
            let (id, raw) = r.map_err(e)?;
            named.insert(id, Named::parse(&raw)?);
        }
        Ok(Self { named, none: Named::default() })
    }

    fn of(&self, source: i64) -> &Named {
        self.named.get(&source).unwrap_or(&self.none)
    }
}

/// Texte d'un `.eml` (corps en texte).
fn eml_text(data_dir: &Path, rel: &str) -> String {
    if rel.is_empty() {
        return String::new();
    }
    std::fs::read(thunderlook_store::resolve(data_dir, rel))
        .ok()
        .and_then(|b| mail_parser::MessageParser::default().parse(&b).and_then(|m| m.body_text(0).map(|t| t.into_owned())))
        .unwrap_or_default()
}

fn write_eml(data_dir: &Path, source_id: i64, id: i64, subject: &str, body: &str) -> Result<String> {
    let rel = format!("eml/{source_id}/tl/{id}.eml");
    let path = thunderlook_store::resolve(data_dir, &rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|x| x.to_string())?;
    }
    let subject = subject.replace(['\r', '\n'], " ");
    std::fs::write(&path, format!("MIME-Version: 1.0\r\nSubject: {subject}\r\nContent-Type: text/plain; charset=\"utf-8\"\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{body}")).map_err(|x| x.to_string())?;
    Ok(rel)
}

fn preview(body: &str) -> String {
    body.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect()
}

fn is_trash(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("supprim") || n == "deleted items" || n.contains("sent") || n.contains("envoy")
}

fn folders_of(conn: &Connection, class_like: &str) -> Result<Vec<ItemFolder>> {
    let mut st = conn
        .prepare(
            "SELECT f.id, f.name, s.display_name, COUNT(*) FROM message m JOIN folder f ON f.id = m.folder_id JOIN source s ON s.id = f.source_id
             WHERE m.message_class LIKE ?1 GROUP BY f.id ORDER BY s.id, COUNT(*) DESC",
        )
        .map_err(e)?;
    let rows = st
        .query_map([class_like], |r| Ok(ItemFolder { id: format!("f{}", r.get::<_, i64>(0)?), name: r.get(1)?, section: r.get(2)?, count: r.get(3)? }))
        .map_err(e)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(e)?;
    Ok(rows.into_iter().filter(|f| !is_trash(&f.name)).collect())
}

fn folder_num(id: &str) -> Result<i64> {
    id.strip_prefix('f').and_then(|x| x.parse().ok()).ok_or_else(|| "dossier inconnu".to_string())
}

// --- Tâches ---------------------------------------------------------------------------------------------------------

struct TaskIds {
    status: Option<u16>,
    start: Option<u16>,
    due: Option<u16>,
    complete: Option<u16>,
    completed_on: Option<u16>,
    percent: Option<u16>,
    keywords: Option<u16>,
    /// `PidLidReminderSet` / `PidLidReminderTime` (PSETID_Common).
    reminder_set: Option<u16>,
    reminder_time: Option<u16>,
    /// `PidLidTaskRecurrence`, `PidLidTaskFRecurring`, `PidLidTaskDeadOccurrence`.
    recur: Option<u16>,
    f_recurring: Option<u16>,
    dead: Option<u16>,
}

fn task_ids(n: &Named) -> TaskIds {
    let t = |lid| n.id(PSETID_TASK, lid);
    TaskIds {
        status: t(0x8101),
        percent: t(0x8102),
        start: t(0x8104),
        due: t(0x8105),
        completed_on: t(0x810F),
        complete: t(0x811C),
        recur: t(0x8116),
        f_recurring: t(0x8126),
        dead: t(0x8109),
        reminder_set: n.id(PSETID_COMMON, 0x8503),
        reminder_time: n.id(PSETID_COMMON, 0x8502),
        keywords: n.name(PS_PUBLIC_STRINGS, "Keywords"),
    }
}

fn status_of(bag: &Bag, ids: &TaskIds) -> i32 {
    if ids.complete.and_then(|i| bag.bool(i)).unwrap_or(false) {
        return 2;
    }
    ids.status.and_then(|i| bag.int(i)).unwrap_or(0) as i32
}

/// `PidLidPercentComplete` (`PtypFloating64`, 0.0..=1.0) → 0..=100, cohérent avec le statut (terminée = 100).
fn percent_of(bag: &Bag, ids: &TaskIds, status: i32) -> i32 {
    let raw = ids.percent.and_then(|i| bag.raw(i)).filter(|(ty, v)| *ty == 0x0005 && v.len() == 8);
    let p = raw.and_then(|(_, v)| v.try_into().ok()).map(f64::from_le_bytes).filter(|p| p.is_finite()).map_or(0, |p| (p * 100.0).round() as i32);
    if status == 2 { 100 } else { p.clamp(0, 99) }
}

fn task_row(id: i64, folder: i64, subject: String, importance: i64, bag: &Bag, ids: &TaskIds) -> TaskRow {
    let status = status_of(bag, ids);
    TaskRow {
        id: format!("t{id}"),
        title: subject,
        due: ids.due.and_then(|i| bag.time_ms(i)).map(day_text).unwrap_or_default(),
        complete: status == 2,
        status,
        importance: importance as i32,
        categories: ids.keywords.and_then(|i| bag.strings(i)).unwrap_or_default(),
        folder: format!("f{folder}"),
        start: ids.start.and_then(|i| bag.time_ms(i)).map(day_text).unwrap_or_default(),
        recurring: ids.f_recurring.and_then(|i| bag.bool(i)).unwrap_or(false),
        percent: percent_of(bag, ids, status),
    }
}

/// Rappel `AAAA-MM-JJTHH:MM` (heure de Paris) → instant UTC.
fn reminder_ms(local: &str) -> Result<i64> {
    let t = chrono::NaiveDateTime::parse_from_str(local.trim(), "%Y-%m-%dT%H:%M").map_err(|_| format!("rappel invalide : {local}"))?;
    Ok(TimeZone::PARIS.to_utc(t).and_utc().timestamp_millis())
}

fn reminder_text(ms: i64) -> Option<String> {
    let utc = chrono::DateTime::from_timestamp_millis(ms)?.naive_utc();
    Some(TimeZone::PARIS.to_local(utc).format("%Y-%m-%dT%H:%M").to_string())
}

/// Toutes les tâches (hors corbeille), non terminées d'abord, par échéance.
pub fn tasks(conn: &Connection) -> Result<TaskList> {
    let src = Sources::load(conn)?;
    let folders = folders_of(conn, "IPM.Task%")?;
    let allowed: Vec<String> = folders.iter().map(|f| f.id.clone()).collect();
    let mut st = conn
        .prepare(
            "SELECT m.id, m.folder_id, f.source_id, m.subject, m.importance, m.props FROM message m JOIN folder f ON f.id = m.folder_id
             WHERE m.message_class LIKE 'IPM.Task%'",
        )
        .map_err(e)?;
    let mut rows = Vec::new();
    let mut q = st.query([]).map_err(e)?;
    while let Some(r) = q.next().map_err(e)? {
        let (id, folder, source): (i64, i64, i64) = (r.get(0).map_err(e)?, r.get(1).map_err(e)?, r.get(2).map_err(e)?);
        if !allowed.contains(&format!("f{folder}")) {
            continue;
        }
        let props: Vec<u8> = r.get(5).map_err(e)?;
        let Ok(bag) = Bag::parse(&props) else { continue };
        rows.push(task_row(id, folder, r.get(3).map_err(e)?, r.get(4).map_err(e)?, &bag, &task_ids(src.of(source))));
    }
    rows.sort_by(|a, b| {
        let key = |t: &TaskRow| (t.complete, t.due.is_empty(), t.due.clone(), t.title.to_lowercase());
        key(a).cmp(&key(b))
    });
    Ok(TaskList { rows, folders })
}

/// Tâches de la vue et du tri demandés (« Toutes / Aujourd'hui / En retard / Cette semaine / Terminées / dossier »).
pub fn tasks_query(conn: &Connection, q: &TaskQuery) -> Result<TaskList> {
    let TaskList { rows, folders } = tasks(conn)?;
    let utc = chrono::DateTime::from_timestamp_millis(now_ms()).unwrap_or_default().naive_utc();
    let today = TimeZone::PARIS.to_local(utc).date();
    Ok(TaskList { rows: filter::filter_tasks(rows, q, today), folders })
}

/// Formulaire d'une tâche existante (`t<message>`).
pub fn task_form(conn: &Connection, data_dir: &Path, id: i64) -> Result<TaskForm> {
    let (folder, source, subject, importance, props, eml): (i64, i64, String, i64, Vec<u8>, String) = conn
        .query_row(
            "SELECT m.folder_id, f.source_id, m.subject, m.importance, m.props, m.eml_path FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .map_err(|_| "tâche introuvable".to_string())?;
    let src = Sources::load(conn)?;
    let ids = task_ids(src.of(source));
    let bag = Bag::parse(&props)?;
    let reminder = match ids.reminder_set.and_then(|i| bag.bool(i)) {
        Some(true) => ids.reminder_time.and_then(|i| bag.time_ms(i)).and_then(reminder_text),
        _ => None,
    };
    let mut form = TaskForm {
        id: Some(format!("t{id}")),
        folder_id: format!("f{folder}"),
        subject,
        start: ids.start.and_then(|i| bag.time_ms(i)).map(day_text).unwrap_or_default(),
        due: ids.due.and_then(|i| bag.time_ms(i)).map(day_text).unwrap_or_default(),
        status: status_of(&bag, &ids),
        importance: importance as i32,
        body: eml_text(data_dir, &eml).trim_end().to_string(),
        categories: ids.keywords.and_then(|i| bag.strings(i)).unwrap_or_default().join(", "),
        reminder,
        ..Default::default()
    };
    form.percent = percent_of(&bag, &ids, form.status);
    let recurring = ids.f_recurring.and_then(|i| bag.bool(i)).unwrap_or(false);
    if let (true, Some(blob)) = (recurring, ids.recur.and_then(|i| bag.binary(i))) {
        // Périodicité illisible : affichée « personnalisée », donc conservée telle quelle à l'enregistrement.
        let f = trecur::decode(blob).map(|r| trecur::fields(&r, trecur::is_regen(blob)));
        form.repeat = f.as_ref().map_or("custom", |f| f.repeat).to_string();
        if let Ok(f) = f {
            (form.repeat_every, form.repeat_end, form.repeat_count, form.repeat_until) = (f.every, f.end.to_string(), f.count, f.until);
        }
    }
    Ok(form)
}

/// `custom` : périodicité à écrire telle quelle (occurrence suivante d'une série « personnalisée »).
fn task_props(n: &Named, f: &TaskForm, custom: Option<&[u8]>) -> Result<Vec<(u16, Option<Value>)>> {
    let ids = task_ids(n);
    let mut v: Vec<(u16, Option<Value>)> = Vec::new();
    let mut put = |id: Option<u16>, val: Option<Value>| {
        if let Some(id) = id {
            v.push((id, val));
        }
    };
    let (status, percent) = task_progress(f.status, f.percent);
    put(ids.status, Some(Value::Long(status)));
    put(ids.complete, Some(Value::Bool(status == 2)));
    put(ids.percent, Some(Value::Raw(0x0005, (f64::from(percent) / 100.0).to_le_bytes().to_vec())));
    put(ids.start, day_ms(&f.start)?.map(Value::time_ms));
    put(ids.due, day_ms(&f.due)?.map(Value::time_ms));
    put(ids.completed_on, if status == 2 { Some(Value::time_ms(now_ms() / DAY_MS * DAY_MS)) } else { None });
    let cats: Vec<String> = f.categories.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    put(ids.keywords, if cats.is_empty() { None } else { Some(Value::Strings(cats)) });
    let reminder = f.reminder.as_deref().map(str::trim).filter(|r| !r.is_empty());
    put(ids.reminder_set, Some(Value::Bool(reminder.is_some())));
    put(ids.reminder_time, reminder.map(reminder_ms).transpose()?.map(Value::time_ms));
    match (f.repeat.as_str(), custom) {
        ("custom", None) => {} // périodicité relue, non modifiée
        ("custom", Some(blob)) => {
            put(ids.recur, Some(Value::Binary(blob.to_vec())));
            put(ids.f_recurring, Some(Value::Bool(true)));
        }
        (kind, _) => match trecur::frequency(kind) {
            Some(freq) => {
                let anchor = [&f.start, &f.due].into_iter().find_map(|d| day_ms(d).ok().flatten()).ok_or("Indiquez un début ou une échéance pour une tâche périodique.")?;
                let anchor = chrono::DateTime::from_timestamp_millis(anchor).unwrap_or_default().date_naive();
                let regen = kind.starts_with(trecur::REGEN);
                // Une tâche qui recommence après sa fin n'a pas de fin de série (comme dans Outlook).
                let (end, count, until) = if regen { ("never", 0, "") } else { (f.repeat_end.as_str(), f.repeat_count, f.repeat_until.as_str()) };
                let rec = trecur::build(freq, f.repeat_every, end, count, until, anchor)?;
                put(ids.recur, Some(Value::Binary(if regen { trecur::encode_regen(&rec) } else { trecur::encode(&rec) })));
                put(ids.f_recurring, Some(Value::Bool(true)));
            }
            None => {
                put(ids.recur, None);
                put(ids.f_recurring, Some(Value::Bool(false)));
            }
        },
    }
    Ok(v)
}

fn new_nid(conn: &Connection, folder: i64) -> Result<i64> {
    conn.query_row("SELECT MAX(COALESCE(MAX(nid), 0), 1879048192) + 1 FROM message WHERE folder_id = ?1", [folder], |r| r.get(0)).map_err(e)
}

/// Création ou modification d'une tâche ; rend son identifiant (`t<message>`).
pub fn task_save(conn: &mut Connection, data_dir: &Path, f: &TaskForm) -> Result<String> {
    let was_done = match &f.id {
        Some(id) => id.strip_prefix('t').and_then(|x| x.parse().ok()).and_then(|n| task_form(conn, data_dir, n).ok()).is_some_and(|o| o.status == 2),
        None => false,
    };
    let id = save_inner(conn, data_dir, f, None)?;
    if task_progress(f.status, f.percent).0 == 2 && !was_done {
        if let Some(n) = id.strip_prefix('t').and_then(|x| x.parse().ok()) {
            spawn_next(conn, data_dir, n)?;
        }
    }
    Ok(id)
}

/// Une tâche périodique vient d'être terminée : crée l'occurrence suivante (début, échéance et rappel décalés) et marque
/// la terminée `PidLidTaskDeadOccurrence` ([MS-OXOTASK] : « plus d'occurrence à générer »), ce qui évite un doublon si on la
/// rouvre puis la termine de nouveau. Rend l'identifiant de la nouvelle tâche, ou `None` (pas périodique, série finie).
fn spawn_next(conn: &mut Connection, data_dir: &Path, id: i64) -> Result<Option<String>> {
    let (source, props): (i64, Vec<u8>) = conn
        .query_row("SELECT f.source_id, m.props FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(e)?;
    let ids = task_ids(Sources::load(conn)?.of(source));
    let bag = Bag::parse(&props)?;
    if ids.dead.and_then(|i| bag.bool(i)).unwrap_or(false) || !ids.f_recurring.and_then(|i| bag.bool(i)).unwrap_or(false) {
        return Ok(None);
    }
    let Some(blob) = ids.recur.and_then(|i| bag.binary(i)) else { return Ok(None) };
    let form = task_form(conn, data_dir, id)?;
    let date = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
    let regen = trecur::is_regen(blob);
    // Série fixe : ancrée sur le début ; « recommencer après la fin » : sur l'échéance, et décalée depuis aujourd'hui.
    let Some(anchor) = (if regen { date(&form.due).or_else(|| date(&form.start)) } else { date(&form.start).or_else(|| date(&form.due)) }) else { return Ok(None) };
    let next = if regen {
        let Some(freq) = trecur::frequency(&form.repeat) else { return Ok(None) };
        let utc = chrono::DateTime::from_timestamp_millis(now_ms()).unwrap_or_default().naive_utc();
        trecur::next_regen(freq, form.repeat_every, TimeZone::PARIS.to_local(utc).date())
    } else {
        trecur::next_after(&trecur::decode(blob)?, anchor)
    };
    let Some(next) = next else { return Ok(None) };
    let shift = next - anchor;
    let moved = |s: &str| date(s).map(|d| (d + shift).format("%Y-%m-%d").to_string()).unwrap_or_default();
    let mut nf = TaskForm { id: None, status: 0, percent: 0, start: moved(&form.start), due: moved(&form.due), ..form.clone() };
    nf.reminder = form.reminder.as_deref().and_then(|r| chrono::NaiveDateTime::parse_from_str(r, "%Y-%m-%dT%H:%M").ok()).map(|t| (t + shift).format("%Y-%m-%dT%H:%M").to_string());
    let custom = if form.repeat == "custom" {
        Some(blob.to_vec())
    } else {
        // Série construite par ThunderLook : ré-ancrée sur la nouvelle date, « N fois » devient « N-1 fois ».
        if form.repeat_end == "count" {
            nf.repeat_count = form.repeat_count.saturating_sub(1);
            if nf.repeat_count == 0 {
                return Ok(None);
            }
        }
        None
    };
    let new_id = save_inner(conn, data_dir, &nf, custom.as_deref())?;
    if let Some(d) = ids.dead {
        let props = bag::merge(&props, &[(d, Some(Value::Bool(true)))])?;
        conn.execute("UPDATE message SET props = ?2 WHERE id = ?1", params![id, props]).map_err(e)?;
    }
    Ok(Some(new_id))
}

fn save_inner(conn: &mut Connection, data_dir: &Path, f: &TaskForm, custom: Option<&[u8]>) -> Result<String> {
    if f.subject.trim().is_empty() {
        return Err("Indiquez un objet.".into());
    }
    let src = Sources::load(conn)?;
    match &f.id {
        Some(id) => {
            let n: i64 = id.strip_prefix('t').and_then(|x| x.parse().ok()).ok_or("tâche inconnue")?;
            let (source, old): (i64, Vec<u8>) = conn
                .query_row("SELECT f.source_id, m.props FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1", [n], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|_| "tâche introuvable".to_string())?;
            let props = bag::merge(&old, &task_props(src.of(source), f, custom)?)?;
            let eml = write_eml(data_dir, source, n, &f.subject, &f.body)?;
            conn.execute(
                "UPDATE message SET subject = ?2, importance = ?3, modified_at = ?4, preview = ?5, eml_path = ?6, props = ?7, indexed = 0 WHERE id = ?1",
                params![n, f.subject.trim(), f.importance.clamp(0, 2), now_ms(), preview(&f.body), eml, props],
            )
            .map_err(e)?;
            conn.execute("DELETE FROM message_fts WHERE rowid = ?1", [n]).map_err(e)?;
            Ok(id.clone())
        }
        None => {
            let folder = folder_num(&f.folder_id)?;
            let source: i64 = conn.query_row("SELECT source_id FROM folder WHERE id = ?1", [folder], |r| r.get(0)).map_err(|_| "dossier inconnu".to_string())?;
            let mut set = task_props(src.of(source), f, custom)?;
            set.retain(|(_, v)| v.is_some());
            let props = bag::merge(&[], &set)?;
            let (nid, now) = (new_nid(conn, folder)?, now_ms());
            let tx = conn.transaction().map_err(e)?;
            tx.execute(
                "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, created_at, modified_at, received_at,
                    message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
                 VALUES (?1, ?2, 'IPM.Task', ?3, '', '', '', '', ?4, ?4, ?4, 0, 1, 0, ?5, 0, 0, 0, 'text', ?6, '', ?7)",
                params![folder, nid, f.subject.trim(), now, f.importance.clamp(0, 2), preview(&f.body), props],
            )
            .map_err(e)?;
            let id = tx.last_insert_rowid();
            let eml = write_eml(data_dir, source, id, &f.subject, &f.body)?;
            tx.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, eml]).map_err(e)?;
            tx.commit().map_err(e)?;
            Ok(format!("t{id}"))
        }
    }
}

/// Marque une tâche terminée (ou non).
pub fn task_complete(conn: &mut Connection, data_dir: &Path, id: i64, done: bool) -> Result<()> {
    let mut f = task_form(conn, data_dir, id)?;
    (f.status, f.percent) = if done { (2, 100) } else { (0, 0) };
    task_save(conn, data_dir, &f).map(|_| ())
}

fn delete_message(conn: &mut Connection, id: i64) -> Result<()> {
    thunderlook_safety::snapshot_conn(conn, id)?;
    conn.execute("DELETE FROM message_fts WHERE rowid = ?1", [id]).map_err(e)?;
    conn.execute("DELETE FROM message WHERE id = ?1", [id]).map_err(e)?;
    Ok(())
}

/// Supprime la tâche et toute sa série : aucune occurrence suivante n'est créée.
pub fn task_delete(conn: &mut Connection, id: i64) -> Result<()> {
    delete_message(conn, id)
}

/// Supprime UNE occurrence d'une tâche périodique non terminée : l'occurrence suivante est créée (comme à la fin de la
/// tâche) puis celle-ci est supprimée. Rend l'identifiant de la suivante, `None` si la tâche n'est pas périodique ou si la
/// série se termine là (alors simple suppression).
pub fn task_delete_occurrence(conn: &mut Connection, data_dir: &Path, id: i64) -> Result<Option<String>> {
    let next = spawn_next(conn, data_dir, id)?;
    delete_message(conn, id)?;
    Ok(next)
}

// --- Notes ----------------------------------------------------------------------------------------------------------

/// Couleur d'une note : `PidLidNoteColor` ([MS-OXONOTE], 0x8B00 dans PSETID_Note ; 0 bleue, 1 verte, 2 rose, 3 jaune,
/// 4 blanche).
fn note_color_id(n: &Named) -> Option<u16> {
    n.id(PSETID_NOTE, 0x8B00)
}

/// Valeur de repli explicite quand la source n'a pas la propriété ou que la note n'en porte pas : jaune.
const NOTE_COLOR_FALLBACK: i32 = 3;

fn note_color(n: &Named, bag: Option<&Bag>) -> i32 {
    bag.zip(note_color_id(n)).and_then(|(b, i)| b.int(i)).filter(|c| (0..=4).contains(c)).map_or(NOTE_COLOR_FALLBACK, |c| c as i32)
}

/// Toutes les notes (hors corbeille), les plus récentes d'abord.
pub fn notes(conn: &Connection, data_dir: &Path) -> Result<NoteList> {
    let mut l = notes_full(conn, data_dir)?;
    l.rows.iter_mut().for_each(|r| r.text = r.text.chars().take(600).collect());
    Ok(l)
}

/// Notes dont le texte (entier) contient la recherche, triées (récentes d'abord ou alphabétique).
pub fn notes_query(conn: &Connection, data_dir: &Path, q: &NoteQuery) -> Result<NoteList> {
    let NoteList { rows, folders } = notes_full(conn, data_dir)?;
    let mut rows = filter::filter_notes(rows, q);
    rows.iter_mut().for_each(|r| r.text = r.text.chars().take(600).collect());
    Ok(NoteList { rows, folders })
}

fn notes_full(conn: &Connection, data_dir: &Path) -> Result<NoteList> {
    let src = Sources::load(conn)?;
    let folders = folders_of(conn, "IPM.StickyNote%")?;
    let allowed: Vec<String> = folders.iter().map(|f| f.id.clone()).collect();
    let mut st = conn
        .prepare(
            "SELECT m.id, m.folder_id, f.source_id, m.subject, COALESCE(m.modified_at, m.created_at, 0), m.props, m.eml_path
             FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.message_class LIKE 'IPM.StickyNote%' ORDER BY 5 DESC",
        )
        .map_err(e)?;
    let mut rows = Vec::new();
    let mut q = st.query([]).map_err(e)?;
    while let Some(r) = q.next().map_err(e)? {
        let (id, folder, source): (i64, i64, i64) = (r.get(0).map_err(e)?, r.get(1).map_err(e)?, r.get(2).map_err(e)?);
        if !allowed.contains(&format!("f{folder}")) {
            continue;
        }
        let (subject, when): (String, i64) = (r.get(3).map_err(e)?, r.get(4).map_err(e)?);
        let props: Vec<u8> = r.get(5).map_err(e)?;
        let eml: String = r.get(6).map_err(e)?;
        let color = note_color(src.of(source), Bag::parse(&props).ok().as_ref());
        let text = eml_text(data_dir, &eml);
        rows.push(NoteRow { id: format!("n{id}"), title: subject, text: text.trim().to_string(), color, date: day_text(when), folder: format!("f{folder}") });
    }
    Ok(NoteList { rows, folders })
}

pub fn note_form(conn: &Connection, data_dir: &Path, id: i64) -> Result<NoteForm> {
    let (folder, source, props, eml): (i64, i64, Vec<u8>, String) = conn
        .query_row("SELECT m.folder_id, f.source_id, m.props, m.eml_path FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .map_err(|_| "note introuvable".to_string())?;
    let color = match Sources::load(conn) {
        Ok(src) => note_color(src.of(source), Bag::parse(&props).ok().as_ref()),
        Err(_) => NOTE_COLOR_FALLBACK,
    };
    Ok(NoteForm { id: Some(format!("n{id}")), folder_id: format!("f{folder}"), body: eml_text(data_dir, &eml).trim_end().to_string(), color })
}

/// Création ou modification d'une note ; le titre est la première ligne du texte. Rend `n<message>`.
pub fn note_save(conn: &mut Connection, data_dir: &Path, f: &NoteForm) -> Result<String> {
    let title: String = f.body.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").chars().take(255).collect();
    if title.is_empty() {
        return Err("Écrivez le texte de la note.".into());
    }
    let src = Sources::load(conn)?;
    let color = f.color.clamp(0, 4);
    match &f.id {
        Some(id) => {
            let n: i64 = id.strip_prefix('n').and_then(|x| x.parse().ok()).ok_or("note inconnue")?;
            let (source, old): (i64, Vec<u8>) = conn
                .query_row("SELECT f.source_id, m.props FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1", [n], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|_| "note introuvable".to_string())?;
            let set: Vec<(u16, Option<Value>)> = note_color_id(src.of(source)).map(|i| vec![(i, Some(Value::Long(color)))]).unwrap_or_default();
            let props = bag::merge(&old, &set)?;
            let eml = write_eml(data_dir, source, n, &title, &f.body)?;
            conn.execute("UPDATE message SET subject = ?2, modified_at = ?3, preview = ?4, eml_path = ?5, props = ?6, indexed = 0 WHERE id = ?1", params![n, title, now_ms(), preview(&f.body), eml, props]).map_err(e)?;
            conn.execute("DELETE FROM message_fts WHERE rowid = ?1", [n]).map_err(e)?;
            Ok(id.clone())
        }
        None => {
            let folder = folder_num(&f.folder_id)?;
            let source: i64 = conn.query_row("SELECT source_id FROM folder WHERE id = ?1", [folder], |r| r.get(0)).map_err(|_| "dossier inconnu".to_string())?;
            let set: Vec<(u16, Option<Value>)> = note_color_id(src.of(source)).map(|i| vec![(i, Some(Value::Long(color)))]).unwrap_or_default();
            let props = bag::merge(&[], &set)?;
            let (nid, now) = (new_nid(conn, folder)?, now_ms());
            let tx = conn.transaction().map_err(e)?;
            tx.execute(
                "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, created_at, modified_at, received_at,
                    message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
                 VALUES (?1, ?2, 'IPM.StickyNote', ?3, '', '', '', '', ?4, ?4, ?4, 0, 1, 0, 1, 0, 0, 0, 'text', ?5, '', ?6)",
                params![folder, nid, title, now, preview(&f.body), props],
            )
            .map_err(e)?;
            let id = tx.last_insert_rowid();
            let eml = write_eml(data_dir, source, id, &title, &f.body)?;
            tx.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, eml]).map_err(e)?;
            tx.commit().map_err(e)?;
            Ok(format!("n{id}"))
        }
    }
}

pub fn note_delete(conn: &mut Connection, id: i64) -> Result<()> {
    delete_message(conn, id)
}

/// Dossier par défaut pour un nouvel élément : celui qui contient le plus d'éléments du même type.
pub fn default_folder(conn: &Connection, class_like: &str) -> Result<Option<String>> {
    Ok(folders_of(conn, class_like)?.into_iter().next().map(|f| f.id))
}

#[allow(dead_code)]
fn _unused(conn: &Connection) -> Option<()> {
    conn.query_row("SELECT 1", [], |_| Ok(())).optional().ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_round_trip() {
        let ms = day_ms("2026-10-23").unwrap().unwrap();
        assert_eq!(day_text(ms), "2026-10-23");
        assert_eq!(day_ms("").unwrap(), None);
    }

    #[test]
    fn trash_folders_are_skipped() {
        assert!(is_trash("Éléments supprimés") && is_trash("Deleted Items") && !is_trash("Tâches"));
    }

    fn named_table(lids: &[(&str, u32)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (n, (guid, lid)) in lids.iter().enumerate() {
            out.extend_from_slice(&(0x8000u16 + n as u16).to_le_bytes());
            out.extend_from_slice(guid.as_bytes());
            out.push(0);
            out.extend_from_slice(&lid.to_le_bytes());
        }
        out
    }

    /// Base vide avec une source dont la table de noms a `lids`, et un dossier `f1`.
    fn db(name: &str, lids: &[(&str, u32)]) -> (Connection, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("tl-items-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let conn = thunderlook_store::open(&dir).unwrap();
        conn.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at, props) VALUES (1, 'pst', '', '', 'Test', 0, 0, ?1)", [named_table(lids)]).unwrap();
        conn.execute("INSERT INTO folder (id, source_id, nid, name, role, hidden, position) VALUES (1, 1, 1, 'Tâches', 'folder', 0, 0)", []).unwrap();
        (conn, dir)
    }

    const TASK_LIDS: [(&str, u32); 12] = [
        (PSETID_TASK, 0x8101),
        (PSETID_TASK, 0x8102),
        (PSETID_TASK, 0x8104),
        (PSETID_TASK, 0x8105),
        (PSETID_TASK, 0x810F),
        (PSETID_TASK, 0x811C),
        (PSETID_TASK, 0x8116),
        (PSETID_TASK, 0x8126),
        (PSETID_TASK, 0x8109),
        (PSETID_COMMON, 0x8503),
        (PSETID_COMMON, 0x8502),
        (PSETID_NOTE, 0x8B00),
    ];

    fn form(subject: &str) -> TaskForm {
        TaskForm { folder_id: "f1".into(), subject: subject.into(), importance: 1, ..Default::default() }
    }

    #[test]
    fn delete_then_undo_gives_the_same_form() {
        let (mut conn, dir) = db("undo", &TASK_LIDS);
        let f = TaskForm { due: "2026-10-30".into(), reminder: Some("2026-10-29T09:30".into()), ..form("Appeler le client") };
        let id = task_save(&mut conn, &dir, &f).unwrap();
        let n: i64 = id[1..].parse().unwrap();
        let before = task_form(&conn, &dir, n).unwrap();
        task_delete(&mut conn, n).unwrap();
        assert!(task_form(&conn, &dir, n).is_err());
        assert_eq!(thunderlook_safety::undo(&mut conn, &dir).unwrap(), "Annulé : suppression du tâche « Appeler le client »");
        assert_eq!(task_form(&conn, &dir, n).unwrap(), before);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn percent_and_status_round_trip() {
        let (mut conn, dir) = db("percent", &TASK_LIDS);
        let id = task_save(&mut conn, &dir, &TaskForm { status: 0, percent: 40, ..form("Rapport") }).unwrap();
        let n: i64 = id[1..].parse().unwrap();
        let back = task_form(&conn, &dir, n).unwrap();
        assert_eq!((back.status, back.percent), (1, 40));
        assert_eq!(tasks(&conn).unwrap().rows[0].percent, 40);
        for status in [3, 4] {
            task_save(&mut conn, &dir, &TaskForm { status, percent: 70, ..back.clone() }).unwrap();
            assert_eq!(task_form(&conn, &dir, n).map(|f| (f.status, f.percent)).unwrap(), (status, 70));
        }
        task_save(&mut conn, &dir, &TaskForm { percent: 100, ..back.clone() }).unwrap();
        let done = task_form(&conn, &dir, n).unwrap();
        assert_eq!((done.status, done.percent), (2, 100));
        assert!(tasks(&conn).unwrap().rows[0].complete);
        task_complete(&mut conn, &dir, n, false).unwrap();
        assert_eq!(task_form(&conn, &dir, n).map(|f| (f.status, f.percent)).unwrap(), (0, 0));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reminder_round_trip() {
        let (mut conn, dir) = db("reminder", &TASK_LIDS);
        let f = TaskForm { due: "2026-10-30".into(), reminder: Some("2026-10-29T09:30".into()), ..form("Appeler") };
        let id = task_save(&mut conn, &dir, &f).unwrap();
        let back = task_form(&conn, &dir, id[1..].parse().unwrap()).unwrap();
        assert_eq!(back.reminder.as_deref(), Some("2026-10-29T09:30"));
        task_save(&mut conn, &dir, &TaskForm { reminder: None, ..back }).unwrap();
        assert_eq!(task_form(&conn, &dir, id[1..].parse().unwrap()).unwrap().reminder, None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn completing_a_weekly_task_creates_the_next_one() {
        let (mut conn, dir) = db("weekly", &TASK_LIDS);
        let f = TaskForm {
            start: "2026-10-23".into(),
            due: "2026-10-25".into(),
            reminder: Some("2026-10-23T08:00".into()),
            repeat: "weekly".into(),
            repeat_every: 1,
            repeat_end: "count".into(),
            repeat_count: 2,
            ..form("Rapport")
        };
        let id = task_save(&mut conn, &dir, &f).unwrap();
        let n: i64 = id[1..].parse().unwrap();
        let read = task_form(&conn, &dir, n).unwrap();
        assert_eq!((read.repeat.as_str(), read.repeat_end.as_str(), read.repeat_count), ("weekly", "count", 2));
        assert!(tasks(&conn).unwrap().rows[0].recurring);

        task_complete(&mut conn, &dir, n, true).unwrap();
        let rows = tasks(&conn).unwrap().rows;
        assert_eq!(rows.len(), 2);
        let next = rows.iter().find(|r| !r.complete).unwrap();
        assert_eq!((next.start.as_str(), next.due.as_str()), ("2026-10-30", "2026-11-01"));
        let nf = task_form(&conn, &dir, next.id[1..].parse().unwrap()).unwrap();
        assert_eq!((nf.reminder.as_deref(), nf.repeat_count), (Some("2026-10-30T08:00"), 1));

        // Rouvrir puis terminer de nouveau la première : pas de doublon.
        task_complete(&mut conn, &dir, n, false).unwrap();
        task_complete(&mut conn, &dir, n, true).unwrap();
        assert_eq!(tasks(&conn).unwrap().rows.len(), 2);

        // La dernière occurrence (N = 1) ne génère rien.
        task_complete(&mut conn, &dir, next.id[1..].parse().unwrap(), true).unwrap();
        assert_eq!(tasks(&conn).unwrap().rows.len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn regenerating_task_restarts_from_the_completion_day() {
        let (mut conn, dir) = db("regen", &TASK_LIDS);
        let f = TaskForm {
            start: "2026-01-01".into(),
            due: "2026-01-05".into(),
            reminder: Some("2026-01-04T09:00".into()),
            repeat: "regen_daily".into(),
            repeat_every: 10,
            repeat_end: "count".into(), // ignorée : pas de fin pour ce type de série
            repeat_count: 2,
            ..form("Arroser")
        };
        let id = task_save(&mut conn, &dir, &f).unwrap();
        let n: i64 = id[1..].parse().unwrap();
        let read = task_form(&conn, &dir, n).unwrap();
        assert_eq!((read.repeat.as_str(), read.repeat_every, read.repeat_end.as_str()), ("regen_daily", 10, "never"));
        task_complete(&mut conn, &dir, n, true).unwrap();
        let rows = tasks(&conn).unwrap().rows;
        assert_eq!(rows.len(), 2);
        let next = rows.iter().find(|r| !r.complete).unwrap();
        let today = {
            let utc = chrono::DateTime::from_timestamp_millis(now_ms()).unwrap().naive_utc();
            TimeZone::PARIS.to_local(utc).date()
        };
        let due = today + chrono::Duration::days(10);
        assert_eq!(next.due, due.format("%Y-%m-%d").to_string());
        // L'écart début/échéance (4 jours) et le rappel (1 jour avant l'échéance) sont conservés.
        assert_eq!(next.start, (due - chrono::Duration::days(4)).format("%Y-%m-%d").to_string());
        let nf = task_form(&conn, &dir, next.id[1..].parse().unwrap()).unwrap();
        assert_eq!(nf.reminder, Some(format!("{}T09:00", (due - chrono::Duration::days(1)).format("%Y-%m-%d"))));
        assert_eq!(nf.repeat, "regen_daily");
        // La suivante est elle aussi périodique : terminée, elle en crée une troisième.
        task_complete(&mut conn, &dir, next.id[1..].parse().unwrap(), true).unwrap();
        assert_eq!(tasks(&conn).unwrap().rows.len(), 3);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn deleting_one_occurrence_keeps_the_series() {
        let (mut conn, dir) = db("delocc", &TASK_LIDS);
        let f = TaskForm {
            start: "2026-10-23".into(),
            repeat: "weekly".into(),
            repeat_every: 1,
            repeat_end: "count".into(),
            repeat_count: 3,
            ..form("Sauvegarde")
        };
        let n: i64 = task_save(&mut conn, &dir, &f).unwrap()[1..].parse().unwrap();
        let next = task_delete_occurrence(&mut conn, &dir, n).unwrap().expect("la suivante est créée");
        let rows = tasks(&conn).unwrap().rows;
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].id.as_str(), rows[0].start.as_str(), rows[0].recurring), (next.as_str(), "2026-10-30", true));
        // « N fois » : 3 → 2 restantes ; supprimer la dernière ne crée rien.
        let last = task_delete_occurrence(&mut conn, &dir, next[1..].parse().unwrap()).unwrap().unwrap();
        assert_eq!(task_delete_occurrence(&mut conn, &dir, last[1..].parse().unwrap()).unwrap(), None);
        assert!(tasks(&conn).unwrap().rows.is_empty());
        // Tâche simple : simple suppression.
        let p: i64 = task_save(&mut conn, &dir, &form("Simple")).unwrap()[1..].parse().unwrap();
        assert_eq!(task_delete_occurrence(&mut conn, &dir, p).unwrap(), None);
        assert!(tasks(&conn).unwrap().rows.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn monthly_task_on_the_31st_stays_one_per_month() {
        let (mut conn, dir) = db("eom", &TASK_LIDS);
        let f = TaskForm { due: "2026-01-31".into(), repeat: "monthly".into(), repeat_every: 1, repeat_end: "never".into(), ..form("Loyer") };
        let mut n: i64 = task_save(&mut conn, &dir, &f).unwrap()[1..].parse().unwrap();
        let mut prev = "2026-01-31".to_string();
        for _ in 0..4 {
            task_complete(&mut conn, &dir, n, true).unwrap();
            let rows = tasks(&conn).unwrap().rows;
            let open = rows.iter().find(|r| !r.complete).unwrap();
            assert!(open.due > prev, "{} après {prev}", open.due);
            prev = open.due.clone();
            n = open.id[1..].parse().unwrap();
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn plain_task_has_no_next_and_repeat_needs_a_date() {
        let (mut conn, dir) = db("plain", &TASK_LIDS);
        let id = task_save(&mut conn, &dir, &form("Simple")).unwrap();
        task_complete(&mut conn, &dir, id[1..].parse().unwrap(), true).unwrap();
        assert_eq!(tasks(&conn).unwrap().rows.len(), 1);
        let bad = TaskForm { repeat: "daily".into(), repeat_every: 1, repeat_end: "never".into(), ..form("Sans date") };
        assert!(task_save(&mut conn, &dir, &bad).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn note_color_property_and_fallback() {
        let note = |c| NoteForm { folder_id: "f1".into(), body: "Courses\npain".into(), color: c, id: None };
        let (mut conn, dir) = db("notecolor", &TASK_LIDS);
        let id = note_save(&mut conn, &dir, &note(1)).unwrap();
        assert_eq!(note_form(&conn, &dir, id[1..].parse().unwrap()).unwrap().color, 1);
        assert_eq!(notes(&conn, &dir).unwrap().rows[0].color, 1);
        let _ = std::fs::remove_dir_all(dir);
        // Source sans PidLidNoteColor : repli explicite (jaune), sans erreur.
        let (mut conn, dir) = db("nocolor", &[(PSETID_TASK, 0x8101)]);
        let id = note_save(&mut conn, &dir, &note(1)).unwrap();
        assert_eq!(note_form(&conn, &dir, id[1..].parse().unwrap()).unwrap().color, NOTE_COLOR_FALLBACK);
        assert_eq!(notes(&conn, &dir).unwrap().rows[0].color, NOTE_COLOR_FALLBACK);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn notes_full_text_search() {
        let (mut conn, dir) = db("search", &TASK_LIDS);
        let long = format!("Titre\n{}fin-secrète", "x ".repeat(400));
        note_save(&mut conn, &dir, &NoteForm { folder_id: "f1".into(), body: long, color: 3, id: None }).unwrap();
        note_save(&mut conn, &dir, &NoteForm { folder_id: "f1".into(), body: "Autre".into(), color: 3, id: None }).unwrap();
        let q = NoteQuery { text: "FIN-SECRÈTE".into(), sort: "alpha".into() };
        let l = notes_query(&conn, &dir, &q).unwrap();
        assert_eq!(l.rows.len(), 1);
        assert!(l.rows[0].text.chars().count() <= 600);
        let _ = std::fs::remove_dir_all(dir);
    }
}
