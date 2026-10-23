//! Créer un rendez-vous, une tâche, une note ou un contact à partir du message sélectionné (lot Q) : le cœur remplit
//! le formulaire, la boîte existante s'ouvre ; rien n'est écrit avant l'enregistrement.

use leptos::{prelude::*, task::spawn_local};
use thunderlook_model::calendar::CalendarQuery;

use crate::{
    appointment, ipc,
    menu::MenuEntry,
    state::Ui,
};

/// Les quatre entrées, pour le menu contextuel d'un message et le menu « Actions rapides ».
pub fn entries(ui: Ui) -> Vec<MenuEntry> {
    vec![
        MenuEntry::action("Créer un rendez-vous", move || appointment(ui)).separated(),
        MenuEntry::action("Créer une tâche", move || task(ui)),
        MenuEntry::action("Créer une note", move || note(ui)),
        MenuEntry::action("Créer un contact à partir de l'expéditeur", move || contact(ui)),
    ]
}

/// Calendrier où créer : celui que le module Calendrier a déjà choisi, sinon (calendrier jamais ouvert) le premier
/// calendrier modifiable de la vue demandée au cœur.
async fn calendar_id(ui: Ui) -> Result<String, String> {
    if let Some(id) = appointment::target_calendar(ui) {
        return Ok(id);
    }
    let kind = ui.settings.with_untracked(|s| s.calendar_view);
    let view = ipc::calendar_view(&CalendarQuery { kind, anchor: String::new(), start: None }).await?;
    view.calendars
        .iter()
        .find(|c| c.outlook_default)
        .or_else(|| view.calendars.iter().find(|c| c.shown && c.shared))
        .or_else(|| view.calendars.iter().find(|c| c.shared))
        .map(|c| c.id.clone())
        .ok_or_else(|| "Aucun calendrier modifiable.".to_string())
}

fn selected(ui: Ui) -> Option<String> {
    let id = ui.selected.get_untracked();
    if id.is_none() {
        ui.status.set(Some("Sélectionnez d'abord un message.".into()));
    }
    id
}

pub fn appointment(ui: Ui) {
    let Some(id) = selected(ui) else { return };
    spawn_local(async move {
        let form = match calendar_id(ui).await {
            Ok(cal) => ipc::from_mail_appointment(&id, &cal).await,
            Err(e) => Err(e),
        };
        match form {
            Ok(f) => ui.appointment.set(Some(f)),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

pub fn task(ui: Ui) {
    let Some(id) = selected(ui) else { return };
    spawn_local(async move {
        match ipc::from_mail_task(&id).await {
            Ok(f) => ui.task_form.set(Some(f)),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

pub fn note(ui: Ui) {
    let Some(id) = selected(ui) else { return };
    spawn_local(async move {
        match ipc::from_mail_note(&id).await {
            Ok(f) => ui.note_form.set(Some(f)),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

/// Contact de l'expéditeur : s'il existe déjà (même adresse), c'est lui qui s'ouvre.
pub fn contact(ui: Ui) {
    let Some(id) = selected(ui) else { return };
    spawn_local(async move {
        match ipc::from_mail_contact(&id).await {
            Ok(f) => ui.contact_form.set(Some(f)),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}
