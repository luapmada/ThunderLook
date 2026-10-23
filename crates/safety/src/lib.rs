//! Filet de sécurité des données modifiables (rendez-vous, contacts, tâches, notes) : ThunderLook est leur seul dépositaire.
//!
//! - **Corbeille interne** : [`snapshot`] enregistre le contenu complet d'un élément (ligne `message`, pièces, catégories,
//!   `.eml`) dans `data_dir/trash/<horodatage>-<type>-<id>.json` ; [`restore`] le remet. [`purge`] vide au bout de 60 jours.
//! - **Annulation** : chaque instantané alimente une pile de 50 niveaux ; [`undo`] défait le dernier.
//! - **Sauvegardes** : [`backup`] écrit un JSON texte par jour (14 gardés) et une copie par semaine (8 gardées), jamais la
//!   base entière ; [`restore_backup`] les relit (fusion ou remise complète).
//!
//! Aucune fonction d'ici ne supprime ni ne réécrit une ligne de la base sans en avoir enregistré l'ancien contenu avant.

use std::{
    fs,
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, Utc};
use serde_json::{Map, Value, json};
use thunderlook_store::rusqlite::{self, Connection, OptionalExtension, params, types::ValueRef};

pub type Result<T> = std::result::Result<T, String>;

/// Durée de garde de la corbeille interne.
pub const TRASH_DAYS: i64 = 60;
/// Niveaux d'annulation.
pub const UNDO_LEVELS: usize = 50;
/// Sauvegardes quotidiennes gardées.
pub const KEEP_DAILY: usize = 14;
/// Copies hebdomadaires gardées.
pub const KEEP_WEEKLY: usize = 8;

const FORMAT_VERSION: i64 = 1;
/// Classes des éléments protégés (`message_class LIKE`).
const CLASSES: [&str; 5] = ["IPM.Appointment%", "IPM.Contact%", "IPM.DistList%", "IPM.Task%", "IPM.StickyNote%"];

fn e(x: rusqlite::Error) -> String {
    x.to_string()
}

fn io(path: &Path, x: std::io::Error) -> String {
    format!("{} : {x}", path.display())
}

// --- Types d'éléments ------------------------------------------------------------------------------------------------

/// (nom de fichier, libellé français) d'après la classe du message.
fn kind_of(class: &str) -> (&'static str, &'static str) {
    let c = class.to_ascii_lowercase();
    if c.starts_with("ipm.appointment") {
        ("appointment", "rendez-vous")
    } else if c.starts_with("ipm.contact") || c.starts_with("ipm.distlist") {
        ("contact", "contact")
    } else if c.starts_with("ipm.task") {
        ("task", "tâche")
    } else if c.starts_with("ipm.stickynote") {
        ("note", "note")
    } else {
        ("item", "élément")
    }
}

// --- Lignes SQL <-> JSON ---------------------------------------------------------------------------------------------

fn encode(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!({ "r": f }),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => json!({ "b": B64.encode(b) }),
    }
}

fn decode(v: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as Sql;
    match v {
        Value::Null => Sql::Null,
        Value::Number(n) => Sql::Integer(n.as_i64().unwrap_or_default()),
        Value::String(s) => Sql::Text(s.clone()),
        Value::Object(o) => {
            if let Some(f) = o.get("r").and_then(Value::as_f64) {
                Sql::Real(f)
            } else if let Some(b) = o.get("b").and_then(Value::as_str) {
                Sql::Blob(B64.decode(b).unwrap_or_default())
            } else {
                Sql::Null
            }
        }
        _ => Sql::Null,
    }
}

/// Lignes de `sql` sous forme d'objets colonne -> valeur (toutes les colonnes : un schéma qui s'enrichit reste couvert).
fn rows_as_objects(conn: &Connection, sql: &str, id: i64) -> Result<Vec<Map<String, Value>>> {
    let mut stmt = conn.prepare(sql).map_err(e)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt
        .query_map([id], |r| {
            let mut o = Map::new();
            for (i, n) in names.iter().enumerate() {
                o.insert(n.clone(), encode(r.get_ref(i)?));
            }
            Ok(o)
        })
        .map_err(e)?;
    rows.collect::<rusqlite::Result<_>>().map_err(e)
}

fn table_columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("SELECT name FROM pragma_table_info('{table}')")).map_err(e)?;
    stmt.query_map([], |r| r.get(0)).map_err(e)?.collect::<rusqlite::Result<_>>().map_err(e)
}

/// Dossier de données d'une connexion ouverte par `thunderlook_store::open` (le fichier de base y vit).
fn dir_of(conn: &Connection) -> Result<PathBuf> {
    conn.path().and_then(|p| Path::new(p).parent()).map(Path::to_path_buf).ok_or_else(|| "base sans fichier : sauvegarde impossible".to_string())
}

