//! Boîtes de dialogue modales du J3 : nouveau dossier, renommer, choix d'un dossier, confirmation,
//! nouvelle catégorie ; réglages de l'affichage (J3b). Échap annule, Entrée valide, le focus reste dans la boîte.

use leptos::{html, prelude::*};
use thunderlook_model::{Folder, FolderOp, MessageOp, SortField, settings::Density};
use wasm_bindgen::JsCast;

use crate::{
    actions::{self, folder_name, real_folder},
    state::{Dialog, PickPurpose, use_ui},
};

/// Dossiers réels (hors favoris), aplatis avec leur profondeur.
pub(crate) fn flat_folders(ui: crate::state::Ui) -> Vec<(String, String, usize, String)> {
    fn walk(out: &mut Vec<(String, String, usize, String)>, folders: &[Folder], depth: usize, section: &str) {
        for f in folders {
            out.push((f.id.clone(), f.name.clone(), depth, section.to_string()));
            walk(out, &f.children, depth + 1, section);
        }
    }
    let mut out = Vec::new();
    ui.tree.with_untracked(|t| {
        if let Some(t) = t {
            for s in t.sections.iter().filter(|s| s.id != "favorites") {
                walk(&mut out, &s.folders, 0, &s.name);
            }
        }
    });
    out
}

fn focus_first(dialog: &web_sys::HtmlElement) {
    if let Ok(Some(el)) = dialog.query_selector("input, [data-autofocus], button") {
        if let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() {
            let _ = el.focus();
        }
    }
}

