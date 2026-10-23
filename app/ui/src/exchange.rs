//! Boîte « Échange avec le téléphone » : dossier d'échange, export, relecture des changements (cases à cocher),
//! application et annulation du dernier import, export automatique facultatif. Le calcul est dans `thunderlook-exchange`.

use leptos::{html, prelude::*, task::spawn_local};
use serde::Deserialize;
use wasm_bindgen::JsCast;

use crate::{ipc, state::use_ui};

/// Réglages du dossier d'échange (reflet de `exchange_cmds::ExchangeSettings`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExchangeSettings {
    pub dir: String,
    pub auto_minutes: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExchangeState {
    pub settings: ExchangeSettings,
    pub last_export: Option<String>,
    pub can_undo: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRow {
    pub key: String,
    pub kind: String,
    pub item: String,
    pub summary: String,
    pub selected: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CheckResult {
    pub changes: Vec<ChangeRow>,
    pub unchanged: usize,
    pub warnings: Vec<String>,
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "new" => "Nouveau",
        "modified" => "Modifié",
        _ => "Supprimé",
    }
}

fn item_label(item: &str) -> &'static str {
    match item {
        "contact" => "Contact",
        "task" => "Tâche",
        "note" => "Note",
        _ => "Rendez-vous",
    }
}

