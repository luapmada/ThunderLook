//! Commandes Tauri des tâches périodiques : suppression d'UNE occurrence (la suivante est créée) ou de toute la série.

use tauri::AppHandle;

/// Supprime l'occurrence `id` d'une tâche périodique ; rend l'identifiant de l'occurrence suivante, s'il y en a une.
#[tauri::command(async)]
pub fn task_delete_occurrence(app: AppHandle, id: String) -> Result<Option<String>, String> {
    let (dir, mut conn) = crate::items_conn(&app)?;
    thunderlook_items::task_delete_occurrence(&mut conn, &dir, crate::task_num(&id)?)
}

/// Supprime la tâche et toute la série (aucune occurrence suivante).
#[tauri::command(async)]
pub fn task_delete_series(app: AppHandle, id: String) -> Result<(), String> {
    let (_, mut conn) = crate::items_conn(&app)?;
    thunderlook_items::task_delete(&mut conn, crate::task_num(&id)?)
}
