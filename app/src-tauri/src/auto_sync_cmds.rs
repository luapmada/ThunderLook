//! Synchronisation automatique avec le dossier d'échange (lot AM) : réglages, passage immédiat, annulation, journal,
//! et la tâche de fond. Tout le travail est dans `thunderlook_exchange::auto` ; ici, le verrou commun avec le passage
//! manuel (`exchange_cmds::BUSY`), le dossier d'échange (`exchange.json`) et les événements de rechargement.
//! Aucune écriture dans Outlook, aucun contrôle de MPE : on lit un dossier de fichiers.

use std::{
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use thunderlook_exchange::auto::{self, AutoState, Outcome, Params, PassReport, Settings};

use crate::{CalendarCache, clear_calendar, data_dir, exchange_cmds::BUSY, settings_path};

/// Délai avant de réessayer quand le passage n'a rien pu lire (fichiers instables, passage manuel en cours).
const RETRY: Duration = Duration::from_secs(120);

/// Passage immédiat demandé par l'utilisateur : la tâche de fond le prend au prochain tour.
static RUN_NOW: Mutex<bool> = Mutex::new(false);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoView {
    #[serde(flatten)]
    pub state: AutoState,
    /// Un dossier d'échange est choisi.
    pub dir_set: bool,
    /// Le dernier instantané peut être annulé.
    pub can_undo: bool,
}

fn exchange_dir(app: &AppHandle) -> Option<PathBuf> {
    let path = settings_path(app).ok()?.with_file_name("exchange.json");
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
    let dir = v.get("dir")?.as_str()?.trim().to_string();
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

fn view(app: &AppHandle) -> Result<AutoView, String> {
    let dir = data_dir(app)?;
    let state = auto::read_state(&dir);
    let can_undo = state.last.as_ref().and_then(|l| l.undo.as_ref()).is_some_and(|u| dir.join("exchange-undo").join(format!("{u}.json")).is_file());
    Ok(AutoView { state, dir_set: exchange_dir(app).is_some(), can_undo })
}

fn notify_changed(app: &AppHandle) {
    if let Some(cache) = app.try_state::<CalendarCache>() {
        let _ = clear_calendar(&cache);
    }
    let _ = app.emit("contacts-changed", ());
    let _ = app.emit("calendar-changed", ());
}

#[tauri::command(async)]
pub fn auto_sync_state(app: AppHandle) -> Result<AutoView, String> {
    view(&app)
}

/// Réglages : l'intervalle est ramené entre 5 et 120 min.
#[tauri::command(async)]
pub fn auto_sync_set(app: AppHandle, enabled: bool, minutes: u32, auto_export: bool) -> Result<AutoView, String> {
    auto::set_settings(&data_dir(&app)?, Settings { enabled, minutes, auto_export })?;
    view(&app)
}

/// Journal des passages, du plus récent au plus ancien.
#[tauri::command(async)]
pub fn auto_sync_journal(app: AppHandle) -> Result<Vec<PassReport>, String> {
    Ok(auto::read_journal(&data_dir(&app)?))
}

/// « Relire maintenant » : prochain tour de la tâche de fond (mêmes garde-fous qu'un passage programmé).
#[tauri::command(async)]
pub fn auto_sync_run_now() -> Result<(), String> {
    *RUN_NOW.lock().map_err(|_| "verrou empoisonné".to_string())? = true;
    Ok(())
}

/// Annule l'instantané du dernier passage automatique.
#[tauri::command(async)]
pub fn auto_sync_undo(app: AppHandle) -> Result<String, String> {
    let dir = data_dir(&app)?;
    let stamp = auto::read_state(&dir).last.and_then(|l| l.undo).ok_or("Aucun passage automatique à annuler.")?;
    let _guard = BUSY.lock().map_err(|_| "verrou de l'échange empoisonné".to_string())?;
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let result = thunderlook_exchange::undo(&mut conn, &dir, &stamp);
    notify_changed(&app);
    let msg = match &result {
        Ok(n) => format!("Passage automatique annulé : {n} élément(s) rétabli(s)"),
        Err(e) => format!("Annulation incomplète : {e}"),
    };
    let _ = auto::mark_undone(&dir, &stamp, &msg);
    result.map(|_| msg)
}

/// Un passage, sous le verrou commun. `None` : le passage manuel tient le verrou, on réessaie plus tard.
fn run_once(app: &AppHandle, settings: &Settings, exchange: &std::path::Path) -> Option<PassReport> {
    let _guard = BUSY.try_lock().ok()?;
    let dir = data_dir(app).ok()?;
    let report = match thunderlook_store::open(&dir) {
        Ok(mut conn) => {
            let p = Params { auto_export: settings.auto_export, ..Params::default() };
            let (report, saved) = auto::run_and_record(&mut conn, &dir, exchange, &p);
            if let Err(e) = saved {
                log::warn!("synchronisation automatique : état non enregistré : {e}");
            }
            report
        }
        Err(e) => {
            let report = PassReport { outcome: Outcome::Failed(e.to_string()), ..Default::default() };
            let _ = auto::record(&dir, &report);
            report
        }
    };
    if report.created + report.modified > 0 {
        notify_changed(app);
    }
    let _ = app.emit("auto-sync", ());
    Some(report)
}

/// Tâche de fond : un passage toutes les `minutes` (5 à 120, 60 par défaut) tant que c'est activé et qu'un dossier est choisi.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(45));
        let mut due = Instant::now();
        let mut was_on = false;
        loop {
            let settings = data_dir(&app).map(|d| auto::read_state(&d).settings.clamped()).unwrap_or_default();
            let exchange = exchange_dir(&app);
            let on = settings.enabled && exchange.is_some();
            let interval = Duration::from_secs(u64::from(settings.minutes) * 60);
            // Activé à l'instant : premier passage après un intervalle, pas tout de suite (sauf « Relire maintenant »).
            if on && !was_on {
                due = Instant::now() + interval;
            }
            was_on = on;
            let now_requested = RUN_NOW.lock().map(|mut f| std::mem::take(&mut *f)).unwrap_or(false);
            if let (Some(exchange), true) = (&exchange, now_requested || (on && Instant::now() >= due)) {
                let next = match run_once(&app, &settings, exchange) {
                    Some(r) if matches!(r.outcome, Outcome::Skipped(_) | Outcome::Failed(_)) => RETRY.min(interval),
                    Some(_) => interval,
                    None => RETRY.min(interval),
                };
                due = Instant::now() + next;
            }
            std::thread::sleep(Duration::from_secs(10));
        }
    });
}
