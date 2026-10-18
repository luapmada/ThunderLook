//! Contenus des menus d'organisation (ruban et menus contextuels) et menu contextuel positionné.

use leptos::{html, prelude::*};
use thunderlook_model::{FlagAction, FolderOp, FolderRole, MessageOp};
use wasm_bindgen::JsCast;

use crate::{
    actions::{self, folder_name, real_folder},
    menu::{MenuEntry, entry_body, items, move_focus},
    state::{ConfirmAction, ContextTarget, Dialog, PickPurpose, Ui, use_ui},
};

pub fn flag_entries(ui: Ui) -> Vec<MenuEntry> {
    let mut v: Vec<MenuEntry> = [FlagAction::Today, FlagAction::Tomorrow, FlagAction::ThisWeek, FlagAction::NextWeek, FlagAction::NoDate]
        .into_iter()
        .map(|a| MenuEntry::action(a.label(), move || actions::run(ui, MessageOp::Flag { action: a })))
        .collect();
    v.push(MenuEntry::action(FlagAction::Complete.label(), move || actions::run(ui, MessageOp::Flag { action: FlagAction::Complete })).separated());
    v.push(MenuEntry::action(FlagAction::Clear.label(), move || actions::run(ui, MessageOp::Flag { action: FlagAction::Clear })));
    v
}

/// Catégories cochées d'après l'élément courant ; cocher applique à toute la sélection.
pub fn category_entries(ui: Ui) -> Vec<MenuEntry> {
    let current: Vec<String> = ui
        .selected_pos
        .get_untracked()
        .and_then(|p| ui.rows.with_untracked(|r| r.get(&p).map(|m| m.categories.iter().map(|c| c.name.clone()).collect())))
        .unwrap_or_default();
    let mut v: Vec<MenuEntry> = ui
        .categories
        .get()
        .into_iter()
        .map(|c| {
            let on = current.iter().any(|n| n.eq_ignore_ascii_case(&c.name));
            let name = c.name.clone();
            MenuEntry::radio(c.name, on, move || actions::run(ui, MessageOp::Category { name: name.clone(), on: !on })).with_swatch(c.color)
        })
        .collect();
    v.push(MenuEntry::action("Effacer toutes les catégories", move || actions::run(ui, MessageOp::ClearCategories)).separated());
    v.push(MenuEntry::action("Nouvelle catégorie…", move || ui.dialog.set(Some(Dialog::NewCategory))));
    v
}

pub fn move_entries(ui: Ui) -> Vec<MenuEntry> {
    let mut v: Vec<MenuEntry> = ui
        .recent_folders
        .get()
        .into_iter()
        .filter(|id| ui.tree.with_untracked(|t| t.as_ref().is_some_and(|t| t.find_folder(id).is_some())))
        .map(|id| {
            let name = folder_name(ui, &id);
            MenuEntry::action(name, move || actions::run(ui, MessageOp::Move { folder_id: id.clone() }))
        })
        .collect();
    let first = v.is_empty();
    let mut other = MenuEntry::action("Autre dossier…", move || ui.dialog.set(Some(Dialog::PickFolder { purpose: PickPurpose::MoveMessages })));
    if !first {
        other = other.separated();
    }
    v.push(other);
    v.push(MenuEntry::action("Copier dans un dossier…", move || ui.dialog.set(Some(Dialog::PickFolder { purpose: PickPurpose::CopyMessages }))));
    v
}

pub fn message_context(ui: Ui) -> Vec<MenuEntry> {
    vec![
        MenuEntry::action("Marquer comme lu", move || actions::run(ui, MessageOp::Read { read: true })),
        MenuEntry::action("Marquer comme non lu", move || actions::run(ui, MessageOp::Read { read: false })),
        MenuEntry::submenu("Catégoriser", category_entries(ui)).separated(),
        MenuEntry::submenu("Assurer un suivi", flag_entries(ui)),
        MenuEntry::submenu("Déplacer", move_entries(ui)).separated(),
        MenuEntry::action("Archiver", move || actions::run(ui, MessageOp::Archive)),
        MenuEntry::action("Supprimer", move || actions::delete(ui, false)).separated(),
        MenuEntry::action("Supprimer définitivement", move || actions::delete(ui, true)),
    ]
}

