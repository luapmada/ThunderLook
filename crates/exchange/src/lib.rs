//! Échange avec le téléphone par FICHIERS (option B de `notes/mpe-plan.md`) : ThunderLook ne parle jamais à MPE.
//!
//! - [`export_all`] écrit dans le dossier d'échange `contacts.vcf`, `taches.ics`, `notes.vnt`, `calendrier.ics` et un
//!   `manifest.json` (horodatage, nombre d'éléments, SHA-256 et identifiants de chaque fichier), chaque fichier de façon
//!   atomique (fichier temporaire puis renommage).
//! - [`import_changes`] relit ces fichiers et ne rend que ce qui a changé par rapport à la base, sans rien écrire ;
//!   un élément modifié des deux côtés est rendu à part (`conflict`).
//! - [`apply`] applique les changements acceptés, après avoir consigné de quoi revenir en arrière dans
//!   `data_dir/exchange-undo/<horodatage>.json` ; [`undo`] les défait.
//!
//! Rien n'est supprimé sans accord : une suppression est une simple proposition ([`Change::default_selected`] est faux).

mod conflict;
mod import;
pub mod mpe_files;
mod sha256;
pub mod vcal10;

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thunderlook_model::{contacts::ContactForm, items::{NoteForm, TaskForm}};
use thunderlook_store::rusqlite::Connection;

pub use conflict::{Origin, Resolution};
pub use import::{ApplyReport, apply, import_changes, import_changes_with, last_undo, undo};

pub type Result<T> = std::result::Result<T, String>;

pub const CONTACTS_FILE: &str = "contacts.vcf";
pub const TASKS_FILE: &str = "taches.ics";
pub const NOTES_FILE: &str = "notes.vnt";
pub const CALENDAR_FILE: &str = "calendrier.ics";
pub const MANIFEST_FILE: &str = "manifest.json";

/// Nature d'un élément échangé.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Contact,
    Task,
    Note,
    Event,
}

impl ItemKind {
    pub fn all() -> [ItemKind; 4] {
        [ItemKind::Contact, ItemKind::Task, ItemKind::Note, ItemKind::Event]
    }

    pub fn file(self) -> &'static str {
        match self {
            ItemKind::Contact => CONTACTS_FILE,
            ItemKind::Task => TASKS_FILE,
            ItemKind::Note => NOTES_FILE,
            ItemKind::Event => CALENDAR_FILE,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ItemKind::Contact => "contact",
            ItemKind::Task => "task",
            ItemKind::Note => "note",
            ItemKind::Event => "event",
        }
    }
}

/// Ce que le manifeste retient d'un fichier écrit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub count: usize,
    pub sha256: String,
    /// Identifiants locaux (`p12`, `t5`, `n3`, `a7`) des éléments écrits : seuls ceux-là peuvent être proposés à la
    /// suppression (un élément créé depuis l'export n'est pas « supprimé sur le téléphone »).
    #[serde(default)]
    pub ids: Vec<String>,
    /// Identifiant écrit dans le fichier (`UID`, `X-OUTLOOK-UID`) → identifiant local : retrouve un élément supprimé ici
    /// depuis l'export, même quand le fichier le désigne par l'identifiant du téléphone.
    #[serde(default)]
    pub uids: HashMap<String, String>,
    /// Identifiant local → empreinte (SHA-256) de la valeur exportée de l'élément. Absent des manifestes d'avant ce
    /// format : la valeur exportée est alors « inconnue » et tout écart est traité en conflit (voir `conflict`).
    #[serde(default)]
    pub values: HashMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// `AAAA-MM-JJTHH:MM:SSZ` (UTC).
    pub exported_at: String,
    pub files: Vec<FileInfo>,
}

/// Résultat d'un export : le manifeste écrit.
pub type Report = Manifest;

impl Manifest {
    pub fn file(&self, name: &str) -> Option<&FileInfo> {
        self.files.iter().find(|f| f.name == name)
    }
}

/// Valeur d'un élément proposé par un changement (de quoi l'appliquer, ou le rétablir).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::large_enum_variant)]
pub enum Payload {
    None,
    Contact(ContactForm),
    Task(TaskForm),
    Note(NoteForm),
    Event(EventData),
}

/// Rendez-vous : les champs que porte un `.ics`. Dates locales (Paris) `AAAA-MM-JJTHH:MM`. Une série porte en plus son
/// `.ics` (`series`) ; `start`/`end` sont alors ceux de sa première occurrence.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EventData {
    /// `a<id>` ; absent pour un nouveau rendez-vous.
    pub id: Option<String>,
    /// `c<dossier>` du calendrier.
    pub calendar_id: String,
    pub subject: String,
    pub location: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
    pub reminder: Option<i32>,
    pub busy: i32,
    pub body: String,
    pub categories: String,
    pub private: bool,
    /// VCALENDAR de la série (maître + occurrences modifiées) ; `None` : rendez-vous simple.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    New,
    Modified,
    Deleted,
}

