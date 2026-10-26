//! Module Contacts (J10). Le cœur calcule la liste triée et la fiche ; ici, dossiers dans le volet de gauche, liste
//! filtrable au centre en trois vues (Personnes, Cartes, Liste), fiche à droite. Lot AX : « Nouveau message » au contact
//! ou à la liste sélectionnés, listes de distribution créées, modifiées et supprimées.

use std::cell::Cell;

use leptos::{prelude::*, task::spawn_local};
use serde::Serialize;
use thunderlook_model::{
    ComposeKind,
    contacts::{ContactCard, ContactFieldKind, ContactRow, ContactRowKind, DistMember},
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

/// Appel d'une commande Tauri de ce module.
async fn call<T: serde::de::DeserializeOwned>(cmd: &str, args: &impl Serialize) -> Result<T, String> {
    let args = args.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).map_err(|e| e.to_string())?;
    let out = invoke(cmd, args).await.map_err(|e| e.as_string().unwrap_or_else(|| format!("{cmd} : {e:?}")))?;
    serde_wasm_bindgen::from_value(out).map_err(|e| format!("{cmd} : {e}"))
}

/// Listes de distribution d'un dossier (commande `contact_dist_lists`).
async fn dist_lists(folder: &str) -> Result<Vec<ContactRow>, String> {
    call("contact_dist_lists", &FolderArg { folder }).await
}

#[derive(Serialize)]
struct SaveListArgs<'a> {
    folder: &'a str,
    id: Option<&'a str>,
    name: &'a str,
    members: &'a [DistMember],
}

#[derive(Serialize)]
struct IdArg<'a> {
    id: &'a str,
}

/// Vue de la liste des contacts (« Affichage actuel » d'Outlook) ; gardée d'une ouverture du module à l'autre.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewKind {
    People,
    Cards,
    List,
}

/// Tri de la vue Liste : colonne, croissant.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SortCol {
    Name,
    Company,
}

thread_local! {
    static VIEW: Cell<ViewKind> = const { Cell::new(ViewKind::People) };
    static SORT: Cell<(SortCol, bool)> = const { Cell::new((SortCol::Name, true)) };
}

