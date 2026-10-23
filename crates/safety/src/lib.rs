//! Filet de sécurité des données modifiables (rendez-vous, contacts, tâches, notes) : ThunderLook est leur seul dépositaire.
//!
//! - **Corbeille interne** : [`snapshot`] enregistre le contenu complet d'un élément (ligne `message`, pièces, catégories,
//!   `.eml`) dans `data_dir/trash/<horodatage>-<type>-<id>.json` ; [`restore`] le remet. [`purge`] vide au bout de 60 jours.
//! - **Annulation** : chaque instantané alimente une pile de 50 niveaux ; [`undo`] défait le dernier.
//! - **Sauvegardes** : [`backup`] écrit chaque jour, en JSON Lines et en lecture seule, les seuls éléments nouveaux, modifiés ou
//!   supprimés depuis la veille (14 gardés), et un « plein » le dimanche (8 gardés), jamais la base entière ni la corbeille ;
//!   [`restore_backup`] reconstitue l'état à une date (plein + incréments ; fusion ou remise complète).
//!
//! Aucune fonction d'ici ne supprime ni ne réécrit une ligne de la base sans en avoir enregistré l'ancien contenu avant.

mod dbcopy;
pub use dbcopy::{DbCopy, KEEP_DB, backup_db, backup_db_on, db_dir, last_failure, list_db_copies, record_failure, restore_db, verify_db, wait_idle};

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

pub(crate) fn e(x: rusqlite::Error) -> String {
    x.to_string()
}

pub(crate) fn io(path: &Path, x: std::io::Error) -> String {
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
//
// Format JSON Lines, un élément par ligne, écrit en flux :
//   ligne 1 : {"version":2,"type":"full"|"incr","date":"AAAA-MM-JJ","created":"…"}
//   puis    : {"op":"put", …enregistrement complet…}  ou  {"op":"del","id":N,"key":"dossier:nid:création"}
//   fin     : {"end":true,"count":N}   (absente = fichier tronqué)
// `plein-AAAA-MM-JJ.jsonl` contient tout (le dimanche, ou si le dernier plein a plus de 7 jours) ; `AAAA-MM-JJ.jsonl` seulement
// ce qui est nouveau, modifié ou supprimé depuis la veille. `index.json` retient, par élément, `modified_at` et une empreinte.
// Les anciens fichiers `.json` (version 1) restent lisibles.

/// Éléments lus entre deux pauses de la sauvegarde.
const PAUSE_EVERY: usize = 500;
const PAUSE: std::time::Duration = std::time::Duration::from_millis(20);
/// Éléments par transaction de lecture.
const BATCH: usize = 200;
const INDEX_FILE: &str = "index.json";
/// Dossiers jamais sauvegardés (corbeille, envoyés), en plus des rôles `deleted` et `sent`.
const SKIPPED_FOLDERS: [&str; 6] = ["éléments supprimés", "deleted items", "sent", "sent items", "éléments envoyés", "elements supprimes"];

pub fn backups_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("backups")
}

/// Empreinte FNV-1a 64 bits (stable d'une version de Rust à l'autre, contrairement à `DefaultHasher`).
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn feed(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
    fn value(&mut self, v: ValueRef<'_>) {
        match v {
            ValueRef::Null => self.feed(&[0]),
            ValueRef::Integer(i) => {
                self.feed(&[1]);
                self.feed(&i.to_le_bytes());
            }
            ValueRef::Real(f) => {
                self.feed(&[2]);
                self.feed(&f.to_le_bytes());
            }
            ValueRef::Text(t) => {
                self.feed(&[3]);
                self.feed(&(t.len() as u64).to_le_bytes());
                self.feed(t);
            }
            ValueRef::Blob(b) => {
                self.feed(&[4]);
                self.feed(&(b.len() as u64).to_le_bytes());
                self.feed(b);
            }
        }
    }
}

/// Identité d'un élément d'une sauvegarde à l'autre (celle de [`place`]) : dossier, `nid`, date de création.
fn identity(folder_id: &Value, nid: &Value, created: &Value) -> String {
    let s = |v: &Value| if v.is_null() { String::new() } else { v.to_string() };
    format!("{}:{}:{}", s(folder_id), s(nid), s(created))
}

