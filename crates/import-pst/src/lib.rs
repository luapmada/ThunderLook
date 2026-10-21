//! Import PST en lecture seule vers le stockage de ThunderLook (jalon J1).
//!
//! - Le PST n'est jamais ouvert en écriture. Par défaut, il est d'abord copié dans `sources/` du
//!   dossier de données ; la lecture « sur place » est refusée sous les dossiers protégés.
//! - Chaque élément de contenu donne une ligne `message` et un `.eml`, ou une ligne `import_issue`.
//!   Chaque contenu associé (FAI) est conservé brut dans `fai`.
//! - Toute propriété MAPI non convertie reste dans le sac brut `props`.
//! - L'import se fait dans une seule transaction : en cas d'échec, la base est inchangée.

mod convert;
pub mod props;

use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc,
    thread,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use outlook_pst::{
    messaging::{folder::Folder, store::Store},
    ndb::node_id::NodeId,
};
use serde::Serialize;
use thunderlook_store::{eml_relative_path, resolve, rusqlite::{self, Connection, params}};

use crate::props::{NamedProps, binary, entry_id_nid, int, text};

/// Dossiers dont les PST ne sont jamais lus sur place (seulement copiés).
pub const PROTECTED_ROOTS: &[&str] = &[r"A:\Fichiers Outlook"];

const NID_ROOT_FOLDER: u32 = 0x122;
const PR_DISPLAY_NAME: u16 = 0x3001;
const PR_CONTAINER_CLASS: u16 = 0x3613;
const PR_CONTENT_COUNT: u16 = 0x3602;
const PR_CONTENT_UNREAD: u16 = 0x3603;
const PR_IPM_SUBTREE_ENTRYID: u16 = 0x35E0;
const PR_IPM_OUTBOX_ENTRYID: u16 = 0x35E2;
const PR_IPM_WASTEBASKET_ENTRYID: u16 = 0x35E3;
const PR_IPM_SENTMAIL_ENTRYID: u16 = 0x35E4;
const PR_IPM_DRAFTS_ENTRYID: u16 = 0x36D7;
/// Dossiers additionnels ([MS-OXOSFLD] 2.2.4) : conflits, synchro, échecs locaux, échecs serveur, indésirables.
const PR_ADDITIONAL_REN_ENTRYIDS: u16 = 0x36D8;
const PR_ROAMING_XML_STREAM: u16 = 0x7C08;

#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Lire le fichier sur place au lieu d'en faire une copie de travail (refusé sous les dossiers protégés).
    pub in_place: bool,
    /// Remplacer une source déjà importée depuis le même fichier (perd les changements faits depuis).
    pub replace: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Copy,
    Import,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: Phase,
    pub done: u64,
    pub total: u64,
    pub folder: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueRecord {
    pub folder: String,
    pub nid: Option<u32>,
    pub severity: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub origin_path: String,
    pub work_path: String,
    pub source_id: i64,
    pub size: u64,
    pub copy_ms: u64,
    pub import_ms: u64,
    pub folders: u64,
    pub hidden_folders: u64,
    pub contents_rows: u64,
    pub messages_imported: u64,
    pub messages_lost: u64,
    pub fai_rows: u64,
    pub fai_imported: u64,
    pub fai_lost: u64,
    pub attachments: u64,
    pub attachments_lost: u64,
    pub embedded_messages: u64,
    pub items_degraded: u64,
    pub categories: u64,
    pub named_properties: u64,
    pub eml_bytes: u64,
    pub classes: BTreeMap<String, u64>,
    pub body_kinds: BTreeMap<String, u64>,
    pub issues_total: u64,
    /// Les 500 premiers incidents (tous sont dans la table `import_issue`).
    pub issues: Vec<IssueRecord>,
}

#[derive(Debug)]
pub struct ImportError(pub String);

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ImportError {}

macro_rules! fail {
    ($($arg:tt)*) => { ImportError(format!($($arg)*)) };
}

impl From<rusqlite::Error> for ImportError {
    fn from(e: rusqlite::Error) -> Self {
        fail!("base : {e}")
    }
}

