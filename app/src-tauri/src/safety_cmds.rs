//! Annulation (Ctrl+Z) et sauvegardes : commandes de l'écran « Sauvegardes » et tâche de lancement (purge de la corbeille
//! interne, sauvegarde du jour). Tout le travail est dans `thunderlook-safety`.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
use std::time::Duration;

use thunderlook_safety::{KEEP_DAILY, KEEP_DB, Mode, TRASH_DAYS};

use crate::{CalendarCache, clear_calendar, data_dir};

/// Au lancement, hors du fil principal : purge de la corbeille (60 jours) puis sauvegardes (éléments du jour, copie de la base
/// une fois par semaine). Elles attendent que la base soit au repos plutôt qu'un délai fixe : au moins 20 s après le
/// lancement, puis 10 s sans écriture d'une autre connexion (indexation, import), au plus 15 min d'attente.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Ok(dir) = data_dir(&app) else { return };
        thunderlook_safety::wait_idle(&dir, Duration::from_secs(20), Duration::from_secs(10), Duration::from_secs(15 * 60));
        let purged = thunderlook_safety::purge(&dir, TRASH_DAYS);
        if purged > 0 {
            log::info!("corbeille interne : {purged} instantané(s) de plus de {TRASH_DAYS} jours supprimé(s)");
        }
        match run(&dir) {
            Ok(done) => done.iter().for_each(|f| log::info!("sauvegarde : {}", f.display())),
            Err(e) => log::warn!("sauvegarde impossible : {e}"),
        }
    });
}

/// Sauvegarde des éléments du jour puis, si la dernière a plus de 7 jours, copie contrôlée de la base. Rend les fichiers écrits ;
/// la première erreur est consignée (écran « Sauvegardes ») jusqu'à la prochaine réussite.
fn run(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut done = Vec::new();
    let items = thunderlook_safety::backup(dir, KEEP_DAILY);
    let base = thunderlook_safety::backup_db(dir, KEEP_DB);
    done.extend(items.as_ref().ok().and_then(Option::as_ref).cloned());
    done.extend(base.as_ref().ok().and_then(Option::as_ref).cloned());
    match (items, base) {
        (Ok(_), Ok(_)) => Ok(done),
        (Err(e), _) => Err(e),
        (_, Err(e)) => {
            thunderlook_safety::record_failure(dir, &e);
            Err(e)
        }
    }
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
    /// Dernier échec de sauvegarde, tant qu'aucune n'a réussi depuis.
    pub error: Option<String>,
}

#[tauri::command(async)]
pub fn backups_list(app: AppHandle) -> Result<Vec<BackupRow>, String> {
    let dir = data_dir(&app)?;
    let error = thunderlook_safety::last_failure(&dir);
    let mut rows: Vec<BackupRow> = thunderlook_safety::list_backups(&dir)
        .into_iter()
        .map(|b| BackupRow { name: b.name, weekly: b.weekly, size: b.size, count: b.count, created: b.created, error: error.clone() })
        .collect();
    // Sans aucune sauvegarde, l'échec doit quand même se voir : ligne sans nom, que l'écran n'affiche pas comme sauvegarde.
    if rows.is_empty() && error.is_some() {
        rows.push(BackupRow { name: String::new(), weekly: false, size: 0, count: 0, created: String::new(), error });
    }
    Ok(rows)
}

/// Sauvegarde à la demande ; sans effet pour ce qui existe déjà (éléments du jour, copie de la base de moins de 7 jours).
#[tauri::command(async)]
pub fn backup_now(app: AppHandle) -> Result<String, String> {
    let dir = data_dir(&app)?;
    // Dans un fil à part (lecture seule, basse priorité) : la commande attend sa fin pour répondre, l'interface non.
    let done = std::thread::spawn(move || run(&dir)).join().map_err(|_| "la sauvegarde s'est arrêtée brutalement".to_string())??;
    Ok(if done.is_empty() { "Rien de nouveau : la sauvegarde d'aujourd'hui existe déjà".to_string() } else { format!("Sauvegarde faite ({} fichier(s))", done.len()) })
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
