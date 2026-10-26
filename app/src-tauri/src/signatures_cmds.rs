//! Signatures propres à ThunderLook (lot BE) : commandes de l'éditeur et du menu « Signature » de la rédaction.
//! Le travail est dans `thunderlook_mailbox::signature_store` ; rien n'est écrit dans le dossier d'Outlook.

use tauri::AppHandle;
use thunderlook_mailbox::StoredSignature;
use thunderlook_model::Signature;

use crate::data_dir;

/// Menu « Signature » de la rédaction : les signatures enregistrées, puis celles d'Outlook de nom différent.
pub fn menu_list(app: &AppHandle) -> Vec<Signature> {
    match data_dir(app) {
        Ok(dir) => thunderlook_mailbox::signatures_menu(&dir),
        Err(_) => thunderlook_mailbox::signatures(),
    }
}

/// Signatures enregistrées (erreur si `signatures.json` est illisible : il n'est alors pas écrasé).
#[tauri::command(async)]
pub fn signatures_stored(app: AppHandle) -> Result<Vec<StoredSignature>, String> {
    thunderlook_mailbox::signatures_load(&data_dir(&app)?)
}

/// Remplace la liste des signatures enregistrées.
#[tauri::command(async)]
pub fn signatures_store_save(app: AppHandle, list: Vec<StoredSignature>) -> Result<(), String> {
    thunderlook_mailbox::signatures_save(&data_dir(&app)?, &list)
}

/// Signatures du dossier d'Outlook, pour les copier dans le stockage de ThunderLook (lecture seule).
#[tauri::command(async)]
pub fn signatures_outlook() -> Vec<Signature> {
    thunderlook_mailbox::signatures()
}