/// Liste de distribution en cours de saisie (création : `id` absent).
#[derive(Clone, Default)]
struct ListDraft {
    id: Option<String>,
    folder: String,
    name: String,
    members: Vec<DistMember>,
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
        // Une liste de distribution (`d<id>`) n'a pas de fiche : son volet est construit avec la ligne.
        let Some(id) = ui.contact_selected.get().filter(|id| id.starts_with('p')) else {
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

/// Destinataire « Nom <adresse> », nom entre guillemets s'il contient un séparateur (comme l'autocomplétion).
fn recipient(name: &str, addr: &str) -> String {
    let name = name.trim();
    if name.is_empty() || name.eq_ignore_ascii_case(addr) {
        addr.to_string()
    } else if name.contains([',', ';', '<', '>', '"']) {
        format!("\"{}\" <{addr}>", name.replace('"', ""))
    } else {
        format!("{name} <{addr}>")
    }
}

/// Nouveau message adressé à ce contact (module Courrier).
fn write_to(ui: Ui, name: String, address: String) {
    compose_to(ui, recipient(&name, &address));
}

/// Nouveau message dont le champ « À » est déjà rempli.
fn compose_to(ui: Ui, to: String) {
    spawn_local(async move {
        match ipc::compose(ComposeKind::New, None, None).await {
            Ok(mut d) => {
                d.to = to;
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

/// Bouton « Communiquer › Nouveau message » : au contact sélectionné (première adresse) ou à tous les membres de la
/// liste de distribution sélectionnée.
pub fn message_selected(ui: Ui) {
    let Some(id) = ui.contact_selected.get_untracked() else {
        ui.error.set(Some("Sélectionnez un contact ou une liste.".into()));
        return;
    };
    let to = if id.starts_with('d') {
        let members = ui.contact_list.with_untracked(|l| l.as_ref().and_then(|l| l.rows.iter().find(|r| r.id == id).map(|r| r.members.clone()))).unwrap_or_default();
        members.iter().filter(|m| m.email.contains('@')).map(|m| recipient(&m.name, &m.email)).collect::<Vec<_>>().join("; ")
    } else {
        ui.contact_card
            .with_untracked(|c| {
                c.as_ref().filter(|c| c.id == id).and_then(|c| {
                    c.sections.iter().flat_map(|(_, f)| f).find(|f| f.kind == ContactFieldKind::Email).map(|f| recipient(&c.full_name, &f.value))
                })
            })
            .unwrap_or_default()
    };
    if to.is_empty() {
        ui.error.set(Some("Pas d'adresse électronique pour ce destinataire.".into()));
    } else {
        compose_to(ui, to);
    }
}

/// Bouton « Supprimer » : contact ou liste de distribution sélectionnés (annulable par Ctrl+Z).
pub fn delete_selected(ui: Ui) {
    let Some(id) = ui.contact_selected.get_untracked() else { return };
    if !id.starts_with('d') {
        crate::contact_form::delete(ui, id);
        return;
    }
    spawn_local(async move {
        match call::<()>("contact_dist_list_delete", &IdArg { id: &id }).await {
            Ok(()) => {
                ui.contact_selected.set(None);
                ui.status.set(Some("Liste de distribution supprimée".into()));
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

#[component]
pub fn ContactsPane() -> impl IntoView {
    let ui = use_ui();
    let view_kind = RwSignal::new(VIEW.get());
    let sort = RwSignal::new(SORT.get());
    Effect::new(move |_| VIEW.set(view_kind.get()));
    Effect::new(move |_| SORT.set(sort.get()));
    let editing = RwSignal::new(None::<ListDraft>);
    // Flèches haut/bas dans la liste : contact précédent/suivant parmi les lignes visibles.
    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        let step: i32 = match ev.key().as_str() {
            "ArrowDown" => 1,
            "ArrowUp" => -1,
            _ => return,
        };
        ev.prevent_default();
        let mut rows: Vec<ContactRow> = visible(ui).into_iter().filter(|r| r.kind == ContactRowKind::Contact).collect();
        if view_kind.get_untracked() == ViewKind::List {
            sort_rows(&mut rows, sort.get_untracked());
        }
        let cur = ui.contact_selected.get_untracked();
        let pos = rows.iter().position(|r| Some(&r.id) == cur.as_ref()).map_or(0, |p| (p as i32 + step).clamp(0, rows.len() as i32 - 1) as usize);
        if let Some(r) = rows.get(pos) {
            ui.contact_selected.set(Some(r.id.clone()));
            if let Some(el) = document().get_element_by_id(&format!("contact-{}", r.id)) {
                el.scroll_into_view_with_bool(false);
            }
        }
    };
    let new_list = move |_| editing.set(Some(ListDraft { folder: ui.contact_folder.get_untracked(), ..Default::default() }));
    let tab = move |kind: ViewKind, label: &'static str| {
        view! {
            <button
                class="btn"
                role="tab"
                aria-selected=move || (view_kind.get() == kind).to_string()
                style=move || if view_kind.get() == kind { "font-weight:600;border-color:var(--accent)" } else { "" }
                on:click=move |_| view_kind.set(kind)
            >
                {label}
            </button>
        }
    };
    view! {
        <section class="pane contacts" aria-label="Contacts">
            <div class="contact-list-pane" style=move || if view_kind.get() == ViewKind::People { "" } else { "width:62%;max-width:none" }>
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
                <div role="tablist" aria-label="Affichage des contacts" style="display:flex;gap:4px;flex-wrap:wrap;margin:0 8px 6px">
                    {tab(ViewKind::People, "Personnes")}
                    {tab(ViewKind::Cards, "Cartes")}
                    {tab(ViewKind::List, "Liste")}
                    <button class="btn" style="margin-left:auto" title="Nouvelle liste de distribution" on:click=new_list>"Nouvelle liste"</button>
                </div>
                {move || ui.contact_list.with(|l| l.as_ref().and_then(|l| l.source_note.clone())).map(|n| view! { <p class="cal-note">{n}</p> })}
                <div class="contact-rows" role="listbox" aria-label="Liste des contacts" tabindex="0" on:keydown=on_key>
                    {move || {
                        if ui.contact_list.with(Option::is_none) {
                            return view! { <div class="placeholder">"Chargement des contacts…"</div> }.into_any();
                        }
                        let rows = visible(ui);
                        if rows.is_empty() {
                            return view! { <div class="placeholder">"Aucun contact."</div> }.into_any();
                        }
                        let (lists, mut rows): (Vec<_>, Vec<_>) = rows.into_iter().partition(|r| r.kind == ContactRowKind::DistList);
                        let contacts = match view_kind.get() {
                            ViewKind::People => people(ui, rows).into_any(),
                            ViewKind::Cards => cards(ui, rows).into_any(),
                            ViewKind::List => {
                                sort_rows(&mut rows, sort.get());
                                table(ui, rows, sort).into_any()
                            }
                        };
                        let dist = (!lists.is_empty()).then(|| {
                            let items = lists.into_iter().map(|r| row_view(ui, r, true)).collect_view();
                            view! {
                                <div class="contact-letter" aria-hidden="true">"Listes de distribution"</div>
                                {items}
                            }
                        });
                        view! { {contacts} {dist} }.into_any()
                    }}
                </div>
            </div>
            <div class="contact-card-pane">
                {move || {
                    let sel = ui.contact_selected.get();
                    if let Some(id) = sel.filter(|id| id.starts_with('d')) {
                        return match ui.contact_list.with(|l| l.as_ref().and_then(|l| l.rows.iter().find(|r| r.id == id).cloned())) {
                            Some(r) => view! { <ListCard row=r editing=editing /> }.into_any(),
                            None => view! { <div class="placeholder">"Sélectionnez un contact."</div> }.into_any(),
                        };
                    }
                    match ui.contact_card.get() {
                        None => view! { <div class="placeholder">"Sélectionnez un contact."</div> }.into_any(),
                        Some(c) => view! { <Card card=c /> }.into_any(),
                    }
                }}
            </div>
            <ListDialog editing=editing />
        </section>
    }
}

/// Tri de la vue Liste (nom = ordre « Classer sous » du cœur, conservé pour les égalités).
fn sort_rows(rows: &mut [ContactRow], (col, asc): (SortCol, bool)) {
    let key = |r: &ContactRow| match col {
        SortCol::Name => String::new(),
        SortCol::Company => r.company.to_lowercase(),
    };
    rows.sort_by(|a, b| key(a).cmp(&key(b)));
    if !asc {
        rows.reverse();
    }
}

/// Ligne de la vue Personnes (contact) ou d'une liste de distribution.
fn row_view(ui: Ui, r: ContactRow, list: bool) -> impl IntoView {
    let (id, sel, aria) = (r.id.clone(), r.id.clone(), r.id.clone());
    let open = r.id.clone();
    view! {
        <div
            class="contact-row"
            id=format!("contact-{}", r.id)
            role="option"
            class:selected=move || ui.contact_selected.get().as_deref() == Some(sel.as_str())
            aria-selected=move || (ui.contact_selected.get().as_deref() == Some(aria.as_str())).to_string()
            on:click=move |_| ui.contact_selected.set(Some(id.clone()))
            on:dblclick=move |_| if !list { crate::contact_form::open_edit(ui, open.clone()) }
        >
            {if list {
                view! { <span class="avatar small" aria-hidden="true"><Svg icon=USERS size=14 /></span> }.into_any()
            } else {
                view! { <span class="avatar small" aria-hidden="true">{r.initials.clone()}</span> }.into_any()
            }}
            <span class="contact-text">
                <span class="contact-title">{r.title.clone()}</span>
                <span class="contact-sub">{r.subtitle.clone()}</span>
            </span>
        </div>
    }
}

/// Vue Personnes : une ligne par contact, avec la lettre de l'index.
fn people(ui: Ui, rows: Vec<ContactRow>) -> impl IntoView {
    let mut letter = String::new();
    rows.into_iter()
        .map(|r| {
            let head = (r.letter != letter).then(|| {
                letter = r.letter.clone();
                view! { <div class="contact-letter" aria-hidden="true">{r.letter.clone()}</div> }
            });
            view! { {head} {row_view(ui, r, false)} }
        })
        .collect_view()
}

/// Vue Cartes (cartes de visite) : nom, société, adresse et téléphone ; double clic pour modifier.
fn cards(ui: Ui, rows: Vec<ContactRow>) -> impl IntoView {
    let items = rows
        .into_iter()
        .map(|r| {
            let (id, sel, aria, open) = (r.id.clone(), r.id.clone(), r.id.clone(), r.id.clone());
            let line = |s: String| (!s.is_empty()).then(|| view! { <span class="contact-sub">{s}</span> });
            view! {
                <div
                    class="contact-row"
                    id=format!("contact-{}", r.id)
                    role="option"
                    style="flex-direction:column;align-items:stretch;gap:2px;border:1px solid var(--border);border-radius:4px;min-width:0"
                    class:selected=move || ui.contact_selected.get().as_deref() == Some(sel.as_str())
                    aria-selected=move || (ui.contact_selected.get().as_deref() == Some(aria.as_str())).to_string()
                    on:click=move |_| ui.contact_selected.set(Some(id.clone()))
                    on:dblclick=move |_| crate::contact_form::open_edit(ui, open.clone())
                >
                    <span class="contact-title" style="border-bottom:1px solid var(--border);padding-bottom:2px">{r.title.clone()}</span>
                    {line(r.company.clone())}
                    {line(r.email.clone())}
                    {line(r.phone.clone())}
                </div>
            }
        })
        .collect_view();
    view! { <div style="display:grid;grid-template-columns:repeat(auto-fill,minmax(200px,1fr));gap:8px;padding:8px">{items}</div> }
}

/// Vue Liste : tableau Nom, Société, Courrier, Téléphone ; clic sur Nom ou Société pour trier.
fn table(ui: Ui, rows: Vec<ContactRow>, sort: RwSignal<(SortCol, bool)>) -> impl IntoView {
    let head = move |col: SortCol, label: &'static str| {
        let arrow = move || match sort.get() {
            (c, true) if c == col => " ▲",
            (c, false) if c == col => " ▼",
            _ => "",
        };
        view! {
            <th style="text-align:left;padding:4px 8px;cursor:pointer;white-space:nowrap" role="columnheader"
                aria-sort=move || match sort.get() { (c, true) if c == col => "ascending", (c, false) if c == col => "descending", _ => "none" }
                on:click=move |_| sort.update(|s| *s = if s.0 == col { (col, !s.1) } else { (col, true) })>
                {label}{arrow}
            </th>
        }
    };
    let body = rows
        .into_iter()
        .map(|r| {
            let (id, sel, aria, open) = (r.id.clone(), r.id.clone(), r.id.clone(), r.id.clone());
            let cell = "padding:4px 8px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:220px";
            view! {
                <tr
                    class="contact-row"
                    style="display:table-row"
                    id=format!("contact-{}", r.id)
                    role="option"
                    class:selected=move || ui.contact_selected.get().as_deref() == Some(sel.as_str())
                    aria-selected=move || (ui.contact_selected.get().as_deref() == Some(aria.as_str())).to_string()
                    on:click=move |_| ui.contact_selected.set(Some(id.clone()))
                    on:dblclick=move |_| crate::contact_form::open_edit(ui, open.clone())
                >
                    <td style=cell>{r.title.clone()}</td>
                    <td style=cell>{r.company.clone()}</td>
                    <td style=cell>{r.email.clone()}</td>
                    <td style=cell>{r.phone.clone()}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <table style="width:100%;border-collapse:collapse;table-layout:auto">
            <thead style="border-bottom:1px solid var(--border)">
                <tr>
                    {head(SortCol::Name, "Nom")}
                    {head(SortCol::Company, "Société")}
                    <th style="text-align:left;padding:4px 8px">"Courrier"</th>
                    <th style="text-align:left;padding:4px 8px">"Téléphone"</th>
                </tr>
            </thead>
            <tbody>{body}</tbody>
        </table>
    }
}

/// Volet de droite d'une liste de distribution : membres (clic = message à ce membre), message à tous, modifier, supprimer.
#[component]
fn ListCard(row: ContactRow, editing: RwSignal<Option<ListDraft>>) -> impl IntoView {
    let ui = use_ui();
    let draft = ListDraft { id: Some(row.id.clone()), folder: ui.contact_folder.get_untracked(), name: row.title.clone(), members: row.members.clone() };
    view! {
        <article class="contact-card" aria-label=row.title.clone()>
            <header class="contact-head">
                <span class="avatar large" aria-hidden="true"><Svg icon=USERS size=28 /></span>
                <div>
                    <div class="contact-actions">
                        <button class="btn" on:click=move |_| message_selected(ui)>"Nouveau message"</button>
                        <button class="btn" on:click=move |_| editing.set(Some(draft.clone()))>"Modifier"</button>
                        <button class="btn" on:click=move |_| delete_selected(ui)>"Supprimer"</button>
                    </div>
                    <h2>{row.title.clone()}</h2>
                    <p class="contact-job">{row.subtitle.clone()}</p>
                </div>
            </header>
            <section class="contact-section">
                <h3>"Membres"</h3>
                <dl>
                    {row
                        .members
                        .into_iter()
                        .map(|m| {
                            let (name, addr) = (m.name.clone(), m.email.clone());
                            view! {
                                <dt>{if m.name.is_empty() { m.email.clone() } else { m.name.clone() }}</dt>
                                <dd>
                                    <button class="link-btn" title="Nouveau message à ce membre" on:click=move |_| write_to(ui, name.clone(), addr.clone())>{m.email.clone()}</button>
                                </dd>
                            }
                        })
                        .collect_view()}
                </dl>
            </section>
        </article>
    }
}

/// Boîte « Liste de distribution » : nom, membres (nom et adresse ; l'adresse se complète avec celles des contacts du
/// dossier), ajout et retrait. Ctrl+S enregistre, Échap ferme.
#[component]
fn ListDialog(editing: RwSignal<Option<ListDraft>>) -> impl IntoView {
    let ui = use_ui();
    let busy = RwSignal::new(false);
    let note = RwSignal::new(None::<String>);
    let (new_name, new_addr) = (RwSignal::new(String::new()), RwSignal::new(String::new()));
    let close = move || {
        editing.set(None);
        note.set(None);
    };
    let set = move |f: &dyn Fn(&mut ListDraft)| editing.update(|d| if let Some(d) = d { f(d) });
    let add = move || {
        let (name, addr) = (new_name.get_untracked().trim().to_string(), new_addr.get_untracked().trim().to_string());
        if addr.is_empty() {
            return;
        }
        // Adresse choisie dans les suggestions : le nom du contact vient avec elle.
        let name = if name.is_empty() {
            ui.contact_list.with_untracked(|l| l.as_ref().and_then(|l| l.rows.iter().find(|r| r.email.eq_ignore_ascii_case(&addr)).map(|r| r.title.clone()))).unwrap_or_default()
        } else {
            name
        };
        set(&|d| d.members.push(DistMember { name: name.clone(), email: addr.clone() }));
        new_name.set(String::new());
        new_addr.set(String::new());
    };
    let save = move || {
        let Some(d) = editing.get_untracked() else { return };
        busy.set(true);
        spawn_local(async move {
            let args = SaveListArgs { folder: &d.folder, id: d.id.as_deref(), name: &d.name, members: &d.members };
            match call::<String>("contact_dist_list_save", &args).await {
                Ok(id) => {
                    ui.status.set(Some("Liste de distribution enregistrée".into()));
                    ui.contact_selected.set(Some(id));
                    close();
                }
                Err(e) => note.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let known = move || ui.contact_list.with(|l| l.as_ref().map(|l| l.rows.iter().filter(|r| r.email.contains('@')).map(|r| (r.email.clone(), r.title.clone())).collect::<Vec<_>>()).unwrap_or_default());
    view! {
        <Show when=move || editing.with(Option::is_some)>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog wide" role="dialog" aria-modal="true" aria-labelledby="dl-title"
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
                    <h2 id="dl-title">{move || if editing.with(|d| d.as_ref().is_some_and(|d| d.id.is_some())) { "Liste de distribution" } else { "Nouvelle liste de distribution" }}</h2>
                    <label class="field">
                        <span>"Nom"</span>
                        <input type="text" aria-label="Nom de la liste" autofocus
                            prop:value=move || editing.with(|d| d.as_ref().map(|d| d.name.clone()).unwrap_or_default())
                            on:input=move |ev| { let v = event_target_value(&ev); set(&|d| d.name = v.clone()); } />
                    </label>
                    <h3>"Membres"</h3>
                    <div role="list" style="max-height:40vh;overflow:auto">
                        {move || editing.with(|d| d.as_ref().map(|d| d.members.clone()).unwrap_or_default())
                            .into_iter()
                            .enumerate()
                            .map(|(i, m)| view! {
                                <div role="listitem" style="display:flex;gap:8px;align-items:center;padding:2px 0">
                                    <span style="flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis">{if m.name.is_empty() { m.email.clone() } else { format!("{} <{}>", m.name, m.email) }}</span>
                                    <button class="btn" title="Retirer ce membre" on:click=move |_| set(&|d| { if i < d.members.len() { d.members.remove(i); } })>"Retirer"</button>
                                </div>
                            })
                            .collect_view()}
                    </div>
                    <div class="appt-row">
                        <label class="field">
                            <span>"Adresse"</span>
                            <input type="email" list="dl-known" aria-label="Adresse du membre à ajouter" prop:value=move || new_addr.get()
                                on:input=move |ev| new_addr.set(event_target_value(&ev))
                                on:keydown=move |ev| if ev.key() == "Enter" { ev.prevent_default(); add(); } />
                        </label>
                        <label class="field">
                            <span>"Nom (facultatif)"</span>
                            <input type="text" aria-label="Nom du membre à ajouter" prop:value=move || new_name.get()
                                on:input=move |ev| new_name.set(event_target_value(&ev))
                                on:keydown=move |ev| if ev.key() == "Enter" { ev.prevent_default(); add(); } />
                        </label>
                        <button class="btn" type="button" on:click=move |_| add()>"Ajouter"</button>
                    </div>
                    <datalist id="dl-known">
                        {move || known().into_iter().map(|(addr, title)| view! { <option value=addr>{title}</option> }).collect_view()}
                    </datalist>
                    <p class="dialog-note">"Les membres sont enregistrés par nom et adresse (sans lien vers la fiche du contact)."</p>
                    {move || note.get().map(|n| view! { <p class="dialog-note" role="alert">{n}</p> })}
                    <div class="dialog-buttons">
                        <button class="btn primary" prop:disabled=move || busy.get() on:click=move |_| save()>"Enregistrer"</button>
                        <button class="btn" on:click=move |_| close()>"Annuler"</button>
                    </div>
                </div>
            </div>
        </Show>
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
                        <button class="btn" on:click=move |_| message_selected(ui)>"Nouveau message"</button>
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
