//! Créer un rendez-vous, une tâche, une note ou un contact à partir d'un message (lot Q). Les commandes ne font que
//! remplir un formulaire : rien n'est écrit en base avant l'enregistrement par l'utilisateur dans la boîte correspondante.

use tauri::{AppHandle, State};
use thunderlook_model::{
    BodyView, MessageView,
    calendar::AppointmentForm,
    contacts::ContactForm,
    items::{NoteForm, TaskForm},
};

#[path = "from_mail_core.rs"]
mod form;

use crate::{Store, data_dir, now_ms, with_mailbox};
use form::Sender;

fn view_of(app: &AppHandle, store: &Store, id: &str) -> Result<MessageView, String> {
    with_mailbox(app, store, |mb| mb.message_view(id, false))
}

fn sender_of(v: &MessageView) -> Sender {
    let address = v.from_address.trim().to_string();
    let name = v.from.trim();
    Sender { name: if name.eq_ignore_ascii_case(&address) { String::new() } else { name.to_string() }, address }
}

fn body_text(v: &MessageView) -> String {
    match &v.body {
        BodyView::Text { text } => text.clone(),
        BodyView::Html { srcdoc } => form::collapse_spaces(&thunderlook_mime::html_to_text(srcdoc)),
        BodyView::Empty => String::new(),
    }
}

fn paris_now() -> chrono::NaiveDateTime {
    thunderlook_calendar::tz::TimeZone::local().to_local(chrono::DateTime::from_timestamp_millis(now_ms()).map(|t| t.naive_utc()).unwrap_or_default())
}

/// Rendez-vous pré-rempli d'après le message `id` ; `calendar_id` (`c<dossier>`) est choisi par l'interface.
#[tauri::command(async)]
pub fn from_mail_appointment(app: AppHandle, store: State<Store>, id: String, calendar_id: String) -> Result<AppointmentForm, String> {
    let v = view_of(&app, &store, &id)?;
    let opts = crate::options_cmds::load(&app);
    Ok(form::appointment_form(&v.subject, &sender_of(&v), &body_text(&v), paris_now(), &calendar_id, opts.default_duration, opts.default_reminder))
}

/// Tâche pré-remplie d'après le message `id`, dans le dossier de tâches le plus fourni.
#[tauri::command(async)]
pub fn from_mail_task(app: AppHandle, store: State<Store>, id: String) -> Result<TaskForm, String> {
    let v = view_of(&app, &store, &id)?;
    let conn = thunderlook_store::open(&data_dir(&app)?).map_err(|e| e.to_string())?;
    let folder = thunderlook_items::default_folder(&conn, "IPM.Task%")?.ok_or("aucun dossier de tâches")?;
    Ok(form::task_form(&folder, &v.subject, &sender_of(&v), &body_text(&v)))
}

/// Note pré-remplie d'après le message `id`.
#[tauri::command(async)]
pub fn from_mail_note(app: AppHandle, store: State<Store>, id: String) -> Result<NoteForm, String> {
    let v = view_of(&app, &store, &id)?;
    let conn = thunderlook_store::open(&data_dir(&app)?).map_err(|e| e.to_string())?;
    let folder = thunderlook_items::default_folder(&conn, "IPM.StickyNote%")?.ok_or("aucun dossier de notes")?;
    let color = crate::options_cmds::load(&app).default_note_color;
    Ok(form::note_form(&folder, &v.subject, &body_text(&v), color))
}

/// Contact de l'expéditeur du message `id` : le contact de même adresse s'il existe (ouvert tel quel), sinon un
/// nouveau contact pré-rempli dans le dossier de contacts le plus fourni.
#[tauri::command(async)]
pub fn from_mail_contact(app: AppHandle, store: State<Store>, id: String) -> Result<ContactForm, String> {
    let v = view_of(&app, &store, &id)?;
    let sender = sender_of(&v);
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let all = thunderlook_contacts::load(&conn, None, None)?;
    let known: Vec<(i64, Vec<String>)> = all.contacts.iter().map(|c| (c.id, c.emails.iter().map(|e| e.address.clone()).collect())).collect();
    if let Some(n) = form::find_by_address(&known, &sender.address) {
        return thunderlook_contacts::write::form_of(&conn, &dir, n);
    }
    let folder = thunderlook_contacts::view::folders(&conn)?.into_iter().max_by_key(|f| f.count).map(|f| f.id).ok_or("aucun dossier de contacts")?;
    Ok(form::contact_form(&folder, &sender))
}
