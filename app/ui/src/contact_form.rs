//! Boîte « Contact » : créer ou modifier un contact dans la base de ThunderLook, et le supprimer.

use leptos::{html, prelude::*, task::spawn_local};
use thunderlook_model::contacts::ContactForm;
use wasm_bindgen::{JsCast, JsValue, prelude::wasm_bindgen};

use crate::{
    ipc,
    state::{Ui, use_ui},
};

// Choix de l'image : lecture du fichier, réduction à `max` pixels de côté au plus et recodage en JPEG par un canvas
// (rien d'autre n'est envoyé au cœur). Rend l'adresse `data:image/jpeg;base64,…`, ou une chaîne vide si rien n'est choisi.
#[wasm_bindgen(inline_js = r##"
export async function pick_photo(input, max) {
  const file = input.files && input.files[0];
  input.value = "";
  if (!file) return "";
  const url = URL.createObjectURL(file);
  try {
    const img = new Image();
    await new Promise((ok, ko) => { img.onload = ok; img.onerror = () => ko("Image illisible."); img.src = url; });
    const k = Math.min(1, max / Math.max(img.naturalWidth, img.naturalHeight));
    const w = Math.max(1, Math.round(img.naturalWidth * k));
    const h = Math.max(1, Math.round(img.naturalHeight * k));
    const canvas = document.createElement("canvas");
    canvas.width = w;
    canvas.height = h;
    const g = canvas.getContext("2d");
    g.fillStyle = "#fff";
    g.fillRect(0, 0, w, h);
    g.drawImage(img, 0, 0, w, h);
    return canvas.toDataURL("image/jpeg", 0.85);
  } finally {
    URL.revokeObjectURL(url);
  }
}
"##)]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn pick_photo(input: &JsValue, max: u32) -> Result<JsValue, JsValue>;
}

