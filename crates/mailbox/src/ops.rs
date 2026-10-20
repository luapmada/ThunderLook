//! Opérations d'organisation (J3) : lu, indicateurs, catégories, déplacer, copier, supprimer, archiver,
//! dossiers et favoris, avec annulation (Ctrl+Z) de la dernière opération réversible.
//!
//! Chaque opération tient dans une transaction. Les fichiers `.eml` ne sont effacés qu'après validation,
//! et seulement pour une suppression définitive, qui n'est pas annulable.

use std::{collections::HashMap, fs};

use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone};
use thunderlook_model::{CategoryInfo, FlagAction, FolderOp, MessageOp, OpResult, SearchScope, Selection, format};
use thunderlook_store::rusqlite::{Connection, OptionalExtension, Transaction, params, params_from_iter};

use crate::{Mailbox, Result, category_color, err, fold, folder_db_id, message_db_id};

/// Profondeur de l'historique d'annulation.
const UNDO_DEPTH: usize = 50;
/// `PidTagMessageFlags` : bit « lu ».
const MSGFLAG_READ: i64 = 0x1;

/// Inverse d'une opération, appliqué par [`Mailbox::undo`].
pub(crate) enum Undo {
    Read(Vec<(i64, bool)>),
    Flag(Vec<FlagRow>),
    Categories(Vec<(i64, Vec<String>)>),
    Move(Vec<(i64, i64)>),
    Copy(Vec<i64>),
    FolderCreate(i64),
    FolderRename(i64, String),
    /// Dossier, ancien parent, ancien nom s'il a fallu le renommer.
    FolderMove(i64, Option<i64>, Option<String>),
    Favorites(Vec<i64>),
}

pub(crate) struct UndoEntry {
    label: String,
    undo: Undo,
}

#[derive(Clone)]
pub(crate) struct FlagRow {
    id: i64,
    status: i64,
    request: Option<String>,
    due: Option<i64>,
    completed: Option<i64>,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{} {many}", format::thousands(n as u32)) }
}

pub(crate) fn local_midnight_ms(d: NaiveDate) -> i64 {
    Local
        .from_local_datetime(&d.and_hms_opt(0, 0, 0).expect("minuit"))
        .earliest()
        .map(|t| t.timestamp_millis())
        .unwrap_or_default()
}

/// Début et échéance d'un indicateur, comme la référence : semaine de travail du lundi au vendredi.
/// Le schéma ne garde que l'échéance ; le début sert aux tests et au futur module Tâches.
fn flag_dates(action: FlagAction, today: NaiveDate) -> (Option<i64>, Option<i64>) {
    let friday = |d: NaiveDate| d + Duration::days(4 - i64::from(d.weekday().num_days_from_monday()));
    let (start, due) = match action {
        FlagAction::Today | FlagAction::Toggle => (Some(today), Some(today)),
        FlagAction::Tomorrow => (Some(today + Duration::days(1)), Some(today + Duration::days(1))),
        FlagAction::ThisWeek => {
            let due = friday(today).max(today);
            (Some((today + Duration::days(2)).min(due)), Some(due))
        }
        FlagAction::NextWeek => {
            let monday = today + Duration::days(7 - i64::from(today.weekday().num_days_from_monday()));
            (Some(monday), Some(friday(monday)))
        }
        _ => (None, None),
    };
    (start.map(local_midnight_ms), due.map(local_midnight_ms))
}

/// Rôles de dossiers qu'on ne renomme, ne déplace ni ne supprime pas.
fn is_special(role: &str) -> bool {
    matches!(role, "inbox" | "drafts" | "sent" | "deleted" | "outbox" | "junk" | "ipm" | "root")
}

struct FolderInfo {
    source: i64,
    parent: Option<i64>,
    name: String,
    role: String,
}

fn folder_info(conn: &Connection, id: i64) -> Result<FolderInfo> {
    conn.query_row("SELECT source_id, parent_id, name, role FROM folder WHERE id = ?1", [id], |r| {
        Ok(FolderInfo { source: r.get(0)?, parent: r.get(1)?, name: r.get(2)?, role: r.get(3)? })
    })
    .optional()
    .map_err(err)?
    .ok_or_else(|| format!("dossier introuvable : f{id}"))
}

/// Le dossier est « Éléments supprimés » ou l'un de ses sous-dossiers.
fn in_deleted(conn: &Connection, mut id: i64) -> Result<bool> {
    loop {
        let f = folder_info(conn, id)?;
        if f.role == "deleted" {
            return Ok(true);
        }
        match f.parent {
            Some(p) => id = p,
            None => return Ok(false),
        }
    }
}

fn is_descendant(conn: &Connection, mut id: i64, ancestor: i64) -> Result<bool> {
    loop {
        if id == ancestor {
            return Ok(true);
        }
        match folder_info(conn, id)?.parent {
            Some(p) => id = p,
            None => return Ok(false),
        }
    }
}

