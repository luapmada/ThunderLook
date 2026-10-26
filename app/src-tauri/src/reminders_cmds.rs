//! Rappels : commandes de l'interface et fil qui, chaque minute, signale les rappels échus (événement `reminders-due`
//! et notification Windows). Le calcul est dans `thunderlook-reminders`.

use std::{collections::HashSet, time::Duration};

use tauri::{AppHandle, Emitter};
use thunderlook_reminders::{Reminder, Window};

use crate::{data_dir, now_ms, toast};

fn compute(app: &AppHandle) -> Result<Vec<Reminder>, String> {
    let dir = data_dir(app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    thunderlook_reminders::due(&conn, &dir, now_ms(), Window::default())
}

/// Rappels à échéance (dernières 24 h, hors ignorés et reportés).
#[tauri::command(async)]
pub fn reminders_due(app: AppHandle) -> Result<Vec<Reminder>, String> {
    compute(&app)
}

#[tauri::command(async)]
pub fn reminder_dismiss(app: AppHandle, id: String) -> Result<(), String> {
    thunderlook_reminders::dismiss(&data_dir(&app)?, &id)
}

/// Reporte le rappel à `until_ms` (millisecondes Unix).
#[tauri::command(async)]
pub fn reminder_snooze(app: AppHandle, id: String, until_ms: i64) -> Result<(), String> {
    thunderlook_reminders::snooze(&data_dir(&app)?, &id, until_ms)
}

fn clock(ms: i64) -> String {
    let local = thunderlook_calendar::tz::TimeZone::local().to_local(chrono::DateTime::from_timestamp_millis(ms).map(|t| t.naive_utc()).unwrap_or_default());
    local.format("%H:%M").to_string()
}

fn notify(r: &Reminder) {
    let when = match &r.location {
        Some(l) if !l.trim().is_empty() => format!("{} · {l}", clock(r.event_start_ms)),
        _ => clock(r.event_start_ms),
    };
    let title = if r.title.trim().is_empty() { "(aucun objet)" } else { r.title.as_str() };
    // Pas de lien d'ouverture propre aux rappels : un clic ramène ThunderLook au premier plan.
    if let Err(e) = toast::show("Rappel", &[title, &when], &format!("{}:reminders", toast::SCHEME)) {
        log::warn!("{e}");
    }
}

/// Fil des rappels : chaque minute, émet `reminders-due` avec les rappels pas encore signalés depuis le démarrage.
/// Un rappel reporté puis échu est signalé de nouveau (clé = identifiant et heure de rappel effective, voir `seen`).
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(5));
        let mut seen: HashSet<String> = HashSet::new();
        loop {
            match compute(&app) {
                Ok(list) => {
                    // Les rappels reportés puis échus n'ont plus d'entrée ignorée au même instant : ils sortent de
                    // `list`, puis y reviennent ; on oublie ceux qui n'y sont plus pour les resignaler.
                    seen.retain(|id| list.iter().any(|r| &r.id == id));
                    let fresh: Vec<Reminder> = list.into_iter().filter(|r| seen.insert(r.id.clone())).collect();
                    if !fresh.is_empty() {
                        fresh.iter().for_each(notify);
                        let _ = app.emit("reminders-due", &fresh);
                    }
                }
                Err(e) => log::warn!("rappels : {e}"),
            }
            std::thread::sleep(Duration::from_secs(60));
        }
    });
}