#[component]
pub fn Dialogs() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let text = RwSignal::new(String::new());
    let picked = RwSignal::new(0usize);
    // Réglages de l'affichage, appliqués seulement sur OK.
    let v_sort = RwSignal::new(SortField::Date);
    let v_desc = RwSignal::new(true);
    let v_conv = RwSignal::new(false);
    let v_compact = RwSignal::new(false);
    let v_mark_read = RwSignal::new(true);

    // À chaque ouverture : champ prérempli et focus.
    Effect::new(move |_| {
        let d = ui.dialog.get();
        text.set(match &d {
            Some(Dialog::Rename { name, .. }) => name.clone(),
            Some(Dialog::SaveSearch) => ui.search_text.get_untracked(),
            _ => String::new(),
        });
        picked.set(0);
        if d == Some(Dialog::ViewSettings) {
            let (sort, desc) = ui.sort.get_untracked();
            let s = ui.settings.get_untracked();
            v_sort.set(sort);
            v_desc.set(desc);
            v_conv.set(s.conversations);
            v_compact.set(s.density == Density::Compact);
            v_mark_read.set(s.mark_read_on_leave);
        }
        if d.is_some() {
            request_animation_frame(move || {
                if let Some(r) = root.get_untracked() {
                    focus_first(&r);
                    if let Ok(Some(i)) = r.query_selector("input") {
                        if let Ok(i) = i.dyn_into::<web_sys::HtmlInputElement>() {
                            i.select();
                        }
                    }
                }
            });
        }
    });

    let close = move || {
        let was = ui.dialog.get_untracked();
        ui.dialog.set(None);
        // Le focus revient au volet d'origine.
        let target: Option<web_sys::HtmlElement> = match was {
            Some(Dialog::NewFolder { .. } | Dialog::Rename { .. }) => ui.refs.tree.get_untracked().map(|e| e.unchecked_into()),
            Some(Dialog::PickFolder { purpose: PickPurpose::MoveFolder }) => ui.refs.tree.get_untracked().map(|e| e.unchecked_into()),
            _ => ui.refs.list.get_untracked().map(|e| e.unchecked_into()),
        };
        if let Some(t) = target {
            let _ = t.focus();
        }
    };

    let candidates = move || {
        let filter = text.get().to_lowercase();
        flat_folders(ui)
            .into_iter()
            .filter(|(_, name, _, _)| filter.is_empty() || name.to_lowercase().contains(&filter))
            .collect::<Vec<_>>()
    };

    let submit = move || {
        let Some(d) = ui.dialog.get_untracked() else { return };
        let value = text.get_untracked();
        match d {
            Dialog::NewFolder { parent } => actions::run_folder(ui, FolderOp::Create { parent_id: parent, name: value }),
            Dialog::Rename { id, .. } => actions::run_folder(ui, FolderOp::Rename { id, name: value }),
            Dialog::NewCategory => {
                if !value.trim().is_empty() {
                    actions::run(ui, MessageOp::Category { name: value.trim().to_string(), on: true });
                }
            }
            Dialog::Confirm { action, .. } => actions::confirm(ui, action),
            Dialog::ViewSettings => {
                ui.sort.set((v_sort.get_untracked(), v_desc.get_untracked()));
                ui.update_settings(|s| {
                    s.conversations = v_conv.get_untracked();
                    s.density = if v_compact.get_untracked() { Density::Compact } else { Density::Comfortable };
                    s.mark_read_on_leave = v_mark_read.get_untracked();
                });
            }
            Dialog::SaveSearch => actions::run_folder(
                ui,
                FolderOp::SaveSearch {
                    name: value,
                    query: ui.search_text.get_untracked(),
                    scope: ui.search_scope.get_untracked(),
                    folder_id: real_folder(&ui.folder.get_untracked()),
                },
            ),
            Dialog::Rules { .. } | Dialog::QuickSteps { .. } | Dialog::Accounts => {}
            Dialog::PickFolder { purpose } => {
                let list = candidates();
                let Some((id, ..)) = list.get(picked.get_untracked()).cloned() else { return };
                match purpose {
                    PickPurpose::MoveMessages => actions::run(ui, MessageOp::Move { folder_id: id }),
                    PickPurpose::CopyMessages => actions::run(ui, MessageOp::Copy { folder_id: id }),
                    PickPurpose::MoveFolder => {
                        let source = real_folder(&ui.tree_focus.get_untracked());
                        actions::run_folder(ui, FolderOp::Move { id: source, parent_id: id });
                    }
                }
            }
        }
        close();
    };

    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        match ev.key().as_str() {
            "Escape" => close(),
            "Enter" => submit(),
            "ArrowDown" | "ArrowUp" if matches!(ui.dialog.get_untracked(), Some(Dialog::PickFolder { .. })) => {
                let n = candidates().len();
                if n > 0 {
                    let cur = picked.get_untracked();
                    let next = if ev.key() == "ArrowDown" { (cur + 1).min(n - 1) } else { cur.saturating_sub(1) };
                    picked.set(next);
                    if let Some(el) = document().get_element_by_id(&format!("pick-{next}")) {
                        el.scroll_into_view_with_bool(false);
                    }
                }
            }
            "Tab" => {
                // Le focus reste dans la boîte.
                if let Some(r) = root.get_untracked() {
                    let Ok(list) = r.query_selector_all("input, select, button") else { return };
                    let els: Vec<web_sys::HtmlElement> = (0..list.length()).filter_map(|i| list.item(i)?.dyn_into().ok()).collect();
                    let active = document().active_element();
                    let pos = els.iter().position(|e| active.as_ref().is_some_and(|a| a == e.unchecked_ref::<web_sys::Element>()));
                    let n = els.len() as i32;
                    if n > 0 {
                        let next = (pos.map_or(0, |p| p as i32) + if ev.shift_key() { -1 } else { 1 }).rem_euclid(n);
                        let _ = els[next as usize].focus();
                    }
                }
            }
            _ => return,
        }
        ev.prevent_default();
        ev.stop_propagation();
    };

    let title = move || match ui.dialog.get() {
        Some(Dialog::NewFolder { .. }) => "Nouveau dossier",
        Some(Dialog::Rename { .. }) => "Renommer le dossier",
        Some(Dialog::PickFolder { purpose: PickPurpose::MoveMessages }) => "Déplacer les éléments",
        Some(Dialog::PickFolder { purpose: PickPurpose::CopyMessages }) => "Copier les éléments",
        Some(Dialog::PickFolder { purpose: PickPurpose::MoveFolder }) => "Déplacer le dossier",
        Some(Dialog::Confirm { .. }) => "Confirmer",
        Some(Dialog::NewCategory) => "Nouvelle catégorie",
        Some(Dialog::SaveSearch) => "Enregistrer la recherche",
        Some(Dialog::ViewSettings) => "Réglages de l'affichage",
        Some(Dialog::Rules { .. } | Dialog::QuickSteps { .. } | Dialog::Accounts) | None => "",
    };

    view! {
        // « Règles et alertes » et les actions rapides ont leur propre boîte (`rules.rs`, `quicksteps.rs`).
        <Show when=move || ui.dialog.get().is_some_and(|d| !matches!(d, Dialog::Rules { .. } | Dialog::QuickSteps { .. } | Dialog::Accounts))>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="dialog-title" node_ref=root on:keydown=on_key>
                    <h2 id="dialog-title">{title}</h2>
                    {move || match ui.dialog.get() {
                        Some(Dialog::NewFolder { parent }) => view! {
                            <label class="field">
                                <span>"Nom"</span>
                                <input type="text" prop:value=move || text.get() on:input=move |ev| text.set(event_target_value(&ev)) />
                            </label>
                            <p class="dialog-note">{format!("Dans « {} »", folder_name(ui, &parent))}</p>
                        }
                        .into_any(),
                        Some(Dialog::SaveSearch) => view! {
                            <label class="field">
                                <span>"Nom du dossier de recherche"</span>
                                <input type="text" prop:value=move || text.get() on:input=move |ev| text.set(event_target_value(&ev)) />
                            </label>
                            <p class="dialog-note">{move || format!("Recherche : {}", ui.search_text.get())}</p>
                        }
                        .into_any(),
                        Some(Dialog::Rename { .. }) | Some(Dialog::NewCategory) => view! {
                            <label class="field">
                                <span>"Nom"</span>
                                <input type="text" prop:value=move || text.get() on:input=move |ev| text.set(event_target_value(&ev)) />
                            </label>
                        }
                        .into_any(),
                        Some(Dialog::Confirm { text: t, .. }) => view! { <p data-autofocus tabindex="-1">{t}</p> }.into_any(),
                        Some(Dialog::ViewSettings) => view! {
                            <label class="field">
                                <span>"Organiser par"</span>
                                <select
                                    data-autofocus
                                    on:change=move |ev| {
                                        let i: usize = event_target_value(&ev).parse().unwrap_or(0);
                                        let f = SortField::ALL[i.min(SortField::ALL.len() - 1)];
                                        v_sort.set(f);
                                        v_desc.set(f.default_descending());
                                    }
                                >
                                    {SortField::ALL
                                        .into_iter()
                                        .enumerate()
                                        .map(|(i, f)| view! { <option value=i.to_string() selected=move || v_sort.get() == f>{f.label()}</option> })
                                        .collect_view()}
                                </select>
                            </label>
                            <label class="check">
                                <input type="checkbox" prop:checked=move || v_desc.get() on:change=move |ev| v_desc.set(event_target_checked(&ev)) />
                                <span>"Ordre décroissant"</span>
                            </label>
                            <label class="check">
                                <input type="checkbox" prop:checked=move || v_conv.get() on:change=move |ev| v_conv.set(event_target_checked(&ev)) />
                                <span>"Afficher en tant que conversations"</span>
                            </label>
                            <p class="dialog-note">"Les conversations s'appliquent au classement par date, pour tous les dossiers."</p>
                            <label class="check">
                                <input type="checkbox" prop:checked=move || v_compact.get() on:change=move |ev| v_compact.set(event_target_checked(&ev)) />
                                <span>"Utiliser un espacement plus étroit"</span>
                            </label>
                            <label class="check">
                                <input
                                    type="checkbox"
                                    prop:checked=move || v_mark_read.get()
                                    on:change=move |ev| v_mark_read.set(event_target_checked(&ev))
                                />
                                <span>"Marquer l'élément comme lu quand la sélection change"</span>
                            </label>
                        }
                        .into_any(),
                        Some(Dialog::PickFolder { .. }) => view! {
                            <label class="field">
                                <span>"Rechercher un dossier"</span>
                                <input
                                    type="text"
                                    role="combobox"
                                    aria-controls="pick-list"
                                    aria-expanded="true"
                                    aria-activedescendant=move || format!("pick-{}", picked.get())
                                    prop:value=move || text.get()
                                    on:input=move |ev| {
                                        text.set(event_target_value(&ev));
                                        picked.set(0);
                                    }
                                />
                            </label>
                            <div class="pick-list" id="pick-list" role="listbox" aria-label="Dossiers">
                                {move || {
                                    let mut last_section = String::new();
                                    candidates()
                                        .into_iter()
                                        .enumerate()
                                        .map(|(i, (_, name, depth, section))| {
                                            let head = (section != last_section).then(|| {
                                                last_section = section.clone();
                                                view! { <div class="pick-section" aria-hidden="true">{section}</div> }
                                            });
                                            view! {
                                                {head}
                                                <div
                                                    role="option"
                                                    id=format!("pick-{i}")
                                                    class="pick-item"
                                                    aria-selected=move || (picked.get() == i).to_string()
                                                    style=format!("padding-left: {}px", 8 + depth * 16)
                                                    on:click=move |_| picked.set(i)
                                                    on:dblclick=move |_| {
                                                        picked.set(i);
                                                        submit();
                                                    }
                                                >
                                                    {name}
                                                </div>
                                            }
                                        })
                                        .collect_view()
                                }}
                            </div>
                        }
                        .into_any(),
                        Some(Dialog::Rules { .. } | Dialog::QuickSteps { .. } | Dialog::Accounts) | None => ().into_any(),
                    }}
                    <div class="dialog-buttons">
                        <button class="btn primary" on:click=move |_| submit()>"OK"</button>
                        <button class="btn" on:click=move |_| close()>"Annuler"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
