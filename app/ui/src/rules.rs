//! Boîte « Règles et alertes » et « Exécuter les règles maintenant » (J6) : liste des règles importées par jeu
//! (flux du PST, `.rwz`), case « active », description avec les libellés d'Outlook, import d'un `.rwz`, aperçu
//! (simulation) puis exécution sur un dossier. Échap ferme, le focus reste dans la boîte.

use std::collections::HashSet;

use leptos::{html, prelude::*, task::spawn_local};
use thunderlook_model::{RuleRunReport, RuleRunRequest, RuleSummary, RunFilter};
use wasm_bindgen::JsCast;

use crate::{
    actions::{self, real_folder},
    dialogs::flat_folders,
    ipc,
    state::{Dialog, use_ui},
};

#[component]
pub fn RulesDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let rules = RwSignal::new(Vec::<RuleSummary>::new());
    let current = RwSignal::new(0usize);
    // Vrai : « Exécuter les règles maintenant » ; faux : « Règles et alertes ».
    let run_mode = RwSignal::new(false);
    let to_run = RwSignal::new(HashSet::<String>::new());
    let folder = RwSignal::new(String::new());
    let subfolders = RwSignal::new(false);
    let filter = RwSignal::new(RunFilter::All);
    let report = RwSignal::new(None::<RuleRunReport>);
    // Nombre de déplacements confirmé après un aperçu (au-delà de la limite de sécurité).
    let confirmed = RwSignal::new(None::<u32>);
    // Copie vers un compte : comptes proposés, et message de résultat.
    let accounts = RwSignal::new(Vec::<thunderlook_model::AccountSummary>::new());
    let copy_note = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    let load = move |select_active: bool| {
        spawn_local(async move {
            match ipc::rules().await {
                Ok(list) => {
                    if select_active {
                        to_run.set(list.iter().filter(|r| r.active && r.error.is_none()).map(|r| r.id.clone()).collect());
                    }
                    rules.set(list);
                }
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };

    Effect::new(move |_| {
        let Some(Dialog::Rules { run }) = ui.dialog.get() else { return };
        run_mode.set(run);
        current.set(0);
        report.set(None);
        folder.set(real_folder(&ui.folder.get_untracked()));
        subfolders.set(false);
        filter.set(RunFilter::All);
        load(true);
        copy_note.set(None);
        spawn_local(async move {
            if let Ok(a) = ipc::accounts().await {
                accounts.set(a);
            }
        });
        request_animation_frame(move || {
            if let Some(r) = root.get_untracked() {
                if let Ok(Some(el)) = r.query_selector("[data-autofocus]") {
                    if let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() {
                        let _ = el.focus();
                    }
                }
            }
        });
    });

    // La règle courante reste visible dans la liste (clavier, changement de mode).
    Effect::new(move |_| {
        let i = current.get();
        run_mode.track();
        request_animation_frame(move || {
            if let Some(el) = document().get_element_by_id(&format!("rule-{i}")) {
                el.scroll_into_view_with_bool(false);
            }
        });
    });

    let close = move || {
        ui.dialog.set(None);
        if let Some(l) = ui.refs.list.get_untracked() {
            let _ = l.focus();
        }
    };

    let set_active = move |id: String, active: bool| {
        spawn_local(async move {
            match ipc::set_rule_active(&id, active).await {
                Ok(()) => rules.update(|v| {
                    if let Some(r) = v.iter_mut().find(|r| r.id == id) {
                        r.active = active;
                    }
                }),
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };

    let import = move || {
        spawn_local(async move {
            match ipc::import_rwz().await {
                Ok(Some(n)) => {
                    ui.status.set(Some(format!("{n} règle(s) importée(s)")));
                    load(false);
                }
                Ok(None) => {}
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };

    let execute = move |apply: bool| {
        if busy.get_untracked() {
            return;
        }
        // Dans l'ordre de la liste, comme Outlook.
        let ids: Vec<String> = rules.with_untracked(|v| v.iter().filter(|r| to_run.with_untracked(|s| s.contains(&r.id))).map(|r| r.id.clone()).collect());
        if ids.is_empty() {
            ui.error.set(Some("Cochez au moins une règle à exécuter.".into()));
            return;
        }
        let req = RuleRunRequest {
            rule_ids: ids,
            folder_id: folder.get_untracked(),
            subfolders: subfolders.get_untracked(),
            filter: filter.get_untracked(),
            apply,
            only: Vec::new(),
            confirm_moves: if apply { confirmed.get_untracked() } else { None },
        };
        busy.set(true);
        spawn_local(async move {
            match ipc::run_rules(&req).await {
                Ok(r) => {
                    if r.applied {
                        ui.status.set(Some(format!("Règles exécutées : {} élément(s) modifié(s)", r.changed)));
                        actions::reopen_view(ui);
                        actions::refresh_tree(ui);
                        actions::load_categories(ui);
                    }
                    confirmed.set(None);
                    report.set(Some(r));
                }
                Err(e) => ui.error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        match ev.key().as_str() {
            "Escape" => close(),
            "Tab" => {
                if let Some(r) = root.get_untracked() {
                    // Ni les cases des règles (`tabindex=-1`, la liste se parcourt aux flèches), ni les boutons grisés : Tab passait par chaque case, ou restait bloqué sur « Aperçu » grisé.
                    let Ok(list) = r.query_selector_all("input:not([disabled]):not([tabindex='-1']), select:not([disabled]), button:not([disabled]), summary, [tabindex='0']") else { return };
                    let els: Vec<web_sys::HtmlElement> = (0..list.length()).filter_map(|i| list.item(i)?.dyn_into().ok()).collect();
                    let active = document().active_element();
                    let pos = els.iter().position(|e| active.as_ref().is_some_and(|a| a == e.unchecked_ref::<web_sys::Element>()));
                    let n = els.len() as i32;
                    if n > 0 {
                        // Focus hors de la liste (après un retrait de ligne) : premier ou dernier contrôle, sans en sauter un.
                        let next = match pos {
                            Some(p) => (p as i32 + if ev.shift_key() { -1 } else { 1 }).rem_euclid(n),
                            None if ev.shift_key() => n - 1,
                            None => 0,
                        };
                        let _ = els[next as usize].focus();
                    }
                }
            }
            "ArrowDown" | "ArrowUp" if ev.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()).is_some_and(|t| t.id() == "rules-list") => {
                let n = rules.with_untracked(Vec::len);
                if n > 0 {
                    let cur = current.get_untracked();
                    current.set(if ev.key() == "ArrowDown" { (cur + 1).min(n - 1) } else { cur.saturating_sub(1) });
                }
            }
            " " if ev.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()).is_some_and(|t| t.id() == "rules-list") => {
                // Espace coche ou décoche la règle courante.
                let Some(r) = rules.with_untracked(|v| v.get(current.get_untracked()).cloned()) else { return };
                if run_mode.get_untracked() {
                    to_run.update(|s| {
                        if !s.remove(&r.id) {
                            s.insert(r.id.clone());
                        }
                    });
                } else {
                    set_active(r.id, !r.active);
                }
            }
            _ => return,
        }
        ev.prevent_default();
        ev.stop_propagation();
    };

    let list = move || {
        let mut last = String::new();
        rules
            .get()
            .into_iter()
            .enumerate()
            .map(|(i, r)| {
                let head = (r.origin != last).then(|| {
                    last = r.origin.clone();
                    view! { <div class="rules-origin" title=r.origin.clone()>{r.origin_label.clone()}</div> }
                });
                let id = r.id.clone();
                let id2 = r.id.clone();
                let checked = move || if run_mode.get() { to_run.with(|s| s.contains(&id)) } else { rules.with(|v| v.get(i).is_some_and(|x| x.active)) };
                let on_check = move |ev: leptos::ev::Event| {
                    let on = event_target_checked(&ev);
                    current.set(i);
                    if run_mode.get_untracked() {
                        to_run.update(|s| {
                            if on {
                                s.insert(id2.clone());
                            } else {
                                s.remove(&id2);
                            }
                        });
                    } else {
                        set_active(id2.clone(), on);
                    }
                };
                let flag = if r.error.is_some() { " ⚠" } else if !r.warnings.is_empty() { " ⓘ" } else { "" };
                view! {
                    {head}
                    <div
                        role="option"
                        id=format!("rule-{i}")
                        class="rules-item"
                        aria-selected=move || (current.get() == i).to_string()
                        on:click=move |_| current.set(i)
                    >
                        <input type="checkbox" tabindex="-1" aria-label=r.name.clone() prop:checked=checked on:change=on_check />
                        <span class="rules-name">{r.name.clone()}{flag}</span>
                    </div>
                }
            })
            .collect_view()
    };

    let description = move || {
        let Some(r) = rules.with(|v| v.get(current.get()).cloned()) else {
            return view! { <p class="dialog-note">"Aucune règle. Importez un fichier .rwz, ou ouvrez un fichier de données qui en contient."</p> }
                .into_any();
        };
        view! {
            <div class="rules-desc" aria-live="polite">
                {r.description.into_iter().map(|l| view! { <p>{l}</p> }).collect_view()}
                {r.warnings.into_iter().map(|w| view! { <p class="rules-warn">{format!("ⓘ {w}")}</p> }).collect_view()}
                {r.error.map(|e| view! { <p class="rules-warn">{format!("⚠ Règle lue incomplètement : {e}")}</p> })}
            </div>
        }
        .into_any()
    };

    let report_view = move || {
        report.get().map(|r| {
            let confirm = r.needs_confirmation.then(|| {
                let n = r.moves;
                view! {
                    <p class="rules-warn" role="alert">
                        {format!("Sécurité : {n} messages changeraient de dossier (plus de {}). Rien n'a été modifié ; vérifiez l'aperçu ci-dessous.", thunderlook_model::RULE_MOVES_LIMIT)}
                    </p>
                    <button class="btn primary" on:click=move |_| {
                        confirmed.set(Some(n));
                        execute(true);
                    }>{format!("Confirmer : déplacer {n} messages")}</button>
                }
            });
            let head = format!(
                "{} : {} élément(s) parcouru(s), {} à modifier{} — {} ms",
                if r.applied { "Exécution" } else { "Aperçu" },
                r.scanned,
                r.changed,
                if r.applied { " (fait)" } else { "" },
                r.elapsed_ms
            );
            view! {
                <div class="rules-report" aria-live="polite">
                    {confirm}
                    <p class="rules-report-head">{head}</p>
                    {r.errors.into_iter().map(|e| view! { <p class="rules-warn">{format!("⚠ {e}")}</p> }).collect_view()}
                    {r
                        .rules
                        .into_iter()
                        .map(|e| {
                            let summary = match &e.skipped {
                                Some(why) => format!("{} — écartée : {why}", e.name),
                                None => format!("{} — {} élément(s)", e.name, e.matched),
                            };
                            view! {
                                <details>
                                    <summary>{summary}</summary>
                                    {e.actions.into_iter().map(|a| view! { <p>{format!("→ {a}")}</p> }).collect_view()}
                                    {e.not_run.into_iter().map(|a| view! { <p class="rules-warn">{format!("✗ {a}")}</p> }).collect_view()}
                                    {e.warnings.into_iter().map(|w| view! { <p class="rules-warn">{format!("ⓘ {w}")}</p> }).collect_view()}
                                    <ul class="rules-samples">
                                        {e.samples.into_iter().map(|s| view! { <li>{s}</li> }).collect_view()}
                                    </ul>
                                </details>
                            }
                        })
                        .collect_view()}
                </div>
            }
        })
    };

    view! {
        <Show when=move || matches!(ui.dialog.get(), Some(Dialog::Rules { .. }))>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog wide" role="dialog" aria-modal="true" aria-labelledby="rules-title" node_ref=root on:keydown=on_key>
                    <h2 id="rules-title">{move || if run_mode.get() { "Exécuter les règles maintenant" } else { "Règles et alertes" }}</h2>
                    <Show when=move || !run_mode.get()>
                        <div class="rules-toolbar">
                            <button class="btn" on:click=move |_| import()>"Importer des règles (.rwz)…"</button>
                            {move || accounts.get().into_iter().map(|a| {
                                let label = format!("Copier ce jeu vers {}", a.address);
                                let id = a.id.clone();
                                view! {
                                    <button class="btn" on:click=move |_| {
                                        let Some(origin) = rules.with_untracked(|v| v.get(current.get_untracked()).map(|r| r.origin.clone())) else { return };
                                        let id = id.clone();
                                        spawn_local(async move {
                                            match ipc::copy_rules(&origin, &id).await {
                                                Ok(m) => {
                                                    copy_note.set(Some(m));
                                                    load(false);
                                                    actions::refresh_tree(ui);
                                                }
                                                Err(e) => copy_note.set(Some(e)),
                                            }
                                        });
                                    }>{label}</button>
                                }
                            }).collect_view()}
                            <button class="btn" on:click=move |_| {
                                // Règles à exécuter : celles qui sont actives maintenant (cases modifiées comprises).
                                to_run.set(rules.with_untracked(|v| v.iter().filter(|r| r.active && r.error.is_none()).map(|r| r.id.clone()).collect()));
                                report.set(None);
                                run_mode.set(true);
                            }>"Exécuter les règles…"</button>
                        </div>
                    </Show>
                    <p class="dialog-note">
                        {move || if run_mode.get() { "Sélectionnez les règles à exécuter :" } else { "Règles (appliquées dans l'ordre indiqué) :" }}
                    </p>
                    <div
                        class="rules-list"
                        id="rules-list"
                        role="listbox"
                        aria-label="Règles"
                        tabindex="0"
                        data-autofocus
                        aria-activedescendant=move || format!("rule-{}", current.get())
                    >
                        {list}
                    </div>
                    {move || copy_note.get().map(|n| view! { <p class="dialog-note" role="status">{n}</p> })}
                    <p class="dialog-note">"Description de la règle :"</p>
                    {description}
                    <Show when=move || run_mode.get()>
                        <label class="field">
                            <span>"Exécuter dans le dossier"</span>
                            // Les options ne suivent que l'arborescence : avant, elles suivaient aussi `folder` et
                            // toutes étaient recréées à chaque choix (flèches du clavier comprises).
                            <select on:change=move |ev| folder.set(event_target_value(&ev))>
                                {move || {
                                    flat_folders(ui)
                                        .into_iter()
                                        .map(|(id, name, depth, section)| {
                                            let label = format!("{}{name}  ({section})", "\u{a0}\u{a0}".repeat(depth));
                                            let value = id.clone();
                                            view! { <option value=value selected=move || folder.get() == id>{label}</option> }
                                        })
                                        .collect_view()
                                }}
                            </select>
                        </label>
                        <label class="check">
                            <input type="checkbox" prop:checked=move || subfolders.get() on:change=move |ev| subfolders.set(event_target_checked(&ev)) />
                            <span>"Inclure les sous-dossiers"</span>
                        </label>
                        <label class="field">
                            <span>"Appliquer les règles à"</span>
                            <select on:change=move |ev| {
                                filter.set(match event_target_value(&ev).as_str() {
                                    "read" => RunFilter::Read,
                                    "unread" => RunFilter::Unread,
                                    _ => RunFilter::All,
                                })
                            }>
                                <option value="all" selected=move || filter.get() == RunFilter::All>"Tous les messages"</option>
                                <option value="read" selected=move || filter.get() == RunFilter::Read>"Messages lus"</option>
                                <option value="unread" selected=move || filter.get() == RunFilter::Unread>"Messages non lus"</option>
                            </select>
                        </label>
                        {report_view}
                    </Show>
                    <div class="dialog-buttons">
                        <Show when=move || run_mode.get()>
                            <button class="btn" disabled=move || busy.get() on:click=move |_| execute(false)>"Aperçu"</button>
                            <button class="btn primary" disabled=move || busy.get() on:click=move |_| execute(true)>
                                {move || if busy.get() { "Exécution…" } else { "Exécuter maintenant" }}
                            </button>
                        </Show>
                        <button class="btn" on:click=move |_| close()>{move || if run_mode.get() { "Fermer" } else { "OK" }}</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
