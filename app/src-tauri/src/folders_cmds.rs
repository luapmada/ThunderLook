//! Dossiers de rendez-vous, contacts, tâches et notes : création, renommage, suppression, couleur des calendriers.
//! Tout le travail est dans `thunderlook_folders`.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use thunderlook_folders::{Content, Kind};

use crate::{CalendarCache, clear_calendar, data_dir};

/// Couleurs personnalisées des calendriers : `CalendarInfo::color` vaut `CUSTOM_COLOR_BASE + indice de palette (0 à 24)`.
pub const CUSTOM_COLOR_BASE: u8 = 8;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderRow {
    /// `c<id>` pour un calendrier, `k<id>` pour un dossier de contacts, `f<id>` pour tâches et notes.
    pub id: String,
    pub source_id: i64,
    /// Nom affiché (lisible, distinct des autres dossiers de la même sorte).
    pub name: String,
    /// Nom enregistré, proposé au renommage.
    pub raw_name: String,
    pub section: String,
    pub count: u32,
    /// Teinte propre au fichier de données (0 à 7).
    pub account: u8,
    /// Palette 0 à 24 (calendriers).
    pub color: Option<u8>,
}

fn prefix(kind: Kind) -> char {
    match kind {
        Kind::Calendar => 'c',
        Kind::Contact => 'k',
        Kind::Task | Kind::Note => 'f',
    }
}

fn num(id: &str) -> Result<i64, String> {
    id.get(1..).and_then(|n| n.parse().ok()).ok_or_else(|| "Dossier inconnu.".to_string())
}

fn changed(app: &AppHandle, kind: Kind) {
    match kind {
        Kind::Calendar => {
            if let Ok(()) = clear_calendar(&app.state::<CalendarCache>()) {
                let _ = app.emit("calendar-changed", ());
            }
        }
        Kind::Contact => {
            let _ = app.emit("contacts-changed", ());
        }
        Kind::Task | Kind::Note => {}
    }
}

/// Tous les dossiers de cette sorte, y compris les vides.
#[tauri::command(async)]
pub fn item_folders(app: AppHandle, kind: Kind) -> Result<Vec<FolderRow>, String> {
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let colors = thunderlook_folders::colors(&dir);
    let p = prefix(kind);
    Ok(thunderlook_folders::list(&conn, kind)?
        .into_iter()
        .map(|f| FolderRow { color: colors.get(&format!("c{}", f.id)).copied(), id: format!("{p}{}", f.id), source_id: f.source_id, name: f.name, raw_name: f.raw_name, section: f.section, count: f.count, account: f.account })
        .collect())
}

/// Crée un dossier ; rend son identifiant.
#[tauri::command(async)]
pub fn item_folder_create(app: AppHandle, kind: Kind, source_id: Option<i64>, name: String) -> Result<String, String> {
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let id = thunderlook_folders::create(&conn, kind, source_id, &name)?;
    changed(&app, kind);
    Ok(format!("{}{id}", prefix(kind)))
}

#[tauri::command(async)]
pub fn item_folder_rename(app: AppHandle, kind: Kind, id: String, name: String) -> Result<(), String> {
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    thunderlook_folders::rename(&conn, kind, num(&id)?, &name)?;
    changed(&app, kind);
    Ok(())
}

/// `content` : `{ "mode": "refuse" }`, `{ "mode": "moveTo", "folder": <id numérique> }` ou `{ "mode": "delete" }`.
/// Rend le nombre d'éléments déplacés ou supprimés.
#[tauri::command(async)]
pub fn item_folder_delete(app: AppHandle, kind: Kind, id: String, content: Content) -> Result<u32, String> {
    let dir = data_dir(&app)?;
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let n = thunderlook_folders::delete(&mut conn, &dir, kind, num(&id)?, content)?;
    changed(&app, kind);
    Ok(n)
}

/// Couleur d'un calendrier (palette 0 à 24) ; absente = automatique.
#[tauri::command(async)]
pub fn calendar_folder_color(app: AppHandle, id: String, color: Option<u8>) -> Result<(), String> {
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    thunderlook_folders::set_color(&conn, &dir, num(&id)?, color)?;
    changed(&app, Kind::Calendar);
    Ok(())
}
