//! Boîte « Contact » : créer ou modifier un contact dans la base de ThunderLook, et le supprimer.

use leptos::{html, prelude::*, task::spawn_local};
use thunderlook_model::contacts::ContactForm;
use wasm_bindgen::JsCast;

use crate::{
    ipc,
    state::{Ui, use_ui},
};

/// Nouveau contact dans le dossier affiché.
pub fn open_new(ui: Ui) {
    let folder = ui.contact_folder.get_untracked();
    spawn_local(async move {
        match ipc::contact_form(None, Some(&folder)).await {
            Ok(f) => ui.contact_form.set(Some(f)),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

/// Ouvre un contact existant (`p<id>`).
pub fn open_edit(ui: Ui, id: String) {
    spawn_local(async move {
        match ipc::contact_form(Some(&id), None).await {
            Ok(f) => ui.contact_form.set(Some(f)),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

/// Supprime un contact (`p<id>`).
pub fn delete(ui: Ui, id: String) {
    spawn_local(async move {
        match ipc::contact_delete(&id).await {
            Ok(()) => {
                ui.contact_selected.set(None);
                ui.status.set(Some("Contact supprimé".into()));
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

#[component]
pub fn ContactDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let busy = RwSignal::new(false);
    let note = RwSignal::new(None::<String>);
    let form = move || ui.contact_form.get();
    let set = move |f: &dyn Fn(&mut ContactForm)| {
        ui.contact_form.update(|c| {
            if let Some(c) = c {
                f(c);
            }
        })
    };
    Effect::new(move |_| {
        if ui.contact_form.with(Option::is_some) {
            note.set(None);
            request_animation_frame(move || {
                if let Some(r) = root.get_untracked() {
                    if let Ok(Some(el)) = r.query_selector("input") {
                        if let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() {
                            let _ = el.focus();
                        }
                    }
                }
            });
        }
    });
    let close = move || ui.contact_form.set(None);
    let save = move || {
        let Some(f) = ui.contact_form.get_untracked() else { return };
        busy.set(true);
        spawn_local(async move {
            match ipc::contact_save(&f).await {
                Ok(id) => {
                    ui.status.set(Some("Contact enregistré".into()));
                    ui.contact_selected.set(Some(id));
                    close();
                }
                Err(e) => note.set(Some(e)),
            }
            busy.set(false);
        });
    };
    // Champ texte lié à une propriété du formulaire.
    let field = move |label: &'static str, get: fn(&ContactForm) -> String, put: fn(&mut ContactForm, String)| {
        view! {
            <label class="field">
                <span>{label}</span>
                <input type="text" aria-label=label prop:value=move || form().map(|f| get(&f)).unwrap_or_default()
                    on:input=move |ev| { let v = event_target_value(&ev); set(&|f| put(f, v.clone())); } />
            </label>
        }
    };
    view! {
        <Show when=move || ui.contact_form.with(Option::is_some)>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog wide appt" role="dialog" aria-modal="true" aria-labelledby="contact-title" node_ref=root
                    on:keydown=move |ev| match ev.key().as_str() {
                        "Escape" => {
                            ev.prevent_default();
                            close();
                        }
                        "s" if ev.ctrl_key() => {
                            ev.prevent_default();
                            save();
                        }
                        _ => {}
                    }>
                    <h2 id="contact-title">{move || if form().is_some_and(|f| f.id.is_some()) { "Contact" } else { "Nouveau contact" }}</h2>
                    <div class="appt-row">
                        {field("Prénom", |f| f.given_name.clone(), |f, v| f.given_name = v)}
                        {field("Nom", |f| f.surname.clone(), |f, v| f.surname = v)}
                    </div>
                    <div class="appt-row">
                        {field("Société", |f| f.company.clone(), |f, v| f.company = v)}
                        {field("Fonction", |f| f.job_title.clone(), |f, v| f.job_title = v)}
                        {field("Service", |f| f.department.clone(), |f, v| f.department = v)}
                    </div>
                    <div class="appt-row">
                        {field("Courrier 1", |f| f.email1.clone(), |f, v| f.email1 = v)}
                        {field("Courrier 2", |f| f.email2.clone(), |f, v| f.email2 = v)}
                        {field("Courrier 3", |f| f.email3.clone(), |f, v| f.email3 = v)}
                    </div>
                    <div class="appt-row">
                        {field("Mobile", |f| f.mobile.clone(), |f, v| f.mobile = v)}
                        {field("Bureau", |f| f.business.clone(), |f, v| f.business = v)}
                        {field("Domicile", |f| f.home_phone.clone(), |f, v| f.home_phone = v)}
                    </div>
                    <div class="appt-row">
                        {field("Adresse du bureau", |f| f.work_street.clone(), |f, v| f.work_street = v)}
                        {field("Code postal", |f| f.work_postal.clone(), |f, v| f.work_postal = v)}
                        {field("Ville", |f| f.work_city.clone(), |f, v| f.work_city = v)}
                        {field("Pays", |f| f.work_country.clone(), |f, v| f.work_country = v)}
                    </div>
                    <div class="appt-row">
                        {field("Adresse du domicile", |f| f.home_street.clone(), |f, v| f.home_street = v)}
                        {field("Code postal (domicile)", |f| f.home_postal.clone(), |f, v| f.home_postal = v)}
                        {field("Ville (domicile)", |f| f.home_city.clone(), |f, v| f.home_city = v)}
                        {field("Pays (domicile)", |f| f.home_country.clone(), |f, v| f.home_country = v)}
                    </div>
                    <div class="appt-row">
                        {field("Page web", |f| f.web_page.clone(), |f, v| f.web_page = v)}
                        <label class="field">
                            <span>"Anniversaire"</span>
                            <input type="date" aria-label="Anniversaire" prop:value=move || form().map(|f| f.birthday).unwrap_or_default()
                                on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.birthday = v.clone()); } />
                        </label>
                    </div>
                    <label class="field">
                        <span>"Notes"</span>
                        <textarea class="ar-text" aria-label="Notes" rows="5" prop:value=move || form().map(|f| f.notes).unwrap_or_default()
                            on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.notes = v.clone()); }></textarea>
                    </label>
                    {move || note.get().map(|n| view! { <p class="dialog-note" role="alert">{n}</p> })}
                    <div class="dialog-buttons">
                        <button class="btn primary" prop:disabled=move || busy.get() on:click=move |_| save()>"Enregistrer et fermer"</button>
                        <button class="btn" on:click=move |_| close()>"Annuler"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

/// Échange avec le téléphone par fichiers : exporte ou importe les contacts du dossier affiché, les tâches ou les notes.
pub fn exchange(ui: Ui, kind: &'static str, import: bool) {
    let folder = ui.contact_folder.get_untracked();
    spawn_local(async move {
        let folder = (kind == "contacts").then_some(folder.as_str());
        let r = if import { ipc::exchange_import(kind, folder).await } else { ipc::exchange_export(kind, folder).await };
        match r {
            Ok(Some(m)) => {
                ui.status.set(Some(m));
                ui.tasks_epoch.update(|e| *e += 1);
                ui.notes_epoch.update(|e| *e += 1);
            }
            Ok(None) => {}
            Err(e) => ui.error.set(Some(e)),
        }
    });
}
