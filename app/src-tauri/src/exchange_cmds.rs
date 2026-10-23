//! Échange avec le téléphone : commandes de l'écran « Échange avec le téléphone » et export automatique optionnel.
//! Tout le travail est dans `thunderlook-exchange` ; ici, le choix du dossier d'échange (`exchange.json`, à côté de
//! `settings.json`), les verrous et les événements de rechargement.

use std::{fs, path::PathBuf, sync::Mutex, time::{Duration, Instant}};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use thunderlook_exchange::{ChangeKind, ItemKind};

use crate::{CalendarCache, clear_calendar, data_dir, settings_path};

/// Un seul export, relecture ou application à la fois (l'export automatique tourne dans son propre fil).
static BUSY: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExchangeSettings {
    /// Dossier d'échange choisi ; vide : aucun.
    pub dir: String,
    /// Export automatique toutes les N minutes ; 0 : désactivé (valeur par défaut).
    pub auto_minutes: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeState {
    pub settings: ExchangeSettings,
    /// Horodatage (UTC) du dernier export présent dans le dossier.
    pub last_export: Option<String>,
    /// Un import appliqué peut être annulé.
    pub can_undo: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRow {
    pub key: String,
    pub kind: ChangeKind,
    pub item: ItemKind,
    pub summary: String,
    /// Coché d'office : jamais une suppression.
    pub selected: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub changes: Vec<ChangeRow>,
    pub unchanged: usize,
    pub warnings: Vec<String>,
}

fn settings_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(settings_path(app)?.with_file_name("exchange.json"))
}

fn read_settings(app: &AppHandle) -> ExchangeSettings {
    settings_file(app).ok().and_then(|p| fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn write_settings(app: &AppHandle, s: &ExchangeSettings) -> Result<(), String> {
    let path = settings_file(app)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&path, serde_json::to_string_pretty(s).map_err(|e| e.to_string())?).map_err(|e| format!("{} : {e}", path.display()))
}

fn exchange_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let s = read_settings(app);
    if s.dir.trim().is_empty() {
        return Err("Choisissez d'abord le dossier d'échange.".into());
    }
    Ok(PathBuf::from(s.dir))
}

fn state_of(app: &AppHandle) -> Result<ExchangeState, String> {
    let settings = read_settings(app);
    let last_export = (!settings.dir.is_empty()).then(|| thunderlook_exchange::read_manifest(std::path::Path::new(&settings.dir))).flatten().map(|m| m.exported_at);
    Ok(ExchangeState { settings, last_export, can_undo: thunderlook_exchange::last_undo(&data_dir(app)?).is_some() })
}

fn lock() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    BUSY.lock().map_err(|_| "verrou de l'échange empoisonné".to_string())
}

/// Les éléments ont changé : les listes de l'interface se rechargent.
fn notify_changed(app: &AppHandle) {
    if let Some(cache) = app.try_state::<CalendarCache>() {
        let _ = clear_calendar(&cache);
    }
    let _ = app.emit("contacts-changed", ());
    let _ = app.emit("calendar-changed", ());
}

#[tauri::command(async)]
pub fn phone_exchange_state(app: AppHandle) -> Result<ExchangeState, String> {
    state_of(&app)
}

/// « Choisir le dossier… » ; `None` si l'utilisateur annule.
#[tauri::command(async)]
pub fn phone_exchange_pick(app: AppHandle) -> Result<Option<ExchangeState>, String> {
    let Some(dir) = app.dialog().file().blocking_pick_folder() else { return Ok(None) };
    let dir = dir.into_path().map_err(|e| e.to_string())?;
    let mut s = read_settings(&app);
    s.dir = dir.to_string_lossy().into_owned();
    write_settings(&app, &s)?;
    state_of(&app).map(Some)
}

/// Export automatique : 0 (désactivé) ou de 1 à 1 440 minutes.
#[tauri::command(async)]
pub fn phone_exchange_set_auto(app: AppHandle, minutes: u32) -> Result<ExchangeState, String> {
    let mut s = read_settings(&app);
    s.auto_minutes = minutes.min(1_440);
    write_settings(&app, &s)?;
    state_of(&app)
}