// --- Enregistrement d'un élément -------------------------------------------------------------------------------------

/// Contenu complet d'un élément : ligne `message`, pièces, catégories et `.eml`.
fn record(conn: &Connection, data_dir: &Path, id: i64, reason: &str) -> Result<Value> {
    let message = rows_as_objects(conn, "SELECT * FROM message WHERE id = ?1", id)?.into_iter().next().ok_or_else(|| format!("élément {id} introuvable"))?;
    let class = message.get("message_class").and_then(Value::as_str).unwrap_or_default();
    let subject = message.get("subject").and_then(Value::as_str).unwrap_or_default().to_string();
    let (_, label) = kind_of(class);
    let attachments = rows_as_objects(conn, "SELECT * FROM attachment WHERE message_id = ?1 ORDER BY position", id)?;
    let categories: Vec<String> = {
        let mut stmt = conn.prepare("SELECT name FROM message_category WHERE message_id = ?1").map_err(e)?;
        stmt.query_map([id], |r| r.get(0)).map_err(e)?.collect::<rusqlite::Result<_>>().map_err(e)?
    };
    let eml = match message.get("eml_path").and_then(Value::as_str).filter(|p| !p.is_empty()) {
        Some(p) => match fs::read(thunderlook_store::resolve(data_dir, p)) {
            Ok(bytes) => Value::String(B64.encode(bytes)),
            Err(_) => Value::Null,
        },
        None => Value::Null,
    };
    Ok(json!({
        "version": FORMAT_VERSION,
        "kind": label,
        "reason": reason,
        "saved_at": Utc::now().timestamp_millis(),
        "id": id,
        "title": subject,
        "message": message,
        "attachments": attachments,
        "categories": categories,
        "eml": eml,
    }))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|x| io(dir, x))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|x| io(&tmp, x))?;
    fs::rename(&tmp, path).map_err(|x| io(path, x))
}

fn trash_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("trash")
}

/// Écrit l'instantané dans la corbeille sans toucher à la pile d'annulation.
fn save_to_trash(conn: &Connection, data_dir: &Path, id: i64, reason: &str) -> Result<(PathBuf, String)> {
    let rec = record(conn, data_dir, id, reason)?;
    let class = rec["message"]["message_class"].as_str().unwrap_or_default();
    let (slug, _) = kind_of(class);
    let stamp = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let dir = trash_dir(data_dir);
    let mut path = dir.join(format!("{stamp}-{slug}-{id}.json"));
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{stamp}-{slug}-{id}-{n}.json"));
        n += 1;
    }
    write_atomic(&path, &serde_json::to_vec(&rec).map_err(|x| x.to_string())?)?;
    Ok((path, describe(&rec)))
}

/// « contact « Dupont » ».
fn describe(rec: &Value) -> String {
    let kind = rec["kind"].as_str().unwrap_or("élément");
    match rec["title"].as_str().filter(|t| !t.trim().is_empty()) {
        Some(t) => format!("{kind} « {t} »"),
        None => format!("{kind} sans titre"),
    }
}

// --- Pile d'annulation -----------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    Delete,
    Edit,
}

struct Entry {
    data_dir: PathBuf,
    change: Change,
    file: PathBuf,
    what: String,
}

static UNDO: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

fn push(entry: Entry) {
    let mut s = UNDO.lock().unwrap_or_else(|p| p.into_inner());
    s.push(entry);
    if s.len() > UNDO_LEVELS {
        let extra = s.len() - UNDO_LEVELS;
        s.drain(..extra);
    }
}

/// Niveaux d'annulation disponibles pour ce dossier de données (bouton « Annuler » grisé à 0).
pub fn undo_depth(data_dir: &Path) -> usize {
    UNDO.lock().unwrap_or_else(|p| p.into_inner()).iter().filter(|x| x.data_dir == data_dir).count()
}

/// À appeler AVANT de supprimer un élément : enregistre son contenu complet dans la corbeille et dans la pile d'annulation.
/// Échoue (et il ne faut alors pas supprimer) si la copie n'a pas pu être écrite.
pub fn snapshot(conn: &Connection, data_dir: &Path, message_id: i64) -> Result<PathBuf> {
    let (file, what) = save_to_trash(conn, data_dir, message_id, "suppression")?;
    push(Entry { data_dir: data_dir.to_path_buf(), change: Change::Delete, file: file.clone(), what });
    Ok(file)
}

