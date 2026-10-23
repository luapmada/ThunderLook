//! Options (Fichier › Options) : lecture des réglages côté Tauri, pour les commandes qui en dépendent
//! (valeurs par défaut d'un nouveau rendez-vous…). L'écriture reste `save_settings` (lib.rs).

use std::fs;

use tauri::AppHandle;
use thunderlook_model::settings::Settings;

use crate::settings_path;

/// Réglages enregistrés ; valeurs par défaut si le fichier manque ou est illisible.
pub fn load(app: &AppHandle) -> Settings {
    settings_path(app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .unwrap_or_default()
        .clamped()
}
