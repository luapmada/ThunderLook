//! Barre supérieure : accès rapide et zone de recherche (au centre, comme la référence).

use leptos::prelude::*;

use crate::{actions, icons::*, state::use_ui};

#[component]
pub fn TopBar() -> impl IntoView {
    let ui = use_ui();
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
                />
            </div>
            <div></div>
        </div>
    }
}