/// Comme [`snapshot`], pour les fonctions qui n'ont que la connexion (dossier de données déduit du fichier de base).
pub fn snapshot_conn(conn: &Connection, message_id: i64) -> Result<PathBuf> {
    snapshot(conn, &dir_of(conn)?, message_id)
}

/// À appeler AVANT de modifier un élément : l'ancien contenu est gardé pour l'annulation. Un élément déjà absent est ignoré.
pub fn snapshot_edit(conn: &Connection, data_dir: &Path, message_id: i64) -> Result<()> {
    let exists = conn.query_row("SELECT 1 FROM message WHERE id = ?1", [message_id], |_| Ok(())).optional().map_err(e)?.is_some();
    if !exists {
        return Ok(());
    }
    let (file, what) = save_to_trash(conn, data_dir, message_id, "modification")?;
    push(Entry { data_dir: data_dir.to_path_buf(), change: Change::Edit, file, what });
    Ok(())
}

/// Défait la dernière suppression ou modification ; rend le message de la barre d'état (« Annulé : … »).
pub fn undo(conn: &mut Connection, data_dir: &Path) -> Result<String> {
    let entry = {
        let mut s = UNDO.lock().unwrap_or_else(|p| p.into_inner());
        let i = s.iter().rposition(|x| x.data_dir == data_dir).ok_or("Rien à annuler")?;
        s.remove(i)
    };
    match restore(conn, data_dir, &entry.file) {
        Ok(_) => Ok(format!("Annulé : {} {}", if entry.change == Change::Delete { "suppression du" } else { "modification du" }, entry.what)),
        Err(err) => {
            let mut s = UNDO.lock().unwrap_or_else(|p| p.into_inner());
            s.push(entry);
            Err(err)
        }
    }
}

// --- Remise en base --------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Placed {
    /// Remis, n'existait pas.
    Added(i64),
    /// Remis à la place d'une ligne présente (l'ancienne est gardée dans la corbeille).
    Replaced(i64),
    /// Déjà présent : laissé tel quel.
    Skipped(i64),
}

/// Remet l'élément de `rec`. `replace` : écrase la ligne présente (identique par dossier, `nid` et date de création) après
/// l'avoir mise en corbeille ; sinon la laisse. Identifiant d'origine repris s'il est libre, sinon un nouveau est attribué.
fn place(conn: &mut Connection, data_dir: &Path, rec: &Value, replace: bool) -> Result<Placed> {
    let msg = rec["message"].as_object().ok_or("sauvegarde illisible : ligne absente")?;
    let col = |n: &str| msg.get(n).cloned().unwrap_or(Value::Null);
    let folder_id = col("folder_id").as_i64().ok_or("sauvegarde illisible : dossier absent")?;
    let source_id: i64 = conn
        .query_row("SELECT source_id FROM folder WHERE id = ?1", [folder_id], |r| r.get(0))
        .optional()
        .map_err(e)?
        .ok_or("le dossier d'origine n'existe plus : élément non remis")?;
    let rec_id = rec["id"].as_i64().or_else(|| col("id").as_i64()).ok_or("sauvegarde illisible : identifiant absent")?;
    let nid = decode(&col("nid"));
    let created = decode(&col("created_at"));

    let present: Option<i64> = conn
        .query_row("SELECT id FROM message WHERE folder_id = ?1 AND nid = ?2 AND created_at IS ?3", params![folder_id, nid, created], |r| r.get(0))
        .optional()
        .map_err(e)?;
    if let Some(id) = present {
        if !replace {
            return Ok(Placed::Skipped(id));
        }
    }
    let target = match present {
        Some(id) => id,
        None => {
            let taken = conn.query_row("SELECT 1 FROM message WHERE id = ?1", [rec_id], |_| Ok(())).optional().map_err(e)?.is_some();
            if taken { 0 } else { rec_id }
        }
    };
    if present.is_some() {
        save_to_trash(conn, data_dir, target, "avant remplacement")?;
    }

    let have = table_columns(conn, "message")?;
    let att_have = table_columns(conn, "attachment")?;
    let tx = conn.transaction().map_err(e)?;
    if present.is_some() {
        tx.execute("DELETE FROM message_fts WHERE rowid = ?1", [target]).map_err(e)?;
        tx.execute("DELETE FROM message WHERE id = ?1", [target]).map_err(e)?;
    }
    let mut names = Vec::new();
    let mut values: Vec<rusqlite::types::Value> = Vec::new();
    for (k, v) in msg {
        if k == "id" || !have.contains(k) {
            continue;
        }
        names.push(k.clone());
        values.push(match k.as_str() {
            "indexed" => rusqlite::types::Value::Integer(0),
            _ => decode(v),
        });
    }
    if target != 0 {
        names.push("id".into());
        values.push(rusqlite::types::Value::Integer(target));
    }
    let sql = format!("INSERT INTO message ({}) VALUES ({})", names.join(", "), vec!["?"; names.len()].join(", "));
    tx.execute(&sql, rusqlite::params_from_iter(values)).map_err(e)?;
    let id = tx.last_insert_rowid();

    for a in rec["attachments"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let Some(o) = a.as_object() else { continue };
        let mut names = vec!["message_id".to_string()];
        let mut values = vec![rusqlite::types::Value::Integer(id)];
        // Identifiant de pièce d'origine repris s'il est libre.
        let free = o.get("id").and_then(Value::as_i64).filter(|a| tx.query_row("SELECT 1 FROM attachment WHERE id = ?1", [a], |_| Ok(())).optional().ok().flatten().is_none());
        if let Some(a) = free {
            names.push("id".into());
            values.push(rusqlite::types::Value::Integer(a));
        }
        for (k, v) in o {
            if k == "id" || k == "message_id" || !att_have.contains(k) {
                continue;
            }
            names.push(k.clone());
            values.push(decode(v));
        }
        let sql = format!("INSERT INTO attachment ({}) VALUES ({})", names.join(", "), vec!["?"; names.len()].join(", "));
        tx.execute(&sql, rusqlite::params_from_iter(values)).map_err(e)?;
    }
    for c in rec["categories"].as_array().map(Vec::as_slice).unwrap_or_default().iter().filter_map(Value::as_str) {
        tx.execute("INSERT OR IGNORE INTO message_category (message_id, name) VALUES (?1, ?2)", params![id, c]).map_err(e)?;
    }
    // Le `.eml` suit l'identifiant : chemin recalculé si l'identifiant a changé.
    let old_path = col("eml_path").as_str().unwrap_or_default().to_string();
    let eml_path = if old_path.is_empty() || id == rec_id { old_path } else { thunderlook_store::eml_relative_path(source_id, id) };
    if eml_path != msg.get("eml_path").and_then(Value::as_str).unwrap_or_default() {
        tx.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, eml_path]).map_err(e)?;
    }
    tx.commit().map_err(e)?;

    if let (Some(b64), false) = (rec["eml"].as_str(), eml_path.is_empty()) {
        let path = thunderlook_store::resolve(data_dir, &eml_path);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|x| io(dir, x))?;
        }
        fs::write(&path, B64.decode(b64).map_err(|x| x.to_string())?).map_err(|x| io(&path, x))?;
    }
    Ok(if present.is_some() { Placed::Replaced(id) } else { Placed::Added(id) })
}