impl From<std::io::Error> for ImportError {
    fn from(e: std::io::Error) -> Self {
        fail!("{e}")
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// Chemin sans le préfixe `\\?\` que `canonicalize` ajoute sous Windows.
fn plain_path(path: &Path) -> String {
    path.to_string_lossy().trim_start_matches(r"\\?\").to_string()
}

fn is_protected(path: &Path) -> bool {
    let p = plain_path(&fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())).to_lowercase();
    PROTECTED_ROOTS.iter().any(|root| {
        let root = root.to_lowercase();
        p == root || p.starts_with(&format!("{root}\\"))
    })
}

/// Copie de travail dans `sources/`. Réutilisée si elle existe avec la même taille et n'est pas plus ancienne.
fn working_copy(src: &Path, data_dir: &Path, progress: &mut dyn FnMut(&Progress)) -> Result<PathBuf, ImportError> {
    let meta = fs::metadata(src)?;
    let name = src.file_name().ok_or_else(|| fail!("nom de fichier invalide : {}", src.display()))?;
    let dir = data_dir.join("sources");
    fs::create_dir_all(&dir)?;
    let dest = dir.join(name);
    if let Ok(existing) = fs::metadata(&dest)
        && existing.len() == meta.len()
        && existing.modified()? >= meta.modified()?
    {
        return Ok(dest);
    }
    let tmp = dest.with_extension("part");
    let mut input = fs::File::open(src)?;
    let mut output = fs::File::create(&tmp)?;
    let mut buf = vec![0u8; 8 << 20];
    let mut done = 0u64;
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n])?;
        done += n as u64;
        progress(&Progress { phase: Phase::Copy, done, total: meta.len(), folder: String::new() });
    }
    output.sync_all()?;
    drop(output);
    fs::rename(&tmp, &dest)?;
    Ok(dest)
}

/// Importe un PST. Une source déjà importée depuis le même fichier d'origine n'est remplacée
/// qu'avec [`Options::replace`].
pub fn import(src: &Path, data_dir: &Path, options: &Options, progress: &mut dyn FnMut(&Progress)) -> Result<Report, ImportError> {
    let origin = fs::canonicalize(src).map_err(|e| fail!("{} : {e}", src.display()))?;
    let origin_str = plain_path(&origin);
    let origin = PathBuf::from(&origin_str);
    if options.in_place && is_protected(&origin) {
        return Err(fail!("lecture sur place refusée sous un dossier protégé : {origin_str} (l'import en fait une copie)"));
    }
    fs::create_dir_all(data_dir)?;
    let data_dir = &PathBuf::from(plain_path(&fs::canonicalize(data_dir)?));
    let t_copy = Instant::now();
    let work = if options.in_place { origin.clone() } else { working_copy(&origin, data_dir, progress)? };
    let copy_ms = t_copy.elapsed().as_millis() as u64;

    let t0 = Instant::now();
    let store = outlook_pst::open_store(&work).map_err(|e| fail!("ouverture du PST {} : {e}", work.display()))?;
    let named = NamedProps::read(&store).map_err(|e| fail!("table des propriétés nommées : {e}"))?;

    let mut conn = thunderlook_store::open(data_dir)?;
    conn.execute_batch("PRAGMA synchronous = OFF;")?;
    if let Some(old) = thunderlook_store::find_source(&conn, &origin_str)? {
        // Remplacer une source efface aussi ce qui a été fait dans ThunderLook (lu, classement, suppressions).
        if !options.replace {
            return Err(fail!(
                "{origin_str} est déjà importé (source {old}). Le réimport remplace la source et perd les changements \
                 faits depuis dans ThunderLook : relancer avec --replace pour confirmer."
            ));
        }
        thunderlook_store::delete_source(&conn, data_dir, old)?;
    }

    let tx = conn.transaction()?;
    let size = fs::metadata(&work)?.len();
    let sp = store.properties();
    let display_name = sp.display_name().unwrap_or_else(|_| {
        origin.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    });
    tx.execute(
        "INSERT INTO source (kind, origin_path, work_path, display_name, size, imported_at, props) VALUES ('pst', ?1, ?2, ?3, ?4, ?5, ?6)",
        params![origin_str, work.to_string_lossy(), display_name, size as i64, now_ms(), named.raw],
    )?;
    let source_id = tx.last_insert_rowid();
    let next_message_id: i64 = tx.query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM message", [], |r| r.get(0))?;

    let mut report = Report {
        origin_path: origin_str.clone(),
        work_path: work.to_string_lossy().into_owned(),
        source_id,
        size,
        copy_ms,
        named_properties: named.len() as u64,
        ..Default::default()
    };

    // Les .eml sont écrits par un fil dédié pendant que celui-ci lit le PST.
    let (sender, receiver) = mpsc::sync_channel::<(PathBuf, Vec<u8>)>(256);
    let writer = thread::spawn(move || -> std::io::Result<()> {
        for (path, bytes) in receiver {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            fs::write(&path, &bytes)?;
        }
        Ok(())
    });

    let roles = Roles::read(&store);
    let mut walker = Walker {
        store: &store,
        named: &named,
        tx: &tx,
        data_dir,
        source_id,
        next_message_id,
        report: &mut report,
        roles,
        sender,
        progress,
        total_hint: 0,
        ipm_folder_id: None,
    };
    let root = walker.open_folder(NID_ROOT_FOLDER).map_err(|e| fail!("dossier racine : {e}"))?;
    walker.walk(root, None, "", 0, true)?;
    let Walker { sender, .. } = walker;
    drop(sender);
    writer.join().map_err(|_| fail!("le fil d'écriture des .eml s'est arrêté"))??;

    report.import_ms = t0.elapsed().as_millis() as u64;
    report.issues_total = tx.query_row("SELECT COUNT(*) FROM import_issue WHERE source_id = ?1", params![source_id], |r| r.get::<_, i64>(0))? as u64;
    let json = serde_json::to_string_pretty(&report).unwrap_or_default();
    tx.execute(
        "INSERT INTO import_run (source_id, started_at, duration_ms, report_json) VALUES (?1, ?2, ?3, ?4)",
        params![source_id, now_ms() - report.import_ms as i64, report.import_ms as i64, json],
    )?;
    tx.commit()?;

    let reports = data_dir.join("reports");
    fs::create_dir_all(&reports)?;
    fs::write(reports.join(format!("import-{source_id}-{}.json", now_ms())), json)?;
    Ok(report)
}