/// Côté maximal de la photo envoyée au cœur.
const PHOTO_SIDE: u32 = 256;

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
    let set = move |f: &dyn Fn(&mut ContactForm)| {
        ui.contact_form.update(|c| {
            if let Some(c) = c {
                f(c);
            }
        })
    };
    // Seulement à l'ouverture : suivre `contact_form` relancerait l'effet à chaque frappe (focus volé).
    let open = Memo::new(move |_| ui.contact_form.with(Option::is_some));
    Effect::new(move |_| {
        if open.get() {
            note.set(None);
            request_animation_frame(move || {
                if let Some(r) = root.get_untracked() {
                    if let Ok(Some(el)) = r.query_selector("input[type=text]") {
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
    // Valeur d'un champ lue sans copier le formulaire : un `get()` du formulaire cloné par chacun des quelque 40 champs, photo comprise,
    // à chaque frappe, ralentissait la saisie.
    let read = move |get: fn(&ContactForm) -> String| ui.contact_form.with(|f| f.as_ref().map(get).unwrap_or_default());
    // Champ texte lié à une propriété du formulaire.
    let field = move |label: &'static str, get: fn(&ContactForm) -> String, put: fn(&mut ContactForm, String)| {
        view! {
            <label class="field">
                <span>{label}</span>
                <input type="text" aria-label=label prop:value=move || read(get)
                    on:input=move |ev| { let v = event_target_value(&ev); set(&|f| put(f, v.clone())); } />
            </label>
        }
    };
    let choose = move |ev: leptos::ev::Event| {
        let Some(input) = ev.target() else { return };
        spawn_local(async move {
            match pick_photo(&input.into(), PHOTO_SIDE).await {
                Ok(v) => {
                    if let Some(uri) = v.as_string().filter(|s| !s.is_empty()) {
                        set(&|f| f.photo = Some(uri.clone()));
                    }
                }
                Err(e) => note.set(Some(e.as_string().unwrap_or_else(|| "Image illisible.".into()))),
            }
        });
    };
    // Photo dans un `Memo` : l'image n'est recréée (et redécodée) qu'au changement de photo, pas à chaque frappe.
    let photo = Memo::new(move |_| ui.contact_form.with(|f| f.as_ref().and_then(|f| f.photo.clone()).filter(|p| !p.is_empty())));
    let has_photo = move || photo.with(Option::is_some);
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
                    <h2 id="contact-title">{move || if ui.contact_form.with(|f| f.as_ref().is_some_and(|f| f.id.is_some())) { "Contact" } else { "Nouveau contact" }}</h2>
                    <div class="appt-row">
                        {move || match photo.get() {
                            Some(src) => view! { <img class="avatar large" src=src alt="Photo du contact" /> }.into_any(),
                            None => view! { <span class="avatar large" aria-hidden="true">"?"</span> }.into_any(),
                        }}
                        <label class="field">
                            <span>"Photo"</span>
                            <input type="file" accept="image/*" aria-label="Choisir une photo" on:change=choose />
                        </label>
                        <button class="btn" type="button" prop:disabled=move || !has_photo() on:click=move |_| set(&|f| f.photo = Some(String::new()))>"Retirer la photo"</button>
                    </div>
                    <div class="appt-row">
                        {field("Titre", |f| f.prefix.clone(), |f, v| f.prefix = v)}
                        {field("Prénom", |f| f.given_name.clone(), |f, v| f.given_name = v)}
                        {field("Deuxième prénom", |f| f.middle_name.clone(), |f, v| f.middle_name = v)}
                        {field("Nom", |f| f.surname.clone(), |f, v| f.surname = v)}
                        {field("Suffixe", |f| f.suffix.clone(), |f, v| f.suffix = v)}
                        {field("Surnom", |f| f.nickname.clone(), |f, v| f.nickname = v)}
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
                        {field("Bureau 2", |f| f.business2.clone(), |f, v| f.business2 = v)}
                        {field("Domicile 2", |f| f.home2.clone(), |f, v| f.home2 = v)}
                        {field("Autre téléphone", |f| f.other_phone.clone(), |f, v| f.other_phone = v)}
                    </div>
                    <div class="appt-row">
                        {field("Télécopie bureau", |f| f.business_fax.clone(), |f, v| f.business_fax = v)}
                        {field("Télécopie domicile", |f| f.home_fax.clone(), |f, v| f.home_fax = v)}
                        {field("Voiture", |f| f.car_phone.clone(), |f, v| f.car_phone = v)}
                        {field("Récepteur d'appels", |f| f.pager.clone(), |f, v| f.pager = v)}
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
                        {field("Autre adresse", |f| f.other_street.clone(), |f, v| f.other_street = v)}
                        {field("Code postal (autre)", |f| f.other_postal.clone(), |f, v| f.other_postal = v)}
                        {field("Ville (autre)", |f| f.other_city.clone(), |f, v| f.other_city = v)}
                        {field("Pays (autre)", |f| f.other_country.clone(), |f, v| f.other_country = v)}
                    </div>
                    <div class="appt-row">
                        {field("Messagerie instantanée", |f| f.im_address.clone(), |f, v| f.im_address = v)}
                        {field("Conjoint(e)", |f| f.spouse.clone(), |f, v| f.spouse = v)}
                    </div>
                    <div class="appt-row">
                        {field("Page web", |f| f.web_page.clone(), |f, v| f.web_page = v)}
                        <label class="field">
                            <span>"Anniversaire"</span>
                            <input type="date" aria-label="Anniversaire" prop:value=move || read(|f| f.birthday.clone())
                                on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.birthday = v.clone()); } />
                        </label>
                    </div>
                    {field("Catégories (séparées par des virgules)", |f| f.categories.clone(), |f, v| f.categories = v)}
                    <label class="field">
                        <span>"Notes"</span>
                        <textarea class="ar-text" aria-label="Notes" rows="5" prop:value=move || read(|f| f.notes.clone())
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