/// Remet un élément de la corbeille (écrase sa version actuelle si elle existe, après l'avoir elle-même mise en corbeille).
/// Rend l'identifiant du message remis.
pub fn restore(conn: &mut Connection, data_dir: &Path, file: &Path) -> Result<i64> {
    let text = fs::read_to_string(file).map_err(|x| io(file, x))?;
    let rec: Value = serde_json::from_str(&text).map_err(|x| format!("{} : {x}", file.display()))?;
    match place(conn, data_dir, &rec, true)? {
        Placed::Added(i) | Placed::Replaced(i) | Placed::Skipped(i) => Ok(i),
    }
}

// --- Purge de la corbeille -------------------------------------------------------------------------------------------

/// Supprime de la corbeille les instantanés de plus de `days` jours ; rend leur nombre.
pub fn purge(data_dir: &Path, days: i64) -> usize {
    purge_at(data_dir, days, Utc::now().naive_utc())
}

pub fn purge_at(data_dir: &Path, days: i64, now: NaiveDateTime) -> usize {
    let Ok(rd) = fs::read_dir(trash_dir(data_dir)) else { return 0 };
    let mut n = 0;
    for f in rd.flatten() {
        let name = f.file_name().to_string_lossy().into_owned();
        let Some(when) = name.get(..15).and_then(|s| NaiveDateTime::parse_from_str(s, "%Y%m%d-%H%M%S").ok()) else { continue };
        if (now - when).num_days() >= days && fs::remove_file(f.path()).is_ok() {
            n += 1;
        }
    }
    n
}

// --- Sauvegardes -----------------------------------------------------------------------------------------------------

pub fn backups_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("backups")
}

/// Sauvegarde du jour (une fois par jour). Rend le fichier écrit, `None` si celle d'aujourd'hui existe déjà.
pub fn backup(conn: &Connection, data_dir: &Path, keep: usize) -> Result<Option<PathBuf>> {
    backup_on(conn, data_dir, keep, Local::now().date_naive())
}

