//! Commandes Tauri des tâches et des notes : vues, filtres et tris calculés par `thunderlook-items`.

use tauri::AppHandle;
use thunderlook_model::items::{NoteList, NoteQuery, TaskList, TaskQuery};

#[tauri::command(async)]
pub fn tasks_query(app: AppHandle, query: TaskQuery) -> Result<TaskList, String> {
    let (_, conn) = crate::items_conn(&app)?;
    thunderlook_items::tasks_query(&conn, &query)
}

#[tauri::command(async)]
pub fn notes_query(app: AppHandle, query: NoteQuery) -> Result<NoteList, String> {
    let (dir, conn) = crate::items_conn(&app)?;
    thunderlook_items::notes_query(&conn, &dir, &query)
}
