//! Annulation (Ctrl+Z) et sauvegardes : commandes de l'écran « Sauvegardes » et tâche de lancement (purge de la corbeille
//! interne, sauvegarde du jour). Tout le travail est dans `thunderlook-safety`.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
use thunderlook_safety::{KEEP_DAILY, Mode, TRASH_DAYS};

use crate::{CalendarCache, clear_calendar, data_dir};

/// Au lancement, hors du fil principal : purge de la corbeille (60 jours) puis sauvegarde du jour (une seule par jour).
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        // Après le chargement de l'interface. La sauvegarde est légère (incrémentale, lecture seule, par paquets, pauses) mais
        // reste différée : le démarrage a la priorité.
        std::thread::sleep(std::time::Duration::from_secs(240));
        let Ok(dir) = data_dir(&app) else { return };
        let purged = thunderlook_safety::purge(&dir, TRASH_DAYS);
        if purged > 0 {
            log::info!("corbeille interne : {purged} instantané(s) de plus de {TRASH_DAYS} jours supprimé(s)");
        }
        match thunderlook_safety::backup(&dir, KEEP_DAILY) {
            Ok(Some(file)) => log::info!("sauvegarde du jour : {}", file.display()),
            Ok(None) => {}
            Err(e) => log::warn!("sauvegarde du jour impossible : {e}"),
        }
    });
}

/// Les listes ouvertes se rechargent (calendrier, contacts ; les tâches et notes sont rechargées par l'interface).
fn changed(app: &AppHandle) {
    if let Ok(()) = clear_calendar(&app.state::<CalendarCache>()) {
        let _ = app.emit("calendar-changed", ());
    }
    let _ = app.emit("contacts-changed", ());
}

/// Ctrl+Z / bouton « Annuler » des modules Calendrier, Contacts, Tâches et Notes : défait la dernière suppression ou
/// modification. Rend le message de la barre d'état.
#[tauri::command(async)]
pub fn safety_undo(app: AppHandle) -> Result<String, String> {
    let dir = data_dir(&app)?;
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let msg = thunderlook_safety::undo(&mut conn, &dir)?;
    changed(&app);
    Ok(msg)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRow {
    pub name: String,
    pub weekly: bool,
    pub size: u64,
    pub count: usize,
    pub created: String,
}

#[tauri::command(async)]
pub fn backups_list(app: AppHandle) -> Result<Vec<BackupRow>, String> {
    let dir = data_dir(&app)?;
    Ok(thunderlook_safety::list_backups(&dir)
        .into_iter()
        .map(|b| BackupRow { name: b.name, weekly: b.weekly, size: b.size, count: b.count, created: b.created })
        .collect())
}

/// Sauvegarde du jour à la demande ; sans effet si elle existe déjà.
#[tauri::command(async)]
pub fn backup_now(app: AppHandle) -> Result<String, String> {
    let dir = data_dir(&app)?;
    // Dans un fil à part (lecture seule, basse priorité) : la commande attend sa fin pour répondre, l'interface non.
    let done = thunderlook_safety::spawn_backup(dir, KEEP_DAILY).join().map_err(|_| "la sauvegarde s'est arrêtée brutalement".to_string())?;
    Ok(match done? {
        Some(_) => "Sauvegarde faite".to_string(),
        None => "La sauvegarde d'aujourd'hui existe déjà".to_string(),
    })
}

/// `mode` : « merge » (éléments manquants seulement) ou « all » (tout remettre ; l'état actuel va en corbeille interne).
#[tauri::command(async)]
pub fn backup_restore(app: AppHandle, name: String, mode: String) -> Result<String, String> {
    let dir = data_dir(&app)?;
    // Le nom vient de la liste : jamais un chemin.
    let info = thunderlook_safety::list_backups(&dir).into_iter().find(|b| b.name == name).ok_or("sauvegarde introuvable")?;
    let mode = match mode.as_str() {
        "merge" => Mode::Merge,
        "all" => Mode::All,
        _ => return Err("mode inconnu".into()),
    };
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let r = thunderlook_safety::restore_backup(&mut conn, &dir, &info.path, mode)?;
    changed(&app);
    let mut msg = format!("Restauration : {} remis, {} remplacé(s), {} déjà présent(s)", r.added, r.replaced, r.skipped);
    if !r.failed.is_empty() {
        msg.push_str(&format!(" — {} non remis : {}", r.failed.len(), r.failed.iter().take(3).cloned().collect::<Vec<_>>().join(" ; ")));
    }
    Ok(msg)
}

#[tauri::command(async)]
pub fn backups_open_folder(app: AppHandle) -> Result<(), String> {
    let dir = thunderlook_safety::backups_dir(&data_dir(&app)?);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}