/// (identité, `modified_at`, empreinte) d'un élément, sans lire son `.eml` en entier (taille et date du fichier suffisent).
fn fingerprint(conn: &Connection, data_dir: &Path, id: i64) -> Result<Option<(String, Option<i64>, u64)>> {
    let mut h = Fnv::new();
    let mut stmt = conn.prepare_cached("SELECT * FROM message WHERE id = ?1").map_err(e)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut rows = stmt.query([id]).map_err(e)?;
    let Some(row) = rows.next().map_err(e)? else { return Ok(None) };
    let (mut key, mut modified, mut eml) = ((Value::Null, Value::Null, Value::Null), None, String::new());
    for (i, n) in names.iter().enumerate() {
        let v = row.get_ref(i).map_err(e)?;
        match n.as_str() {
            "folder_id" => key.0 = encode(v),
            "nid" => key.1 = encode(v),
            "created_at" => key.2 = encode(v),
            "modified_at" => modified = v.as_i64().ok(),
            "eml_path" => eml = v.as_str().map(str::to_string).unwrap_or_default(),
            _ => {}
        }
        // `indexed` bouge à chaque passage de l'indexeur de recherche : ce n'est pas une modification de l'utilisateur.
        if n != "indexed" {
            h.feed(n.as_bytes());
            h.value(v);
        }
    }
    drop(rows);
    if !eml.is_empty() {
        if let Ok(m) = fs::metadata(thunderlook_store::resolve(data_dir, &eml)) {
            h.feed(&m.len().to_le_bytes());
            let t = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis());
            h.feed(&t.unwrap_or(0).to_le_bytes());
        }
    }
    let mut stmt = conn.prepare_cached("SELECT * FROM attachment WHERE message_id = ?1 ORDER BY position, id").map_err(e)?;
    let n = stmt.column_count();
    let mut rows = stmt.query([id]).map_err(e)?;
    while let Some(r) = rows.next().map_err(e)? {
        for i in 0..n {
            h.value(r.get_ref(i).map_err(e)?);
        }
    }
    let mut stmt = conn.prepare_cached("SELECT name FROM message_category WHERE message_id = ?1 ORDER BY name").map_err(e)?;
    let mut rows = stmt.query([id]).map_err(e)?;
    while let Some(r) = rows.next().map_err(e)? {
        h.value(r.get_ref(0).map_err(e)?);
    }
    Ok(Some((identity(&key.0, &key.1, &key.2), modified, h.0)))
}

