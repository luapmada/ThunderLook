//! Contacts : commandes ajoutées à celles de `lib.rs` (listes de distribution, en lecture seule).

use tauri::AppHandle;
use thunderlook_model::contacts::ContactRow;

use crate::data_dir;

/// Listes de distribution (`IPM.DistList`) d'un dossier de contacts (`k<id>`), en lignes `d<id>` avec leurs membres.
#[tauri::command(async)]
pub fn contact_dist_lists(app: AppHandle, folder: String) -> Result<Vec<ContactRow>, String> {
    let id: i64 = folder.strip_prefix('k').and_then(|f| f.parse().ok()).ok_or("dossier de contacts inconnu")?;
    let conn = thunderlook_store::open(&data_dir(&app)?).map_err(|e| e.to_string())?;
    thunderlook_contacts::view::dist_rows(&conn, id)
}