pub fn backup_on(conn: &Connection, data_dir: &Path, keep: usize, today: NaiveDate) -> Result<Option<PathBuf>> {
    let dir = backups_dir(data_dir);
    let daily = dir.join(format!("{}.json", today.format("%Y-%m-%d")));
    if daily.exists() {
        return Ok(None);
    }
    fs::create_dir_all(&dir).map_err(|x| io(&dir, x))?;
    let ids: Vec<i64> = {
        let clause = CLASSES.iter().map(|_| "message_class LIKE ?").collect::<Vec<_>>().join(" OR ");
        let mut stmt = conn.prepare(&format!("SELECT id FROM message WHERE {clause} ORDER BY id")).map_err(e)?;
        stmt.query_map(rusqlite::params_from_iter(CLASSES), |r| r.get(0)).map_err(e)?.collect::<rusqlite::Result<_>>().map_err(e)?
    };
    let tmp = daily.with_extension("tmp");
    {
        let mut w = BufWriter::new(fs::File::create(&tmp).map_err(|x| io(&tmp, x))?);
        let werr = |x: std::io::Error| io(&tmp, x);
        writeln!(w, "{{\"version\":{FORMAT_VERSION},\"created\":\"{}\",\"count\":{},\"items\":[", Local::now().format("%Y-%m-%dT%H:%M:%S"), ids.len()).map_err(werr)?;
        for (i, id) in ids.iter().enumerate() {
            let rec = record(conn, data_dir, *id, "sauvegarde")?;
            w.write_all(&serde_json::to_vec(&rec).map_err(|x| x.to_string())?).map_err(werr)?;
            w.write_all(if i + 1 < ids.len() { b",\n" } else { b"\n" }).map_err(werr)?;
        }
        w.write_all(b"]}\n").map_err(werr)?;
        w.flush().map_err(werr)?;
    }
    fs::rename(&tmp, &daily).map_err(|x| io(&daily, x))?;

    // Une copie par semaine (la première sauvegarde de la semaine ISO).
    let iso = today.iso_week();
    let weekly = dir.join(format!("hebdo-{}-S{:02}.json", iso.year(), iso.week()));
    if !weekly.exists() {
        fs::copy(&daily, &weekly).map_err(|x| io(&weekly, x))?;
    }
    prune(&dir, keep, KEEP_WEEKLY);
    Ok(Some(daily))
}