/// Identifiants des éléments à sauvegarder : classes protégées, hors corbeille, envoyés et rapports.
fn candidates(conn: &Connection) -> Result<Vec<i64>> {
    let clause = CLASSES.iter().map(|_| "m.message_class LIKE ?").collect::<Vec<_>>().join(" OR ");
    let mut stmt = conn
        .prepare(&format!("SELECT m.id, f.role, f.name FROM message m JOIN folder f ON f.id = m.folder_id WHERE ({clause}) AND m.message_class NOT LIKE 'REPORT%' ORDER BY m.id"))
        .map_err(e)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(CLASSES), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
        .map_err(e)?;
    let mut ids = Vec::new();
    for row in rows {
        let (id, role, name) = row.map_err(e)?;
        if role != "deleted" && role != "sent" && !SKIPPED_FOLDERS.contains(&name.trim().to_lowercase().as_str()) {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// Un autre fil vient d'écrire dans la base (`data_version` change quand une AUTRE connexion valide) : on lui laisse la place.
fn wait_quiet(conn: &Connection, last: &mut i64) {
    let version = |c: &Connection| c.query_row("PRAGMA data_version", [], |r| r.get::<_, i64>(0)).unwrap_or(0);
    for _ in 0..20 {
        let v = version(conn);
        if v == *last {
            return;
        }
        *last = v;
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

#[derive(Clone, Debug)]
struct IndexEntry {
    modified: Option<i64>,
    hash: u64,
    key: String,
}

fn read_index(dir: &Path) -> Option<(String, std::collections::HashMap<i64, IndexEntry>)> {
    let v: Value = serde_json::from_slice(&fs::read(dir.join(INDEX_FILE)).ok()?).ok()?;
    let full = v["full"].as_str()?.to_string();
    let items = v["items"].as_object()?;
    let map = items
        .iter()
        .filter_map(|(k, o)| {
            Some((k.parse().ok()?, IndexEntry { modified: o["m"].as_i64(), hash: u64::from_str_radix(o["h"].as_str()?, 16).ok()?, key: o["k"].as_str()?.to_string() }))
        })
        .collect();
    Some((full, map))
}

fn write_index(dir: &Path, full: &str, items: &std::collections::HashMap<i64, IndexEntry>) -> Result<()> {
    let items: Map<String, Value> = items.iter().map(|(id, x)| (id.to_string(), json!({ "m": x.modified, "h": format!("{:016x}", x.hash), "k": x.key }))).collect();
    write_atomic(&dir.join(INDEX_FILE), &serde_json::to_vec(&json!({ "version": 1, "full": full, "items": items })).map_err(|x| x.to_string())?)
}

/// (date, plein ?) d'après le nom d'un fichier `.jsonl` de sauvegarde.
fn parse_name(name: &str) -> Option<(NaiveDate, bool)> {
    let stem = name.strip_suffix(".jsonl")?;
    let (full, date) = match stem.strip_prefix("plein-") {
        Some(d) => (true, d),
        None => (false, stem),
    };
    Some((NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?, full))
}

/// Sauvegarde du jour (une fois par jour), en lecture seule et par paquets. Rend le fichier écrit, `None` si celle d'aujourd'hui existe déjà.
///
/// Un échec est consigné (`last_failure`, affiché dans l'écran « Sauvegardes ») jusqu'à la prochaine réussite.
pub fn backup(data_dir: &Path, keep: usize) -> Result<Option<PathBuf>> {
    let r = backup_on(data_dir, keep, Local::now().date_naive());
    match &r {
        Ok(_) => dbcopy::set_failure(data_dir, None),
        Err(x) => dbcopy::set_failure(data_dir, Some(x)),
    }
    r
}

/// Comme [`backup`], dans un fil à part (le lancement n'attend pas).
pub fn spawn_backup(data_dir: PathBuf, keep: usize) -> std::thread::JoinHandle<Result<Option<PathBuf>>> {
    std::thread::spawn(move || backup(&data_dir, keep))
}

pub fn backup_on(data_dir: &Path, keep: usize, today: NaiveDate) -> Result<Option<PathBuf>> {
    let dir = backups_dir(data_dir);
    let day = today.format("%Y-%m-%d").to_string();
    let incr = dir.join(format!("{day}.jsonl"));
    let full = dir.join(format!("plein-{day}.jsonl"));
    if incr.exists() || full.exists() || dir.join(format!("{day}.json")).exists() {
        return Ok(None);
    }
    fs::create_dir_all(&dir).map_err(|x| io(&dir, x))?;
    let conn = thunderlook_store::open_readonly(data_dir).map_err(e)?;

    // Plein : le dimanche, ou si le dernier plein a plus de 7 jours, ou sans index exploitable.
    let last_full = fs::read_dir(&dir)
        .map(|rd| rd.flatten().filter_map(|f| parse_name(&f.file_name().to_string_lossy())).filter(|(_, is_full)| *is_full).map(|(d, _)| d).max())
        .unwrap_or(None);
    let index = read_index(&dir);
    let is_full = today.weekday() == chrono::Weekday::Sun || index.is_none() || last_full.is_none_or(|d| (today - d).num_days() >= 7);
    let previous = if is_full { Default::default() } else { index.map(|x| x.1).unwrap_or_default() };
    let path = if is_full { full } else { incr };

    let tmp = path.with_extension("tmp");
    let ids = candidates(&conn)?;
    let mut next = std::collections::HashMap::with_capacity(ids.len());
    let mut written = 0usize;
    {
        let mut w = BufWriter::new(fs::File::create(&tmp).map_err(|x| io(&tmp, x))?);
        let werr = |x: std::io::Error| io(&tmp, x);
        let head = json!({ "version": 2, "type": if is_full { "full" } else { "incr" }, "date": day, "created": Local::now().format("%Y-%m-%dT%H:%M:%S").to_string() });
        writeln!(w, "{head}").map_err(werr)?;
        let line = |w: &mut BufWriter<fs::File>, v: &Value| -> Result<()> {
            serde_json::to_writer(&mut *w, v).map_err(|x| x.to_string())?;
            w.write_all(b"\n").map_err(werr)
        };
        let mut data_version = 0;
        let mut seen = 0usize;
        for batch in ids.chunks(BATCH) {
            wait_quiet(&conn, &mut data_version);
            conn.execute_batch("BEGIN").map_err(e)?;
            let r = (|| -> Result<()> {
                for &id in batch {
                    seen += 1;
                    if seen % PAUSE_EVERY == 0 {
                        std::thread::sleep(PAUSE);
                    }
                    let Some((key, modified, hash)) = fingerprint(&conn, data_dir, id)? else { continue };
                    let entry = IndexEntry { modified, hash, key };
                    let unchanged = previous.get(&id).is_some_and(|p| p.hash == entry.hash && p.key == entry.key);
                    if !unchanged {
                        if let Some(old) = previous.get(&id).filter(|p| p.key != entry.key) {
                            line(&mut w, &json!({ "op": "del", "id": id, "key": old.key }))?;
                        }
                        let mut rec = record(&conn, data_dir, id, "sauvegarde")?;
                        rec["op"] = json!("put");
                        line(&mut w, &rec)?;
                        written += 1;
                    }
                    next.insert(id, entry);
                }
                Ok(())
            })();
            conn.execute_batch("COMMIT").map_err(e)?;
            r?;
        }
        for (id, old) in &previous {
            if !next.contains_key(id) {
                line(&mut w, &json!({ "op": "del", "id": id, "key": old.key }))?;
                written += 1;
            }
        }
        line(&mut w, &json!({ "end": true, "count": written }))?;
        w.flush().map_err(werr)?;
    }
    fs::rename(&tmp, &path).map_err(|x| io(&path, x))?;
    // L'index n'avance qu'une fois le fichier en place : un échec avant fait simplement repartir de l'ancien état.
    let full_date = if is_full { day.clone() } else { last_full.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_else(|| day.clone()) };
    write_index(&dir, &full_date, &next)?;
    prune(&dir, keep, KEEP_WEEKLY);
    Ok(Some(path))
}

/// Ne garde que les `daily` derniers incréments et les `weekly` derniers pleins, plus tout incrément dont dépend le plus ancien
/// jour gardé (pas de trou dans la chaîne plein + incréments). Les anciens `.json` suivent la règle d'avant.
fn prune(dir: &Path, daily: usize, weekly: usize) {
    let (mut incs, mut fulls, mut legacy_d, mut legacy_w) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    if let Ok(rd) = fs::read_dir(dir) {
        for f in rd.flatten() {
            let name = f.file_name().to_string_lossy().into_owned();
            if let Some((date, is_full)) = parse_name(&name) {
                if is_full { &mut fulls } else { &mut incs }.push((date, f.path()));
            } else if name.starts_with("hebdo-") && name.ends_with(".json") {
                legacy_w.push(f.path());
            } else if name.len() == 15 && name.ends_with(".json") && NaiveDate::parse_from_str(&name[..10], "%Y-%m-%d").is_ok() {
                legacy_d.push(f.path());
            }
        }
    }
    incs.sort();
    fulls.sort();
    if fulls.len() > weekly {
        for (_, p) in fulls.drain(..fulls.len() - weekly) {
            let _ = fs::remove_file(p);
        }
    }
    if incs.len() > daily {
        let oldest_kept = incs[incs.len() - daily].0;
        // Le plein qui sert de base au plus ancien incrément gardé fixe la limite : rien de plus récent que lui n'est supprimé.
        let base = fulls.iter().map(|(d, _)| *d).filter(|d| *d <= oldest_kept).max().unwrap_or(oldest_kept);
        for (d, p) in &incs {
            if *d < base {
                let _ = fs::remove_file(p);
            }
        }
    }
    for (mut list, keep) in [(legacy_d, daily), (legacy_w, weekly)] {
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
    /// `true` : sauvegarde complète (le dimanche, ou ancienne copie hebdomadaire).
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
            let size = f.metadata().map(|m| m.len()).unwrap_or(0);
            if let Some((_, is_full)) = parse_name(&name) {
                let (count, created) = header_v2(&f.path()).unwrap_or((0, String::new()));
                return Some(BackupInfo { weekly: is_full, size, path: f.path(), name, count, created });
            }
            if !name.ends_with(".json") || name == INDEX_FILE {
                return None;
            }
            let (count, created) = header(&f.path()).unwrap_or((0, String::new()));
            Some(BackupInfo { weekly: name.starts_with("hebdo-"), size, path: f.path(), name, count, created })
        })
        .collect();
    out.sort_by(|a, b| b.created.cmp(&a.created).then(b.name.cmp(&a.name)));
    out
}

/// Version 2 : date de création en première ligne, nombre de lignes écrites dans la dernière.
fn header_v2(file: &Path) -> Result<(usize, String)> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = fs::File::open(file).map_err(|x| io(file, x))?;
    let mut first = String::new();
    BufReader::new(&mut f).read_line(&mut first).map_err(|x| io(file, x))?;
    let created = serde_json::from_str::<Value>(&first).map_err(|x| x.to_string())?["created"].as_str().unwrap_or_default().to_string();
    let len = f.metadata().map_err(|x| io(file, x))?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(128))).map_err(|x| io(file, x))?;
    let mut tail = String::new();
    f.read_to_string(&mut tail).map_err(|x| io(file, x))?;
    let count = tail.lines().last().and_then(|l| serde_json::from_str::<Value>(l).ok()).and_then(|v| v["count"].as_u64()).unwrap_or(0);
    Ok((count as usize, created))
}

/// Version 1, première ligne : `{"version":1,"created":"…","count":N,"items":[`.
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

/// Remet l'état de la date de `file` : un plein, ou le dernier plein précédent rejoué avec les incréments jusqu'à elle.
/// Rien n'est supprimé : les éléments absents de la sauvegarde ou supprimés depuis restent comme ils sont.
pub fn restore_backup(conn: &mut Connection, data_dir: &Path, file: &Path, mode: Mode) -> Result<Restored> {
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let Some((date, is_full)) = parse_name(&name) else { return restore_v1(conn, data_dir, file, mode) };
    let dir = file.parent().unwrap_or(Path::new("."));
    let mut chain: Vec<(NaiveDate, PathBuf)> = Vec::new();
    if is_full {
        chain.push((date, file.to_path_buf()));
    } else {
        let mut files: Vec<(NaiveDate, bool, PathBuf)> =
            fs::read_dir(dir).map_err(|x| io(dir, x))?.flatten().filter_map(|f| parse_name(&f.file_name().to_string_lossy()).map(|(d, full)| (d, full, f.path()))).collect();
        files.sort();
        let base = files.iter().filter(|(d, full, _)| *full && *d <= date).map(|(d, ..)| *d).max().ok_or("aucune sauvegarde complète avant cette date : état impossible à reconstituer")?;
        chain.extend(files.into_iter().filter(|(d, full, _)| (*full && *d == base) || (!*full && *d > base && *d <= date)).map(|(d, _, p)| (d, p)));
    }
    // Du plus récent au plus ancien : la première version rencontrée d'un élément est la bonne, les suivantes sont ignorées.
    chain.sort();
    chain.reverse();
    let mut out = Restored::default();
    let mut seen = std::collections::HashSet::new();
    for (_, path) in &chain {
        let rd = BufReader::new(fs::File::open(path).map_err(|x| io(path, x))?);
        let mut lines = rd.lines();
        let first = lines.next().ok_or("sauvegarde vide")?.map_err(|x| io(path, x))?;
        if serde_json::from_str::<Value>(&first).ok().and_then(|v| v["version"].as_i64()) != Some(2) {
            return Err(format!("{} n'est pas une sauvegarde ThunderLook", path.display()));
        }
        let mut closed = false;
        for line in lines {
            let line = line.map_err(|x| io(path, x))?;
            if line.trim().is_empty() {
                continue;
            }
            let Ok(rec) = serde_json::from_str::<Value>(&line) else { break };
            if rec["end"].as_bool() == Some(true) {
                closed = true;
                break;
            }
            match rec["op"].as_str() {
                Some("del") => {
                    seen.insert(rec["key"].as_str().unwrap_or_default().to_string());
                }
                Some("put") => {
                    let m = &rec["message"];
                    if !seen.insert(identity(&m["folder_id"], &m["nid"], &m["created_at"])) {
                        continue;
                    }
                    match place(conn, data_dir, &rec, mode == Mode::All) {
                        Ok(Placed::Added(_)) => out.added += 1,
                        Ok(Placed::Replaced(_)) => out.replaced += 1,
                        Ok(Placed::Skipped(_)) => out.skipped += 1,
                        Err(err) => out.failed.push(format!("{} : {err}", describe(&rec))),
                    }
                }
                _ => {}
            }
        }
        if !closed {
            out.failed.push(format!("{} : fichier tronqué, la fin de la sauvegarde manque", path.file_name().unwrap_or_default().to_string_lossy()));
        }
    }
    Ok(out)
}

/// Ancien format (version 1) : un JSON `{"items":[ … ]}` avec un élément par ligne.
fn restore_v1(conn: &mut Connection, data_dir: &Path, file: &Path, mode: Mode) -> Result<Restored> {
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

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn lines_of(file: &Path) -> Vec<Value> {
        fs::read_to_string(file).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    fn subjects(conn: &Connection) -> Vec<String> {
        conn.prepare("SELECT subject FROM message ORDER BY subject").unwrap().query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
    }

    #[test]
    fn backup_once_a_day_full_on_sunday_and_pruned() {
        let (dir, conn) = fixture("backup");
        add(&conn, &dir, "IPM.Contact", "Dupont");
        add(&conn, &dir, "IPM.Appointment", "RDV");
        add(&conn, &dir, "IPM.Note", "un courrier, pas une note"); // IPM.Note = courrier : exclu
        let start = d(2026, 1, 1); // jeudi
        let first = backup_on(&dir, KEEP_DAILY, start).unwrap().unwrap();
        assert!(first.ends_with("plein-2026-01-01.jsonl"), "pas d'index : plein");
        assert!(backup_on(&dir, KEEP_DAILY, start).unwrap().is_none(), "une fois par jour");
        let list = list_backups(&dir);
        assert_eq!((list[0].count, list[0].weekly), (2, true));
        let second = backup_on(&dir, KEEP_DAILY, d(2026, 1, 2)).unwrap().unwrap();
        assert!(second.ends_with("2026-01-02.jsonl"));
        assert_eq!(lines_of(&second).len(), 2, "en-tête et fin seulement : rien n'a changé");
        for day in 2..100 {
            backup_on(&dir, KEEP_DAILY, start + chrono::Duration::days(day)).unwrap();
        }
        let files: Vec<String> = list_backups(&dir).into_iter().map(|b| b.name).collect();
        let fulls = files.iter().filter(|n| n.starts_with("plein-")).count();
        assert_eq!(fulls, KEEP_WEEKLY);
        assert!(files.contains(&"2026-04-10.jsonl".to_string()), "la plus récente est gardée");
        assert!(!dir.join("backups").join("2026-01-02.jsonl").exists());
        // Chaîne sans trou : le plus ancien incrément gardé a son plein et tous les incréments intermédiaires.
        let oldest_inc = files.iter().filter(|n| !n.starts_with("plein-")).min().unwrap().clone();
        let (od, _) = parse_name(&oldest_inc).unwrap();
        let base = files.iter().filter_map(|n| parse_name(n)).filter(|(x, f)| *f && *x <= od).map(|(x, _)| x).max().unwrap();
        for day in 0..=(od - base).num_days() {
            let dd = base + chrono::Duration::days(day);
            let n = dd.format("%Y-%m-%d").to_string();
            assert!(files.contains(&format!("plein-{n}.jsonl")) || files.contains(&format!("{n}.jsonl")), "trou dans la chaîne le {n}");
        }
        assert!(dir.join("backups/index.json").exists());
    }

    #[test]
    fn incremental_keeps_only_changes_and_restores_the_state_at_a_date() {
        let (dir, mut conn) = fixture("incr");
        let a = add(&conn, &dir, "IPM.Contact", "A");
        let b = add(&conn, &dir, "IPM.Task", "B");
        let c = add(&conn, &dir, "IPM.StickyNote", "C");
        let day1 = backup_on(&dir, KEEP_DAILY, d(2026, 2, 5)).unwrap().unwrap(); // jeudi : plein
        assert_eq!(lines_of(&day1).len(), 5);

        // Entre deux sauvegardes : A modifié, B supprimé, D créé ; C intact.
        conn.execute("UPDATE message SET subject = 'A2' WHERE id = ?1", [a]).unwrap();
        conn.execute("DELETE FROM message WHERE id = ?1", [b]).unwrap();
        let dd = add(&conn, &dir, "IPM.Appointment", "D");
        let day2 = backup_on(&dir, KEEP_DAILY, d(2026, 2, 6)).unwrap().unwrap();
        let ops: Vec<(String, i64)> = lines_of(&day2).iter().filter_map(|v| Some((v["op"].as_str()?.to_string(), v["id"].as_i64()?))).collect();
        assert_eq!(ops, [("put".to_string(), a), ("put".to_string(), dd), ("del".to_string(), b)], "C n'y est pas");
        assert!(day2.ends_with("2026-02-06.jsonl"));

        // Un 3e jour sans changement, puis le dimanche : plein.
        let day3 = backup_on(&dir, KEEP_DAILY, d(2026, 2, 7)).unwrap().unwrap();
        assert_eq!(lines_of(&day3).len(), 2);
        let sunday = backup_on(&dir, KEEP_DAILY, d(2026, 2, 8)).unwrap().unwrap();
        assert!(sunday.ends_with("plein-2026-02-08.jsonl"));
        assert_eq!(lines_of(&sunday).len(), 2 + 3, "A2, C, D");

        // Tout casser, puis reconstituer l'état du 6 février : plein du 5 + incrément du 6.
        conn.execute("DELETE FROM message", []).unwrap();
        let r = restore_backup(&mut conn, &dir, &day2, Mode::Merge).unwrap();
        assert_eq!((r.added, r.skipped, r.failed.len()), (3, 0, 0));
        assert_eq!(subjects(&conn), ["A2", "C", "D"], "A modifié, B supprimé n'est pas ressuscité");
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM attachment", [], |r| r.get::<_, i64>(0)).unwrap(), 3);

        // Le 5 février (plein seul) : A, B, C d'origine ; l'existant est fusionné sans doublon.
        let r = restore_backup(&mut conn, &dir, &day1, Mode::Merge).unwrap();
        assert_eq!((r.added, r.skipped), (1, 2), "B revient, C déjà là, A déjà là (version actuelle gardée)");
        assert_eq!(subjects(&conn), ["A2", "B", "C", "D"]);
        let _ = c;

        // « Tout remettre » à la date du 6 : A reprend A2 ; rien n'est dupliqué.
        let r = restore_backup(&mut conn, &dir, &day3, Mode::All).unwrap();
        assert_eq!((r.added, r.replaced), (0, 3));
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM message", [], |r| r.get::<_, i64>(0)).unwrap(), 4);
    }

    #[test]
    fn trash_folders_and_reports_are_not_saved() {
        let (dir, conn) = fixture("skip");
        add(&conn, &dir, "IPM.Contact", "gardé");
        conn.execute("INSERT INTO folder (id, source_id, nid, name, role, hidden, position) VALUES (2, 1, 2, 'Éléments supprimés', 'folder', 0, 1)", []).unwrap();
        conn.execute("INSERT INTO folder (id, source_id, nid, name, role, hidden, position) VALUES (3, 1, 3, 'Whatever', 'deleted', 0, 2)", []).unwrap();
        conn.execute("INSERT INTO folder (id, source_id, nid, name, role, hidden, position) VALUES (4, 1, 4, 'Sent', 'folder', 0, 3)", []).unwrap();
        for (f, class, subject) in [(2, "IPM.Contact", "corbeille fr"), (3, "IPM.Task", "corbeille role"), (4, "IPM.Contact", "sent"), (1, "REPORT.IPM.Contact.NDR", "rapport")] {
            let id = add(&conn, &dir, class, subject);
            conn.execute("UPDATE message SET folder_id = ?2 WHERE id = ?1", [id, f]).unwrap();
        }
        let file = backup_on(&dir, KEEP_DAILY, d(2026, 3, 2)).unwrap().unwrap();
        let subjects: Vec<String> = lines_of(&file).iter().filter_map(|v| v["title"].as_str().map(str::to_string)).collect();
        assert_eq!(subjects, ["gardé"]);
    }

    #[test]
    fn old_v1_backups_are_still_listed_and_restored() {
        let (dir, mut conn) = fixture("v1");
        let a = add(&conn, &dir, "IPM.Contact", "A");
        let rec = record(&conn, &dir, a, "sauvegarde").unwrap();
        let file = backups_dir(&dir).join("2026-01-01.json");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, format!("{{\"version\":1,\"created\":\"2026-01-01T10:00:00\",\"count\":1,\"items\":[\n{}\n]}}\n", serde_json::to_string(&rec).unwrap())).unwrap();
        assert_eq!(list_backups(&dir).iter().map(|b| (b.name.as_str(), b.count)).collect::<Vec<_>>(), [("2026-01-01.json", 1)]);
        conn.execute("DELETE FROM message", []).unwrap();
        assert_eq!(restore_backup(&mut conn, &dir, &file, Mode::Merge).unwrap().added, 1);
    }

    #[test]
    fn backup_reads_while_another_connection_writes() {
        let (dir, conn) = fixture("concurrent");
        for i in 0..300 {
            add(&conn, &dir, "IPM.Contact", &format!("c{i}"));
        }
        let dir2 = dir.clone();
        let writer = std::thread::spawn(move || {
            let w = thunderlook_store::open(&dir2).unwrap();
            for i in 0..200 {
                w.execute("UPDATE message SET preview = ?1 WHERE id = ?2", params![format!("p{i}"), i % 300 + 1]).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        });
        let file = backup_on(&dir, KEEP_DAILY, d(2026, 3, 2)).unwrap().unwrap();
        writer.join().unwrap();
        assert_eq!(list_backups(&dir)[0].count, 300);
        assert!(file.exists());
    }

    #[test]
    fn merge_restore_adds_missing_without_duplicating() {
        let (dir, mut conn) = fixture("merge");
        let a = add(&conn, &dir, "IPM.Contact", "A");
        let b = add(&conn, &dir, "IPM.Task", "B");
        let c = add(&conn, &dir, "IPM.StickyNote", "C");
        let file = backup_on(&dir, KEEP_DAILY, d(2026, 2, 1)).unwrap().unwrap();
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
        let file = backup_on(&dir, KEEP_DAILY, d(2026, 2, 1)).unwrap().unwrap();
        conn.execute("DELETE FROM folder", []).unwrap();
        let r = restore_backup(&mut conn, &dir, &file, Mode::Merge).unwrap();
        assert_eq!((r.added, r.failed.len()), (0, 1));
    }

    #[test]
    fn truncated_backup_is_flagged() {
        let (dir, mut conn) = fixture("trunc");
        add(&conn, &dir, "IPM.Contact", "A");
        let file = backup_on(&dir, KEEP_DAILY, d(2026, 2, 1)).unwrap().unwrap();
        let text = fs::read_to_string(&file).unwrap();
        fs::write(&file, text.trim_end().rsplit_once('\n').unwrap().0).unwrap();
        let r = restore_backup(&mut conn, &dir, &file, Mode::Merge).unwrap();
        assert!(r.failed.iter().any(|f| f.contains("tronqué")));
    }

    /// Mesure (notes/perf.md) : `cargo test -p thunderlook-safety --release -- --ignored --nocapture measure`.
    #[test]
    #[ignore]
    fn measure_sizes() {
        let (dir, conn) = fixture("measure");
        conn.execute_batch("BEGIN").unwrap();
        for i in 0..10_000 {
            add(&conn, &dir, "IPM.Contact", &format!("Contact {i}"));
        }
        for i in 0..4_200 {
            add(&conn, &dir, "IPM.Appointment", &format!("RDV {i}"));
        }
        for i in 0..400 {
            add(&conn, &dir, "IPM.StickyNote", &format!("Note {i}"));
        }
        conn.execute_batch("COMMIT").unwrap();
        let t = std::time::Instant::now();
        let full = backup_on(&dir, KEEP_DAILY, d(2026, 2, 5)).unwrap().unwrap();
        println!("plein : {} octets en {:?}", fs::metadata(&full).unwrap().len(), t.elapsed());
        for i in 0..50 {
            conn.execute("UPDATE message SET subject = subject || '!' WHERE id = ?1", [i + 1]).unwrap();
        }
        let t = std::time::Instant::now();
        let inc = backup_on(&dir, KEEP_DAILY, d(2026, 2, 6)).unwrap().unwrap();
        println!("incrément (50 modifiés) : {} octets en {:?}", fs::metadata(&inc).unwrap().len(), t.elapsed());
        let t = std::time::Instant::now();
        let inc = backup_on(&dir, KEEP_DAILY, d(2026, 2, 7)).unwrap().unwrap();
        println!("incrément (rien) : {} octets en {:?}", fs::metadata(&inc).unwrap().len(), t.elapsed());
    }

    fn d_(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn db_copy_is_checked_rotated_and_weekly() {
        let (dir, conn) = fixture("dbcopy");
        add(&conn, &dir, "IPM.Contact", "A");
        let first = backup_db_on(&dir, 2, d_(2026, 4, 1)).unwrap().unwrap();
        verify_db(&first).unwrap();
        assert!(backup_db_on(&dir, 2, d_(2026, 4, 5)).unwrap().is_none(), "une par semaine");
        backup_db_on(&dir, 2, d_(2026, 4, 8)).unwrap().unwrap();
        backup_db_on(&dir, 2, d_(2026, 4, 15)).unwrap().unwrap();
        let names: Vec<String> = list_db_copies(&dir).iter().map(|c| c.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["base-2026-04-15.sqlite", "base-2026-04-08.sqlite"]);
        assert!(!first.exists());
        assert!(!db_dir(&dir).join("base-2026-04-15.tmp").exists());
    }

    #[test]
    fn db_copy_restores_and_refuses_a_corrupted_copy() {
        let (dir, mut conn) = fixture("dbrestore");
        add(&conn, &dir, "IPM.Contact", "A");
        add(&conn, &dir, "IPM.Task", "B");
        let before = subjects(&conn);
        let copy = backup_db_on(&dir, 2, d_(2026, 5, 1)).unwrap().unwrap();
        // Modifier : supprimer une ligne, en ajouter une.
        conn.execute("DELETE FROM message WHERE subject = 'A'", []).unwrap();
        add(&conn, &dir, "IPM.Contact", "Z");
        assert_ne!(subjects(&conn), before);
        restore_db(&mut conn, &dir, &copy).unwrap();
        assert_eq!(subjects(&conn), before);
        assert_eq!(conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)).unwrap(), "ok");
        assert!(db_dir(&dir).join("avant-restauration.sqlite").exists());
        // Corrompre la copie : contrôle et restauration refusés, la base reste intacte.
        let mut bytes = fs::read(&copy).unwrap();
        for b in bytes.iter_mut().skip(4096).take(8192) {
            *b = 0xA5;
        }
        fs::write(&copy, &bytes).unwrap();
        assert!(verify_db(&copy).is_err());
        conn.execute("DELETE FROM message WHERE subject = 'B'", []).unwrap();
        assert!(restore_db(&mut conn, &dir, &copy).is_err());
        assert_eq!(subjects(&conn), ["A"]);
    }

    #[test]
    fn failure_is_recorded_until_next_success() {
        let (dir, _conn) = fixture("failure");
        assert!(last_failure(&dir).is_none());
        fs::create_dir_all(backups_dir(&dir)).unwrap();
        dbcopy::set_failure(&dir, Some("disque plein"));
        assert!(last_failure(&dir).unwrap().ends_with("disque plein"));
        backup(&dir, KEEP_DAILY).unwrap();
        assert!(last_failure(&dir).is_none());
    }

    #[test]
    fn wait_idle_returns_when_quiet_or_after_max() {
        let (dir, _conn) = fixture("idle");
        let t = std::time::Instant::now();
        wait_idle(&dir, std::time::Duration::ZERO, std::time::Duration::from_secs(1), std::time::Duration::from_secs(10));
        assert!(t.elapsed() < std::time::Duration::from_secs(5));
    }
}