pub fn is_favorite(ui: Ui, id: &str) -> bool {
    let real = real_folder(id);
    ui.tree.with_untracked(|t| {
        t.as_ref().is_some_and(|t| t.sections.iter().any(|s| s.id == "favorites" && s.folders.iter().any(|f| real_folder(&f.id) == real)))
    })
}

fn folder_role(ui: Ui, id: &str) -> FolderRole {
    ui.tree.with_untracked(|t| t.as_ref().and_then(|t| t.find_folder(id)).map(|f| f.role).unwrap_or(FolderRole::Folder))
}

pub fn folder_context(ui: Ui, id: String) -> Vec<MenuEntry> {
    let real = real_folder(&id);
    let special = folder_role(ui, &real) != FolderRole::Folder && folder_role(ui, &real) != FolderRole::Archive;
    let name = folder_name(ui, &real);
    let fav = is_favorite(ui, &id);
    let mut v = vec![MenuEntry::action("Nouveau dossier…", {
        let real = real.clone();
        move || ui.dialog.set(Some(Dialog::NewFolder { parent: real.clone() }))
    })];
    if special {
        v.push(MenuEntry::pending("Renommer le dossier"));
        v.push(MenuEntry::pending("Déplacer le dossier"));
        v.push(MenuEntry::pending("Supprimer le dossier"));
    } else {
        v.push(MenuEntry::action("Renommer le dossier", {
            let (real, name) = (real.clone(), name.clone());
            move || ui.dialog.set(Some(Dialog::Rename { id: real.clone(), name: name.clone() }))
        }));
        v.push(MenuEntry::action("Déplacer le dossier…", {
            let real = real.clone();
            move || {
                ui.tree_focus.set(real.clone());
                ui.dialog.set(Some(Dialog::PickFolder { purpose: PickPurpose::MoveFolder }))
            }
        }));
        v.push(MenuEntry::action("Supprimer le dossier", {
            let (real, name) = (real.clone(), name.clone());
            move || {
                ui.dialog.set(Some(Dialog::Confirm {
                    text: format!("Supprimer le dossier « {name} » et tout son contenu ?"),
                    action: ConfirmAction::DeleteFolder(real.clone()),
                }))
            }
        }));
    }
    v.push(
        MenuEntry::action("Tout marquer comme lu", {
            let real = real.clone();
            move || actions::run_folder(ui, FolderOp::MarkAllRead { id: real.clone() })
        })
        .separated(),
    );
    v.push(MenuEntry::action("Vider le dossier", {
        let (real, name) = (real.clone(), name.clone());
        move || {
            ui.dialog.set(Some(Dialog::Confirm {
                text: format!("Supprimer tous les éléments du dossier « {name} » ?"),
                action: ConfirmAction::EmptyFolder(real.clone()),
            }))
        }
    }));
    v.push(
        if fav {
            MenuEntry::action("Retirer des favoris", move || actions::run_folder(ui, FolderOp::RemoveFavorite { id: real.clone() }))
        } else {
            MenuEntry::action("Ajouter aux favoris", move || actions::run_folder(ui, FolderOp::AddFavorite { id: real.clone() }))
        }
        .separated(),
    );
    v
}

/// Ouvre le menu contextuel sur l'élément ayant le focus (touche Menu, Maj+F10).
pub fn open_at_element(ui: Ui, el: &web_sys::Element, target: ContextTarget) {
    let r = el.get_bounding_client_rect();
    ui.context_menu.set(Some(crate::state::ContextMenuAt { x: r.left() + 24.0, y: r.top() + r.height() / 2.0, target }));
}

