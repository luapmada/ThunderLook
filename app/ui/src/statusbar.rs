//! Barre d'état : éléments et non-lus de l'affichage, réglages, zoom.

use leptos::prelude::*;
use thunderlook_model::format::thousands;

use crate::{
    icons::*,
    state::{Module, use_ui},
};

#[component]
pub fn StatusBar() -> impl IntoView {
    let ui = use_ui();
    let counts = Memo::new(move |_| {
        let msgs = ui.visible_messages();
        (msgs.len() as u32, msgs.iter().filter(|m| m.unread).count() as u32)
    });
    let demo = move || ui.snapshot.with(|s| s.as_ref().is_some_and(|s| s.demo));

    view! {
        <footer class="statusbar" aria-label="Barre d'état">
            {move || match ui.module.get() {
                Module::Mail => view! {
                    <span>{move || format!("Éléments : {}", thousands(counts.get().0))}</span>
                    <span>{move || format!("Non lus : {}", thousands(counts.get().1))}</span>
                }
                .into_any(),
                other => view! { <span>{format!("{} : module prévu après le courrier", other.label())}</span> }.into_any(),
            }}
            <Show when=demo>
                <span class="badge-demo">"Données de démonstration"</span>
            </Show>
            {move || ui.error.get().map(|e| view! { <span class="status-error" role="alert">{e}</span> })}
            <div class="end">
                <button class="cmd" aria-disabled="true" tabindex="-1" title="Réglages de l'affichage — pas encore disponible">
                    <Svg icon=SETTINGS size=14 />
                    <span>"Réglages de l'affichage"</span>
                </button>
                <button class="icon-btn" aria-disabled="true" tabindex="-1" aria-label="Zoom arrière">
                    <Svg icon=MINUS size=14 />
                </button>
                <span aria-label="Zoom">"100 %"</span>
                <button class="icon-btn" aria-disabled="true" tabindex="-1" aria-label="Zoom avant">
                    <Svg icon=PLUS size=14 />
                </button>
            </div>
        </footer>
    }
}
