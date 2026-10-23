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

#[allow(dead_code)] // remplacé par `DetailRow` ; gardé avec `ipc::phone_exchange_check`
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRow {
    pub key: String,
    pub kind: String,
    pub item: String,
    pub summary: String,
    pub selected: bool,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CheckResult {
    pub changes: Vec<ChangeRow>,
    pub unchanged: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDiff {
    pub label: String,
    pub old: String,
    pub new: String,
}

/// Un changement ou un conflit avec ses valeurs (reflet de `exchange_cmds::DetailRow`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailRow {
    pub key: String,
    pub kind: String,
    pub item: String,
    pub summary: String,
    pub selected: bool,
    #[serde(default)]
    pub conflict: Option<String>,
    #[serde(default)]
    pub fields: Vec<FieldDiff>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DetailCheck {
    pub changes: Vec<DetailRow>,
    pub conflicts: Vec<DetailRow>,
    pub unchanged: usize,
    pub warnings: Vec<String>,
}

/// Récapitulatif d'une application (reflet de `exchange_cmds::ApplySummary`).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ApplySummary {
    pub created: usize,
    pub modified: usize,
    pub deleted: usize,
    pub kept: usize,
    pub ignored: usize,
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

/// Modules dans l'ordre de l'écran : (identifiant, titre).
const MODULES: [(&str, &str); 4] = [("contact", "Contacts"), ("event", "Rendez-vous"), ("note", "Notes"), ("task", "Tâches")];

/// Ancienne et nouvelle valeur de chaque champ qui change.
fn fields_view(fields: &[FieldDiff]) -> impl IntoView + use<> {
    let items = fields
        .iter()
        .map(|f| {
            let (old, new) = (f.old.clone(), f.new.clone());
            let value = match (old.is_empty(), new.is_empty()) {
                (true, _) => view! { <ins>{new}</ins> }.into_any(),
                (_, true) => view! { <del>{old}</del> }.into_any(),
                _ => view! { <del>{old}</del>" → "<ins>{new}</ins> }.into_any(),
            };
            view! { <li><strong>{f.label.clone()}" : "</strong>{value}</li> }
        })
        .collect_view();
    view! { <ul class="exchange-fields">{items}</ul> }
}

#[component]
pub fn ExchangeDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let state = RwSignal::new(ExchangeState::default());
    let rows = RwSignal::new(Vec::<DetailRow>::new());
    let conflicts = RwSignal::new(Vec::<DetailRow>::new());
    // Choix par conflit : "thunderlook", "phone" ou absent (= ignorer). Jamais renseigné d'office.
    let choices = RwSignal::new(std::collections::HashMap::<String, String>::new());
    let confirming = RwSignal::new(false);
    let summary = RwSignal::new(Option::<ApplySummary>::None);
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
        conflicts.set(Vec::new());
        choices.set(Default::default());
        confirming.set(false);
        summary.set(None);
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
                    conflicts.set(Vec::new());
                    confirming.set(false);
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
        confirming.set(false);
        summary.set(None);
        spawn_local(async move {
            match ipc::phone_exchange_check_detail().await {
                Ok(r) => {
                    let total = r.changes.len() + r.conflicts.len();
                    note.set(if total == 0 {
                        format!("Aucun changement ({} élément(s) identiques).", r.unchanged)
                    } else {
                        format!("{} changement(s) et {} conflit(s) proposés, {} identique(s).", r.changes.len(), r.conflicts.len(), r.unchanged)
                    });
                    warnings.set(r.warnings);
                    choices.set(Default::default());
                    conflicts.set(r.conflicts);
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
        let picks: Vec<(String, String)> = choices.with_untracked(|c| c.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        confirming.set(false);
        if keys.is_empty() && picks.is_empty() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match ipc::phone_exchange_apply_detail(&keys, &picks).await {
                Ok(sum) => {
                    ui.status.set(Some(format!("Échange : {} créé(s), {} modifié(s), {} supprimé(s)", sum.created, sum.modified, sum.deleted)));
                    note.set(String::new());
                    summary.set(Some(sum));
                    rows.set(Vec::new());
                    conflicts.set(Vec::new());
                    choices.set(Default::default());
                    warnings.set(Vec::new());
                    checked_once.set(false);
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
                    conflicts.set(Vec::new());
                    summary.set(None);
                    confirming.set(false);
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
    // (créations, modifications, suppressions) cochées, plus les conflits tranchés pour le téléphone.
    let counts = move || {
        let (mut n, mut m, mut d) = (0, 0, 0);
        rows.with(|r| {
            for c in r.iter().filter(|c| c.selected) {
                match c.kind.as_str() {
                    "new" => n += 1,
                    "modified" => m += 1,
                    _ => d += 1,
                }
            }
        });
        m += choices.with(|c| c.values().filter(|v| v.as_str() == "phone").count());
        (n, m, d)
    };
    let kept_count = move || choices.with(|c| c.values().filter(|v| v.as_str() == "thunderlook").count());
    let apply_count = move || {
        let (n, m, d) = counts();
        n + m + d
    };
    let can_apply = move || apply_count() > 0 || kept_count() > 0;
    let ignored_count = move || rows.with(|r| r.iter().filter(|c| !c.selected).count()) + conflicts.with(Vec::len) - choices.with(|c| c.len());

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
                    <Show when=move || !warnings.with(Vec::is_empty)>
                        <div class="exchange-warnings" role="alert">
                            <strong>{move || format!("{} avertissement(s) de l'import — à lire avant d'appliquer", warnings.with(Vec::len))}</strong>
                            <ul>{move || warnings.get().into_iter().map(|w| view! { <li>{w}</li> }).collect_view()}</ul>
                        </div>
                    </Show>
                    <Show when=move || checked_once.get() && !conflicts.with(Vec::is_empty)>
                        <section class="exchange-conflicts" aria-label="Conflits">
                            <h3>{move || format!("Conflits ({}) — modifiés des deux côtés", conflicts.with(Vec::len))}</h3>
                            <p class="dialog-note">"Rien n'est choisi d'office : sans choix, l'élément est ignoré."</p>
                            <ul class="exchange-changes">
                                {move || {
                                    conflicts
                                        .get()
                                        .into_iter()
                                        .map(|c| {
                                            let why = if c.conflict.as_deref() == Some("unknown") { "valeur exportée inconnue : impossible de savoir qui a changé" } else { "modifié dans ThunderLook et sur le téléphone" };
                                            let name = format!("{group}_{key}", group = "conflit", key = c.key);
                                            let radio = |value: &'static str, label: &'static str| {
                                                let key = c.key.clone();
                                                let key2 = c.key.clone();
                                                let name = name.clone();
                                                view! {
                                                    <label class="check">
                                                        <input type="radio" name=name
                                                            prop:checked=move || choices.with(|m| m.get(&key).map(String::as_str) == if value.is_empty() { None } else { Some(value) })
                                                            on:change=move |_| choices.update(|m| {
                                                                if value.is_empty() {
                                                                    m.remove(&key2);
                                                                } else {
                                                                    m.insert(key2.clone(), value.to_string());
                                                                }
                                                            }) />
                                                        <span>{label}</span>
                                                    </label>
                                                }
                                            };
                                            view! {
                                                <li>
                                                    <div><strong>{item_label(&c.item)}" : "{c.summary.clone()}</strong>" — "<em>{why}</em></div>
                                                    {fields_view(&c.fields)}
                                                    <div class="exchange-choice" role="radiogroup" aria-label="Choix pour ce conflit">
                                                        {radio("thunderlook", "Garder ThunderLook")}
                                                        {radio("phone", "Garder le téléphone")}
                                                        {radio("", "Ignorer")}
                                                    </div>
                                                </li>
                                            }
                                        })
                                        .collect_view()
                                }}
                            </ul>
                        </section>
                    </Show>
                    <Show when=move || checked_once.get() && !rows.with(Vec::is_empty)>
                        <div class="exchange-groups">
                            {MODULES
                                .iter()
                                .map(|&(module, title)| {
                                    let count = move || rows.with(|r| r.iter().filter(|c| c.item == module).count());
                                    view! {
                                        <Show when=move || { count() > 0 }>
                                            <section aria-label=title>
                                                <h3>{move || format!("{title} ({})", count())}</h3>
                                                <ul class="exchange-changes">
                                                    {move || {
                                                        rows.get()
                                                            .into_iter()
                                                            .enumerate()
                                                            .filter(|(_, c)| c.item == module)
                                                            .map(|(i, c)| {
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
                                                                            <span><strong>{kind_label(&c.kind)}</strong>" — "{c.summary.clone()}</span>
                                                                            {deletion.then(|| view! { <em>" (suppression : à cocher vous-même)"</em> })}
                                                                        </label>
                                                                        {fields_view(&c.fields)}
                                                                    </li>
                                                                }
                                                            })
                                                            .collect_view()
                                                    }}
                                                </ul>
                                            </section>
                                        </Show>
                                    }
                                })
                                .collect_view()}
                        </div>
                    </Show>
                    <Show when=move || checked_once.get() && (!rows.with(Vec::is_empty) || !conflicts.with(Vec::is_empty))>
                        <p class="exchange-count" aria-live="polite">
                            {move || {
                                let (n, m, d) = counts();
                                format!("Sera appliqué : {n} création(s), {m} modification(s), {d} suppression(s). Gardés côté ThunderLook : {}. Ignorés : {}.", kept_count(), ignored_count())
                            }}
                        </p>
                    </Show>
                    <Show when=move || confirming.get()>
                        <div class="exchange-confirm" role="alertdialog" aria-label="Confirmation">
                            <p>{move || {
                                let (_, _, d) = counts();
                                let tail = if d > 0 { format!(", dont {d} suppression(s)") } else { String::new() };
                                format!("Appliquer {} élément(s) dans ThunderLook{tail} ? L'import pourra être annulé ensuite.", apply_count())
                            }}</p>
                            <button class="btn primary" prop:disabled=move || busy.get() on:click=move |_| apply()>"Confirmer"</button>
                            " "
                            <button class="btn" on:click=move |_| confirming.set(false)>"Retour"</button>
                        </div>
                    </Show>
                    {move || summary.get().map(|sum| view! {
                        <div class="exchange-summary" role="status">
                            <strong>"Import terminé"</strong>
                            <ul>
                                <li>{format!("Créés : {}", sum.created)}</li>
                                <li>{format!("Modifiés : {}", sum.modified)}</li>
                                <li>{format!("Supprimés : {}", sum.deleted)}</li>
                                <li>{format!("Gardés côté ThunderLook : {}", sum.kept)}</li>
                                <li>{format!("Ignorés : {}", sum.ignored)}</li>
                                <li>{format!("Avertissements : {}", sum.warnings.len())}</li>
                            </ul>
                            {(!sum.warnings.is_empty()).then(|| view! {
                                <ul class="exchange-warnings">{sum.warnings.iter().map(|w| view! { <li>{w.clone()}</li> }).collect_view()}</ul>
                            })}
                        </div>
                    })}
                    <div class="dialog-buttons">
                        <button class="btn primary" prop:disabled=move || busy.get() || !can_apply() on:click=move |_| confirming.set(true)>
                            {move || format!("Appliquer ({})", apply_count() + kept_count())}
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
