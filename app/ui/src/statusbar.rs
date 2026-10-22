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
    let counts = Memo::new(move |_| ui.view.with(|v| v.as_ref().map(|v| (v.folder_total, v.folder_unread)).unwrap_or_default()));

    view! {
        <footer class="statusbar" aria-label="Barre d'état">
            {move || match ui.module.get() {
                Module::Mail => view! {
                    <span>{move || {
                        let label = if ui.search_text.get().is_empty() { "Éléments" } else { "Résultats" };
                        format!("{label} : {}", thousands(counts.get().0))
                    }}</span>
                    <span>{move || format!("Non lus : {}", thousands(counts.get().1))}</span>
                    {move || {
                        let s = ui.index.get();
                        (s.running && s.total > 0).then(|| {
                            let pct = (s.done as f64 / s.total as f64 * 100.0).floor();
                            view! { <span class="status-msg" title="Les résultats de recherche se complètent pendant l'indexation">{format!("Indexation : {pct} %")}</span> }
                        })
                    }}
                }
                .into_any(),
                Module::Calendar => view! {
                    <span>{move || {
                        ui.cal_view.with(|v| {
                            let n: usize = v.as_ref().map(|v| v.days.iter().map(|d| d.all_day.iter().filter(|e| !e.continued).count() + d.timed.iter().filter(|t| !t.event.continued).count()).sum()).unwrap_or(0);
                            format!("Éléments : {}", thousands(n as u32))
                        })
                    }}</span>
                }
                .into_any(),
                Module::Contacts => view! {
                    <span>{move || format!("Éléments : {}", thousands(ui.contact_list.with(|l| l.as_ref().map_or(0, |l| l.rows.len())) as u32))}</span>
                }
                .into_any(),
                other => view! { <span>{format!("{} : module prévu après le courrier", other.label())}</span> }.into_any(),
            }}
            <Show when=move || ui.module.get() == Module::Mail && ui.unread_only.get()>
                <span class="badge-filter">"Filtre appliqué"</span>
            </Show>
            {move || ui.error.get().map(|e| view! { <span class="status-error" role="alert">{e}</span> })}
            {move || (ui.error.get().is_none()).then(|| ui.status.get()).flatten().map(|m| view! { <span class="status-msg" role="status">{m}</span> })}
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