/// Dossier spécial de premier niveau d'une source (le premier dans l'ordre du PST s'il y en a plusieurs).
fn special_folder(conn: &Connection, source: i64, role: &str) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT f.id FROM folder f JOIN folder p ON p.id = f.parent_id
         WHERE f.source_id = ?1 AND f.role = ?2 AND f.hidden = 0 AND p.role = 'ipm' ORDER BY f.position LIMIT 1",
        params![source, role],
        |r| r.get(0),
    )
    .optional()
    .map_err(err)
}

fn ipm_root(conn: &Connection, source: i64) -> Result<i64> {
    conn.query_row("SELECT id FROM folder WHERE source_id = ?1 AND role = 'ipm'", [source], |r| r.get(0))
        .map_err(|_| format!("sous-arborescence IPM absente pour la source {source}"))
}

fn next_position(conn: &Connection, parent: i64) -> Result<i64> {
    conn.query_row("SELECT IFNULL(MAX(position), -1) + 1 FROM folder WHERE parent_id = ?1", [parent], |r| r.get(0)).map_err(err)
}

fn create_folder(tx: &Transaction, parent: i64, name: &str, role: &str) -> Result<i64> {
    let p = folder_info(tx, parent)?;
    let position = next_position(tx, parent)?;
    tx.execute(
        "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position)
         VALUES (?1, ?2, 0, ?3, ?4, 'IPF.Note', 0, ?5)",
        params![p.source, parent, name, role, position],
    )
    .map_err(err)?;
    Ok(tx.last_insert_rowid())
}

fn check_name(conn: &Connection, parent: i64, name: &str, except: Option<i64>) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Le nom du dossier ne peut pas être vide.".into());
    }
    if name.chars().any(|c| c == '\\' || c.is_control()) {
        return Err("Le nom du dossier contient un caractère interdit (\\).".into());
    }
    let mut stmt = conn.prepare("SELECT id, name FROM folder WHERE parent_id = ?1").map_err(err)?;
    let taken = stmt
        .query_map([parent], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(err)?
        .filter_map(|r| r.ok())
        .any(|(id, n)| Some(id) != except && fold(&n) == fold(name));
    if taken {
        return Err(format!("Un dossier « {name} » existe déjà à cet endroit."));
    }
    Ok(name.to_string())
}

/// Supprime définitivement des messages ; renvoie les `.eml` à effacer après validation.
fn purge(tx: &Transaction, ids: &[i64]) -> Result<Vec<String>> {
    let mut paths = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(500) {
        let marks = vec!["?"; chunk.len()].join(",");
        let mut stmt = tx.prepare(&format!("SELECT eml_path FROM message WHERE id IN ({marks})")).map_err(err)?;
        let rows = stmt.query_map(params_from_iter(chunk), |r| r.get::<_, String>(0)).map_err(err)?;
        for p in rows {
            paths.push(p.map_err(err)?);
        }
        tx.execute(&format!("DELETE FROM message_fts WHERE rowid IN ({marks})"), params_from_iter(chunk)).map_err(err)?;
        tx.execute(&format!("DELETE FROM message WHERE id IN ({marks})"), params_from_iter(chunk)).map_err(err)?;
    }
    Ok(paths)
}

/// Messages d'un dossier et de tous ses sous-dossiers.
fn subtree_messages(conn: &Connection, folder: i64) -> Result<Vec<i64>> {
    let mut stmt = conn
        .prepare(
            "WITH RECURSIVE t(id) AS (SELECT ?1 UNION ALL SELECT f.id FROM folder f JOIN t ON f.parent_id = t.id)
             SELECT m.id FROM message m WHERE m.folder_id IN (SELECT id FROM t)",
        )
        .map_err(err)?;
    stmt.query_map([folder], |r| r.get(0)).map_err(err)?.collect::<std::result::Result<_, _>>().map_err(err)
}

impl Mailbox {
    /// Identifiants (base) d'une sélection.
    pub fn resolve(&self, sel: &Selection) -> Result<Vec<i64>> {
        let mut out = Vec::with_capacity(sel.len() as usize);
        if !sel.ranges.is_empty() {
            for &(a, b) in &sel.ranges {
                out.extend(self.view_targets(sel.token, a, b).ok_or("vue expirée : rouvrir le dossier")?);
            }
        }
        for id in &sel.ids {
            out.push(message_db_id(id)?);
        }
        out.sort_unstable();
        out.dedup();
        Ok(out)
    }

    fn push_undo(&mut self, label: String, undo: Undo) {
        self.undo.push(UndoEntry { label, undo });
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
    }

    pub fn undo_label(&self) -> Option<String> {
        self.undo.last().map(|u| u.label.clone())
    }

