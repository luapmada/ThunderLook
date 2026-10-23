//! Commandes Tauri du courrier propres au lot J : enregistrer toutes les pièces jointes d'un message.

use std::fs;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::{Store, safe_file_name, with_mailbox};

/// « Enregistrer toutes les pièces jointes » : un dossier choisi par l'utilisateur, un fichier par pièce jointe
/// (noms rendus uniques). Renvoie le nombre de fichiers écrits, 0 si l'utilisateur annule.
#[tauri::command(async)]
pub fn save_all_attachments(app: AppHandle, store: State<Store>, id: String) -> Result<u32, String> {
    let count = with_mailbox(&app, &store, |mb| Ok(mb.message_view(&id, false)?.attachments.len()))?;
    if count == 0 {
        return Ok(0);
    }
    let Some(dir) = app.dialog().file().blocking_pick_folder() else { return Ok(0) };
    let dir = dir.into_path().map_err(|e| e.to_string())?;
    let atts = with_mailbox(&app, &store, |mb| (0..count as u32).map(|i| mb.attachment(&id, i)).collect::<Result<Vec<_>, _>>())?;
    let names: Vec<String> = atts.iter().map(|a| safe_file_name(&a.name)).collect();
    for (att, name) in atts.iter().zip(thunderlook_mailbox::unique_names(&names)) {
        let path = dir.join(name);
        fs::write(&path, &att.bytes).map_err(|e| format!("{} : {e}", path.display()))?;
    }
    Ok(atts.len() as u32)
}
