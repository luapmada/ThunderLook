//! Module Contacts (J10, lecture seule : Outlook fait foi). Le cœur calcule la liste triée et la fiche ; ici, dossiers
//! dans le volet de gauche, liste filtrable au centre, fiche à droite (vue « Personnes » d'Outlook).

use leptos::{prelude::*, task::spawn_local};
use serde::Serialize;
use thunderlook_model::{
    ComposeKind,
    contacts::{ContactCard, ContactFieldKind, ContactRow, ContactRowKind},
};
use wasm_bindgen::prelude::*;

use crate::{
    icons::*,
    ipc,
    item_folders::{FolderHost, FolderKind, FolderUi},
    state::{Module, Ui, use_ui},
};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

#[derive(Serialize)]
struct FolderArg<'a> {
    folder: &'a str,
}

/// Listes de distribution d'un dossier (commande `contact_dist_lists`), lecture seule.
async fn dist_lists(folder: &str) -> Result<Vec<ContactRow>, String> {
    let args = (FolderArg { folder }).serialize(&serde_wasm_bindgen::Serializer::json_compatible()).map_err(|e| e.to_string())?;
    let out = invoke("contact_dist_lists", args).await.map_err(|e| e.as_string().unwrap_or_else(|| format!("contact_dist_lists : {e:?}")))?;
    serde_wasm_bindgen::from_value(out).map_err(|e| format!("contact_dist_lists : {e}"))
}

/// Charge les dossiers à la première ouverture du module, la liste à chaque changement de dossier, la fiche à chaque
/// changement de sélection.
pub fn watch(ui: Ui) {
    ipc::on_contacts_changed(move || ui.contacts_epoch.update(|e| *e += 1));
    Effect::new(move |_| {
        if ui.module.get() != Module::Contacts || !ui.contact_folders.with_untracked(Vec::is_empty) {
            return;
        }
        spawn_local(async move {
            match crate::item_folders::contact_folders().await {
                Ok(f) => {
                    if ui.contact_folder.get_untracked().is_empty() {
                        if let Some(first) = f.first() {
                            ui.contact_folder.set(first.id.clone());
                        }
                    }
                    ui.contact_folders.set(f);
                }
                Err(e) => ui.error.set(Some(e)),
            }
        });
    });
    let last = StoredValue::new((String::new(), 0u64));
    Effect::new(move |_| {
        let folder = ui.contact_folder.get();
        let epoch = ui.contacts_epoch.get();
        let refresh = last.with_value(|(f, e)| *f == folder && *e != epoch) && ui.contact_list.with_untracked(Option::is_some);
        last.set_value((folder.clone(), epoch));
        if folder.is_empty() {
            return;
        }
        // Rechargement après une synchronisation : la liste et la sélection restent en place.
        if !refresh {
            ui.contact_list.set(None);
        }
        spawn_local(async move {
            match ipc::contact_list(&folder).await {
                Ok(mut l) if ui.contact_folder.get_untracked() == folder => {
                    // Les listes de distribution viennent après les contacts ; une erreur ne masque pas les contacts.
                    match dist_lists(&folder).await {
                        Ok(d) => l.rows.extend(d),
                        Err(e) => l.issues.push(e),
                    }
                    if !refresh || ui.contact_selected.get_untracked().is_none() {
                        ui.contact_selected.set(l.rows.iter().find(|r| r.kind == ContactRowKind::Contact).map(|r| r.id.clone()));
                    }
                    ui.contact_list.set(Some(l));
                }
                Ok(_) => {}
                Err(e) => ui.error.set(Some(e)),
            }
        });
    });
    Effect::new(move |_| {
        let Some(id) = ui.contact_selected.get() else {
            ui.contact_card.set(None);
            return;
        };
        spawn_local(async move {
            match ipc::contact_card(&id).await {
                Ok(c) if ui.contact_selected.get_untracked().as_deref() == Some(id.as_str()) => ui.contact_card.set(Some(c)),
                Ok(_) => {}
                Err(e) => ui.error.set(Some(e)),
            }
        });
    });
}

