//! Volet des dossiers (arborescence accessible) et navigation basse entre modules.

use std::collections::HashSet;

use leptos::prelude::*;
use thunderlook_model::{Folder, FolderTree};

use crate::{
    icons::*,
    state::{Module, use_ui},
};

/// Nœud visible de l'arbre (sections et dossiers), dans l'ordre d'affichage.
#[derive(Clone, PartialEq)]
struct Row {
    id: String,
    label: String,
    level: u32,
    section: bool,
    expandable: bool,
    expanded: bool,
    badge: Option<String>,
    badge_total: bool,
    parent: Option<String>,
}

fn flatten(tree: &FolderTree, collapsed: &HashSet<String>, expanded_set: &HashSet<String>) -> Vec<Row> {
    fn push_folders(rows: &mut Vec<Row>, folders: &[Folder], level: u32, parent: &str, expanded_set: &HashSet<String>) {
        for f in folders {
            let expanded = expanded_set.contains(&f.id);
            rows.push(Row {
                id: f.id.clone(),
                label: f.name.clone(),
                level,
                section: false,
                expandable: !f.children.is_empty(),
                expanded,
                badge: f.badge(),
                badge_total: f.role.shows_total(),
                parent: Some(parent.to_string()),
            });
            if expanded {
                push_folders(rows, &f.children, level + 1, &f.id, expanded_set);
            }
        }
    }
    let mut rows = Vec::new();
    for s in &tree.sections {
        let expanded = !collapsed.contains(&s.id);
        rows.push(Row {
            id: s.id.clone(),
            label: s.name.clone(),
            level: 1,
            section: true,
            expandable: true,
            expanded,
            badge: None,
            badge_total: false,
            parent: None,
        });
        if expanded {
            push_folders(&mut rows, &s.folders, 2, &s.id, expanded_set);
        }
    }
    rows
}

fn node_dom_id(id: &str) -> String {
    format!("tn-{id}")
}

