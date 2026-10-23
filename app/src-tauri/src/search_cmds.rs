//! Recherche globale : rendez-vous, contacts, tâches et notes (le courrier a son propre index, voir `search`).
//! Le calcul est dans `thunderlook-search`.

use tauri::AppHandle;
use thunderlook_search::{Hit, Scope};

use crate::data_dir;

/// Éléments (hors courrier) dont un mot de `text` se retrouve ; au plus [`thunderlook_search::DEFAULT_LIMIT`].
#[tauri::command(async)]
pub fn global_search(app: AppHandle, text: String) -> Result<Vec<Hit>, String> {
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    Ok(thunderlook_search::query(&conn, &dir, &text, Scope::All))
}