#[component]
pub fn ExchangeDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let state = RwSignal::new(ExchangeState::default());
    let rows = RwSignal::new(Vec::<ChangeRow>::new());
    let checked_once = RwSignal::new(false);
    let note = RwSignal::new(String::new());
    let warnings = RwSignal::new(Vec::<String>::new());
    // Fichiers modifiés par le téléphone qu'un export écraserait : l'export est alors proposé « quand même ».
    let blocked = RwSignal::new(Option::<String>::None);
    let busy = RwSignal::new(false);

    Effect::new(move |_| {
        if !ui.phone_exchange.get() {
            return;
        }
        rows.set(Vec::new());
        checked_once.set(false);
        note.set(String::new());
        warnings.set(Vec::new());
        blocked.set(None);
        spawn_local(async move {
            match ipc::phone_exchange_state().await {
                Ok(s) => state.set(s),
                Err(e) => ui.error.set(Some(e)),
            }
            request_animation_frame(move || {
                if let Some(Ok(el)) = root.get_untracked().and_then(|r| r.query_selector("button").ok().flatten()).map(|el| el.dyn_into::<web_sys::HtmlElement>()) {
                    let _ = el.focus();
                }
            });
        });
    });

    let close = move || {
        ui.phone_exchange.set(false);
        if let Some(l) = ui.refs.list.get_untracked() {
            let _ = l.focus();
        }
    };
    // Les listes (contacts, tâches, notes, calendrier) se rechargent après un import ou une annulation.
    let reload_lists = move || {
        ui.contacts_epoch.update(|e| *e += 1);
        ui.tasks_epoch.update(|e| *e += 1);
        ui.notes_epoch.update(|e| *e += 1);
        ui.cal_epoch.update(|e| *e += 1);
    };
    let refresh_state = move || {
        spawn_local(async move {
            if let Ok(s) = ipc::phone_exchange_state().await {
                state.set(s);
            }
        });
    };

    let pick = move || {
        spawn_local(async move {
            match ipc::phone_exchange_pick().await {
                Ok(Some(s)) => {
                    state.set(s);
                    rows.set(Vec::new());
                    checked_once.set(false);
                }
                Ok(None) => {}
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };
    let export = move |force: bool| {
        busy.set(true);
        spawn_local(async move {
            match ipc::phone_exchange_export(force).await {
                Ok(m) => {
                    blocked.set(None);
                    note.set(m);
                    refresh_state();
                }
                Err(e) => match e.strip_prefix("MODIFIED:") {
                    Some(files) => blocked.set(Some(files.to_string())),
                    None => ui.error.set(Some(e)),
                },
            }
            busy.set(false);
        });
    };
    let check = move || {
        busy.set(true);
        spawn_local(async move {
            match ipc::phone_exchange_check().await {
                Ok(r) => {
                    note.set(if r.changes.is_empty() { format!("Aucun changement ({} élément(s) identiques).", r.unchanged) } else { format!("{} changement(s) proposé(s), {} identique(s).", r.changes.len(), r.unchanged) });
                    warnings.set(r.warnings);
                    rows.set(r.changes);
                    checked_once.set(true);
                }
                Err(e) => ui.error.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let apply = move || {
        let keys: Vec<String> = rows.with_untracked(|r| r.iter().filter(|c| c.selected).map(|c| c.key.clone()).collect());
        if keys.is_empty() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match ipc::phone_exchange_apply(&keys).await {
                Ok(m) => {
                    ui.status.set(Some(m.clone()));
                    note.set(m);
                    rows.set(Vec::new());
                    reload_lists();
                    refresh_state();
                }
                Err(e) => ui.error.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let undo = move || {
        busy.set(true);
        spawn_local(async move {
            match ipc::phone_exchange_undo().await {
                Ok(m) => {
                    ui.status.set(Some(m.clone()));
                    note.set(m);
                    rows.set(Vec::new());
                    reload_lists();
                }
                Err(e) => ui.error.set(Some(e)),
            }
            refresh_state();
            busy.set(false);
        });
    };
    let set_auto = move |minutes: u32| {
        spawn_local(async move {
            match ipc::phone_exchange_set_auto(minutes).await {
                Ok(s) => state.set(s),
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };

    let has_dir = move || state.with(|s| !s.settings.dir.is_empty());
    let selected_count = move || rows.with(|r| r.iter().filter(|c| c.selected).count());

    view! {
        <Show when=move || ui.phone_exchange.get()>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog wide" role="dialog" aria-modal="true" aria-labelledby="px-title" node_ref=root
                    on:keydown=move |ev| {
                        if ev.key() == "Escape" {
                            ev.prevent_default();
                            close();
                        }
                    }>
                    <h2 id="px-title">"Échange avec le téléphone"</h2>
                    <p class="dialog-note">"ThunderLook écrit et relit des fichiers (contacts, tâches, notes, calendrier) dans un dossier ; c'est la synchronisation du téléphone qui les lit et les écrit."</p>
                    <div class="field">
                        <span>"Dossier d'échange"</span>
                        <div>
                            <code>{move || state.with(|s| if s.settings.dir.is_empty() { "(aucun)".to_string() } else { s.settings.dir.clone() })}</code>
                            " "
                            <button class="btn" on:click=move |_| pick()>"Choisir le dossier…"</button>
                        </div>
                    </div>
                    <p class="dialog-note">{move || state.with(|s| match &s.last_export {
                        Some(t) => format!("Dernier export : {}", t.replace('T', " ").trim_end_matches('Z')),
                        None => "Aucun export dans ce dossier.".to_string(),
                    })}</p>
                    <div class="dialog-buttons">
                        <button class="btn primary" prop:disabled=move || busy.get() || !has_dir() on:click=move |_| export(false)>"Exporter maintenant"</button>
                        <button class="btn" prop:disabled=move || busy.get() || !has_dir() on:click=move |_| check()>"Vérifier les changements"</button>
                    </div>
                    <Show when=move || blocked.get().is_some()>
                        <p class="dialog-note" role="alert">
                            {move || format!("Le téléphone a modifié des fichiers depuis le dernier export ({}) : un export les écraserait. Vérifiez d'abord les changements.", blocked.get().unwrap_or_default())}
                            " "
                            <button class="btn" on:click=move |_| export(true)>"Exporter quand même"</button>
                        </p>
                    </Show>
                    <Show when=move || checked_once.get() && !rows.with(Vec::is_empty)>
                        <ul class="exchange-changes" aria-label="Changements proposés" style="max-height: 16em; overflow: auto; list-style: none; padding: 0; margin: 0.5em 0;">
                            {move || {
                                rows.get()
                                    .into_iter()
                                    .enumerate()
                                    .map(|(i, c)| {
                                        let label = format!("{} — {} : {}", kind_label(&c.kind), item_label(&c.item), c.summary);
                                        let deletion = c.kind == "deleted";
                                        view! {
                                            <li>
                                                <label class="check">
                                                    <input type="checkbox" prop:checked=c.selected
                                                        on:change=move |ev| {
                                                            let on = event_target_checked(&ev);
                                                            rows.update(|r| {
                                                                if let Some(c) = r.get_mut(i) {
                                                                    c.selected = on;
                                                                }
                                                            });
                                                        } />
                                                    <span>{label}</span>
                                                    {deletion.then(|| view! { <em>" (proposée, jamais appliquée d'office)"</em> })}
                                                </label>
                                            </li>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </ul>
                    </Show>
                    <Show when=move || !warnings.with(Vec::is_empty)>
                        <ul class="dialog-note" aria-label="Avertissements">
                            {move || warnings.get().into_iter().map(|w| view! { <li>{w}</li> }).collect_view()}
                        </ul>
                    </Show>
                    <div class="dialog-buttons">
                        <button class="btn primary" prop:disabled=move || busy.get() || selected_count() == 0 on:click=move |_| apply()>
                            {move || format!("Appliquer ({})", selected_count())}
                        </button>
                        <button class="btn" prop:disabled=move || busy.get() || !state.with(|s| s.can_undo) on:click=move |_| undo()>"Annuler le dernier import"</button>
                    </div>
                    <label class="check">
                        <input type="checkbox" prop:disabled=move || !has_dir()
                            prop:checked=move || state.with(|s| s.settings.auto_minutes > 0)
                            on:change=move |ev| set_auto(if event_target_checked(&ev) { 15 } else { 0 }) />
                        <span>"Exporter automatiquement toutes les"</span>
                    </label>
                    <label class="field">
                        <span>"Minutes (1 à 1 440)"</span>
                        <input type="number" min="1" max="1440" aria-label="Minutes entre deux exports"
                            prop:disabled=move || state.with(|s| s.settings.auto_minutes == 0)
                            prop:value=move || state.with(|s| s.settings.auto_minutes.to_string())
                            on:change=move |ev| {
                                if let Ok(m) = event_target_value(&ev).trim().parse::<u32>() {
                                    set_auto(m.clamp(1, 1_440));
                                }
                            } />
                    </label>
                    <p class="dialog-note" aria-live="polite">{move || note.get()}</p>
                    <div class="dialog-buttons">
                        <button class="btn" on:click=move |_| close()>"Fermer"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
