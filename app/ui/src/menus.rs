//! Contenus des menus d'organisation (ruban et menus contextuels) et menu contextuel positionné.

use leptos::{html, prelude::*};
use leptos::task::spawn_local;
use thunderlook_model::{ComposeKind, FlagAction, FolderOp, FolderRole, JunkMark, MessageOp};
use wasm_bindgen::JsCast;

use crate::{
    actions::{self, folder_name, real_folder},
    appointment,
    ipc,
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

/// Clic droit sur un rendez-vous (libellés relevés dans Outlook). Les changements passent par la boîte de rendez-vous
/// enregistrée dans Outlook, donc seuls les rendez-vous du calendrier commun sont modifiables.
fn calendar_event_context(ui: Ui, id: String) -> Vec<MenuEntry> {
    let Some(ev) = ui.cal_selected.get_untracked().filter(|e| e.id == id) else { return Vec::new() };
    let shared = ui.cal_view.with_untracked(|v| v.as_ref().is_some_and(|v| v.calendars.iter().any(|c| c.id == ev.calendar_id && c.shared)));
    let editable = move |e: MenuEntry| if shared { e } else { MenuEntry::pending(e.label) };
    let busy_choices = [("Disponible", 0, "free"), ("Provisoire", 1, "tentative"), ("Occupé(e)", 2, "busy"), ("Absent(e)", 3, "oof"), ("Travaille ailleurs", 4, "elsewhere")];
    let busy: Vec<MenuEntry> = busy_choices
        .iter()
        .map(|&(label, code, key)| {
            let id = id.clone();
            editable(MenuEntry::radio(label, ev.busy == key, move || appointment::update(ui, id.clone(), move |f| f.busy = code)))
        })
        .collect();
    let current: Vec<String> = ev.categories.clone();
    let mut cats: Vec<MenuEntry> = ui
        .categories
        .get()
        .into_iter()
        .map(|c| {
            let on = current.iter().any(|n| n.eq_ignore_ascii_case(&c.name));
            let (id, name) = (id.clone(), c.name.clone());
            editable(
                MenuEntry::radio(c.name, on, move || {
                    let name = name.clone();
                    appointment::update(ui, id.clone(), move |f| {
                        let mut list: Vec<String> = f.categories.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                        if on {
                            list.retain(|n| !n.eq_ignore_ascii_case(&name));
                        } else {
                            list.push(name);
                        }
                        f.categories = list.join(", ");
                    })
                })
                .with_swatch(c.color),
            )
        })
        .collect();
    let clear_id = id.clone();
    cats.push(editable(MenuEntry::action("Effacer toutes les catégories", move || appointment::update(ui, clear_id.clone(), |f| f.categories.clear()))).separated());
    let (private_id, delete_id) = (id.clone(), id.clone());
    let private = ev.private;
    vec![
        MenuEntry::pending("Transférer"),
        editable(MenuEntry::radio("Privé", private, move || appointment::update(ui, private_id.clone(), move |f| f.private = !private))).separated(),
        MenuEntry::submenu("Afficher en tant que", busy),
        MenuEntry::submenu("Classer", cats),
        editable(MenuEntry::action("Supprimer", move || appointment::delete(ui, delete_id.clone(), false))).separated(),
    ]
}

/// Clic droit sur un créneau vide.
fn calendar_slot_context(ui: Ui, start: String) -> Vec<MenuEntry> {
    let (a, b) = (start.clone(), start);
    vec![
        MenuEntry::action("Nouveau rendez-vous", move || appointment::open_new_at(ui, Some(a.clone()), false)),
        MenuEntry::action("Nouvel événement d'une journée entière", move || appointment::open_new_at(ui, Some(b.clone()), true)),
        MenuEntry::pending("Nouvelle demande de réunion"),
        MenuEntry::pending("Nouveau rendez-vous périodique").separated(),
        MenuEntry::pending("Nouvel événement périodique"),
        MenuEntry::pending("Nouvelle réunion périodique"),
    ]
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

/// Ajoute les expéditeurs de la sélection à une liste, puis déplace la sélection (bloquer : vers le Courrier
/// indésirable ; « ce n'est pas du courrier indésirable » : vers la Boîte de réception).
fn junk_mark(ui: Ui, mark: JunkMark, then: Option<FolderRole>) {
    let Some(sel) = actions::selection(ui) else { return };
    spawn_local(async move {
        match ipc::junk_mark(&sel, mark).await {
            Ok(msg) => {
                ui.status.set(Some(msg));
                if let Some(f) = then.and_then(|role| actions::section_folder(ui, role)) {
                    actions::run(ui, MessageOp::Move { folder_id: f });
                }
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

/// Menu « Courrier indésirable » (clic droit et ruban), comme celui d'Outlook.
pub fn junk_entries(ui: Ui) -> Vec<MenuEntry> {
    let in_junk = actions::section_folder(ui, FolderRole::Junk).is_some_and(|j| real_folder(&ui.folder.get_untracked()) == j);
    let mut v = vec![
        MenuEntry::action("Bloquer l'expéditeur", move || junk_mark(ui, JunkMark::Block, Some(FolderRole::Junk))),
        MenuEntry::action("Ne jamais bloquer l'expéditeur", move || junk_mark(ui, JunkMark::Safe, None)),
        MenuEntry::action("Ne jamais bloquer le domaine de l'expéditeur", move || junk_mark(ui, JunkMark::SafeDomain, None)),
    ];
    let not_junk = MenuEntry::action("Ce n'est pas du courrier indésirable", move || junk_mark(ui, JunkMark::Safe, Some(FolderRole::Inbox))).separated();
    v.push(if in_junk { not_junk } else { MenuEntry::pending("Ce n'est pas du courrier indésirable").separated() });
    if !in_junk {
        if let Some(j) = actions::section_folder(ui, FolderRole::Junk) {
            v.push(MenuEntry::action("Déplacer vers le Courrier indésirable", move || actions::run(ui, MessageOp::Move { folder_id: j.clone() })));
        }
    }
    v.push(MenuEntry::action("Options du courrier indésirable…", move || ui.dialog.set(Some(Dialog::JunkOptions))).separated());
    v
}

pub fn message_context(ui: Ui) -> Vec<MenuEntry> {
    vec![
        MenuEntry::action("Répondre", move || actions::compose(ui, ComposeKind::Reply)),
        MenuEntry::action("Répondre à tous", move || actions::compose(ui, ComposeKind::ReplyAll)),
        MenuEntry::action("Transférer", move || actions::compose(ui, ComposeKind::Forward)),
        MenuEntry::action("Marquer comme lu", move || actions::run(ui, MessageOp::Read { read: true })).separated(),
        MenuEntry::action("Marquer comme non lu", move || actions::run(ui, MessageOp::Read { read: false })),
        MenuEntry::submenu("Catégoriser", category_entries(ui)).separated(),
        MenuEntry::submenu("Assurer un suivi", flag_entries(ui)),
        MenuEntry::submenu("Actions rapides", quick_step_entries(ui)),
        MenuEntry::submenu("Règles", rule_entries(ui)),
        MenuEntry::submenu("Déplacer", move_entries(ui)).separated(),
        MenuEntry::submenu("Courrier indésirable", junk_entries(ui)),
        MenuEntry::action("Archiver", move || actions::run(ui, MessageOp::Archive)),
        MenuEntry::action("Supprimer", move || actions::delete(ui, false)).separated(),
        MenuEntry::action("Supprimer définitivement", move || actions::delete(ui, true)),
    ]
}

/// Sous-menu « Règles » d'Outlook : créer une règle d'après le message, ou gérer les règles.
fn rule_entries(ui: Ui) -> Vec<MenuEntry> {
    vec![
        MenuEntry::action("Créer une règle…", move || ui.dialog.set(Some(Dialog::CreateRule))),
        MenuEntry::action("Gérer les règles et alertes…", move || ui.dialog.set(Some(Dialog::Rules { run: false }))),
    ]
}

fn quick_step_entries(ui: Ui) -> Vec<MenuEntry> {
    let steps = ui.quick_steps.get_untracked();
    let mut v: Vec<MenuEntry> =
        steps.iter().enumerate().map(|(i, s)| MenuEntry::action(crate::quicksteps::step_label(s), move || crate::quicksteps::run(ui, i))).collect();
    v.push(MenuEntry::action("Gérer les actions rapides…", move || ui.dialog.set(Some(Dialog::QuickSteps { edit: None, first_use: false }))).separated());
    v
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
    if id.starts_with("s-") {
        let custom = id[2..].parse::<i64>().is_ok();
        return vec![if custom {
            MenuEntry::action("Supprimer le dossier de recherche", move || actions::run_folder(ui, FolderOp::DeleteSearchFolder { id: id.clone() }))
        } else {
            MenuEntry::pending("Dossier de recherche prédéfini")
        }];
    }
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
    // Sous-menu affiché à la place du menu principal : seulement son libellé. Ses entrées sont reconstruites à chaque
    // rendu ; gardées d'un rendu à l'autre, leurs rappels appartiendraient à une portée déjà détruite (sans effet).
    let sub = RwSignal::new(None::<String>);

    let root = Memo::new(move |_| ui.context_menu.get());
    let entries = move || -> Vec<MenuEntry> {
        let top = match root.get() {
            Some(at) => match at.target {
                ContextTarget::Messages => message_context(ui),
                ContextTarget::Folder(id) => folder_context(ui, id),
                ContextTarget::Flag => flag_entries(ui),
                ContextTarget::Categories => category_entries(ui),
                ContextTarget::CalEvent(id) => calendar_event_context(ui, id),
                ContextTarget::CalSlot(start) => calendar_slot_context(ui, start),
            },
            None => Vec::new(),
        };
        match sub.get() {
            Some(label) => top.into_iter().find(|e| e.label == label).map(|e| e.children).unwrap_or_default(),
            None => top,
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
                    let back = sub.get().map(|label| {
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
                            let has_children = !entry.children.is_empty();
                            let label = entry.label.clone();
                            view! {
                                {entry.separator.then(|| view! { <div class="menu-sep" role="separator"></div> })}
                                <button
                                    role=role
                                    tabindex="-1"
                                    aria-checked=entry.checked.map(|c| c.to_string())
                                    aria-disabled=disabled.then_some("true")
                                    aria-haspopup=has_children.then_some("menu")
                                    on:click=move |_| {
                                        if has_children {
                                            sub.set(Some(label.clone()));
                                        } else if let Some(cb) = on_select {
                                            // L'action d'abord : fermer le menu détruit la portée qui possède le rappel.
                                            cb.run(());
                                            close();
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
