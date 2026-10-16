//! Cœur de ThunderLook (Tauri). J0 : données de démonstration et réglages d'affichage.

use std::{fs, path::PathBuf};

use tauri::{AppHandle, Manager};
use thunderlook_model::{MailSnapshot, MessageBody, demo, settings::Settings};

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

#[tauri::command]
fn mail_snapshot() -> MailSnapshot {
    demo::snapshot(chrono::Local::now())
}

#[tauri::command]
fn message_body(id: String) -> Result<MessageBody, String> {
    demo::body(&id).ok_or_else(|| format!("Message introuvable : {id}"))
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Settings {
    settings_path(&app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .unwrap_or_default()
        .clamped()
}

/// Écriture atomique : fichier temporaire puis renommage.
#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let path = settings_path(&app)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(&settings.clamped()).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![mail_snapshot, message_body, load_settings, save_settings])
        .run(tauri::generate_context!())
        .expect("échec du démarrage de ThunderLook");
}
