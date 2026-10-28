//! Lot AI : « Importer depuis MPE », dans la boîte « Échange avec le téléphone ». Lit en lecture seule les fichiers de
//! données de MyPhoneExplorer (`Phonebook.dat`, `Organizer.dat`, `Notes.dat`), montre ce qui diffère (nouveau / modifié /
//! supprimé / conflit), n'applique que ce qui est coché, après confirmation, et reste annulable. Le calcul est dans
//! `thunderlook-exchange` (`mpe_files`), les commandes dans `app/src-tauri/src/mpe_import_cmds.rs`.

use leptos::{prelude::*, task::spawn_local};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use wasm_bindgen::prelude::*;

use crate::{
    exchange::{ApplySummary, DetailCheck, DetailRow, fields_view, item_label, kind_label},
    state::use_ui,
};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

async fn call<A: Serialize, T: DeserializeOwned>(cmd: &str, args: &A) -> Result<T, String> {
    let ser = serde_wasm_bindgen::Serializer::json_compatible();
    let args = args.serialize(&ser).map_err(|e| e.to_string())?;
    let out = invoke(cmd, args).await.map_err(|e| e.as_string().unwrap_or_else(|| format!("{cmd} : {e:?}")))?;
    serde_wasm_bindgen::from_value(out).map_err(|e| format!("{cmd} : {e}"))
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Device {
    name: String,
    path: String,
    files: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Devices {
    root: String,
    devices: Vec<Device>,
}

#[derive(Serialize)]
struct RootArg {
    root: Option<String>,
}

#[derive(Serialize)]
struct DeviceArg<'a> {
    device: &'a str,
}

#[derive(Serialize)]
struct ChoiceArg<'a> {
    key: &'a str,
    choice: &'a str,
}

#[derive(Serialize)]
struct ApplyArgs<'a> {
    device: &'a str,
    keys: &'a [String],
    choices: Vec<ChoiceArg<'a>>,
}

#[derive(Serialize)]
struct NoArgs {}

