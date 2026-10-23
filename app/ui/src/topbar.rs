//! Barre supérieure : accès rapide et zone de recherche (au centre, comme la référence).

use leptos::prelude::*;

use crate::{actions, global_search, icons::*, state::use_ui};

/// Délai avant de lancer la recherche pendant la frappe.
const DEBOUNCE_MS: u64 = 350;

#[component]
pub fn TopBar() -> impl IntoView {
    let ui = use_ui();
    // Génération de frappe : seule la dernière déclenche la recherche.
    let typing = StoredValue::new(0u64);
    let commit = move |text: String| {
        let text = text.trim().to_string();
        if ui.search_all.get_untracked() {
            global_search::run(ui, text);
        } else {
            ui.search_text.set(text);
        }
    };
    let on_input = move |ev: leptos::ev::Event| {
        let text = event_target_value(&ev);
        let generation = typing.get_value() + 1;
        typing.set_value(generation);
        set_timeout(
            move || {
                if typing.get_value() == generation {
                    commit(text);
                }
            },
            std::time::Duration::from_millis(DEBOUNCE_MS),
        );
    };
    let on_key = move |ev: leptos::ev::KeyboardEvent| match ev.key().as_str() {
        "Enter" => {
            typing.update_value(|g| *g += 1);
            commit(event_target_value(&ev));
        }
        "Escape" => {
            ev.prevent_default();
            typing.update_value(|g| *g += 1);
            if let Some(s) = ui.refs.search.get_untracked() {
                s.set_value("");
            }
            ui.search_text.set(String::new());
            global_search::close(ui);
            if let Some(l) = ui.refs.list.get_untracked() {
                let _ = l.focus();
            }
        }
        _ => {}
    };
    view! {
        <div class="topbar">
            <div class="qat" role="toolbar" aria-label="Accès rapide">
                <button class="icon-btn" aria-disabled="true" tabindex="-1" aria-label="Tout envoyer et recevoir" title="Tout envoyer et recevoir (F9) — pas encore disponible">
                    <Svg icon=REFRESH />
                </button>
                <button
                    class="icon-btn"
                    aria-label=move || ui.undo_label.get().map(|l| format!("Annuler : {l}")).unwrap_or_else(|| "Annuler".into())
                    title=move || ui.undo_label.get().map(|l| format!("Annuler : {l} (Ctrl+Z)")).unwrap_or_else(|| "Annuler (Ctrl+Z)".into())
                    aria-keyshortcuts="Control+Z"
                    on:click=move |_| actions::undo(ui)
                >
                    <Svg icon=UNDO />
                </button>
            </div>
            <div class="search" role="search">
                <Svg icon=SEARCH />
                <input
                    type="search"
                    node_ref=ui.refs.search
                    placeholder="Rechercher (Ctrl+E)"
                    aria-label="Rechercher"
                    aria-keyshortcuts="Control+E F3"
                    spellcheck="false"
                    on:input=on_input
                    on:keydown=on_key
                />
                <global_search::ScopeMenu />
            </div>
            <div></div>
        </div>
    }
}