/// « Exporter maintenant ». Refusé si le téléphone a modifié des fichiers depuis le dernier export (ils seraient
/// écrasés) tant que `force` n'est pas vrai.
#[tauri::command(async)]
pub fn phone_exchange_export(app: AppHandle, force: bool) -> Result<String, String> {
    let out = exchange_dir(&app)?;
    let _guard = lock()?;
    let touched = thunderlook_exchange::modified_files(&out);
    if !touched.is_empty() && !force {
        return Err(format!("MODIFIED:{}", touched.join(", ")));
    }
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let report = thunderlook_exchange::export_all(&conn, &dir, &out)?;
    let total: usize = report.files.iter().map(|f| f.count).sum();
    Ok(format!("{total} élément(s) exportés dans {}", out.display()))
}

/// « Vérifier les changements » : ce qui diffère entre les fichiers du dossier et la base. N'écrit rien.
#[tauri::command(async)]
pub fn phone_exchange_check(app: AppHandle) -> Result<CheckResult, String> {
    let inp = exchange_dir(&app)?;
    let _guard = lock()?;
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let rep = thunderlook_exchange::import_changes(&conn, &dir, &inp)?;
    let changes = rep.changes.iter().map(|c| ChangeRow { key: c.key.clone(), kind: c.kind, item: c.item, summary: c.summary.clone(), selected: c.default_selected() }).collect();
    Ok(CheckResult { changes, unchanged: rep.unchanged, warnings: rep.warnings })
}

/// « Appliquer » les changements cochés (`keys`). La liste est relue : seuls les changements qui existent encore sont appliqués.
#[tauri::command(async)]
pub fn phone_exchange_apply(app: AppHandle, keys: Vec<String>) -> Result<String, String> {
    let inp = exchange_dir(&app)?;
    let _guard = lock()?;
    let dir = data_dir(&app)?;
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let rep = thunderlook_exchange::import_changes(&conn, &dir, &inp)?;
    let chosen: Vec<_> = rep.changes.into_iter().filter(|c| keys.contains(&c.key)).collect();
    if chosen.is_empty() {
        return Ok("Rien à appliquer.".into());
    }
    let done = thunderlook_exchange::apply(&mut conn, &dir, &chosen)?;
    notify_changed(&app);
    let mut msg = format!("{} changement(s) appliqué(s)", done.applied);
    if !done.errors.is_empty() {
        msg.push_str(&format!(", {} échec(s) : {}", done.errors.len(), done.errors.join(" ; ")));
    }
    Ok(msg)
}

/// « Annuler le dernier import ».
#[tauri::command(async)]
pub fn phone_exchange_undo(app: AppHandle) -> Result<String, String> {
    let _guard = lock()?;
    let dir = data_dir(&app)?;
    let stamp = thunderlook_exchange::last_undo(&dir).ok_or("Aucun import à annuler.")?;
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let result = thunderlook_exchange::undo(&mut conn, &dir, &stamp);
    notify_changed(&app);
    result.map(|n| format!("Import annulé : {n} élément(s) rétabli(s)"))
}

/// Export automatique (désactivé par défaut) : toutes les `auto_minutes` minutes tant que le téléphone n'a pas modifié
/// les fichiers du dossier ; sinon rien n'est écrasé et il faut les relire (« Vérifier les changements »).
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(20));
        let mut last = Instant::now();
        let mut was_on = false;
        loop {
            let s = read_settings(&app);
            let on = s.auto_minutes > 0 && !s.dir.trim().is_empty();
            // Activé à l'instant : premier export après N minutes, pas tout de suite.
            if on && !was_on {
                last = Instant::now();
            }
            was_on = on;
            if on && last.elapsed() >= Duration::from_secs(u64::from(s.auto_minutes) * 60) {
                last = Instant::now();
                let out = PathBuf::from(&s.dir);
                if let Ok(_guard) = lock() {
                    let touched = thunderlook_exchange::modified_files(&out);
                    if touched.is_empty() {
                        let result = data_dir(&app).and_then(|dir| {
                            let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
                            thunderlook_exchange::export_all(&conn, &dir, &out)
                        });
                        if let Err(e) = result {
                            log::warn!("échange téléphone (export automatique) : {e}");
                        }
                    } else {
                        log::warn!("échange téléphone : export automatique suspendu, fichiers modifiés par le téléphone : {}", touched.join(", "));
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(30));
        }
    });
}