/// Le bouton « Importer depuis MPE » et, dépliée dessous, la fenêtre de lecture des fichiers de MPE.
#[component]
pub fn MpeImport() -> impl IntoView {
    let ui = use_ui();
    let open = RwSignal::new(false);
    let found = RwSignal::new(Devices::default());
    let device = RwSignal::new(String::new());
    let rows = RwSignal::new(Vec::<DetailRow>::new());
    let conflicts = RwSignal::new(Vec::<DetailRow>::new());
    // Choix par conflit : "thunderlook", "phone" ou absent (= ignorer). Jamais renseigné d'office.
    let choices = RwSignal::new(std::collections::HashMap::<String, String>::new());
    let confirming = RwSignal::new(false);
    let summary = RwSignal::new(Option::<ApplySummary>::None);
    let checked_once = RwSignal::new(false);
    let note = RwSignal::new(String::new());
    let warnings = RwSignal::new(Vec::<String>::new());
    let can_undo = RwSignal::new(false);
    let busy = RwSignal::new(false);
    // Cocher une case ne reconstruit pas la liste des changements (voir `exchange::row_keys`).
    let keys = Memo::new(move |_| rows.with(|r| crate::exchange::row_keys(r)));

    let reset = move || {
        rows.set(Vec::new());
        conflicts.set(Vec::new());
        choices.set(Default::default());
        confirming.set(false);
        summary.set(None);
        checked_once.set(false);
        warnings.set(Vec::new());
    };
    let reload_lists = move || {
        ui.contacts_epoch.update(|e| *e += 1);
        ui.notes_epoch.update(|e| *e += 1);
        ui.cal_epoch.update(|e| *e += 1);
    };
    let set_found = move |d: Devices| {
        // Un seul appareil : choisi d'office (rien n'est lu pour autant, il faut cliquer sur « Vérifier »).
        device.set(if d.devices.len() == 1 { d.devices[0].path.clone() } else { String::new() });
        note.set(if d.devices.is_empty() { format!("Aucun appareil MPE trouvé dans {}.", d.root) } else { String::new() });
        found.set(d);
        reset();
    };
    let scan = move || {
        spawn_local(async move {
            match call::<_, Devices>("mpe_import_devices", &RootArg { root: None }).await {
                Ok(d) => set_found(d),
                Err(e) => {
                    found.set(Devices::default());
                    note.set(e);
                }
            }
        });
    };
    let toggle = move || {
        let now = !open.get_untracked();
        open.set(now);
        if now {
            reset();
            scan();
        }
    };
    let pick = move || {
        spawn_local(async move {
            match call::<_, Option<Devices>>("mpe_import_pick", &NoArgs {}).await {
                Ok(Some(d)) => set_found(d),
                Ok(None) => {}
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };
    let check = move || {
        let path = device.get_untracked();
        if path.is_empty() {
            return;
        }
        busy.set(true);
        reset();
        spawn_local(async move {
            match call::<_, DetailCheck>("mpe_import_check", &DeviceArg { device: &path }).await {
                Ok(r) => {
                    let total = r.changes.len() + r.conflicts.len();
                    note.set(if total == 0 {
                        format!("Aucun changement ({} élément(s) identiques).", r.unchanged)
                    } else {
                        format!("{} changement(s) et {} conflit(s) proposés, {} identique(s).", r.changes.len(), r.conflicts.len(), r.unchanged)
                    });
                    warnings.set(r.warnings);
                    conflicts.set(r.conflicts);
                    rows.set(r.changes);
                    checked_once.set(true);
                }
                // Fichier tronqué ou en cours d'écriture : refus complet, rien n'a été lu.
                Err(e) => {
                    note.set(String::new());
                    warnings.set(vec![e]);
                }
            }
            busy.set(false);
        });
    };
    let apply = move || {
        let path = device.get_untracked();
        let keys: Vec<String> = rows.with_untracked(|r| r.iter().filter(|c| c.selected).map(|c| c.key.clone()).collect());
        let picks: Vec<(String, String)> = choices.with_untracked(|c| c.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        confirming.set(false);
        if path.is_empty() || (keys.is_empty() && picks.is_empty()) {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            let choices = picks.iter().map(|(key, choice)| ChoiceArg { key, choice }).collect();
            match call::<_, ApplySummary>("mpe_import_apply", &ApplyArgs { device: &path, keys: &keys, choices }).await {
                Ok(sum) => {
                    ui.status.set(Some(format!("Import MPE : {} créé(s), {} modifié(s), {} supprimé(s)", sum.created, sum.modified, sum.deleted)));
                    reset();
                    note.set(String::new());
                    summary.set(Some(sum));
                    can_undo.set(true);
                    reload_lists();
                }
                Err(e) => ui.error.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let undo = move || {
        busy.set(true);
        spawn_local(async move {
            match call::<_, String>("phone_exchange_undo", &NoArgs {}).await {
                Ok(m) => {
                    ui.status.set(Some(m.clone()));
                    reset();
                    note.set(m);
                    can_undo.set(false);
                    reload_lists();
                }
                Err(e) => ui.error.set(Some(e)),
            }
            busy.set(false);
        });
    };

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

    view! {
        <div class="mpe-import">
            <div class="dialog-buttons">
                <button class="btn" aria-expanded=move || open.get().to_string() on:click=move |_| toggle()>"Importer depuis MPE"</button>
            </div>
            <Show when=move || open.get()>
                <section aria-label="Importer depuis MPE">
                    <p class="dialog-note">
                        "Lecture seule des fichiers de MyPhoneExplorer (contacts, rendez-vous des 90 jours avant et après, notes). "
                        <strong>"MPE reste la source et ne reçoit rien en retour"</strong>
                        " : sens téléphone → ThunderLook seulement. Rien n'est écrit dans le dossier de MPE, rien n'est appliqué sans votre accord."
                    </p>
                    <div class="field">
                        <span>"Dossier exploré"</span>
                        <div>
                            <code>{move || found.with(|f| if f.root.is_empty() { "(aucun)".to_string() } else { f.root.clone() })}</code>
                            " "
                            <button class="btn" on:click=move |_| pick()>"Choisir un autre dossier…"</button>
                        </div>
                    </div>
                    <Show when=move || found.with(|f| !f.devices.is_empty())>
                        <div class="exchange-choice" role="radiogroup" aria-label="Appareil MPE">
                            {move || {
                                found
                                    .get()
                                    .devices
                                    .into_iter()
                                    .map(|d| {
                                        let path = d.path.clone();
                                        let path2 = d.path.clone();
                                        view! {
                                            <label class="check">
                                                <input type="radio" name="mpe-device"
                                                    prop:checked=move || device.get() == path
                                                    on:change=move |_| {
                                                        device.set(path2.clone());
                                                        reset();
                                                    } />
                                                <span><strong>{d.name.clone()}</strong>" — "{d.files.join(", ")}</span>
                                            </label>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </div>
                    </Show>
                    <div class="dialog-buttons">
                        <button class="btn primary" prop:disabled=move || busy.get() || device.with(String::is_empty) on:click=move |_| check()>"Vérifier les changements"</button>
                    </div>
                    <Show when=move || !warnings.with(Vec::is_empty)>
                        <div class="exchange-warnings" role="alert">
                            <strong>{move || format!("{} avertissement(s) — à lire avant d'appliquer", warnings.with(Vec::len))}</strong>
                            <ul>{move || warnings.get().into_iter().map(|w| view! { <li>{w}</li> }).collect_view()}</ul>
                        </div>
                    </Show>
                    <Show when=move || checked_once.get() && !conflicts.with(Vec::is_empty)>
                        <section class="exchange-conflicts" aria-label="Conflits">
                            <h3>{move || format!("Conflits ({}) — à trancher", conflicts.with(Vec::len))}</h3>
                            <p class="dialog-note">"Rien n'est choisi d'office : sans choix, l'élément est ignoré."</p>
                            <ul class="exchange-changes">
                                {move || {
                                    conflicts
                                        .get()
                                        .into_iter()
                                        .map(|c| {
                                            let why = if c.conflict.as_deref() == Some("unknown") { "valeur de référence inconnue : impossible de savoir qui a changé" } else { "modifié dans ThunderLook et sur le téléphone" };
                                            let name = format!("mpe-conflit_{}", c.key);
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
                                                        {radio("phone", "Garder le téléphone (MPE)")}
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
                        <section aria-label="Changements">
                            <h3>{move || format!("Changements ({})", rows.with(Vec::len))}</h3>
                            <ul class="exchange-changes">
                                {move || {
                                    keys.track();
                                    rows.get_untracked()
                                        .into_iter()
                                        .enumerate()
                                        .map(|(i, c)| {
                                            let deletion = c.kind == "deleted";
                                            view! {
                                                <li>
                                                    <label class="check">
                                                        <input type="checkbox" prop:checked=move || rows.with(|r| r.get(i).is_some_and(|c| c.selected))
                                                            on:change=move |ev| {
                                                                let on = event_target_checked(&ev);
                                                                rows.update(|r| {
                                                                    if let Some(c) = r.get_mut(i) {
                                                                        c.selected = on;
                                                                    }
                                                                });
                                                            } />
                                                        <span><strong>{kind_label(&c.kind)}</strong>" — "{item_label(&c.item)}" : "{c.summary.clone()}</span>
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
                    <Show when=move || checked_once.get() && (!rows.with(Vec::is_empty) || !conflicts.with(Vec::is_empty))>
                        <p class="exchange-count" aria-live="polite">
                            {move || {
                                let (n, m, d) = counts();
                                format!("Sera appliqué : {n} création(s), {m} modification(s), {d} suppression(s). Gardés côté ThunderLook : {}.", kept_count())
                            }}
                        </p>
                    </Show>
                    <Show when=move || confirming.get()>
                        <div class="exchange-confirm" role="alertdialog" aria-label="Confirmation">
                            <p>{move || {
                                let (_, _, d) = counts();
                                let tail = if d > 0 { format!(", dont {d} suppression(s)") } else { String::new() };
                                format!("Appliquer {} élément(s) dans ThunderLook{tail} ? MPE ne reçoit rien ; l'import pourra être annulé ensuite.", apply_count())
                            }}</p>
                            <button class="btn primary" prop:disabled=move || busy.get() on:click=move |_| apply()>"Confirmer"</button>
                            " "
                            <button class="btn" on:click=move |_| confirming.set(false)>"Retour"</button>
                        </div>
                    </Show>
                    {move || summary.get().map(|sum| view! {
                        <div class="exchange-summary" role="status">
                            <strong>"Import MPE terminé"</strong>
                            <ul>
                                <li>{format!("Créés : {}", sum.created)}</li>
                                <li>{format!("Modifiés : {}", sum.modified)}</li>
                                <li>{format!("Supprimés : {}", sum.deleted)}</li>
                                <li>{format!("Gardés côté ThunderLook : {}", sum.kept)}</li>
                                <li>{format!("Ignorés : {}", sum.ignored)}</li>
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
                        <button class="btn" prop:disabled=move || busy.get() || !can_undo.get() on:click=move |_| undo()>"Annuler le dernier import"</button>
                    </div>
                    <p class="dialog-note" aria-live="polite">{move || note.get()}</p>
                </section>
            </Show>
        </div>
    }
}
