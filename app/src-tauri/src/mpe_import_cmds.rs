//! Lot AI : « Importer depuis MPE ». Lit en LECTURE SEULE `Phonebook.dat`, `Organizer.dat` et `Notes.dat` du dossier d'un
//! appareil de MyPhoneExplorer (`%APPDATA%\MyPhoneExplorer\<appareil>\`) et passe le résultat au circuit de comparaison de
//! l'échange par fichiers. Sens téléphone → ThunderLook seulement : rien n'est jamais écrit dans le dossier de MPE, MPE
//! n'est ni lancé ni piloté. Tout le calcul est dans `thunderlook_exchange::mpe_files`.
//!
//! L'application et l'annulation passent par les mêmes mécanismes que l'écran d'échange (`apply`, `phone_exchange_undo`).

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use thunderlook_exchange::{Change, ChangeKind, ItemKind, Origin, Payload, Resolution, mpe_files};

use crate::{CalendarCache, clear_calendar, data_dir};

/// Une seule lecture ou application MPE à la fois.
static BUSY: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MpeDevices {
    /// Dossier exploré (`%APPDATA%\MyPhoneExplorer` par défaut).
    pub root: String,
    pub devices: Vec<mpe_files::Device>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDiff {
    pub label: String,
    pub old: String,
    pub new: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailRow {
    pub key: String,
    pub kind: ChangeKind,
    pub item: ItemKind,
    pub summary: String,
    pub selected: bool,
    /// `Some("both")` / `Some("unknown")` pour un conflit.
    pub conflict: Option<String>,
    pub fields: Vec<FieldDiff>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailCheck {
    pub changes: Vec<DetailRow>,
    pub conflicts: Vec<DetailRow>,
    pub unchanged: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConflictChoice {
    pub key: String,
    pub choice: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplySummary {
    pub created: usize,
    pub modified: usize,
    pub deleted: usize,
    pub kept: usize,
    pub ignored: usize,
    pub warnings: Vec<String>,
}

fn lock() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    BUSY.lock().map_err(|_| "verrou de l'import MPE empoisonné".to_string())
}

fn devices_of(root: &Path) -> Result<MpeDevices, String> {
    Ok(MpeDevices { root: root.to_string_lossy().into_owned(), devices: mpe_files::list_devices(root)? })
}

/// Appareils trouvés dans `root`, ou dans `%APPDATA%\MyPhoneExplorer` si `root` est absent. Lecture de dossiers seulement.
#[tauri::command(async)]
pub fn mpe_import_devices(root: Option<String>) -> Result<MpeDevices, String> {
    let root = match root.filter(|r| !r.trim().is_empty()) {
        Some(r) => PathBuf::from(r),
        None => mpe_files::default_root().ok_or("Dossier %APPDATA%\\MyPhoneExplorer introuvable : choisissez-le à la main.")?,
    };
    devices_of(&root)
}

/// « Choisir un autre dossier… » : le dossier de MPE ou directement celui d'un appareil ; `None` si l'utilisateur annule.
#[tauri::command(async)]
pub fn mpe_import_pick(app: AppHandle) -> Result<Option<MpeDevices>, String> {
    let mut dialog = app.dialog().file();
    if let Some(root) = mpe_files::default_root() {
        dialog = dialog.set_directory(root);
    }
    let Some(dir) = dialog.blocking_pick_folder() else { return Ok(None) };
    devices_of(&dir.into_path().map_err(|e| e.to_string())?).map(Some)
}

// --- Aperçu ---------------------------------------------------------------------------------------------------------

fn field_label(key: &str) -> &str {
    match key {
        "givenName" => "Prénom",
        "surname" => "Nom",
        "company" => "Société",
        "jobTitle" => "Fonction",
        "department" => "Service",
        "email1" => "E-mail",
        "email2" => "E-mail 2",
        "email3" => "E-mail 3",
        "mobile" => "Mobile",
        "business" => "Tél. professionnel",
        "homePhone" => "Tél. domicile",
        "workStreet" => "Rue (pro)",
        "workCity" => "Ville (pro)",
        "workPostal" => "Code postal (pro)",
        "workCountry" => "Pays (pro)",
        "homeStreet" => "Rue (domicile)",
        "homeCity" => "Ville (domicile)",
        "homePostal" => "Code postal (domicile)",
        "homeCountry" => "Pays (domicile)",
        "webPage" => "Site web",
        "birthday" => "Anniversaire",
        "notes" => "Notes",
        "subject" => "Objet",
        "location" => "Lieu",
        "start" => "Début",
        "end" => "Fin",
        "body" => "Texte",
        "categories" => "Catégories",
        "reminder" => "Rappel",
        "allDay" => "Journée entière",
        "busy" => "Disponibilité",
        "private" => "Confidentiel",
        "color" => "Couleur",
        other => other,
    }
}

/// Valeurs affichables d'un élément : champs simples seulement (ni identifiant, ni dossier, ni photo, ni série).
fn fields_of(payload: &Payload) -> BTreeMap<String, String> {
    let value = match payload {
        Payload::None => return BTreeMap::new(),
        Payload::Contact(f) => serde_json::to_value(f),
        Payload::Task(f) => serde_json::to_value(f),
        Payload::Note(f) => serde_json::to_value(f),
        Payload::Event(f) => serde_json::to_value(f),
    };
    let mut out = BTreeMap::new();
    if let Ok(serde_json::Value::Object(map)) = value {
        for (k, v) in map {
            if matches!(k.as_str(), "id" | "folderId" | "calendarId" | "photo" | "series") {
                continue;
            }
            let text = match v {
                serde_json::Value::Null => String::new(),
                serde_json::Value::String(s) => s,
                serde_json::Value::Bool(b) => (if b { "oui" } else { "non" }).to_string(),
                other => other.to_string(),
            };
            out.insert(k, text);
        }
    }
    out
}

fn current_payload(conn: &thunderlook_store::rusqlite::Connection, dir: &Path, item: ItemKind, id: &str) -> Option<Payload> {
    let num = |p: char| id.strip_prefix(p)?.parse::<i64>().ok();
    match item {
        ItemKind::Contact => thunderlook_contacts::write::form_of(conn, dir, num('p')?).ok().map(Payload::Contact),
        ItemKind::Note => thunderlook_items::note_form(conn, dir, num('n')?).ok().map(Payload::Note),
        ItemKind::Task => thunderlook_items::task_form(conn, dir, num('t')?).ok().map(Payload::Task),
        ItemKind::Event => {
            let n = num('a')?;
            let cal = thunderlook_calendar::load(conn).ok()?;
            let a = cal.appointments.iter().find(|a| a.id == n)?;
            let local = |ms: i64| thunderlook_calendar::view::local_now(ms).format("%Y-%m-%dT%H:%M").to_string();
            Some(Payload::Event(thunderlook_exchange::EventData {
                subject: a.subject.clone(),
                location: a.location.clone().unwrap_or_default(),
                start: local(a.start_ms),
                end: local(a.end_ms),
                all_day: a.all_day,
                body: thunderlook_calendar::description(a, dir).map(|b| b.trim_end().to_string()).unwrap_or_default(),
                categories: a.categories.join(", "),
                private: a.private,
                ..Default::default()
            }))
        }
    }
}

fn shorten(s: &str) -> String {
    let flat = s.replace(['\r', '\n'], " ");
    if flat.chars().count() > 120 { format!("{}…", flat.chars().take(120).collect::<String>()) } else { flat }
}

fn detail_row(conn: &thunderlook_store::rusqlite::Connection, dir: &Path, c: &Change) -> DetailRow {
    let shown = |m: BTreeMap<String, String>, old: bool| -> Vec<FieldDiff> {
        m.into_iter()
            .filter(|(_, v)| !v.is_empty() && v != "0")
            .map(|(k, v)| FieldDiff { label: field_label(&k).to_string(), old: if old { shorten(&v) } else { String::new() }, new: if old { String::new() } else { shorten(&v) } })
            .collect()
    };
    let fields = match c.kind {
        ChangeKind::New => shown(fields_of(&c.payload), false),
        ChangeKind::Deleted => shown(current_payload(conn, dir, c.item, &c.id).map(|p| fields_of(&p)).unwrap_or_default(), true),
        ChangeKind::Modified => {
            let old = current_payload(conn, dir, c.item, &c.id).map(|p| fields_of(&p)).unwrap_or_default();
            fields_of(&c.payload)
                .iter()
                // Un champ que la base n'expose pas (rappel, disponibilité d'un rendez-vous…) n'est pas comparable.
                .filter(|(k, v)| old.get(*k).is_some_and(|o| o != *v))
                .map(|(k, v)| FieldDiff { label: field_label(k).to_string(), old: shorten(&old[k]), new: shorten(v) })
                .collect()
        }
    };
    DetailRow {
        key: c.key.clone(),
        kind: c.kind,
        item: c.item,
        summary: c.summary.clone(),
        selected: c.default_selected(),
        conflict: c.conflict.map(|o| match o {
            Origin::Both => "both".to_string(),
            Origin::Unknown => "unknown".to_string(),
        }),
        fields,
    }
}

fn open_report(app: &AppHandle, device: &str) -> Result<(thunderlook_store::rusqlite::Connection, PathBuf, thunderlook_exchange::ImportReport), String> {
    let dir = data_dir(app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let rep = mpe_files::import_changes_mpe(&conn, &dir, Path::new(device), &mpe_files::MpeOptions::default())?;
    Ok((conn, dir, rep))
}

/// « Vérifier » : ce qui diffère entre les fichiers de MPE et ThunderLook. N'écrit rien, ni dans MPE ni dans la base.
#[tauri::command(async)]
pub fn mpe_import_check(app: AppHandle, device: String) -> Result<DetailCheck, String> {
    let _guard = lock()?;
    let (conn, dir, rep) = open_report(&app, &device)?;
    Ok(DetailCheck {
        changes: rep.changes.iter().map(|c| detail_row(&conn, &dir, c)).collect(),
        conflicts: rep.conflicts.iter().map(|c| detail_row(&conn, &dir, c)).collect(),
        unchanged: rep.unchanged,
        warnings: rep.warnings,
    })
}

/// « Appliquer » : `keys` = changements cochés, `choices` = conflits tranchés (`thunderlook` ou `phone` ; les autres sont
/// ignorés). Les fichiers sont relus : seuls les éléments qui existent encore sont appliqués. Annulable
/// (`phone_exchange_undo`).
#[tauri::command(async)]
pub fn mpe_import_apply(app: AppHandle, device: String, keys: Vec<String>, choices: Vec<ConflictChoice>) -> Result<ApplySummary, String> {
    let _guard = lock()?;
    let (mut conn, dir, rep) = open_report(&app, &device)?;
    let proposed = rep.changes.len() + rep.conflicts.len();
    let mut chosen: Vec<Change> = rep.changes.into_iter().filter(|c| keys.contains(&c.key)).collect();
    for mut c in rep.conflicts {
        c.resolution = match choices.iter().find(|x| x.key == c.key).map(|x| x.choice.as_str()) {
            Some("thunderlook") => Some(Resolution::KeepThunderlook),
            Some("phone") => Some(Resolution::KeepPhone),
            _ => None,
        };
        if c.resolution.is_some() {
            chosen.push(c);
        }
    }
    let mut sum = ApplySummary { ignored: proposed - chosen.len(), warnings: rep.warnings, ..Default::default() };
    if chosen.is_empty() {
        return Ok(sum);
    }
    let done = thunderlook_exchange::apply(&mut conn, &dir, &chosen)?;
    if let Some(cache) = app.try_state::<CalendarCache>() {
        let _ = clear_calendar(&cache);
    }
    let _ = app.emit("contacts-changed", ());
    let _ = app.emit("calendar-changed", ());
    sum.kept = done.kept;
    for c in chosen.iter().filter(|c| c.resolution != Some(Resolution::KeepThunderlook)) {
        if done.errors.iter().any(|e| e.starts_with(&format!("{} : ", c.summary))) {
            continue;
        }
        match c.kind {
            ChangeKind::New => sum.created += 1,
            ChangeKind::Modified => sum.modified += 1,
            ChangeKind::Deleted => sum.deleted += 1,
        }
    }
    sum.warnings.extend(done.errors);
    Ok(sum)
}