/// Un changement proposé. Identifié par `key`, stable d'une lecture à l'autre tant que les fichiers ne bougent pas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub key: String,
    pub kind: ChangeKind,
    pub item: ItemKind,
    /// Identifiant local de l'élément touché (`p12`…) ; pour un nouvel élément, son identifiant dans le fichier.
    pub id: String,
    pub summary: String,
    /// Identifiant du fichier (`UID`, `X-OUTLOOK-UID`), s'il en a un : retenu après application pour reconnaître l'élément.
    pub ext_uid: Option<String>,
    pub payload: Payload,
    /// Présent pour un conflit (voir [`ImportReport::conflicts`]) : jamais appliqué sans [`Change::resolution`].
    #[serde(default)]
    pub conflict: Option<Origin>,
    /// Choix de l'utilisateur, à renseigner avant [`apply`] pour un conflit.
    #[serde(default)]
    pub resolution: Option<Resolution>,
}

impl Change {
    /// Coché d'office dans la liste ? Jamais une suppression.
    pub fn default_selected(&self) -> bool {
        self.kind != ChangeKind::Deleted && self.conflict.is_none()
    }
}

/// Réglages de la lecture des fichiers du téléphone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportOptions {
    /// Les notes viennent du téléphone (`NoteSyncMode=2` de MPE, forcé téléphone → PC) : une note du fichier différente
    /// de celle de la base la remplace sans conflit, même si ThunderLook l'a modifiée depuis l'export. Les autres
    /// éléments restent bidirectionnels. Vrai par défaut.
    pub notes_from_phone: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions { notes_from_phone: true }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ImportReport {
    pub changes: Vec<Change>,
    /// Éléments modifiés des deux côtés (ou dont la valeur exportée est inconnue) : à part, jamais appliqués sans choix.
    #[serde(default)]
    pub conflicts: Vec<Change>,
    /// Éléments du fichier identiques à ceux de la base.
    pub unchanged: usize,
    pub warnings: Vec<String>,
}

// --- Export ---------------------------------------------------------------------------------------------------------