#[component]
pub fn FolderPane() -> impl IntoView {
    let ui = use_ui();
    let rows = Memo::new(move |_| {
        let collapsed = ui.collapsed_nodes.get();
        let expanded = ui.expanded_nodes.get();
        ui.tree.with(|t| t.as_ref().map(|t| flatten(t, &collapsed, &expanded)).unwrap_or_default())
    });

    let focus_row = move |row: &Row| {
        ui.tree_focus.set(row.id.clone());
        if !row.section {
            // La sélection suit le focus, comme dans le volet des dossiers de référence.
            if ui.folder.get_untracked() != row.id {
                ui.folder.set(row.id.clone());
            }
        }
        if let Some(el) = document().get_element_by_id(&node_dom_id(&row.id)) {
            el.scroll_into_view_with_bool(false);
        }
    };

    let toggle = move |id: String, expand: bool| {
        let section = rows.with_untracked(|r| r.iter().any(|x| x.id == id && x.section));
        if section {
            ui.collapsed_nodes.update(|c| {
                if expand { c.remove(&id) } else { c.insert(id) };
            });
        } else {
            ui.expanded_nodes.update(|c| {
                if expand { c.insert(id) } else { c.remove(&id) };
            });
        }
    };

    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        let rows = rows.get_untracked();
        let Some(pos) = rows.iter().position(|r| r.id == ui.tree_focus.get_untracked()) else {
            if let Some(first) = rows.first() {
                focus_row(first);
            }
            return;
        };
        let row = &rows[pos];
        match ev.key().as_str() {
            "ArrowDown" if pos + 1 < rows.len() => focus_row(&rows[pos + 1]),
            "ArrowUp" if pos > 0 => focus_row(&rows[pos - 1]),
            "Home" => focus_row(&rows[0]),
            "End" => focus_row(&rows[rows.len() - 1]),
            "ArrowRight" if row.expandable && !row.expanded => toggle(row.id.clone(), true),
            "ArrowRight" if row.expandable && pos + 1 < rows.len() => focus_row(&rows[pos + 1]),
            "ArrowLeft" if row.expandable && row.expanded => toggle(row.id.clone(), false),
            "ArrowLeft" => {
                if let Some(parent) = row.parent.as_ref().and_then(|p| rows.iter().find(|r| &r.id == p)) {
                    focus_row(parent);
                }
            }
            "Enter" | " " if row.expandable => toggle(row.id.clone(), !row.expanded),
            _ => return,
        }
        ev.prevent_default();
    };

    let collapsed_pane = move || ui.settings.get().folder_pane_collapsed;

    view! {
        <nav class="pane folders" aria-label="Volet des dossiers">
            <div class="folders-head">
                <button
                    class="icon-btn"
                    aria-label=move || if collapsed_pane() { "Développer le volet des dossiers" } else { "Réduire le volet des dossiers" }
                    title=move || if collapsed_pane() { "Développer le volet des dossiers" } else { "Réduire le volet des dossiers" }
                    on:click=move |_| ui.update_settings(|s| s.folder_pane_collapsed = !s.folder_pane_collapsed)
                >
                    {move || if collapsed_pane() { view! { <Svg icon=PANEL_LEFT_OPEN /> } } else { view! { <Svg icon=PANEL_LEFT_CLOSE /> } }}
                </button>
            </div>
            <Show when=move || !collapsed_pane()>
                <div
                    class="tree"
                    role="tree"
                    aria-label="Dossiers"
                    tabindex="0"
                    node_ref=ui.refs.tree
                    aria-activedescendant=move || node_dom_id(&ui.tree_focus.get())
                    on:keydown=on_key
                >
                    {move || {
                        rows.get()
                            .into_iter()
                            .map(|row| {
                                let id = row.id.clone();
                                let is_focus = {
                                    let id = id.clone();
                                    move || ui.tree_focus.get() == id
                                };
                                let is_selected = {
                                    let id = id.clone();
                                    move || ui.folder.get() == id
                                };
                                let row_click = row.clone();
                                let toggle_id = row.id.clone();
                                let expanded = row.expanded;
                                let indent = format!("padding-left: {}px", 4 + (row.level.saturating_sub(1)) * 16);
                                view! {
                                    <div
                                        role="treeitem"
                                        id=node_dom_id(&row.id)
                                        class=if row.section { "tree-section" } else { "treeitem" }
                                        class:focused=is_focus
                                        // `aria-level` absent de la liste typée de Leptos 0.8 : propriété DOM
                                        // reflétée en attribut par Chromium (WebView2).
                                        prop:ariaLevel=row.level.to_string()
                                        aria-expanded=row.expandable.then(|| row.expanded.to_string())
                                        aria-selected=(!row.section).then(move || is_selected().to_string())
                                        style=indent
                                        on:click=move |_| {
                                            focus_row(&row_click);
                                            if let Some(t) = ui.refs.tree.get_untracked() {
                                                let _ = t.focus();
                                            }
                                        }
                                    >
                                        <span
                                            class="twisty"
                                            on:click=move |ev| {
                                                ev.stop_propagation();
                                                toggle(toggle_id.clone(), !expanded);
                                            }
                                        >
                                            {row.expandable.then(|| {
                                                if row.expanded {
                                                    view! { <Svg icon=CHEVRON_DOWN size=14 /> }
                                                } else {
                                                    view! { <Svg icon=CHEVRON_RIGHT size=14 /> }
                                                }
                                            })}
                                        </span>
                                        <span class="name">{row.label.clone()}</span>
                                        {row.badge.clone().map(|b| {
                                            view! {
                                                <span class="count" class:total=row.badge_total class:unread=!row.badge_total>
                                                    <span class="sr-only">{if row.badge_total { "Total : " } else { "Non lus : " }}</span>
                                                    {b}
                                                </span>
                                            }
                                        })}
                                    </div>
                                }
                            })
                            .collect_view()
                    }}
                </div>
            </Show>
            <NavBar />
        </nav>
    }
}

#[component]
fn NavBar() -> impl IntoView {
    let ui = use_ui();
    let icon = |m: Module| match m {
        Module::Mail => MAIL,
        Module::Calendar => CALENDAR,
        Module::Contacts => USERS,
        Module::Tasks => SQUARE_CHECK,
    };
    view! {
        <div class="navbar" role="navigation" aria-label="Modules">
            {Module::ALL
                .into_iter()
                .map(|m| {
                    view! {
                        <button
                            class="icon-btn"
                            aria-label=m.label()
                            title=format!("{} ({})", m.label(), m.shortcut())
                            aria-keyshortcuts=m.shortcut().replace("Ctrl", "Control")
                            aria-current=move || (ui.module.get() == m).then_some("page")
                            on:click=move |_| ui.module.set(m)
                        >
                            <Svg icon=icon(m) size=20 />
                        </button>
                    }
                })
                .collect_view()}
            <button class="icon-btn" aria-label="Autres modules" title="Autres modules — pas encore disponible" aria-disabled="true">
                <Svg icon=ELLIPSIS size=20 />
            </button>
        </div>
    }
}
