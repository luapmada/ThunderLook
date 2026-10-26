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

pub(crate) fn kind_label(kind: &str) -> &'static str {
    match kind {
        "new" => "Nouveau",
        "modified" => "Modifié",
        _ => "Supprimé",
    }
}

pub(crate) fn item_label(item: &str) -> &'static str {
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
pub(crate) fn fields_view(fields: &[FieldDiff]) -> impl IntoView + use<> {
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
                    <AutoSyncZone />
                    <crate::mpe_import::MpeImport />
                    <div class="dialog-buttons">
                        <button class="btn" on:click=move |_| close()>"Fermer"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

// --- Synchronisation automatique (lot AM) ---------------------------------------------------------------------------
//
// Reflet de `thunderlook_exchange::auto` : la tâche de fond relit le dossier d'échange toute seule ; cette zone règle
// l'intervalle (5 à 120 min), montre ce qui est mis de côté (jamais appliqué sans toi) et annule le dernier passage.

use std::time::Duration;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

#[derive(serde::Serialize)]
struct NoArgs {}

#[derive(serde::Serialize)]
struct RunArgs {
    force: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AutoSetArgs {
    enabled: bool,
    minutes: u32,
    auto_export: bool,
}

async fn auto_call<T: serde::de::DeserializeOwned>(cmd: &str, args: &impl serde::Serialize) -> Result<T, String> {
    let ser = serde_wasm_bindgen::Serializer::json_compatible();
    let args = serde::Serialize::serialize(args, &ser).map_err(|e| e.to_string())?;
    let out = tauri_invoke(cmd, args).await.map_err(|e| e.as_string().unwrap_or_else(|| format!("{cmd} : {e:?}")))?;
    serde_wasm_bindgen::from_value(out).map_err(|e| format!("{cmd} : {e}"))
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoSettings {
    pub enabled: bool,
    pub minutes: u32,
    pub auto_export: bool,
}

impl Default for AutoSettings {
    fn default() -> Self {
        AutoSettings { enabled: true, minutes: 15, auto_export: true }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoAside {
    pub deletions: usize,
    pub conflicts: usize,
    pub series: usize,
    pub warnings: usize,
}

impl AutoAside {
    fn total(&self) -> usize {
        self.deletions + self.conflicts + self.series + self.warnings
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct AutoOutcome {
    pub kind: String,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoPass {
    pub at: String,
    pub outcome: AutoOutcome,
    pub created: usize,
    pub modified: usize,
    pub aside: AutoAside,
    pub undo: Option<String>,
    pub exported: Option<bool>,
    pub export_note: String,
    pub errors: Vec<String>,
    /// Dossier écrit par MPE : relu seul, jamais écrit.
    pub files_only: bool,
    /// Champs de fiche que ThunderLook ne reprend pas (télécopie…) : informatif.
    pub losses: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoView {
    pub settings: AutoSettings,
    pub last: Option<AutoPass>,
    pub last_read: Option<AutoPass>,
    pub last_checked: Option<String>,
    pub dir_set: bool,
    pub can_undo: bool,
}

/// `2026-10-24T22:54:51Z` (UTC) → « 24/10 à 00:54 » (heure locale).
fn local_time(at: &str) -> String {
    let d = js_sys::Date::new(&JsValue::from_str(at));
    if d.get_time().is_nan() {
        return at.to_string();
    }
    format!("{:02}/{:02} à {:02}:{:02}", d.get_date(), d.get_month() + 1, d.get_hours(), d.get_minutes())
}

fn outcome_text(p: &AutoPass) -> String {
    let detail = p.outcome.detail.clone().unwrap_or_default();
    match p.outcome.kind.as_str() {
        "skipped" => format!("ignoré — {detail}"),
        "blocked" => format!("BLOQUÉ par le seuil de sécurité, rien appliqué — {detail}"),
        "failed" => format!("échec — {detail}"),
        _ => {
            let mut s = format!("{} créé(s), {} modifié(s)", p.created, p.modified);
            if p.aside.total() > 0 {
                s.push_str(&format!(", {} mis de côté", p.aside.total()));
            }
            match p.exported {
                Some(true) => s.push_str(", exporté"),
                Some(false) => s.push_str(&format!(", {}", p.export_note)),
                None => {}
            }
            if p.files_only && p.losses > 0 {
                s.push_str(&format!(" ({} champ(s) de fiche non repris)", p.losses));
            }
            s
        }
    }
}

fn fetch_auto(view: RwSignal<AutoView>) {
    spawn_local(async move {
        if let Ok(v) = auto_call::<AutoView>("auto_sync_state", &NoArgs {}).await {
            view.set(v);
        }
    });
}

/// Rafraîchit `view` toutes les 20 s tant que le composant vit.
fn poll_auto(view: RwSignal<AutoView>) {
    fetch_auto(view);
    if let Ok(h) = set_interval_with_handle(move || fetch_auto(view), Duration::from_secs(20)) {
        on_cleanup(move || h.clear());
    }
}

/// État dans la barre d'état : dernier passage et compteur de ce qui est mis de côté ; vide si c'est désactivé.
#[component]
pub fn AutoSyncStatus() -> impl IntoView {
    let view_state = RwSignal::new(AutoView::default());
    poll_auto(view_state);
    view! {
        {move || {
            let v = view_state.get();
            if !v.settings.enabled {
                return ().into_any();
            }
            let aside = v.last_read.as_ref().map_or(0, |p| p.aside.total());
            let blocked = v.last_read.as_ref().is_some_and(|p| p.outcome.kind == "blocked");
            let text = match &v.last {
                None => format!("Synchro auto : toutes les {} min", v.settings.minutes),
                Some(p) => {
                    // Fichiers inchangés : le dernier passage date d'avant, mais le dossier a bien été regardé depuis.
                    let checked = v.last_checked.as_ref().filter(|c| **c != p.at).map(|c| format!(" (vérifié {})", local_time(c))).unwrap_or_default();
                    format!("Synchro auto : {} — {}{checked}", local_time(&p.at), outcome_text(p))
                }
            };
            let title = "Relecture automatique du dossier d'échange du téléphone (Fichier › Échange avec le téléphone)";
            view! {
                <span class="status-msg auto-sync-status" title=title>{text}</span>
                {(aside > 0 && !blocked).then(|| view! { <span class="badge-filter auto-sync-aside" title="Suppressions, conflits, séries refusées, avertissements : à traiter à la main">{format!("{aside} de côté")}</span> })}
                {blocked.then(|| view! { <span class="status-error auto-sync-blocked" role="alert">"Synchro auto bloquée (seuil)"</span> })}
            }
            .into_any()
        }}
    }
}

/// Zone « Synchronisation automatique » de la boîte d'échange.
#[component]
pub fn AutoSyncZone() -> impl IntoView {
    let ui = use_ui();
    let st = RwSignal::new(AutoView::default());
    let journal = RwSignal::new(Vec::<AutoPass>::new());
    let message = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    poll_auto(st);

    let load_journal = move || {
        spawn_local(async move {
            if let Ok(j) = auto_call::<Vec<AutoPass>>("auto_sync_journal", &NoArgs {}).await {
                journal.set(j.into_iter().take(10).collect());
            }
        });
    };
    load_journal();

    let save = move |enabled: bool, minutes: u32, auto_export: bool| {
        spawn_local(async move {
            match auto_call::<AutoView>("auto_sync_set", &AutoSetArgs { enabled, minutes: minutes.clamp(5, 120), auto_export }).await {
                Ok(v) => st.set(v),
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };
    let confirm_force = RwSignal::new(false);
    let run_now = move |force: bool| {
        busy.set(true);
        confirm_force.set(false);
        message.set("Passage demandé : il démarre dans quelques secondes (fichiers instables = reporté).".into());
        spawn_local(async move {
            if let Err(e) = auto_call::<()>("auto_sync_run_now", &RunArgs { force }).await {
                ui.error.set(Some(e));
            }
            busy.set(false);
        });
        let _ = set_timeout_with_handle(
            move || {
                fetch_auto(st);
                load_journal();
            },
            Duration::from_secs(15),
        );
    };
    let undo = move || {
        busy.set(true);
        spawn_local(async move {
            match auto_call::<String>("auto_sync_undo", &NoArgs {}).await {
                Ok(m) => {
                    ui.status.set(Some(m.clone()));
                    message.set(m);
                    ui.contacts_epoch.update(|e| *e += 1);
                    ui.tasks_epoch.update(|e| *e += 1);
                    ui.notes_epoch.update(|e| *e += 1);
                    ui.cal_epoch.update(|e| *e += 1);
                }
                Err(e) => ui.error.set(Some(e)),
            }
            fetch_auto(st);
            load_journal();
            busy.set(false);
        });
    };

    let s = move || st.with(|v| v.settings.clone());
    view! {
        <section class="auto-sync" aria-label="Synchronisation automatique">
            <h3>"Synchronisation automatique"</h3>
            <p class="dialog-note">"Quand MPE a écrit dans le dossier (contacts.vcf, calendrier.ics, notes.vnt), ThunderLook relit les fichiers tout seul, jamais pendant qu'ils s'écrivent, et n'y écrit rien. Il applique seulement les créations et modifications venues du téléphone ; suppressions, conflits et séries refusées restent de côté. Un rendez-vous créé ou modifié sur le téléphone apparaît à la relecture suivante."</p>
            <label class="check">
                <input type="checkbox" prop:disabled=move || !st.with(|v| v.dir_set)
                    prop:checked=move || s().enabled
                    on:change=move |ev| { let s = s(); save(event_target_checked(&ev), s.minutes, s.auto_export) } />
                <span>"Relire automatiquement le dossier"</span>
            </label>
            <label class="field">
                <span>"Toutes les (minutes, 5 à 120)"</span>
                <input type="number" min="5" max="120" aria-label="Minutes entre deux relectures automatiques"
                    prop:disabled=move || !s().enabled
                    prop:value=move || s().minutes.to_string()
                    on:change=move |ev| {
                        if let Ok(m) = event_target_value(&ev).trim().parse::<u32>() {
                            let s = s();
                            save(s.enabled, m, s.auto_export);
                        }
                    } />
            </label>
            <label class="check">
                <input type="checkbox" prop:disabled=move || !s().enabled
                    prop:checked=move || s().auto_export
                    on:change=move |ev| { let s = s(); save(s.enabled, s.minutes, event_target_checked(&ev)) } />
                <span>"Exporter après la relecture, seulement si le téléphone n'a rien modifié depuis le dernier export (sans effet pour un dossier écrit par MPE : il n'est jamais écrit)"</span>
            </label>
            {move || {
                let v = st.get();
                v.last.as_ref().map(|p| view! { <p class="dialog-note" role="status">{format!("Dernier passage : {} — {}", local_time(&p.at), outcome_text(p))}</p> })
            }}
            {move || {
                let v = st.get();
                let p = v.last_read?;
                let a = p.aside.clone();
                let blocked = p.outcome.kind == "blocked";
                Some(view! {
                    <div class="auto-sync-aside-box" aria-label="Mis de côté">
                        {blocked.then(|| view! {
                            <p class="exchange-warnings" role="alert">{format!("Seuil de sécurité dépassé : rien n'a été appliqué. {} S'il s'agit de la première lecture des fichiers de MPE, ou d'un changement attendu, appliquez-le en une fois ; sinon vérifiez les fichiers du téléphone, puis « Vérifier les changements » ci-dessus.", p.outcome.detail.clone().unwrap_or_default())}</p>
                            <div class="dialog-buttons">
                                {move || if confirm_force.get() {
                                    view! {
                                        <button class="btn danger" prop:disabled=move || busy.get() on:click=move |_| run_now(true)>"Oui, appliquer tout (annulable)"</button>
                                        <button class="btn" on:click=move |_| confirm_force.set(false)>"Non"</button>
                                    }.into_any()
                                } else {
                                    view! { <button class="btn" prop:disabled=move || busy.get() on:click=move |_| confirm_force.set(true)>"Appliquer malgré le seuil…"</button> }.into_any()
                                }}
                            </div>
                        })}
                        <ul class="auto-sync-counters">
                            <li class:zero=a.deletions == 0>{format!("Suppressions : {}", a.deletions)}</li>
                            <li class:zero=a.conflicts == 0>{format!("Conflits : {}", a.conflicts)}</li>
                            <li class:zero=a.series == 0>{format!("Séries refusées : {}", a.series)}</li>
                            <li class:zero=a.warnings == 0>{format!("Avertissements : {}", a.warnings)}</li>
                        </ul>
                        {(a.total() > 0).then(|| view! { <p class="dialog-note">"À traiter avec « Vérifier les changements » : rien de cela n'est appliqué sans ton choix."</p> })}
                    </div>
                })
            }}
            <div class="dialog-buttons">
                <button class="btn" prop:disabled=move || busy.get() || !st.with(|v| v.dir_set) on:click=move |_| run_now(false)>"Relire maintenant"</button>
                <button class="btn" prop:disabled=move || busy.get() || !st.with(|v| v.can_undo) on:click=move |_| undo()>"Annuler le dernier passage automatique"</button>
            </div>
            <p class="dialog-note" aria-live="polite">{move || message.get()}</p>
            <details class="auto-sync-journal" on:toggle=move |_| load_journal()>
                <summary>"Journal des passages"</summary>
                <ul>
                    {move || {
                        let j = journal.get();
                        if j.is_empty() {
                            return view! { <li>"Aucun passage pour l'instant."</li> }.into_any();
                        }
                        j.into_iter().map(|p| view! { <li><strong>{local_time(&p.at)}</strong>" — "{outcome_text(&p)}</li> }).collect_view().into_any()
                    }}
                </ul>
            </details>
        </section>
    }
}