pub(crate) fn now_stamp() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Écrit `bytes` dans `path` sans jamais laisser un fichier à moitié écrit : fichier voisin puis renommage.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).ok_or("nom de fichier invalide")?;
    let tmp = path.with_file_name(format!(".{name}.tmp"));
    let result = fs::write(&tmp, bytes).and_then(|()| fs::rename(&tmp, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(|e| format!("{} : {e}", path.display()))
}

pub(crate) fn event_uid(a: &thunderlook_calendar::Appointment) -> String {
    a.uid.clone().unwrap_or_else(|| format!("thunderlook-{}-{}", a.source_id, a.id))
}

/// Identifiant sous lequel le téléphone connaît un élément : celui qu'il a donné à la création (retenu dans
/// `exchange-map.json`), sinon l'identifiant local. Le garder d'un export à l'autre évite que MPE voie « un nouvel
/// élément » et « un élément disparu » là où il n'y en a qu'un.
fn phone_uid(map: Option<&HashMap<String, String>>, local: &str) -> String {
    map.and_then(|m| m.iter().filter(|(_, l)| l.as_str() == local).map(|(u, _)| u).min()).cloned().unwrap_or_else(|| local.to_string())
}

/// Exporte contacts, tâches, notes et calendrier dans `out_dir` (créé au besoin).
pub fn export_all(conn: &Connection, data_dir: &Path, out_dir: &Path) -> Result<Report> {
    fs::create_dir_all(out_dir).map_err(|e| format!("{} : {e}", out_dir.display()))?;
    let mut files: Vec<FileInfo> = Vec::new();
    let maps = read_map(data_dir);
    let uid_of = |kind: ItemKind, local: &str| phone_uid(maps.get(kind.name()), local);

    let contacts = thunderlook_contacts::load(conn, None, None)?;
    let forms: Vec<ContactForm> = contacts.contacts.iter().map(|c| thunderlook_contacts::write::form_from(c, data_dir)).collect();
    let shown: Vec<ContactForm> = forms.iter().map(|f| ContactForm { id: f.id.as_deref().map(|i| uid_of(ItemKind::Contact, i)), ..f.clone() }).collect();
    let text = thunderlook_contacts::vcard::export(&shown, thunderlook_contacts::vcard::Version::V30);
    let pairs = forms.iter().zip(&shown).filter_map(|(f, s)| Some((f.id.clone()?, s.id.clone()?))).collect();
    let values = forms.iter().filter_map(|f| Some((f.id.clone()?, conflict::fingerprint(&import::canon_contact(f))))).collect();
    files.push(write_file(out_dir, CONTACTS_FILE, &text, pairs, values)?);

    let tasks = thunderlook_items::tasks(conn)?;
    let forms: Vec<TaskForm> = tasks
        .rows
        .iter()
        .filter_map(|r| r.id.strip_prefix('t').and_then(|n| n.parse().ok()))
        .map(|n| thunderlook_items::task_form(conn, data_dir, n))
        .collect::<Result<_>>()?;
    let shown: Vec<TaskForm> = forms.iter().map(|f| TaskForm { id: f.id.as_deref().map(|i| uid_of(ItemKind::Task, i)), ..f.clone() }).collect();
    let pairs = forms.iter().zip(&shown).filter_map(|(f, s)| Some((f.id.clone()?, s.id.clone()?))).collect();
    let values = forms.iter().filter_map(|f| Some((f.id.clone()?, conflict::fingerprint(&import::canon_task(f))))).collect();
    files.push(write_file(out_dir, TASKS_FILE, &thunderlook_items::vtodo::export(&shown), pairs, values)?);

    let notes = thunderlook_items::notes(conn, data_dir)?;
    let forms: Vec<NoteForm> = notes
        .rows
        .iter()
        .filter_map(|r| r.id.strip_prefix('n').and_then(|n| n.parse().ok()))
        .map(|n| thunderlook_items::note_form(conn, data_dir, n))
        .collect::<Result<_>>()?;
    let shown: Vec<NoteForm> = forms.iter().map(|f| NoteForm { id: f.id.as_deref().map(|i| uid_of(ItemKind::Note, i)), ..f.clone() }).collect();
    let pairs = forms.iter().zip(&shown).filter_map(|(f, s)| Some((f.id.clone()?, s.id.clone()?))).collect();
    let values = forms.iter().filter_map(|f| Some((f.id.clone()?, conflict::fingerprint(&import::canon_note(f))))).collect();
    files.push(write_file(out_dir, NOTES_FILE, &thunderlook_items::vnote::export(&shown), pairs, values)?);

    let calendar = thunderlook_calendar::load(conn)?;
    let shown: Vec<thunderlook_calendar::Appointment> = calendar
        .appointments
        .iter()
        .map(|a| {
            let phone = uid_of(ItemKind::Event, &format!("a{}", a.id));
            thunderlook_calendar::Appointment { uid: Some(if phone == format!("a{}", a.id) { event_uid(a) } else { phone }), ..a.clone() }
        })
        .collect();
    let text = thunderlook_calendar::ics::export(&shown.iter().collect::<Vec<_>>(), "ThunderLook", &|a| thunderlook_calendar::description(a, data_dir));
    let pairs = shown.iter().map(|a| (format!("a{}", a.id), event_uid(a))).collect();
    let values = calendar.appointments.iter().map(|a| (format!("a{}", a.id), conflict::fingerprint(&import::canon_event(a, data_dir)))).collect();
    files.push(write_file(out_dir, CALENDAR_FILE, &text, pairs, values)?);

    let manifest = Manifest { exported_at: now_stamp(), files };
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    write_atomic(&out_dir.join(MANIFEST_FILE), json.as_bytes())?;
    Ok(manifest)
}

fn write_file(dir: &Path, name: &str, text: &str, pairs: Vec<(String, String)>, values: HashMap<String, String>) -> Result<FileInfo> {
    write_atomic(&dir.join(name), text.as_bytes())?;
    let count = match name {
        CONTACTS_FILE => text.matches("BEGIN:VCARD").count(),
        TASKS_FILE => text.matches("BEGIN:VTODO").count(),
        NOTES_FILE => text.matches("BEGIN:VNOTE").count(),
        _ => text.matches("BEGIN:VEVENT").count(),
    };
    Ok(FileInfo { name: name.to_string(), count, sha256: sha256::hex(text.as_bytes()), ids: pairs.iter().map(|(id, _)| id.clone()).collect(), uids: pairs.into_iter().map(|(id, uid)| (uid, id)).collect(), values })
}

/// Manifeste du dossier d'échange, s'il y en a un de lisible.
pub fn read_manifest(dir: &Path) -> Option<Manifest> {
    serde_json::from_str(&fs::read_to_string(dir.join(MANIFEST_FILE)).ok()?).ok()
}

/// Fichiers du dossier d'échange qui ne sont plus tels qu'`export_all` les a écrits (modifiés par le téléphone, ou absents) :
/// les ré-exporter sans les avoir relus ferait perdre ces modifications. Vide sans manifeste.
pub fn modified_files(out_dir: &Path) -> Vec<String> {
    let Some(manifest) = read_manifest(out_dir) else { return Vec::new() };
    manifest
        .files
        .iter()
        .filter(|f| fs::read(out_dir.join(&f.name)).map_or(true, |b| sha256::hex(&b) != f.sha256))
        .map(|f| f.name.clone())
        .collect()
}

// --- Correspondance identifiant du fichier → élément local ----------------------------------------------------------

/// `data_dir/exchange-map.json` : identifiants du fichier (ceux de MPE ne sont pas les nôtres) → identifiant local,
/// par nature d'élément. Écrit par `apply` seulement.
pub(crate) type IdMap = HashMap<String, HashMap<String, String>>;

pub(crate) fn map_path(data_dir: &Path) -> PathBuf {
    data_dir.join("exchange-map.json")
}

pub(crate) fn read_map(data_dir: &Path) -> IdMap {
    fs::read_to_string(map_path(data_dir)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub(crate) fn write_map(data_dir: &Path, map: &IdMap) -> Result<()> {
    write_atomic(&map_path(data_dir), serde_json::to_string_pretty(map).map_err(|e| e.to_string())?.as_bytes())
}

pub(crate) fn undo_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("exchange-undo")
}