/// Le filtre retrouve la ligne par son libellé, son sous-titre, sa société, ses adresses électroniques ou ses
/// téléphones (casse sans effet ; pour un numéro, espaces et points sans effet non plus).
fn matches(r: &ContactRow, filter: &str) -> bool {
    if filter.is_empty() || r.title.to_lowercase().contains(filter) || r.subtitle.to_lowercase().contains(filter) || r.search.contains(filter) {
        return true;
    }
    let digits: String = filter.chars().filter(char::is_ascii_digit).collect();
    let phone_like = filter.chars().all(|c| c.is_ascii_digit() || " .-+()".contains(c));
    phone_like && digits.len() >= 3 && r.search.contains(&digits)
}

/// Lignes de la liste après le filtre (contacts et listes de distribution, sans tenir compte de la casse).
pub fn visible(ui: Ui) -> Vec<ContactRow> {
    let filter = ui.contact_filter.get().trim().to_lowercase();
    ui.contact_list.with(|l| {
        l.as_ref()
            .map(|l| {
                l.rows
                    .iter()
                    .filter(|r| matches(r, &filter))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// Nouveau message adressé à ce contact (module Courrier).
fn write_to(ui: Ui, name: String, address: String) {
    spawn_local(async move {
        match ipc::compose(ComposeKind::New, None, None).await {
            Ok(mut d) => {
                d.to = if name.is_empty() { address } else { format!("{name} <{address}>") };
                ui.module.set(Module::Mail);
                ui.compose_dirty.set(false);
                ui.compose.set(Some(d));
                ui.compose_epoch.update(|e| *e += 1);
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

#[component]
pub fn ContactNav() -> impl IntoView {
    let ui = use_ui();
    let fu = FolderUi::new(FolderKind::Contact);
    // Après création, renommage ou suppression : liste des dossiers relue (le dossier affiché peut avoir disparu).
    let changed = move |()| {
        spawn_local(async move {
            match crate::item_folders::contact_folders().await {
                Ok(f) => {
                    if !f.iter().any(|x| x.id == ui.contact_folder.get_untracked()) {
                        ui.contact_folder.set(f.first().map(|x| x.id.clone()).unwrap_or_default());
                    }
                    ui.contact_folders.set(f);
                    ui.contacts_epoch.update(|e| *e += 1);
                }
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };
    view! {
        <FolderHost f=fu on_change=changed />
        <div class="cal-nav contact-nav" role="listbox" aria-label="Mes contacts" on:contextmenu=move |ev| fu.open_menu(&ev, "")>
            {move || {
                let mut section = String::new();
                ui.contact_folders
                    .get()
                    .into_iter()
                    .map(|f| {
                        let head = (f.section != section).then(|| {
                            section = f.section.clone();
                            view! { <div class="cal-section">{f.section.clone()}</div> }
                        });
                        let (id, sel, menu_id) = (f.id.clone(), f.id.clone(), f.id.clone());
                        view! {
                            {head}
                            <div
                                on:contextmenu=move |ev| fu.open_menu(&ev, &menu_id)
                                class="cal-item"
                                role="option"
                                class:selected=move || ui.contact_folder.get() == sel
                                aria-selected=move || (ui.contact_folder.get() == f.id).to_string()
                                on:click=move |_| {
                                    ui.contact_filter.set(String::new());
                                    ui.contact_folder.set(id.clone());
                                }
                            >
                                <Svg icon=USERS size=14 />
                                <span class="name">{f.name.clone()}</span>
                                <span class="count">{f.count}</span>
                            </div>
                        }
                    })
                    .collect_view()
            }}
        </div>
    }
}

#[component]
pub fn ContactsPane() -> impl IntoView {
    let ui = use_ui();
    // Flèches haut/bas dans la liste : contact précédent/suivant parmi les lignes visibles.
    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        let step: i32 = match ev.key().as_str() {
            "ArrowDown" => 1,
            "ArrowUp" => -1,
            _ => return,
        };
        ev.prevent_default();
        let rows: Vec<ContactRow> = visible(ui).into_iter().filter(|r| r.kind == ContactRowKind::Contact).collect();
        let cur = ui.contact_selected.get_untracked();
        let pos = rows.iter().position(|r| Some(&r.id) == cur.as_ref()).map_or(0, |p| (p as i32 + step).clamp(0, rows.len() as i32 - 1) as usize);
        if let Some(r) = rows.get(pos) {
            ui.contact_selected.set(Some(r.id.clone()));
            if let Some(el) = document().get_element_by_id(&format!("contact-{}", r.id)) {
                el.scroll_into_view_with_bool(false);
            }
        }
    };
    // Listes de distribution dépliées (`d<id>`).
    let opened = RwSignal::new(Vec::<String>::new());
    view! {
        <section class="pane contacts" aria-label="Contacts">
            <div class="contact-list-pane">
                <div class="contact-search">
                    <Svg icon=SEARCH size=14 />
                    <input
                        type="search"
                        placeholder="Rechercher dans les contacts"
                        aria-label="Rechercher dans les contacts"
                        prop:value=move || ui.contact_filter.get()
                        on:input=move |ev| ui.contact_filter.set(event_target_value(&ev))
                    />
                </div>
                {move || ui.contact_list.with(|l| l.as_ref().and_then(|l| l.source_note.clone())).map(|n| view! { <p class="cal-note">{n}</p> })}
                <div class="contact-rows" role="listbox" aria-label="Liste des contacts" tabindex="0" on:keydown=on_key>
                    {move || match ui.contact_list.with(Option::is_none) {
                        true => view! { <div class="placeholder">"Chargement des contacts…"</div> }.into_any(),
                        false => {
                            let mut letter = String::new();
                            let rows = visible(ui);
                            if rows.is_empty() {
                                return view! { <div class="placeholder">"Aucun contact."</div> }.into_any();
                            }
                            let (lists, rows): (Vec<_>, Vec<_>) = rows.into_iter().partition(|r| r.kind == ContactRowKind::DistList);
                            let dist = (!lists.is_empty()).then(|| {
                                let items = lists
                                    .into_iter()
                                    .map(|r| {
                                        let (id, aria, shown) = (r.id.clone(), r.id.clone(), r.id.clone());
                                        let members = r.members.clone();
                                        view! {
                                            <div
                                                class="contact-row"
                                                role="option"
                                                aria-expanded=move || opened.with(|o| o.contains(&aria)).to_string()
                                                on:click=move |_| opened.update(|o| match o.iter().position(|x| *x == id) {
                                                    Some(p) => { o.remove(p); }
                                                    None => o.push(id.clone()),
                                                })
                                            >
                                                <span class="avatar small" aria-hidden="true"><Svg icon=USERS size=14 /></span>
                                                <span class="contact-text">
                                                    <span class="contact-title">{r.title.clone()}</span>
                                                    <span class="contact-sub">{r.subtitle.clone()}</span>
                                                </span>
                                            </div>
                                            <Show when=move || opened.with(|o| o.contains(&shown))>
                                                {members
                                                    .iter()
                                                    .map(|m| {
                                                        let (name, addr) = (m.name.clone(), m.email.clone());
                                                        let label = if m.name.is_empty() { m.email.clone() } else { format!("{} — {}", m.name, m.email) };
                                                        view! {
                                                            <div class="contact-row" style="padding-left: 56px">
                                                                <button class="link-btn" title="Nouveau message à ce membre" on:click=move |_| write_to(ui, name.clone(), addr.clone())>{label}</button>
                                                            </div>
                                                        }
                                                    })
                                                    .collect_view()}
                                            </Show>
                                        }
                                    })
                                    .collect_view();
                                view! {
                                    <div class="contact-letter" aria-hidden="true">"Listes de distribution"</div>
                                    {items}
                                }
                            });
                            let contacts = rows.into_iter()
                                .map(|r| {
                                    let head = (r.letter != letter).then(|| {
                                        letter = r.letter.clone();
                                        view! { <div class="contact-letter" aria-hidden="true">{r.letter.clone()}</div> }
                                    });
                                    let (id, sel, aria) = (r.id.clone(), r.id.clone(), r.id.clone());
                                    view! {
                                        {head}
                                        <div
                                            class="contact-row"
                                            id=format!("contact-{}", r.id)
                                            role="option"
                                            class:selected=move || ui.contact_selected.get().as_deref() == Some(sel.as_str())
                                            aria-selected=move || (ui.contact_selected.get().as_deref() == Some(aria.as_str())).to_string()
                                            on:click=move |_| ui.contact_selected.set(Some(id.clone()))
                                        >
                                            <span class="avatar small" aria-hidden="true">{r.initials.clone()}</span>
                                            <span class="contact-text">
                                                <span class="contact-title">{r.title.clone()}</span>
                                                <span class="contact-sub">{r.subtitle.clone()}</span>
                                            </span>
                                        </div>
                                    }
                                })
                                .collect_view();
                            view! { {contacts} {dist} }.into_any()
                        }
                    }}
                </div>
            </div>
            <div class="contact-card-pane">
                {move || match ui.contact_card.get() {
                    None => view! { <div class="placeholder">"Sélectionnez un contact."</div> }.into_any(),
                    Some(c) => view! { <Card card=c /> }.into_any(),
                }}
            </div>
        </section>
    }
}

#[component]
fn Card(card: ContactCard) -> impl IntoView {
    let ui = use_ui();
    let name = card.full_name.clone();
    view! {
        <article class="contact-card" aria-label=card.title.clone()>
            <header class="contact-head">
                {match card.photo.clone() {
                    Some(src) => view! { <img class="avatar large" src=src alt="" /> }.into_any(),
                    None => view! { <span class="avatar large" aria-hidden="true">{card.initials.clone()}</span> }.into_any(),
                }}
                <div>
                    <div class="contact-actions">
                        <button class="btn" on:click={ let id = card.id.clone(); move |_| crate::contact_form::open_edit(ui, id.clone()) }>"Modifier"</button>
                    </div>
                    <h2>{card.full_name.clone()}</h2>
                    {(!card.job.is_empty()).then(|| view! { <p class="contact-job">{card.job.clone()}</p> })}
                    {(!card.categories.is_empty()).then(|| view! { <p class="contact-cats">{card.categories.join(", ")}</p> })}
                </div>
            </header>
            {card
                .sections
                .into_iter()
                .map(|(title, fields)| {
                    let name = name.clone();
                    view! {
                        <section class="contact-section">
                            <h3>{title}</h3>
                            <dl>
                                {fields
                                    .into_iter()
                                    .map(|f| {
                                        let value = match f.kind {
                                            ContactFieldKind::Email => {
                                                let (name, addr) = (name.clone(), f.value.clone());
                                                view! {
                                                    <button class="link-btn" title="Nouveau message à ce contact" on:click=move |_| write_to(ui, name.clone(), addr.clone())>
                                                        {f.value.clone()}
                                                    </button>
                                                }
                                                .into_any()
                                            }
                                            ContactFieldKind::Phone => view! { <a href=format!("tel:{}", f.value.replace(' ', ""))>{f.value.clone()}</a> }.into_any(),
                                            _ => view! { <span class="multiline">{f.value.clone()}</span> }.into_any(),
                                        };
                                        view! {
                                            <dt>{f.label}</dt>
                                            <dd>{value}</dd>
                                        }
                                    })
                                    .collect_view()}
                            </dl>
                        </section>
                    }
                })
                .collect_view()}
            {card.notes.map(|n| view! {
                <section class="contact-section">
                    <h3>"Notes"</h3>
                    <p class="multiline">{n}</p>
                </section>
            })}
        </article>
    }
}