/// Ne garde que les `daily` dernières sauvegardes quotidiennes et les `weekly` dernières copies hebdomadaires.
fn prune(dir: &Path, daily: usize, weekly: usize) {
    let (mut d, mut w) = (Vec::new(), Vec::new());
    if let Ok(rd) = fs::read_dir(dir) {
        for f in rd.flatten() {
            let name = f.file_name().to_string_lossy().into_owned();
            if name.starts_with("hebdo-") && name.ends_with(".json") {
                w.push(f.path());
            } else if name.len() == 15 && name.ends_with(".json") && NaiveDate::parse_from_str(&name[..10], "%Y-%m-%d").is_ok() {
                d.push(f.path());
            }
        }
    }
    for (mut list, keep) in [(d, daily), (w, weekly)] {
        list.sort();
        if list.len() > keep {
            for p in &list[..list.len() - keep] {
                let _ = fs::remove_file(p);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupInfo {
    pub name: String,
    pub path: PathBuf,
    /// `true` : copie hebdomadaire.
    pub weekly: bool,
    pub size: u64,
    pub count: usize,
    /// Date et heure de création, telles qu'écrites dans le fichier.
    pub created: String,
}

/// Sauvegardes présentes, les plus récentes d'abord.
pub fn list_backups(data_dir: &Path) -> Vec<BackupInfo> {
    let Ok(rd) = fs::read_dir(backups_dir(data_dir)) else { return Vec::new() };
    let mut out: Vec<BackupInfo> = rd
        .flatten()
        .filter_map(|f| {
            let name = f.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".json") {
                return None;
            }
            let (count, created) = header(&f.path()).unwrap_or((0, String::new()));
            Some(BackupInfo { weekly: name.starts_with("hebdo-"), size: f.metadata().map(|m| m.len()).unwrap_or(0), path: f.path(), name, count, created })
        })
        .collect();
    out.sort_by(|a, b| b.created.cmp(&a.created).then(b.name.cmp(&a.name)));
    out
}

/// Première ligne : `{"version":1,"created":"…","count":N,"items":[`.
fn header(file: &Path) -> Result<(usize, String)> {
    let mut line = String::new();
    BufReader::new(fs::File::open(file).map_err(|x| io(file, x))?).read_line(&mut line).map_err(|x| io(file, x))?;
    let v: Value = serde_json::from_str(&format!("{}}}", line.trim().trim_end_matches(",\"items\":["))).map_err(|x| x.to_string())?;
    Ok((v["count"].as_u64().unwrap_or(0) as usize, v["created"].as_str().unwrap_or_default().to_string()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// « Fusionner les éléments manquants » : ne remet que ce qui n'existe plus.
    Merge,
    /// « Tout remettre » : chaque élément reprend son contenu de la sauvegarde (l'actuel est gardé en corbeille).
    All,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Restored {
    pub added: usize,
    pub replaced: usize,
    pub skipped: usize,
    /// Éléments non remis (dossier disparu…), avec la raison.
    pub failed: Vec<String>,
}

/// Relit une sauvegarde et remet ses éléments. Rien n'est supprimé : les éléments absents de la sauvegarde restent.
pub fn restore_backup(conn: &mut Connection, data_dir: &Path, file: &Path, mode: Mode) -> Result<Restored> {
    let rd = BufReader::new(fs::File::open(file).map_err(|x| io(file, x))?);
    let mut out = Restored::default();
    let mut lines = rd.lines();
    let first = lines.next().ok_or("sauvegarde vide")?.map_err(|x| io(file, x))?;
    if !first.contains("\"items\":[") {
        return Err(format!("{} n'est pas une sauvegarde ThunderLook", file.display()));
    }
    let mut closed = false;
    for line in lines {
        let line = line.map_err(|x| io(file, x))?;
        let t = line.trim().trim_end_matches(',');
        if t == "]}" {
            closed = true;
            break;
        }
        if t.is_empty() {
            continue;
        }
        let rec: Value = serde_json::from_str(t).map_err(|x| format!("{} : {x}", file.display()))?;
        match place(conn, data_dir, &rec, mode == Mode::All) {
            Ok(Placed::Added(_)) => out.added += 1,
            Ok(Placed::Replaced(_)) => out.replaced += 1,
            Ok(Placed::Skipped(_)) => out.skipped += 1,
            Err(err) => out.failed.push(format!("{} : {err}", describe(&rec))),
        }
    }
    if !closed {
        out.failed.push("fichier tronqué : la fin de la sauvegarde manque".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Base fabriquée : une source, un dossier, des éléments de classes variées avec pièce, catégorie et `.eml`.
    fn fixture(name: &str) -> (PathBuf, Connection) {
        let dir = std::env::temp_dir().join(format!("tl-safety-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let conn = thunderlook_store::open(&dir).unwrap();
        conn.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (1, 'pst', 'a.pst', 'a.pst', 'A', 0, 0)", []).unwrap();
        conn.execute("INSERT INTO folder (id, source_id, nid, name, role, hidden, position) VALUES (1, 1, 1, 'Contacts', 'folder', 0, 0)", []).unwrap();
        (dir, conn)
    }

    /// Dates de création distinctes, comme les millisecondes réelles.
    static CLOCK: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

    fn add(conn: &Connection, dir: &Path, class: &str, subject: &str) -> i64 {
        let nid: i64 = conn.query_row("SELECT COALESCE(MAX(nid), 0) + 1 FROM message WHERE folder_id = 1", [], |r| r.get(0)).unwrap();
        conn.execute(
            "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, created_at, message_flags, is_read, flag_status,
                importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
             VALUES (1, ?1, ?2, ?3, '', '', '', '', ?4, 0, 1, 0, 1, 0, 0, 1, 'text', 'aperçu', '', ?5)",
            params![nid, class, subject, 1_700_000_000_000 + CLOCK.fetch_add(1, std::sync::atomic::Ordering::Relaxed), vec![1u8, 2, 3, 255, 0]],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        let rel = thunderlook_store::eml_relative_path(1, id);
        let path = thunderlook_store::resolve(dir, &rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, format!("Subject: {subject}\r\n\r\ncorps é\r\n")).unwrap();
        conn.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, rel]).unwrap();
        conn.execute("INSERT INTO attachment (message_id, position, filename, mime_type, size, method, inline, hidden, props) VALUES (?1, 0, 'a.txt', 'text/plain', 3, 1, 0, 0, x'00ff')", [id]).unwrap();
        conn.execute("INSERT INTO message_category (message_id, name) VALUES (?1, 'VIP')", [id]).unwrap();
        id
    }

    /// Tout le contenu d'un élément, sans l'identifiant : de quoi comparer avant/après.
    fn dump(conn: &Connection, dir: &Path, id: i64) -> Value {
        let mut r = record(conn, dir, id, "").unwrap();
        r["saved_at"] = Value::Null;
        r
    }

    #[test]
    fn delete_then_undo_restores_everything() {
        let (dir, mut conn) = fixture("undo");
        let id = add(&conn, &dir, "IPM.Contact", "Dupont");
        let before = dump(&conn, &dir, id);
        let eml = thunderlook_store::resolve(&dir, before["message"]["eml_path"].as_str().unwrap());

        snapshot(&conn, &dir, id).unwrap();
        conn.execute("DELETE FROM message WHERE id = ?1", [id]).unwrap();
        fs::remove_file(&eml).unwrap();
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM attachment", [], |r| r.get::<_, i64>(0)).unwrap(), 0);

        let msg = undo(&mut conn, &dir).unwrap();
        assert_eq!(msg, "Annulé : suppression du contact « Dupont »");
        assert_eq!(dump(&conn, &dir, id), before);
        assert!(eml.exists());
        assert!(undo(&mut conn, &dir).is_err(), "pile vide");
    }

    #[test]
    fn edit_then_undo_restores_old_content_and_keeps_the_new_in_trash() {
        let (dir, mut conn) = fixture("edit");
        let id = add(&conn, &dir, "IPM.StickyNote", "avant");
        let before = dump(&conn, &dir, id);
        snapshot_edit(&conn, &dir, id).unwrap();
        conn.execute("UPDATE message SET subject = 'après', props = x'09' WHERE id = ?1", [id]).unwrap();
        conn.execute("DELETE FROM attachment", []).unwrap();
        assert_eq!(undo(&mut conn, &dir).unwrap(), "Annulé : modification du note « avant »");
        assert_eq!(dump(&conn, &dir, id), before);
        let trash: Vec<_> = fs::read_dir(trash_dir(&dir)).unwrap().flatten().collect();
        assert_eq!(trash.len(), 2, "l'ancienne version et la version écrasée");
    }

    #[test]
    fn id_taken_by_another_item_gets_a_new_id() {
        let (dir, mut conn) = fixture("reuse");
        let id = add(&conn, &dir, "IPM.Task", "ancienne");
        let file = snapshot(&conn, &dir, id).unwrap();
        conn.execute("DELETE FROM message WHERE id = ?1", [id]).unwrap();
        let other = add(&conn, &dir, "IPM.Task", "nouvelle");
        assert_eq!(other, id, "SQLite réutilise l'identifiant");
        let back = restore(&mut conn, &dir, &file).unwrap();
        assert_ne!(back, id);
        let subjects: Vec<String> = conn.prepare("SELECT subject FROM message ORDER BY subject").unwrap().query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(subjects, ["ancienne", "nouvelle"]);
        let eml = conn.query_row("SELECT eml_path FROM message WHERE id = ?1", [back], |r| r.get::<_, String>(0)).unwrap();
        assert!(eml.ends_with(&format!("{back}.eml")) && thunderlook_store::resolve(&dir, &eml).exists());
    }

    #[test]
    fn undo_stack_is_capped_at_50() {
        let (dir, conn) = fixture("cap");
        let id = add(&conn, &dir, "IPM.Contact", "X");
        for _ in 0..60 {
            snapshot_edit(&conn, &dir, id).unwrap();
        }
        assert_eq!(undo_depth(&dir), UNDO_LEVELS);
    }

    #[test]
    fn purge_removes_only_old_snapshots() {
        let (dir, conn) = fixture("purge");
        let id = add(&conn, &dir, "IPM.Contact", "X");
        let new = snapshot(&conn, &dir, id).unwrap();
        let old = trash_dir(&dir).join("20250101-120000-contact-9.json");
        let edge = trash_dir(&dir).join("20250301-000000-note-3.json");
        fs::write(&old, "{}").unwrap();
        fs::write(&edge, "{}").unwrap();
        let now = NaiveDate::from_ymd_opt(2025, 4, 30).unwrap().and_hms_opt(0, 0, 0).unwrap();
        assert_eq!(purge_at(&dir, 60, now), 2, "le 1er janvier (119 j) et le 1er mars (60 j) partent");
        assert!(!old.exists() && !edge.exists());
        let young = trash_dir(&dir).join("20250301-000001-note-4.json");
        fs::write(&young, "{}").unwrap();
        assert_eq!(purge_at(&dir, 60, now), 0, "59 j 23 h 59 : gardé");
        assert!(young.exists());
        assert!(new.exists());
    }

    #[test]
    fn backup_once_a_day_keeps_14_and_8_weeks() {
        let (dir, conn) = fixture("backup");
        add(&conn, &dir, "IPM.Contact", "Dupont");
        add(&conn, &dir, "IPM.Appointment", "RDV");
        add(&conn, &dir, "IPM.Note", "un courrier, pas une note"); // IPM.Note = courrier : exclu
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let first = backup_on(&conn, &dir, KEEP_DAILY, start).unwrap().unwrap();
        assert!(backup_on(&conn, &dir, KEEP_DAILY, start).unwrap().is_none(), "une fois par jour");
        let list = list_backups(&dir);
        let info = list.iter().find(|b| !b.weekly).unwrap();
        assert_eq!((info.count, info.weekly), (2, false));
        assert!(first.ends_with("2026-01-01.json"));
        for d in 1..100 {
            backup_on(&conn, &dir, KEEP_DAILY, start + chrono::Duration::days(d)).unwrap();
        }
        let list = list_backups(&dir);
        assert_eq!(list.iter().filter(|b| !b.weekly).count(), KEEP_DAILY);
        assert_eq!(list.iter().filter(|b| b.weekly).count(), KEEP_WEEKLY);
        assert!(list.iter().filter(|b| !b.weekly).any(|b| b.name == "2026-04-10.json"), "la plus récente est gardée");
        assert!(!dir.join("backups").join("2026-01-01.json").exists());
    }

    #[test]
    fn merge_restore_adds_missing_without_duplicating() {
        let (dir, mut conn) = fixture("merge");
        let a = add(&conn, &dir, "IPM.Contact", "A");
        let b = add(&conn, &dir, "IPM.Task", "B");
        let c = add(&conn, &dir, "IPM.StickyNote", "C");
        let file = backup_on(&conn, &dir, KEEP_DAILY, NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()).unwrap().unwrap();
        let before = dump(&conn, &dir, a);

        // Rien de manquant : rien ne change.
        let r = restore_backup(&mut conn, &dir, &file, Mode::Merge).unwrap();
        assert_eq!((r.added, r.replaced, r.skipped), (0, 0, 3));

        // B perdu, A modifié depuis : la fusion ne remet que B et ne touche pas à A.
        conn.execute("DELETE FROM message WHERE id = ?1", [b]).unwrap();
        conn.execute("UPDATE message SET subject = 'A modifié' WHERE id = ?1", [a]).unwrap();
        let r = restore_backup(&mut conn, &dir, &file, Mode::Merge).unwrap();
        assert_eq!((r.added, r.replaced, r.skipped), (1, 0, 2));
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM message", [], |r| r.get::<_, i64>(0)).unwrap(), 3);
        assert_eq!(conn.query_row("SELECT subject FROM message WHERE id = ?1", [a], |r| r.get::<_, String>(0)).unwrap(), "A modifié");
        assert_eq!(conn.query_row("SELECT subject FROM message WHERE id = ?1", [b], |r| r.get::<_, String>(0)).unwrap(), "B");
        let again = restore_backup(&mut conn, &dir, &file, Mode::Merge).unwrap();
        assert_eq!((again.added, again.skipped), (0, 3), "relancer ne duplique rien");

        // Tout remettre : A retrouve son contenu, l'état modifié est gardé en corbeille ; C n'est pas dupliqué.
        let r = restore_backup(&mut conn, &dir, &file, Mode::All).unwrap();
        assert_eq!((r.added, r.replaced), (0, 3));
        assert_eq!(dump(&conn, &dir, a), before);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM message", [], |r| r.get::<_, i64>(0)).unwrap(), 3);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM attachment", [], |r| r.get::<_, i64>(0)).unwrap(), 3);
        assert!(fs::read_dir(trash_dir(&dir)).unwrap().flatten().any(|f| fs::read_to_string(f.path()).unwrap().contains("A modifié")));
        let _ = c;
    }

    #[test]
    fn missing_folder_is_reported_not_fatal() {
        let (dir, mut conn) = fixture("nofolder");
        add(&conn, &dir, "IPM.Contact", "A");
        let file = backup_on(&conn, &dir, KEEP_DAILY, NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()).unwrap().unwrap();
        conn.execute("DELETE FROM folder", []).unwrap();
        let r = restore_backup(&mut conn, &dir, &file, Mode::Merge).unwrap();
        assert_eq!((r.added, r.failed.len()), (0, 1));
    }

    #[test]
    fn truncated_backup_is_flagged() {
        let (dir, mut conn) = fixture("trunc");
        add(&conn, &dir, "IPM.Contact", "A");
        let file = backup_on(&conn, &dir, KEEP_DAILY, NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()).unwrap().unwrap();
        let text = fs::read_to_string(&file).unwrap();
        fs::write(&file, text.trim_end().trim_end_matches("]}")).unwrap();
        let r = restore_backup(&mut conn, &dir, &file, Mode::Merge).unwrap();
        assert!(r.failed.iter().any(|f| f.contains("tronqué")));
    }
}
