//! Tâches et notes de ThunderLook : lues dans les sacs MAPI importés ([MS-OXOTASK], [MS-OXONOTE]) et écrites dans la
//! base, sans Outlook. Le texte (description d'une tâche, contenu d'une note) est dans le `.eml` de l'élément.

use std::{collections::HashMap, path::Path};

use thunderlook_calendar::{
    bag::{self, Bag, Value},
    named::{Named, PS_PUBLIC_STRINGS},
};
use thunderlook_model::items::{ItemFolder, NoteForm, NoteList, NoteRow, TaskForm, TaskList, TaskRow};
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
        keywords: n.name(PS_PUBLIC_STRINGS, "Keywords"),
    }
}

fn status_of(bag: &Bag, ids: &TaskIds) -> i32 {
    if ids.complete.and_then(|i| bag.bool(i)).unwrap_or(false) {
        return 2;
    }
    ids.status.and_then(|i| bag.int(i)).unwrap_or(0) as i32
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
    }
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
    Ok(TaskForm {
        id: Some(format!("t{id}")),
        folder_id: format!("f{folder}"),
        subject,
        start: ids.start.and_then(|i| bag.time_ms(i)).map(day_text).unwrap_or_default(),
        due: ids.due.and_then(|i| bag.time_ms(i)).map(day_text).unwrap_or_default(),
        status: status_of(&bag, &ids),
        importance: importance as i32,
        body: eml_text(data_dir, &eml).trim_end().to_string(),
        categories: ids.keywords.and_then(|i| bag.strings(i)).unwrap_or_default().join(", "),
    })
}

fn task_props(n: &Named, f: &TaskForm) -> Result<Vec<(u16, Option<Value>)>> {
    let ids = task_ids(n);
    let mut v: Vec<(u16, Option<Value>)> = Vec::new();
    let mut put = |id: Option<u16>, val: Option<Value>| {
        if let Some(id) = id {
            v.push((id, val));
        }
    };
    let status = f.status.clamp(0, 4);
    put(ids.status, Some(Value::Long(status)));
    put(ids.complete, Some(Value::Bool(status == 2)));
    put(ids.percent, Some(Value::Raw(0x0005, (if status == 2 { 1.0f64 } else { 0.0 }).to_le_bytes().to_vec())));
    put(ids.start, day_ms(&f.start)?.map(Value::time_ms));
    put(ids.due, day_ms(&f.due)?.map(Value::time_ms));
    put(ids.completed_on, if status == 2 { Some(Value::time_ms(now_ms() / DAY_MS * DAY_MS)) } else { None });
    let cats: Vec<String> = f.categories.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    put(ids.keywords, if cats.is_empty() { None } else { Some(Value::Strings(cats)) });
    Ok(v)
}

fn new_nid(conn: &Connection, folder: i64) -> Result<i64> {
    conn.query_row("SELECT MAX(COALESCE(MAX(nid), 0), 1879048192) + 1 FROM message WHERE folder_id = ?1", [folder], |r| r.get(0)).map_err(e)
}

/// Création ou modification d'une tâche ; rend son identifiant (`t<message>`).
pub fn task_save(conn: &mut Connection, data_dir: &Path, f: &TaskForm) -> Result<String> {
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
            let props = bag::merge(&old, &task_props(src.of(source), f)?)?;
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
            let mut set = task_props(src.of(source), f)?;
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
    f.status = if done { 2 } else { 0 };
    task_save(conn, data_dir, &f).map(|_| ())
}

fn delete_message(conn: &mut Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM message_fts WHERE rowid = ?1", [id]).map_err(e)?;
    conn.execute("DELETE FROM message WHERE id = ?1", [id]).map_err(e)?;
    Ok(())
}

pub fn task_delete(conn: &mut Connection, id: i64) -> Result<()> {
    delete_message(conn, id)
}

// --- Notes ----------------------------------------------------------------------------------------------------------

/// Couleur d'une note : `PidLidNoteColor` (0 bleue, 1 verte, 2 rose, 3 jaune, 4 blanche), jaune par défaut.
fn note_color_id(n: &Named) -> Option<u16> {
    n.id(PSETID_NOTE, 0x8B00)
}

/// Toutes les notes (hors corbeille), les plus récentes d'abord.
pub fn notes(conn: &Connection, data_dir: &Path) -> Result<NoteList> {
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
        let color = Bag::parse(&props).ok().and_then(|b| note_color_id(src.of(source)).and_then(|i| b.int(i))).unwrap_or(3) as i32;
        let text = eml_text(data_dir, &eml);
        rows.push(NoteRow { id: format!("n{id}"), title: subject, text: text.trim().chars().take(600).collect(), color, date: day_text(when), folder: format!("f{folder}") });
    }
    Ok(NoteList { rows, folders })
}

pub fn note_form(conn: &Connection, data_dir: &Path, id: i64) -> Result<NoteForm> {
    let (folder, source, props, eml): (i64, i64, Vec<u8>, String) = conn
        .query_row("SELECT m.folder_id, f.source_id, m.props, m.eml_path FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .map_err(|_| "note introuvable".to_string())?;
    let color = Bag::parse(&props).ok().and_then(|b| { let src = Sources::load(conn).ok()?; note_color_id(src.of(source)).and_then(|i| b.int(i)) }).unwrap_or(3) as i32;
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
}
