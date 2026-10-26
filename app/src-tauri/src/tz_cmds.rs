//! Lot AZ : fuseau horaire de l'utilisateur et second fuseau du calendrier, gardés dans `fuseaux.json` (dossier de
//! données) et appliqués au démarrage. Les fichiers échangés (`.ics`, MPE, CSV) restent en heure de Paris.

use std::fs;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use thunderlook_calendar::tz;

use crate::data_dir;

const FILE: &str = "fuseaux.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    primary: String,
    #[serde(default)]
    second: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneItem {
    id: &'static str,
    label: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneSettings {
    zones: Vec<ZoneItem>,
    primary: &'static str,
    second: Option<&'static str>,
}

/// Au démarrage : applique le réglage enregistré (Europe/Paris sans fichier ou si le fichier est illisible).
pub fn start(app: &AppHandle) {
    let Some(saved) = data_dir(app).ok().and_then(|d| fs::read_to_string(d.join(FILE)).ok()).and_then(|s| serde_json::from_str::<Saved>(&s).ok()) else {
        return;
    };
    if let Err(e) = tz::set_zones(&saved.primary, saved.second.as_deref()) {
        log::warn!("{FILE} : {e}");
    }
}

#[tauri::command]
pub fn time_zones() -> ZoneSettings {
    ZoneSettings {
        zones: tz::ZONES.iter().map(|z| ZoneItem { id: z.id, label: z.label }).collect(),
        primary: tz::primary_zone().id,
        second: tz::second_zone().map(|z| z.id),
    }
}

/// Change les fuseaux (second vide = aucun), les enregistre et fait recalculer la vue du calendrier.
#[tauri::command]
pub fn set_time_zones(app: AppHandle, primary: String, second: Option<String>) -> Result<(), String> {
    tz::set_zones(&primary, second.as_deref())?;
    let saved = Saved { primary, second: second.filter(|s| !s.is_empty()) };
    let dir = data_dir(&app)?;
    fs::write(dir.join(FILE), serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let _ = app.emit("calendar-changed", ());
    Ok(())
}