    /// Catégories connues : définies dans les sources, plus celles employées sans définition (sans couleur).
    pub fn categories(&self) -> Result<Vec<CategoryInfo>> {
        let mut by_name: HashMap<String, CategoryInfo> = HashMap::new();
        let mut stmt = self.conn.prepare("SELECT name, color FROM category ORDER BY source_id").map_err(err)?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))).map_err(err)?;
        for row in rows {
            let (name, color) = row.map_err(err)?;
            by_name.entry(fold(&name)).or_insert(CategoryInfo { name, color: category_color(color) });
        }
        let mut stmt = self.conn.prepare("SELECT DISTINCT name FROM message_category").map_err(err)?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(err)?;
        for name in rows {
            let name = name.map_err(err)?;
            by_name.entry(fold(&name)).or_insert(CategoryInfo { name, color: None });
        }
        let mut v: Vec<CategoryInfo> = by_name.into_values().collect();
        v.sort_by_key(|c| fold(&c.name));
        Ok(v)
    }

    pub fn apply(&mut self, sel: &Selection, op: &MessageOp) -> Result<OpResult> {
        let ids = self.resolve(sel)?;
        if ids.is_empty() {
            return Ok(OpResult::default());
        }
        let n = ids.len();
        let items = plural(n, "élément", "éléments");
        match op {
            MessageOp::Read { read } => self.set_read(&ids, *read, &items, true),
            MessageOp::Seen => self.set_read(&ids, true, &items, false),
            MessageOp::ToggleRead => {
                let any_unread = self.count_where(&ids, "is_read = 0")? > 0;
                self.set_read(&ids, any_unread, &items, true)
            }
            MessageOp::Flag { action } => self.set_flag(&ids, *action, &items),
            MessageOp::Category { name, on } => self.set_category(&ids, name, *on, &items),
            MessageOp::ClearCategories => self.clear_categories(&ids, &items),
            MessageOp::Move { folder_id } => {
                let target = folder_db_id(folder_id)?;
                let name = folder_info(&self.conn, target)?.name;
                self.move_to(&ids, target, format!("{items} déplacé{} vers « {name} »", if n > 1 { "s" } else { "" }))
            }
            MessageOp::Copy { folder_id } => self.copy_to(&ids, folder_db_id(folder_id)?, &items),
            MessageOp::Delete => self.delete(&ids, false),
            MessageOp::DeletePermanently => self.delete(&ids, true),
            MessageOp::Archive => self.archive(&ids, &items),
        }
    }

    fn count_where(&self, ids: &[i64], cond: &str) -> Result<usize> {
        let mut total = 0;
        for chunk in ids.chunks(500) {
            let marks = vec!["?"; chunk.len()].join(",");
            let c: i64 = self
                .conn
                .query_row(&format!("SELECT COUNT(*) FROM message WHERE id IN ({marks}) AND {cond}"), params_from_iter(chunk), |r| r.get(0))
                .map_err(err)?;
            total += c as usize;
        }
        Ok(total)
    }

    fn set_read(&mut self, ids: &[i64], read: bool, items: &str, undoable: bool) -> Result<OpResult> {
        let tx = self.conn.transaction().map_err(err)?;
        let mut prev = Vec::new();
        {
            let mut get = tx.prepare_cached("SELECT is_read FROM message WHERE id = ?1").map_err(err)?;
            let mut set = tx
                .prepare_cached(
                    "UPDATE message SET is_read = ?2,
                       message_flags = CASE WHEN ?2 THEN message_flags | ?3 ELSE message_flags & ~?3 END
                     WHERE id = ?1",
                )
                .map_err(err)?;
            for &id in ids {
                let was: bool = get.query_row([id], |r| Ok(r.get::<_, i64>(0)? != 0)).map_err(err)?;
                if was != read {
                    prev.push((id, was));
                    set.execute(params![id, read, MSGFLAG_READ]).map_err(err)?;
                }
            }
        }
        tx.commit().map_err(err)?;
        let changed = prev.len();
        let what = if read { "lu" } else { "non lu" };
        if changed > 0 && undoable {
            self.push_undo(format!("marquer comme {what}"), Undo::Read(prev));
        }
        Ok(OpResult {
            affected: changed as u32,
            message: format!("{items} marqué{} comme {what}", if ids.len() > 1 { "s" } else { "" }),
            undo_label: self.undo_label(),
            created_id: None,
        })
    }

    fn flag_rows(&self, ids: &[i64]) -> Result<Vec<FlagRow>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT flag_status, flag_request, flag_due, flag_completed FROM message WHERE id = ?1")
            .map_err(err)?;
        let mut out = Vec::with_capacity(ids.len());
        for &id in ids {
            let row = stmt
                .query_row([id], |r| {
                    Ok(FlagRow { id, status: r.get(0)?, request: r.get(1)?, due: r.get(2)?, completed: r.get(3)? })
                })
                .map_err(err)?;
            out.push(row);
        }
        Ok(out)
    }

    fn write_flags(&mut self, rows: &[FlagRow]) -> Result<()> {
        let tx = self.conn.transaction().map_err(err)?;
        {
            let mut set = tx
                .prepare_cached("UPDATE message SET flag_status = ?2, flag_request = ?3, flag_due = ?4, flag_completed = ?5 WHERE id = ?1")
                .map_err(err)?;
            for r in rows {
                set.execute(params![r.id, r.status, r.request, r.due, r.completed]).map_err(err)?;
            }
        }
        tx.commit().map_err(err)
    }

    fn set_flag(&mut self, ids: &[i64], action: FlagAction, items: &str) -> Result<OpResult> {
        let prev = self.flag_rows(ids)?;
        let now = Local::now();
        let (_, due) = flag_dates(action, now.date_naive());
        let next: Vec<FlagRow> = prev
            .iter()
            .map(|p| {
                let action = match action {
                    FlagAction::Toggle if p.status == 2 => FlagAction::Complete,
                    a => a,
                };
                match action {
                    FlagAction::Clear => FlagRow { status: 0, request: None, due: None, completed: None, ..p.clone() },
                    FlagAction::Complete => FlagRow { status: 1, completed: Some(now.timestamp_millis()), ..p.clone() },
                    _ => FlagRow {
                        status: 2,
                        request: p.request.clone().filter(|r| !r.trim().is_empty()).or_else(|| Some("Assurer un suivi".into())),
                        due,
                        completed: None,
                        ..p.clone()
                    },
                }
            })
            .collect();
        self.write_flags(&next)?;
        let label = format!("indicateur « {} »", action.label());
        self.push_undo(label.clone(), Undo::Flag(prev));
        Ok(OpResult { affected: ids.len() as u32, message: format!("{items} : {label}"), undo_label: self.undo_label(), created_id: None })
    }

    fn category_sets(&self, ids: &[i64]) -> Result<Vec<(i64, Vec<String>)>> {
        let mut stmt = self.conn.prepare_cached("SELECT name FROM message_category WHERE message_id = ?1").map_err(err)?;
        let mut out = Vec::with_capacity(ids.len());
        for &id in ids {
            let names = stmt.query_map([id], |r| r.get::<_, String>(0)).map_err(err)?.collect::<std::result::Result<Vec<_>, _>>().map_err(err)?;
            out.push((id, names));
        }
        Ok(out)
    }

    fn write_categories(&mut self, sets: &[(i64, Vec<String>)]) -> Result<()> {
        let tx = self.conn.transaction().map_err(err)?;
        {
            let mut del = tx.prepare_cached("DELETE FROM message_category WHERE message_id = ?1").map_err(err)?;
            let mut ins = tx.prepare_cached("INSERT OR IGNORE INTO message_category (message_id, name) VALUES (?1, ?2)").map_err(err)?;
            for (id, names) in sets {
                del.execute([id]).map_err(err)?;
                for n in names {
                    ins.execute(params![id, n]).map_err(err)?;
                }
            }
        }
        tx.commit().map_err(err)
    }

    fn set_category(&mut self, ids: &[i64], name: &str, on: bool, items: &str) -> Result<OpResult> {
        let name = name.trim();
        if name.is_empty() || name.contains(',') || name.contains(';') {
            return Err("Nom de catégorie invalide (vide, ou avec « , » ou « ; »).".into());
        }
        let prev = self.category_sets(ids)?;
        let next: Vec<(i64, Vec<String>)> = prev
            .iter()
            .map(|(id, names)| {
                let mut v: Vec<String> = names.iter().filter(|n| fold(n) != fold(name)).cloned().collect();
                if on {
                    v.push(name.to_string());
                }
                (*id, v)
            })
            .collect();
        self.write_categories(&next)?;
        let label = if on { format!("catégorie « {name} »") } else { format!("retrait de la catégorie « {name} »") };
        self.push_undo(label.clone(), Undo::Categories(prev));
        Ok(OpResult { affected: ids.len() as u32, message: format!("{items} : {label}"), undo_label: self.undo_label(), created_id: None })
    }

    fn clear_categories(&mut self, ids: &[i64], items: &str) -> Result<OpResult> {
        let prev = self.category_sets(ids)?;
        let next: Vec<(i64, Vec<String>)> = ids.iter().map(|id| (*id, Vec::new())).collect();
        self.write_categories(&next)?;
        self.push_undo("effacer les catégories".into(), Undo::Categories(prev));
        Ok(OpResult { affected: ids.len() as u32, message: format!("{items} : catégories effacées"), undo_label: self.undo_label(), created_id: None })
    }

    fn current_folders(&self, ids: &[i64]) -> Result<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare_cached("SELECT folder_id FROM message WHERE id = ?1").map_err(err)?;
        ids.iter().map(|&id| stmt.query_row([id], |r| Ok((id, r.get::<_, i64>(0)?))).map_err(err)).collect()
    }

    fn write_folders(&mut self, moves: &[(i64, i64)]) -> Result<()> {
        let tx = self.conn.transaction().map_err(err)?;
        {
            let mut set = tx.prepare_cached("UPDATE message SET folder_id = ?2 WHERE id = ?1").map_err(err)?;
            for (id, f) in moves {
                set.execute([id, f]).map_err(err)?;
            }
        }
        tx.commit().map_err(err)
    }

    fn move_to(&mut self, ids: &[i64], target: i64, message: String) -> Result<OpResult> {
        let prev: Vec<(i64, i64)> = self.current_folders(ids)?.into_iter().filter(|(_, f)| *f != target).collect();
        let next: Vec<(i64, i64)> = prev.iter().map(|(id, _)| (*id, target)).collect();
        self.write_folders(&next)?;
        if !prev.is_empty() {
            self.push_undo("déplacer".into(), Undo::Move(prev.clone()));
        }
        Ok(OpResult { affected: prev.len() as u32, message, undo_label: self.undo_label(), created_id: None })
    }

    fn copy_to(&mut self, ids: &[i64], target: i64, items: &str) -> Result<OpResult> {
        let f = folder_info(&self.conn, target)?;
        let data_dir = self.data_dir.clone();
        let tx = self.conn.transaction().map_err(err)?;
        let mut created = Vec::with_capacity(ids.len());
        let mut written: Vec<std::path::PathBuf> = Vec::new();
        let result = (|| -> Result<()> {
            for &id in ids {
                tx.execute(
                    "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line,
                        sent_at, received_at, created_at, modified_at, message_flags, is_read, flag_status, flag_request,
                        flag_due, flag_completed, importance, sensitivity, size, attachment_count, body_kind, preview,
                        internet_message_id, in_reply_to, conversation_topic, conversation_index, eml_path, props, conversation_id)
                     SELECT ?2, 0, message_class, subject, from_name, from_addr, to_line, cc_line,
                        sent_at, received_at, created_at, modified_at, message_flags, is_read, flag_status, flag_request,
                        flag_due, flag_completed, importance, sensitivity, size, attachment_count, body_kind, preview,
                        internet_message_id, in_reply_to, conversation_topic, conversation_index, '', props, conversation_id
                     FROM message WHERE id = ?1",
                    params![id, target],
                )
                .map_err(err)?;
                let new_id = tx.last_insert_rowid();
                let old: String = tx.query_row("SELECT eml_path FROM message WHERE id = ?1", [id], |r| r.get(0)).map_err(err)?;
                let rel = thunderlook_store::eml_relative_path(f.source, new_id);
                let dest = thunderlook_store::resolve(&data_dir, &rel);
                if let Some(dir) = dest.parent() {
                    fs::create_dir_all(dir).map_err(err)?;
                }
                fs::copy(thunderlook_store::resolve(&data_dir, &old), &dest).map_err(|e| format!("copie du message : {e}"))?;
                written.push(dest);
                tx.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![new_id, rel]).map_err(err)?;
                tx.execute(
                    "INSERT INTO attachment (message_id, position, filename, mime_type, size, method, content_id, inline, hidden, props)
                     SELECT ?2, position, filename, mime_type, size, method, content_id, inline, hidden, props FROM attachment WHERE message_id = ?1",
                    params![id, new_id],
                )
                .map_err(err)?;
                tx.execute(
                    "INSERT INTO message_category (message_id, name) SELECT ?2, name FROM message_category WHERE message_id = ?1",
                    params![id, new_id],
                )
                .map_err(err)?;
                created.push(new_id);
            }
            Ok(())
        })();
        if let Err(e) = result {
            drop(tx);
            for p in written {
                let _ = fs::remove_file(p);
            }
            return Err(e);
        }
        tx.commit().map_err(err)?;
        self.push_undo("copier".into(), Undo::Copy(created.clone()));
        Ok(OpResult {
            affected: created.len() as u32,
            message: format!("{items} copié{} vers « {} »", if ids.len() > 1 { "s" } else { "" }, f.name),
            undo_label: self.undo_label(),
            created_id: None,
        })
    }

    fn purge_ids(&mut self, ids: &[i64]) -> Result<()> {
        let tx = self.conn.transaction().map_err(err)?;
        let paths = purge(&tx, ids)?;
        tx.commit().map_err(err)?;
        for p in paths {
            let _ = fs::remove_file(thunderlook_store::resolve(&self.data_dir, &p));
        }
        Ok(())
    }

    fn delete(&mut self, ids: &[i64], permanent: bool) -> Result<OpResult> {
        // Par source : ce qui est déjà dans « Éléments supprimés » part définitivement, le reste y va.
        let folders = self.current_folders(ids)?;
        let mut to_purge = Vec::new();
        let mut to_trash: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut deleted_cache: HashMap<i64, bool> = HashMap::new();
        for (id, f) in folders {
            let already = match deleted_cache.get(&f) {
                Some(b) => *b,
                None => {
                    let b = in_deleted(&self.conn, f)?;
                    deleted_cache.insert(f, b);
                    b
                }
            };
            if permanent || already {
                to_purge.push(id);
            } else {
                let source = folder_info(&self.conn, f)?.source;
                let trash = special_folder(&self.conn, source, "deleted")?;
                match trash {
                    Some(t) => to_trash.entry(t).or_default().push(id),
                    None => to_purge.push(id),
                }
            }
        }
        let mut moved = Vec::new();
        for (trash, list) in &to_trash {
            for id in list {
                moved.push((*id, *trash));
            }
        }
        let prev: Vec<(i64, i64)> = self.current_folders(&moved.iter().map(|(id, _)| *id).collect::<Vec<_>>())?;
        self.write_folders(&moved)?;
        if !to_purge.is_empty() {
            self.purge_ids(&to_purge)?;
        }
        if !prev.is_empty() {
            self.push_undo("supprimer".into(), Undo::Move(prev));
        }
        let message = match (moved.len(), to_purge.len()) {
            (0, p) => format!("{} définitivement", plural(p, "élément supprimé", "éléments supprimés")),
            (m, 0) => format!("{} vers « Éléments supprimés »", plural(m, "élément déplacé", "éléments déplacés")),
            (m, p) => format!("{} vers « Éléments supprimés », {} définitivement", plural(m, "élément déplacé", "éléments déplacés"), plural(p, "supprimé", "supprimés")),
        };
        Ok(OpResult { affected: ids.len() as u32, message, undo_label: self.undo_label(), created_id: None })
    }

    fn archive(&mut self, ids: &[i64], items: &str) -> Result<OpResult> {
        // Un dossier « Archives » par source, créé au premier usage.
        let folders = self.current_folders(ids)?;
        let mut by_target: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut created = None;
        for (id, f) in folders {
            let source = folder_info(&self.conn, f)?.source;
            let target = match special_folder(&self.conn, source, "archive")? {
                Some(t) => t,
                None => {
                    let root = ipm_root(&self.conn, source)?;
                    let tx = self.conn.transaction().map_err(err)?;
                    let t = create_folder(&tx, root, "Archives", "archive")?;
                    tx.commit().map_err(err)?;
                    created = Some(format!("f{t}"));
                    t
                }
            };
            if target != f {
                by_target.entry(target).or_default().push(id);
            }
        }
        let mut prev = Vec::new();
        for (target, list) in by_target {
            prev.extend(self.current_folders(&list)?);
            let moves: Vec<(i64, i64)> = list.iter().map(|id| (*id, target)).collect();
            self.write_folders(&moves)?;
        }
        if !prev.is_empty() {
            self.push_undo("archiver".into(), Undo::Move(prev));
        }
        Ok(OpResult {
            affected: ids.len() as u32,
            message: format!("{items} archivé{} dans « Archives »", if ids.len() > 1 { "s" } else { "" }),
            undo_label: self.undo_label(),
            created_id: created,
        })
    }

    /// Favoris enregistrés, ou ceux par défaut (réception et envoyés de la première source).
    pub(crate) fn favorite_ids(&self) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare("SELECT folder_id FROM favorite ORDER BY position").map_err(err)?;
        let saved: Vec<i64> = stmt.query_map([], |r| r.get(0)).map_err(err)?.collect::<std::result::Result<_, _>>().map_err(err)?;
        if !saved.is_empty() {
            return Ok(saved);
        }
        let first: Option<i64> = self.conn.query_row("SELECT MIN(id) FROM source", [], |r| r.get(0)).map_err(err)?;
        let Some(source) = first else { return Ok(Vec::new()) };
        let mut out = Vec::new();
        for role in ["inbox", "sent"] {
            if let Some(f) = special_folder(&self.conn, source, role)? {
                out.push(f);
            }
        }
        Ok(out)
    }

    fn write_favorites(&mut self, ids: &[i64]) -> Result<()> {
        let tx = self.conn.transaction().map_err(err)?;
        tx.execute("DELETE FROM favorite", []).map_err(err)?;
        for (i, id) in ids.iter().enumerate() {
            tx.execute("INSERT INTO favorite (folder_id, position) VALUES (?1, ?2)", params![id, i as i64]).map_err(err)?;
        }
        tx.commit().map_err(err)
    }

    pub fn folder_op(&mut self, op: &FolderOp) -> Result<OpResult> {
        let done = |message: String, created_id: Option<String>, mb: &Self| OpResult {
            affected: 1,
            message,
            undo_label: mb.undo_label(),
            created_id,
        };
        match op {
            FolderOp::Create { parent_id, name } => {
                let parent = folder_db_id(parent_id)?;
                let name = check_name(&self.conn, parent, name, None)?;
                let tx = self.conn.transaction().map_err(err)?;
                let id = create_folder(&tx, parent, &name, "folder")?;
                tx.commit().map_err(err)?;
                self.push_undo(format!("créer le dossier « {name} »"), Undo::FolderCreate(id));
                Ok(done(format!("Dossier « {name} » créé"), Some(format!("f{id}")), self))
            }
            FolderOp::Rename { id, name } => {
                let id = folder_db_id(id)?;
                let f = folder_info(&self.conn, id)?;
                if is_special(&f.role) {
                    return Err(format!("« {} » est un dossier spécial : il ne peut pas être renommé.", f.name));
                }
                let parent = f.parent.ok_or("dossier racine")?;
                let name = check_name(&self.conn, parent, name, Some(id))?;
                self.conn.execute("UPDATE folder SET name = ?2 WHERE id = ?1", params![id, name]).map_err(err)?;
                self.push_undo(format!("renommer « {} »", f.name), Undo::FolderRename(id, f.name.clone()));
                Ok(done(format!("Dossier « {} » renommé en « {name} »", f.name), None, self))
            }
            FolderOp::Move { id, parent_id } => {
                let id = folder_db_id(id)?;
                let parent = folder_db_id(parent_id)?;
                let f = folder_info(&self.conn, id)?;
                let p = folder_info(&self.conn, parent)?;
                if is_special(&f.role) {
                    return Err(format!("« {} » est un dossier spécial : il ne peut pas être déplacé.", f.name));
                }
                if p.source != f.source {
                    return Err("Déplacer un dossier vers un autre fichier de données n'est pas encore possible.".into());
                }
                if is_descendant(&self.conn, parent, id)? {
                    return Err("Un dossier ne peut pas être déplacé dans lui-même ou dans l'un de ses sous-dossiers.".into());
                }
                check_name(&self.conn, parent, &f.name, Some(id))?;
                let position = next_position(&self.conn, parent)?;
                self.conn.execute("UPDATE folder SET parent_id = ?2, position = ?3 WHERE id = ?1", params![id, parent, position]).map_err(err)?;
                self.push_undo(format!("déplacer le dossier « {} »", f.name), Undo::FolderMove(id, f.parent, None));
                Ok(done(format!("Dossier « {} » déplacé dans « {} »", f.name, p.name), None, self))
            }
            FolderOp::Delete { id } => {
                let id = folder_db_id(id)?;
                let f = folder_info(&self.conn, id)?;
                if is_special(&f.role) {
                    return Err(format!("« {} » est un dossier spécial : il ne peut pas être supprimé.", f.name));
                }
                if in_deleted(&self.conn, id)? {
                    let msgs = subtree_messages(&self.conn, id)?;
                    let tx = self.conn.transaction().map_err(err)?;
                    let paths = purge(&tx, &msgs)?;
                    tx.execute("DELETE FROM folder WHERE id = ?1", [id]).map_err(err)?;
                    tx.commit().map_err(err)?;
                    for p in paths {
                        let _ = fs::remove_file(thunderlook_store::resolve(&self.data_dir, &p));
                    }
                    return Ok(done(format!("Dossier « {} » supprimé définitivement ({})", f.name, plural(msgs.len(), "élément", "éléments")), None, self));
                }
                let trash = special_folder(&self.conn, f.source, "deleted")?.ok_or("pas de dossier « Éléments supprimés » dans ce fichier")?;
                let name = check_name(&self.conn, trash, &f.name, Some(id)).unwrap_or_else(|_| format!("{} ({})", f.name, Local::now().format("%d-%m-%Y %H.%M")));
                let position = next_position(&self.conn, trash)?;
                self.conn
                    .execute("UPDATE folder SET parent_id = ?2, position = ?3, name = ?4 WHERE id = ?1", params![id, trash, position, name])
                    .map_err(err)?;
                let old_name = (name != f.name).then(|| f.name.clone());
                self.push_undo(format!("supprimer le dossier « {} »", f.name), Undo::FolderMove(id, f.parent, old_name));
                Ok(done(format!("Dossier « {} » déplacé vers « Éléments supprimés »", f.name), None, self))
            }
            FolderOp::MarkAllRead { id } => {
                let fid = folder_db_id(id)?;
                let ids: Vec<i64> = self
                    .conn
                    .prepare("SELECT id FROM message WHERE folder_id = ?1 AND is_read = 0")
                    .map_err(err)?
                    .query_map([fid], |r| r.get(0))
                    .map_err(err)?
                    .collect::<std::result::Result<_, _>>()
                    .map_err(err)?;
                let items = plural(ids.len(), "élément", "éléments");
                self.set_read(&ids, true, &items, true)
            }
            FolderOp::Empty { id } => {
                let fid = folder_db_id(id)?;
                let ids: Vec<i64> = self
                    .conn
                    .prepare("SELECT id FROM message WHERE folder_id = ?1")
                    .map_err(err)?
                    .query_map([fid], |r| r.get(0))
                    .map_err(err)?
                    .collect::<std::result::Result<_, _>>()
                    .map_err(err)?;
                self.delete(&ids, false)
            }
            FolderOp::SaveSearch { name, query, scope, folder_id } => {
                let name = name.trim();
                if name.is_empty() || query.trim().is_empty() {
                    return Err("Nom ou recherche vide.".into());
                }
                // Étendue enregistrée : tout, ou le fichier de données du dossier courant.
                let scope = match (scope, folder_db_id(folder_id)) {
                    (SearchScope::All, _) | (_, Err(_)) => "all".to_string(),
                    (_, Ok(f)) => format!("source:{}", folder_info(&self.conn, f)?.source),
                };
                let position: i64 = self.conn.query_row("SELECT IFNULL(MAX(position), -1) + 1 FROM search_folder", [], |r| r.get(0)).map_err(err)?;
                self.conn
                    .execute("INSERT INTO search_folder (name, query, scope, position) VALUES (?1, ?2, ?3, ?4)", params![name, query.trim(), scope, position])
                    .map_err(err)?;
                let id = self.conn.last_insert_rowid();
                Ok(done(format!("Dossier de recherche « {name} » créé"), Some(format!("s-{id}")), self))
            }
            FolderOp::DeleteSearchFolder { id } => {
                let n: i64 = id.strip_prefix("s-").and_then(|n| n.parse().ok()).ok_or("seuls les dossiers de recherche enregistrés se suppriment")?;
                self.conn.execute("DELETE FROM search_folder WHERE id = ?1", [n]).map_err(err)?;
                Ok(done("Dossier de recherche supprimé (les éléments restent dans leurs dossiers)".into(), None, self))
            }
            FolderOp::AddFavorite { id } | FolderOp::RemoveFavorite { id } => {
                let fid = folder_db_id(id)?;
                let name = folder_info(&self.conn, fid)?.name;
                let prev = self.favorite_ids()?;
                let add = matches!(op, FolderOp::AddFavorite { .. });
                let mut next: Vec<i64> = prev.iter().copied().filter(|f| *f != fid).collect();
                if add {
                    next.push(fid);
                }
                if next == prev {
                    return Ok(OpResult::default());
                }
                // Une liste vide ramène les favoris par défaut.
                self.write_favorites(&next)?;
                self.push_undo(if add { "ajouter aux favoris".into() } else { "retirer des favoris".into() }, Undo::Favorites(prev));
                Ok(done(if add { format!("« {name} » ajouté aux favoris") } else { format!("« {name} » retiré des favoris") }, None, self))
            }
        }
    }

    /// Annule la dernière opération réversible ; renvoie son libellé.
    pub fn undo(&mut self) -> Result<Option<String>> {
        let Some(entry) = self.undo.pop() else { return Ok(None) };
        match entry.undo {
            Undo::Read(prev) => {
                let tx = self.conn.transaction().map_err(err)?;
                {
                    let mut set = tx
                        .prepare_cached(
                            "UPDATE message SET is_read = ?2,
                               message_flags = CASE WHEN ?2 THEN message_flags | ?3 ELSE message_flags & ~?3 END
                             WHERE id = ?1",
                        )
                        .map_err(err)?;
                    for (id, was) in prev {
                        set.execute(params![id, was, MSGFLAG_READ]).map_err(err)?;
                    }
                }
                tx.commit().map_err(err)?;
            }
            Undo::Flag(prev) => self.write_flags(&prev)?,
            Undo::Categories(prev) => self.write_categories(&prev)?,
            Undo::Move(prev) => self.write_folders(&prev)?,
            Undo::Copy(created) => self.purge_ids(&created)?,
            Undo::FolderCreate(id) => {
                let msgs = subtree_messages(&self.conn, id)?;
                let tx = self.conn.transaction().map_err(err)?;
                let paths = purge(&tx, &msgs)?;
                tx.execute("DELETE FROM folder WHERE id = ?1", [id]).map_err(err)?;
                tx.commit().map_err(err)?;
                for p in paths {
                    let _ = fs::remove_file(thunderlook_store::resolve(&self.data_dir, &p));
                }
            }
            Undo::FolderRename(id, name) => {
                self.conn.execute("UPDATE folder SET name = ?2 WHERE id = ?1", params![id, name]).map_err(err)?;
            }
            Undo::FolderMove(id, parent, old_name) => {
                let position = match parent {
                    Some(p) => next_position(&self.conn, p)?,
                    None => 0,
                };
                self.conn.execute("UPDATE folder SET parent_id = ?2, position = ?3 WHERE id = ?1", params![id, parent, position]).map_err(err)?;
                if let Some(name) = old_name {
                    self.conn.execute("UPDATE folder SET name = ?2 WHERE id = ?1", params![id, name]).map_err(err)?;
                }
            }
            Undo::Favorites(prev) => self.write_favorites(&prev)?,
        }
        Ok(Some(entry.label))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_dates_follow_work_week() {
        let mon = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let fri = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        let next_mon = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let next_fri = NaiveDate::from_ymd_opt(2026, 10, 30).unwrap();
        assert_eq!(flag_dates(FlagAction::ThisWeek, mon), (Some(local_midnight_ms(mon + Duration::days(2))), Some(local_midnight_ms(fri))));
        assert_eq!(flag_dates(FlagAction::NextWeek, mon), (Some(local_midnight_ms(next_mon)), Some(local_midnight_ms(next_fri))));
        // Samedi : « cette semaine » ne remonte pas dans le passé.
        let sat = NaiveDate::from_ymd_opt(2026, 10, 24).unwrap();
        assert_eq!(flag_dates(FlagAction::ThisWeek, sat).1, Some(local_midnight_ms(sat)));
        assert_eq!(flag_dates(FlagAction::NoDate, mon), (None, None));
    }
}