/// Bilan d'un rechargement des calendriers.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarRefresh {
    /// Marque reconnue par `thunderlook-calendar` (mention « lu le … » de la vue).
    pub calendar_refresh: bool,
    pub origin_path: String,
    pub work_path: String,
    pub source_id: i64,
    pub folders: Vec<String>,
    pub removed: u64,
    pub imported: u64,
    pub lost: u64,
    pub import_ms: u64,
}

/// J9, option A : relit seulement les calendriers d'un PST plus récent (même fichier d'origine, copie plus fraîche)
/// dans une source déjà importée. Les dossiers de rendez-vous sont retrouvés par leur NID ; leurs éléments sont
/// remplacés par ceux du PST, le reste de la source (courrier, contacts…) n'est pas touché. Une seule transaction ;
/// les anciens `.eml` ne sont effacés qu'après sa validation.
pub fn refresh_calendars(src: &Path, data_dir: &Path, source_id: i64, progress: &mut dyn FnMut(&Progress)) -> Result<CalendarRefresh, ImportError> {
    let origin = PathBuf::from(plain_path(&fs::canonicalize(src).map_err(|e| fail!("{} : {e}", src.display()))?));
    let data_dir = &PathBuf::from(plain_path(&fs::canonicalize(data_dir)?));
    let work = working_copy(&origin, data_dir, progress)?;
    let t0 = Instant::now();
    let store = outlook_pst::open_store(&work).map_err(|e| fail!("ouverture du PST {} : {e}", work.display()))?;
    let named = NamedProps::read(&store).map_err(|e| fail!("table des propriétés nommées : {e}"))?;

    let mut conn = thunderlook_store::open(data_dir)?;
    // Les sacs déjà importés pointent dans la table des propriétés nommées de la source : la nouvelle doit la prolonger.
    let old_named: Vec<u8> = conn
        .query_row("SELECT props FROM source WHERE id = ?1 AND kind = 'pst'", params![source_id], |r| r.get::<_, Option<Vec<u8>>>(0))
        .map_err(|_| fail!("source {source_id} introuvable ou qui n'est pas un PST"))?
        .unwrap_or_default();
    if !named.raw.starts_with(&old_named) {
        return Err(fail!(
            "la table des propriétés nommées de {} ne prolonge pas celle de la source {source_id} : ce n'est pas une version du même fichier",
            origin.display()
        ));
    }

    let tx = conn.transaction()?;
    tx.execute("UPDATE source SET props = ?2 WHERE id = ?1", params![source_id, named.raw])?;
    let next_message_id: i64 = tx.query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM message", [], |r| r.get(0))?;
    let mut report = Report { source_id, ..Default::default() };
    let (sender, receiver) = mpsc::sync_channel::<(PathBuf, Vec<u8>)>(256);
    let writer = thread::spawn(move || -> std::io::Result<()> {
        for (path, bytes) in receiver {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            fs::write(&path, &bytes)?;
        }
        Ok(())
    });
    let mut out = CalendarRefresh {
        calendar_refresh: true,
        origin_path: plain_path(&origin),
        work_path: work.to_string_lossy().into_owned(),
        source_id,
        ..Default::default()
    };
    let mut old_emls: Vec<String> = Vec::new();
    let mut walker = Walker {
        store: &store,
        named: &named,
        tx: &tx,
        data_dir,
        source_id,
        next_message_id,
        report: &mut report,
        roles: Roles::default(),
        sender,
        progress,
        total_hint: 0,
        ipm_folder_id: None,
    };
    // Parcours de l'arborescence du PST : seuls les dossiers de rendez-vous déjà connus de la source sont rechargés.
    let mut stack = vec![(NID_ROOT_FOLDER, String::new())];
    while let Some((nid, path)) = stack.pop() {
        let folder = walker.open_folder(nid).map_err(|e| fail!("dossier {nid:#x} : {e}"))?;
        let fp = folder.properties();
        let name = text(fp.get(PR_DISPLAY_NAME)).unwrap_or_default();
        let here = if path.is_empty() { name.clone() } else { format!("{path}/{name}") };
        if let Some(table) = folder.hierarchy_table() {
            stack.extend(table.rows_matrix().map(|r| (u32::from(r.id()), here.clone())));
        }
        let class = text(fp.get(PR_CONTAINER_CLASS)).unwrap_or_default();
        if !class.starts_with("IPF.Appointment") {
            continue;
        }
        let Some(folder_id) = tx
            .query_row("SELECT id FROM folder WHERE source_id = ?1 AND nid = ?2", params![source_id, nid], |r| r.get::<_, i64>(0))
            .map(Some)
            .or_else(|e| if matches!(e, rusqlite::Error::QueryReturnedNoRows) { Ok(None) } else { Err(e) })?
        else {
            continue;
        };
        let old: Vec<(i64, String)> = tx
            .prepare("SELECT id, eml_path FROM message WHERE folder_id = ?1")?
            .query_map(params![folder_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (id, rel) in old {
            tx.execute("DELETE FROM message_fts WHERE rowid = ?1", params![id])?;
            tx.execute("DELETE FROM message WHERE id = ?1", params![id])?;
            old_emls.push(rel);
            out.removed += 1;
        }
        tx.execute(
            "UPDATE folder SET name = ?2, pst_content_count = ?3, pst_unread_count = ?4, props = ?5 WHERE id = ?1",
            params![folder_id, name, int(fp.get(PR_CONTENT_COUNT)), int(fp.get(PR_CONTENT_UNREAD)), props::propbag(fp.iter(), &[])],
        )?;
        if let Some(table) = folder.contents_table() {
            let rows: Vec<u32> = table.rows_matrix().map(|r| u32::from(r.id())).collect();
            for row_nid in rows {
                walker.report.contents_rows += 1;
                walker.message(folder_id, row_nid, &here)?;
            }
        }
        out.folders.push(here);
    }
    let Walker { sender, .. } = walker;
    drop(sender);
    writer.join().map_err(|_| fail!("le fil d'écriture des .eml s'est arrêté"))??;
    out.imported = report.messages_imported;
    out.lost = report.messages_lost;
    out.import_ms = t0.elapsed().as_millis() as u64;
    let json = serde_json::to_string_pretty(&out).unwrap_or_default();
    tx.execute(
        "INSERT INTO import_run (source_id, started_at, duration_ms, report_json) VALUES (?1, ?2, ?3, ?4)",
        params![source_id, now_ms() - out.import_ms as i64, out.import_ms as i64, json],
    )?;
    tx.commit()?;
    // Les nouveaux `.eml` ont d'autres noms (identifiants nouveaux) : les anciens peuvent partir.
    for rel in old_emls {
        let _ = fs::remove_file(resolve(data_dir, &rel));
    }
    Ok(out)
}

/// Rôles des dossiers spéciaux, par NID.
#[derive(Default)]
struct Roles {
    ipm_subtree: Option<u32>,
    by_nid: BTreeMap<u32, &'static str>,
}

impl Roles {
    fn read(store: &Rc<dyn Store>) -> Self {
        let sp = store.properties();
        let nid = |id| binary(sp.get(id)).and_then(entry_id_nid);
        let mut r = Roles { ipm_subtree: nid(PR_IPM_SUBTREE_ENTRYID), ..Default::default() };
        for (id, role) in [(PR_IPM_OUTBOX_ENTRYID, "outbox"), (PR_IPM_WASTEBASKET_ENTRYID, "deleted"), (PR_IPM_SENTMAIL_ENTRYID, "sent")] {
            if let Some(n) = nid(id) {
                r.by_nid.insert(n, role);
            }
        }
        r
    }

    /// La Boîte de réception porte les EntryID des Brouillons et des dossiers additionnels
    /// ([MS-OXOSFLD] 2.2.1) : on la reconnaît à ces propriétés et on en tire les autres rôles.
    fn learn_from(&mut self, nid: u32, folder: &dyn Folder) {
        let fp = folder.properties();
        let Some(drafts) = binary(fp.get(PR_IPM_DRAFTS_ENTRYID)).and_then(entry_id_nid) else { return };
        self.by_nid.entry(drafts).or_insert("drafts");
        if Some(nid) != self.ipm_subtree && nid != NID_ROOT_FOLDER {
            self.by_nid.entry(nid).or_insert("inbox");
        }
        if let Some(outlook_pst::ltp::prop_context::PropertyValue::MultipleBinary(list)) = fp.get(PR_ADDITIONAL_REN_ENTRYIDS)
            && let Some(junk) = list.get(4).and_then(|b| entry_id_nid(b.buffer()))
            && junk != 0
        {
            self.by_nid.entry(junk).or_insert("junk");
        }
    }
}

struct Walker<'a> {
    store: &'a Rc<dyn Store>,
    named: &'a NamedProps,
    tx: &'a Connection,
    data_dir: &'a Path,
    source_id: i64,
    next_message_id: i64,
    report: &'a mut Report,
    roles: Roles,
    sender: mpsc::SyncSender<(PathBuf, Vec<u8>)>,
    progress: &'a mut dyn FnMut(&Progress),
    total_hint: u64,
    ipm_folder_id: Option<i64>,
}

impl Walker<'_> {
    fn open_folder(&self, nid: u32) -> std::io::Result<Rc<dyn Folder>> {
        let id = self.store.properties().make_entry_id(NodeId::from(nid))?;
        self.store.open_folder(&id)
    }

    fn issue(&mut self, folder: &str, nid: Option<u32>, severity: &str, kind: &str, detail: &str) -> Result<(), ImportError> {
        self.tx
            .prepare_cached("INSERT INTO import_issue (source_id, folder_path, nid, severity, kind, detail) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?
            .execute(params![self.source_id, folder, nid, severity, kind, detail])?;
        if self.report.issues.len() < 500 {
            self.report.issues.push(IssueRecord {
                folder: folder.into(),
                nid,
                severity: severity.into(),
                kind: kind.into(),
                detail: detail.into(),
            });
        }
        Ok(())
    }

    fn walk(&mut self, folder: Rc<dyn Folder>, parent: Option<i64>, path: &str, position: usize, hidden: bool) -> Result<(), ImportError> {
        let fp = folder.properties();
        let nid = u32::from(fp.node_id());
        let name = text(fp.get(PR_DISPLAY_NAME)).unwrap_or_default();
        let here = if path.is_empty() { name.clone() } else { format!("{path}/{name}") };
        self.roles.learn_from(nid, folder.as_ref());
        let is_ipm = Some(nid) == self.roles.ipm_subtree;
        let hidden = hidden && !is_ipm;
        let role = if nid == NID_ROOT_FOLDER {
            "root"
        } else if is_ipm {
            "ipm"
        } else if let Some(role) = self.roles.by_nid.get(&nid) {
            role
        } else if parent.is_some() && parent == self.ipm_folder_id {
            role_from_name(&name)
        } else {
            "folder"
        };
        self.tx
            .prepare_cached(
                "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position, pst_content_count, pst_unread_count, props)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?
            .execute(params![
                self.source_id,
                parent,
                nid,
                name,
                role,
                text(fp.get(PR_CONTAINER_CLASS)),
                hidden,
                position as i64,
                int(fp.get(PR_CONTENT_COUNT)),
                int(fp.get(PR_CONTENT_UNREAD)),
                props::propbag(fp.iter(), &[]),
            ])?;
        let folder_id = self.tx.last_insert_rowid();
        if is_ipm {
            self.ipm_folder_id = Some(folder_id);
        }
        self.report.folders += 1;
        if hidden {
            self.report.hidden_folders += 1;
        }

        if let Some(table) = folder.contents_table() {
            let rows: Vec<u32> = table.rows_matrix().map(|r| u32::from(r.id())).collect();
            self.total_hint += rows.len() as u64;
            for row_nid in rows {
                self.report.contents_rows += 1;
                self.message(folder_id, row_nid, &here)?;
                if self.report.contents_rows % 500 == 0 {
                    (self.progress)(&Progress {
                        phase: Phase::Import,
                        done: self.report.contents_rows,
                        total: self.total_hint,
                        folder: here.clone(),
                    });
                }
            }
        }

        if let Some(table) = folder.associated_table() {
            let rows: Vec<u32> = table.rows_matrix().map(|r| u32::from(r.id())).collect();
            for row_nid in rows {
                self.report.fai_rows += 1;
                self.fai(folder_id, row_nid, &here)?;
            }
        }

        if let Some(table) = folder.hierarchy_table() {
            let children: Vec<u32> = table.rows_matrix().map(|r| u32::from(r.id())).collect();
            for (i, child) in children.into_iter().enumerate() {
                match self.open_folder(child) {
                    Ok(sub) => self.walk(sub, Some(folder_id), &here, i, hidden)?,
                    Err(e) => self.issue(&here, Some(child), "lost", "folder-open", &format!("sous-dossier illisible : {e}"))?,
                }
            }
        }
        Ok(())
    }

    fn open_message(&self, nid: u32) -> std::io::Result<Rc<dyn outlook_pst::messaging::message::Message>> {
        let id = self.store.properties().make_entry_id(NodeId::from(nid))?;
        self.store.open_message(&id, None)
    }

    fn message(&mut self, folder_id: i64, nid: u32, folder: &str) -> Result<(), ImportError> {
        let msg = match self.open_message(nid) {
            Ok(m) => m,
            Err(e) => {
                self.report.messages_lost += 1;
                return self.issue(folder, Some(nid), "lost", "message-open", &e.to_string());
            }
        };
        let c = convert::convert(&msg, self.named);
        let id = self.next_message_id;
        self.next_message_id += 1;
        let rel = eml_relative_path(self.source_id, id);
        self.report.eml_bytes += c.eml.len() as u64;
        self.tx
            .prepare_cached(
                "INSERT INTO message (id, folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line,
                    sent_at, received_at, created_at, modified_at, message_flags, is_read, flag_status, flag_request, flag_due,
                    flag_completed, importance, sensitivity, size, attachment_count, body_kind, preview, internet_message_id,
                    in_reply_to, conversation_topic, conversation_index, eml_path, props, conversation_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23,
                    ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32)",
            )?
            .execute(params![
                id, folder_id, nid, c.message_class, c.subject, c.from_name, c.from_addr, c.to_line, c.cc_line,
                c.sent_at, c.received_at, c.created_at, c.modified_at, c.message_flags, c.is_read, c.flag_status,
                c.flag_request, c.flag_due, c.flag_completed, c.importance, c.sensitivity, c.size,
                c.attachments.len() as i64, c.body_kind, c.preview, c.internet_message_id, c.in_reply_to,
                c.conversation_topic, c.conversation_index, rel, c.props,
                c.conversation_id.map_or_else(Vec::new, |g| g.to_vec()),
            ])?;
        for (position, a) in c.attachments.iter().enumerate() {
            self.tx
                .prepare_cached(
                    "INSERT INTO attachment (message_id, position, filename, mime_type, size, method, content_id, inline, hidden, props)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                )?
                .execute(params![id, position as i64, a.filename, a.mime_type, a.size, a.method, a.content_id, a.inline, a.hidden, a.props])?;
            self.report.attachments += 1;
            if a.embedded {
                self.report.embedded_messages += 1;
            }
        }
        for name in &c.categories {
            self.tx
                .prepare_cached("INSERT OR IGNORE INTO message_category (message_id, name) VALUES (?1, ?2)")?
                .execute(params![id, name])?;
        }
        let mut degraded = false;
        for i in &c.issues {
            if i.kind.starts_with("attachment") && i.severity == "lost" {
                self.report.attachments_lost += 1;
            }
            degraded = true;
            self.issue(folder, Some(nid), i.severity, i.kind, &format!("« {} » : {}", c.subject, i.detail))?;
        }
        if degraded {
            self.report.items_degraded += 1;
        }
        *self.report.classes.entry(c.message_class).or_default() += 1;
        *self.report.body_kinds.entry(c.body_kind.to_string()).or_default() += 1;
        self.report.messages_imported += 1;
        self.sender
            .send((resolve(self.data_dir, &rel), c.eml))
            .map_err(|_| fail!("le fil d'écriture des .eml s'est arrêté"))
    }

    fn fai(&mut self, folder_id: i64, nid: u32, folder: &str) -> Result<(), ImportError> {
        let msg = match self.open_message(nid) {
            Ok(m) => m,
            Err(e) => {
                self.report.fai_lost += 1;
                return self.issue(folder, Some(nid), "lost", "fai-open", &e.to_string());
            }
        };
        let p = msg.properties();
        let class = text(p.get(0x001A)).unwrap_or_default();
        self.tx
            .prepare_cached("INSERT INTO fai (folder_id, nid, message_class, subject, props) VALUES (?1, ?2, ?3, ?4, ?5)")?
            .execute(params![folder_id, nid, class, text(p.get(0x0037)).map(|s| thunderlook_store::strip_subject_marker(&s).to_string()).unwrap_or_default(), props::propbag(p.iter(), &[])])?;
        self.report.fai_imported += 1;
        if class.eq_ignore_ascii_case("IPM.Configuration.CategoryList")
            && let Some(xml) = binary(p.get(PR_ROAMING_XML_STREAM))
        {
            match parse_categories(xml) {
                Ok(list) => {
                    for c in list {
                        self.tx
                            .prepare_cached("INSERT OR REPLACE INTO category (source_id, name, color, shortcut, guid) VALUES (?1, ?2, ?3, ?4, ?5)")?
                            .execute(params![self.source_id, c.name, c.color, c.shortcut, c.guid])?;
                        self.report.categories += 1;
                    }
                }
                Err(e) => self.issue(folder, Some(nid), "degraded", "categories", &format!("liste des catégories illisible : {e}"))?,
            }
        }
        Ok(())
    }
}

/// Repli pour les PST sans EntryID de dossiers spéciaux (PST de comptes IMAP) : noms français et
/// anglais des dossiers de premier niveau créés par Outlook.
fn role_from_name(name: &str) -> &'static str {
    match name.to_lowercase().as_str() {
        "boîte de réception" | "inbox" => "inbox",
        "brouillons" | "drafts" => "drafts",
        "éléments envoyés" | "sent items" => "sent",
        "éléments supprimés" | "deleted items" => "deleted",
        "courrier indésirable" | "junk e-mail" | "junk email" => "junk",
        "boîte d'envoi" | "outbox" => "outbox",
        "archive" | "archives" => "archive",
        _ => "folder",
    }
}

struct Category {
    name: String,
    color: i64,
    shortcut: Option<i64>,
    guid: Option<String>,
}

/// Liste des catégories ([MS-OXOCFG] 2.2.5, `CategoryList.xsd`).
fn parse_categories(xml: &[u8]) -> Result<Vec<Category>, String> {
    use quick_xml::{Reader, XmlVersion, events::Event};
    let xml = xml.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(xml);
    let mut reader = Reader::from_reader(xml);
    let mut out = Vec::new();
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf).map_err(|e| e.to_string())? {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == "category" => {
                let mut c = Category { name: String::new(), color: -1, shortcut: None, guid: None };
                for attr in e.attributes().flatten() {
                    let value = attr.normalized_value(XmlVersion::Implicit1_0).map_err(|e| e.to_string())?.into_owned();
                    match attr.key.local_name().as_ref() {
                        "name" => c.name = value,
                        "color" => c.color = value.parse().unwrap_or(-1),
                        "keyboardShortcut" => c.shortcut = value.parse().ok(),
                        "guid" => c.guid = Some(value),
                        _ => {}
                    }
                }
                if !c.name.is_empty() {
                    out.push(c);
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}
