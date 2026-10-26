//! Listes de distribution : création, modification, suppression (lot AX). La lecture reste dans `contacts_cmds.rs`.

use tauri::{AppHandle, Emitter};
use thunderlook_model::contacts::DistMember;

use crate::data_dir;

fn list_id(id: &str) -> Result<i64, String> {
    id.strip_prefix('d').and_then(|f| f.parse().ok()).ok_or_else(|| "liste de distribution inconnue".to_string())
}

/// Enregistre la liste `id` (`d<id>`, absent pour une nouvelle) du dossier `folder` (`k<id>`) ; rend son identifiant.
/// Une modification est annulable (Ctrl+Z), comme pour un contact.
#[tauri::command(async)]
pub fn contact_dist_list_save(app: AppHandle, folder: String, id: Option<String>, name: String, members: Vec<DistMember>) -> Result<String, String> {
    let folder: i64 = folder.strip_prefix('k').and_then(|f| f.parse().ok()).ok_or("choisissez un dossier de contacts")?;
    let dir = data_dir(&app)?;
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let id = id.as_deref().map(list_id).transpose()?;
    if let Some(n) = id {
        thunderlook_safety::snapshot_edit(&conn, &dir, n)?;
    }
    let n = thunderlook_contacts::distlist::save(&mut conn, folder, id, &name, &members)?;
    let _ = app.emit("contacts-changed", ());
    Ok(format!("d{n}"))
}

/// Supprime la liste `d<id>` (annulable).
#[tauri::command(async)]
pub fn contact_dist_list_delete(app: AppHandle, id: String) -> Result<(), String> {
    let n = list_id(&id)?;
    let mut conn = thunderlook_store::open(&data_dir(&app)?).map_err(|e| e.to_string())?;
    thunderlook_contacts::write::delete(&mut conn, n)?;
    let _ = app.emit("contacts-changed", ());
    Ok(())
}