#[component]
pub fn ContextMenu() -> impl IntoView {
    let ui = use_ui();
    let menu: NodeRef<html::Div> = NodeRef::new();
    // Sous-menu affiché à la place du menu principal.
    let sub = RwSignal::new(None::<(String, Vec<MenuEntry>)>);

    let root = Memo::new(move |_| ui.context_menu.get());
    let entries = move || -> Vec<MenuEntry> {
        match (sub.get(), root.get()) {
            (Some((_, children)), _) => children,
            (None, Some(at)) => match at.target {
                ContextTarget::Messages => message_context(ui),
                ContextTarget::Folder(id) => folder_context(ui, id),
            },
            (None, None) => Vec::new(),
        }
    };

    let close = move || {
        sub.set(None);
        ui.context_menu.set(None);
    };

    let outside = window_event_listener(leptos::ev::pointerdown, move |ev| {
        if ui.context_menu.get_untracked().is_none() {
            return;
        }
        let inside = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
            .zip(menu.get_untracked())
            .is_some_and(|(t, m)| m.contains(Some(&t)));
        if !inside {
            close();
        }
    });
    on_cleanup(move || outside.remove());

    Effect::new(move |_| {
        root.track();
        sub.track();
        if let Some(m) = menu.get() {
            request_animation_frame(move || move_focus(&m, 1));
        }
    });

    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        let Some(m) = menu.get_untracked() else { return };
        match ev.key().as_str() {
            "ArrowDown" => move_focus(&m, 1),
            "ArrowUp" => move_focus(&m, -1),
            "Home" => {
                if let Some(f) = items(&m).first() {
                    let _ = f.focus();
                }
            }
            "End" => {
                if let Some(l) = items(&m).last() {
                    let _ = l.focus();
                }
            }
            "ArrowLeft" if sub.get_untracked().is_some() => sub.set(None),
            "ArrowRight" => {
                if let Some(a) = document().active_element() {
                    let _ = a.dyn_into::<web_sys::HtmlElement>().map(|e| e.click());
                }
            }
            "Escape" => close(),
            "Tab" => close(),
            _ => return,
        }
        ev.prevent_default();
        ev.stop_propagation();
    };

    let style = move || {
        root.get()
            .map(|at| {
                // Reste dans la fenêtre : menu d'environ 260 × 360 px au plus.
                let w = window().inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1200.0);
                let h = window().inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(800.0);
                format!("left: {}px; top: {}px", at.x.min(w - 270.0).max(0.0), at.y.min(h - 370.0).max(0.0))
            })
            .unwrap_or_default()
    };

    view! {
        <Show when=move || root.get().is_some()>
            <div class="menu context-menu" role="menu" aria-label="Menu contextuel" style=style node_ref=menu on:keydown=on_key>
                {move || {
                    let back = sub.get().map(|(label, _)| {
                        view! {
                            <button role="menuitem" tabindex="-1" class="menu-back" on:click=move |_| sub.set(None)>
                                <span class="check"></span>
                                {format!("‹ {label}")}
                            </button>
                            <div class="menu-sep" role="separator"></div>
                        }
                    });
                    let list = entries()
                        .into_iter()
                        .map(|entry| {
                            let disabled = !entry.is_enabled();
                            let role = if entry.checked.is_some() { "menuitemradio" } else { "menuitem" };
                            let body = entry_body(&entry);
                            let on_select = entry.on_select;
                            let children = entry.children.clone();
                            let label = entry.label.clone();
                            view! {
                                {entry.separator.then(|| view! { <div class="menu-sep" role="separator"></div> })}
                                <button
                                    role=role
                                    tabindex="-1"
                                    aria-checked=entry.checked.map(|c| c.to_string())
                                    aria-disabled=disabled.then_some("true")
                                    aria-haspopup=(!children.is_empty()).then_some("menu")
                                    on:click=move |_| {
                                        if !children.is_empty() {
                                            sub.set(Some((label.clone(), children.clone())));
                                        } else if let Some(cb) = on_select {
                                            close();
                                            cb.run(());
                                        }
                                    }
                                >
                                    {body}
                                </button>
                            }
                        })
                        .collect_view();
                    view! { {back} {list} }
                }}
            </div>
        </Show>
    }
}
