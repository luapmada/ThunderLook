//! Rappels de suivi du courrier : heure du rappel posée sur un message marqué (dialogue « Personnalisé… » de
//! l'indicateur). Stockée dans `reminders.json` (`thunderlook-reminders`) ; rien n'est écrit dans Outlook ni dans le
//! message. `message_id` : identifiant de l'interface (`m<n>`).

use crate::data_dir;
use tauri::AppHandle;

fn check(id: &str) -> Result<(), String> {
    thunderlook_mailbox::message_db_id(id).map(|_| ())
}

/// Heure du rappel (ms Unix) posée sur le message, s'il y en a une.
#[tauri::command(async)]
pub fn mail_reminder_get(app: AppHandle, message_id: String) -> Result<Option<i64>, String> {
    check(&message_id)?;
    Ok(thunderlook_reminders::mail_get(&data_dir(&app)?, &message_id))
}

/// Pose ou déplace le rappel de suivi du message à `when_ms` (ms Unix).
#[tauri::command(async)]
pub fn mail_reminder_set(app: AppHandle, message_id: String, when_ms: i64) -> Result<(), String> {
    check(&message_id)?;
    thunderlook_reminders::mail_set(&data_dir(&app)?, &message_id, when_ms)
}

/// Retire le rappel de suivi du message.
#[tauri::command(async)]
pub fn mail_reminder_clear(app: AppHandle, message_id: String) -> Result<(), String> {
    check(&message_id)?;
    thunderlook_reminders::mail_clear(&data_dir(&app)?, &message_id)
}
