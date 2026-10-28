//! Module Tâches : liste (non terminées d'abord, par échéance), case à cocher « terminée », boîte « Tâche » pour créer
//! et modifier. Tout est écrit dans la base de ThunderLook.

use leptos::{html, prelude::*, task::spawn_local};
use serde::{Serialize, de::DeserializeOwned};
use thunderlook_model::items::{TaskForm, TaskList, TaskQuery};
use wasm_bindgen::{JsCast, prelude::*};

use crate::{
    icons::*,
    ipc,
    item_folders::{FolderHost, FolderKind, FolderPicker, FolderUi},
    state::{Module, Ui, use_ui},
};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

/// Appel d'une commande de `items_cmds.rs` (hors `ipc.rs`) ; `pub(crate)` pour `notes.rs`.
pub(crate) async fn call<A: Serialize, T: DeserializeOwned>(cmd: &str, args: &A) -> Result<T, String> {
    let args = args.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).map_err(|e| e.to_string())?;
    let out = tauri_invoke(cmd, args).await.map_err(|e| e.as_string().unwrap_or_else(|| format!("{cmd} : {e:?}")))?;
    serde_wasm_bindgen::from_value(out).map_err(|e| format!("{cmd} : {e}"))
}

#[derive(Serialize)]
struct QueryArg<'a> {
    query: &'a TaskQuery,
}

const VIEWS: [(&str, &str); 5] = [("all", "Toutes"), ("today", "Aujourd'hui"), ("overdue", "En retard"), ("week", "Cette semaine"), ("done", "Terminées")];
const SORTS: [(&str, &str); 3] = [("due", "Échéance"), ("importance", "Importance"), ("title", "Titre")];
const REPEATS: [(&str, &str); 9] = [
    ("", "Jamais"),
    ("daily", "Chaque jour"),
    ("weekly", "Chaque semaine"),
    ("monthly", "Chaque mois"),
    ("yearly", "Chaque année"),
    ("regen_daily", "N jours après la fin"),
    ("regen_weekly", "N semaines après la fin"),
    ("regen_monthly", "N mois après la fin"),
    ("regen_yearly", "N ans après la fin"),
];
const REPEAT_ENDS: [(&str, &str); 3] = [("never", "Sans fin"), ("count", "Après N fois"), ("until", "Jusqu'au")];

const STATUSES: [(i32, &str); 5] = [(0, "Pas commencée"), (1, "En cours"), (2, "Terminée"), (3, "En attente de quelqu'un d'autre"), (4, "Reportée")];
const IMPORTANCES: [(i32, &str); 3] = [(0, "Basse"), (1, "Normale"), (2, "Haute")];

/// Recharge la liste à chaque ouverture du module et après chaque modification.
pub fn watch(ui: Ui) {
    Effect::new(move |_| {
        ui.tasks_epoch.track();
        if ui.module.get() != Module::Tasks {
            return;
        }
        spawn_local(async move {
            match ipc::tasks_list().await {
                Ok(l) => ui.tasks.set(Some(l)),
                Err(e) => ui.error.set(Some(e)),
            }
        });
    });
}

fn changed(ui: Ui) {
    ui.tasks_epoch.update(|e| *e += 1);
}

pub fn open_new(ui: Ui) {
    spawn_local(async move {
        match ipc::task_form(None).await {
            Ok(f) => ui.task_form.set(Some(f)),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

fn open_edit(ui: Ui, id: String) {
    spawn_local(async move {
        match ipc::task_form(Some(&id)).await {
            Ok(f) => ui.task_form.set(Some(f)),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

fn set_complete(ui: Ui, id: String, done: bool) {
    spawn_local(async move {
        match ipc::task_complete(&id, done).await {
            Ok(()) => changed(ui),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

/// Ruban : marque la tâche sélectionnée terminée (ou non terminée si elle l'est déjà).
pub fn toggle_selected(ui: Ui) {
    let Some(id) = ui.task_selected.get_untracked() else { return };
    let done = ui.tasks.with_untracked(|l| l.as_ref().and_then(|l| l.rows.iter().find(|r| r.id == id)).is_some_and(|r| r.complete));
    set_complete(ui, id, !done);
}

pub fn delete_selected(ui: Ui) {
    let Some(id) = ui.task_selected.get_untracked() else { return };
    // Tâche périodique en cours : seule cette occurrence est supprimée, la suivante est créée.
    let occurrence = ui.tasks.with_untracked(|l| l.as_ref().and_then(|l| l.rows.iter().find(|r| r.id == id)).is_some_and(|r| r.recurring && !r.complete));
    spawn_local(async move {
        let done = if occurrence {
            ipc::task_delete_occurrence(&id).await.map(|next| if next.is_some() { "Occurrence supprimée, la suivante est créée" } else { "Tâche supprimée" })
        } else {
            ipc::task_delete(&id).await.map(|()| "Tâche supprimée")
        };
        match done {
            Ok(msg) => {
                ui.task_selected.set(None);
                ui.status.set(Some(msg.into()));
                changed(ui);
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}


/// `AAAA-MM-JJ` → `JJ/MM/AAAA`.
fn fr_date(d: &str) -> String {
    match d.split('-').collect::<Vec<_>>().as_slice() {
        [y, m, day] => format!("{day}/{m}/{y}"),
        _ => d.to_string(),
    }
}

fn today() -> String {
    let d = js_sys::Date::new_0();
    format!("{:04}-{:02}-{:02}", d.get_full_year(), d.get_month() + 1, d.get_date())
}

#[component]
pub fn TasksPane() -> impl IntoView {
    let ui = use_ui();
    let (view_sel, sort, shown) = (RwSignal::new("all".to_string()), RwSignal::new("due".to_string()), RwSignal::new(None::<TaskList>));
    let fu = FolderUi::new(FolderKind::Task);
    fu.refresh();
    // Vue, tri et recherche sont calculés par le cœur (`tasks_query`) ; rechargés aussi après chaque modification.
    Effect::new(move |_| {
        ui.tasks_epoch.track();
        let (v, so, text) = (view_sel.get(), sort.get(), ui.task_filter.get());
        let (view, folder) = if v.starts_with('f') { ("folder".to_string(), v) } else { (v, String::new()) };
        let query = TaskQuery { view, folder, sort: so, text };
        spawn_local(async move {
            match call::<_, TaskList>("tasks_query", &QueryArg { query: &query }).await {
                Ok(l) => {
                    crate::print::remember_tasks(&l);
                    shown.set(Some(l));
                }
                Err(e) => ui.error.set(Some(e)),
            }
        });
    });
    view! {
        <section class="pane tasks" aria-label="Tâches">
            <FolderHost f=fu on_change=move |()| {
                if view_sel.get_untracked().starts_with('f') && !fu.list.with_untracked(|l| l.iter().any(|d| d.id == view_sel.get_untracked())) {
                    view_sel.set("all".into());
                }
                changed(ui);
            } />
            <div class="task-views">
                <FolderPicker f=fu value=Signal::derive(move || Some(view_sel.get()).filter(|v| v.starts_with('f')).unwrap_or_default())
                    on_pick=move |v: String| view_sel.set(if v.is_empty() { "all".into() } else { v }) all_label="Tous les dossiers" />
                <select aria-label="Afficher" on:change=move |ev| view_sel.set(event_target_value(&ev))>
                    {VIEWS.iter().map(|&(v, l)| view! { <option value=v selected=move || view_sel.get() == v>{l}</option> }).collect_view()}
                </select>
                <select aria-label="Trier par" on:change=move |ev| sort.set(event_target_value(&ev))>
                    {SORTS.iter().map(|&(v, l)| view! { <option value=v selected=move || sort.get() == v>{format!("Tri : {l}")}</option> }).collect_view()}
                </select>
            </div>
            <div class="contact-search">
                <Svg icon=SEARCH size=14 />
                <input type="search" placeholder="Rechercher dans les tâches" aria-label="Rechercher dans les tâches"
                    prop:value=move || ui.task_filter.get() on:input=move |ev| ui.task_filter.set(event_target_value(&ev)) />
            </div>
            <div class="task-rows" role="listbox" aria-label="Liste des tâches">
                {move || match shown.with(Option::is_none) {
                    true => view! { <div class="placeholder">"Chargement des tâches…"</div> }.into_any(),
                    false => {
                        let rows = shown.with(|l| l.as_ref().map(|l| l.rows.clone()).unwrap_or_default());
                        if rows.is_empty() {
                            return view! { <div class="placeholder">"Aucune tâche."</div> }.into_any();
                        }
                        let now = today();
                        rows.into_iter()
                            .map(|r| {
                                let (sel, aria, edit_id, check_id, click_id) = (r.id.clone(), r.id.clone(), r.id.clone(), r.id.clone(), r.id.clone());
                                let overdue = !r.complete && !r.due.is_empty() && r.due < now;
                                let done = r.complete;
                                view! {
                                    <div class="task-row" role="option" class:selected=move || ui.task_selected.get().as_deref() == Some(sel.as_str())
                                        aria-selected=move || (ui.task_selected.get().as_deref() == Some(aria.as_str())).to_string()
                                        class:done=done on:click=move |_| ui.task_selected.set(Some(click_id.clone()))
                                        on:dblclick=move |_| open_edit(ui, edit_id.clone())>
                                        <input type="checkbox" aria-label="Terminée" prop:checked=done
                                            on:click=move |ev| ev.stop_propagation()
                                            on:change=move |ev| set_complete(ui, check_id.clone(), event_target_checked(&ev)) />
                                        <span class="task-title">{r.title.clone()}</span>
                                        {r.recurring.then(|| view! { <span class="task-flag" title="Tâche périodique">"↻"</span> })}
                                        {(r.importance == 2).then(|| view! { <span class="task-flag" title="Importance haute">"!"</span> })}
                                        {(!r.categories.is_empty()).then(|| view! { <span class="task-cats">{r.categories.join(", ")}</span> })}
                                        {(!r.due.is_empty()).then(|| view! { <span class="task-due" class:overdue=overdue>{fr_date(&r.due)}</span> })}
                                    </div>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }
                }}
            </div>
        </section>
    }
}

#[component]
pub fn TaskDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let busy = RwSignal::new(false);
    let note = RwSignal::new(None::<String>);
    let form = move || ui.task_form.get();
    let set = move |f: &dyn Fn(&mut TaskForm)| {
        ui.task_form.update(|c| {
            if let Some(c) = c {
                f(c);
            }
        })
    };
    // Seulement à l'ouverture : suivre `task_form` relancerait l'effet à chaque frappe (focus volé).
    let open = Memo::new(move |_| ui.task_form.with(Option::is_some));
    Effect::new(move |_| {
        if open.get() {
            note.set(None);
            request_animation_frame(move || {
                if let Some(r) = root.get_untracked() {
                    if let Ok(Some(el)) = r.query_selector("input") {
                        if let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() {
                            let _ = el.focus();
                        }
                    }
                }
            });
        }
    });
    let close = move || ui.task_form.set(None);
    let save = move || {
        let Some(f) = ui.task_form.get_untracked() else { return };
        busy.set(true);
        spawn_local(async move {
            match ipc::task_save(&f).await {
                Ok(id) => {
                    ui.task_selected.set(Some(id));
                    ui.status.set(Some("Tâche enregistrée".into()));
                    changed(ui);
                    close();
                }
                Err(e) => note.set(Some(e)),
            }
            busy.set(false);
        });
    };
    view! {
        <Show when=move || ui.task_form.with(Option::is_some)>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog wide appt" role="dialog" aria-modal="true" aria-labelledby="task-title" node_ref=root
                    on:keydown=move |ev| match ev.key().as_str() {
                        "Escape" => {
                            ev.prevent_default();
                            close();
                        }
                        "s" if ev.ctrl_key() => {
                            ev.prevent_default();
                            save();
                        }
                        _ => {}
                    }>
                    <h2 id="task-title">{move || if form().is_some_and(|f| f.id.is_some()) { "Tâche" } else { "Nouvelle tâche" }}</h2>
                    <label class="field">
                        <span>"Objet"</span>
                        <input type="text" aria-label="Objet" prop:value=move || form().map(|f| f.subject).unwrap_or_default()
                            on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.subject = v.clone()); } />
                    </label>
                    <div class="appt-row">
                        <label class="field">
                            <span>"Début"</span>
                            <input type="date" aria-label="Date de début" prop:value=move || form().map(|f| f.start).unwrap_or_default()
                                on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.start = v.clone()); } />
                        </label>
                        <label class="field">
                            <span>"Échéance"</span>
                            <input type="date" aria-label="Échéance" prop:value=move || form().map(|f| f.due).unwrap_or_default()
                                on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.due = v.clone()); } />
                        </label>
                        <label class="field">
                            <span>"État"</span>
                            <select aria-label="État" on:change=move |ev| { let v = event_target_value(&ev).parse().unwrap_or(0); set(&|f| f.status = v); }>
                                {STATUSES.iter().map(|&(v, l)| view! { <option value=v.to_string() selected=move || form().is_some_and(|f| f.status == v)>{l}</option> }).collect_view()}
                            </select>
                        </label>
                        <label class="field">
                            <span>"Importance"</span>
                            <select aria-label="Importance" on:change=move |ev| { let v = event_target_value(&ev).parse().unwrap_or(1); set(&|f| f.importance = v); }>
                                {IMPORTANCES.iter().map(|&(v, l)| view! { <option value=v.to_string() selected=move || form().is_some_and(|f| f.importance == v)>{l}</option> }).collect_view()}
                            </select>
                        </label>
                    </div>
                    <div class="appt-row">
                        <label class="field">
                            <span>"Rappel"</span>
                            <input type="datetime-local" aria-label="Rappel" prop:value=move || form().and_then(|f| f.reminder).unwrap_or_default()
                                on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.reminder = Some(v.clone()).filter(|r| !r.is_empty())); } />
                        </label>
                        <label class="field">
                            <span>"Répéter"</span>
                            <select aria-label="Répéter" on:change=move |ev| {
                                let v = event_target_value(&ev);
                                set(&|f| {
                                    f.repeat = v.clone();
                                    if f.repeat_every == 0 { f.repeat_every = 1; }
                                    if f.repeat_end.is_empty() { f.repeat_end = "never".into(); }
                                    if f.repeat_count == 0 { f.repeat_count = 5; }
                                });
                            }>
                                {REPEATS.iter().map(|&(v, l)| view! { <option value=v selected=move || form().is_some_and(|f| f.repeat == v)>{l}</option> }).collect_view()}
                                <Show when=move || form().is_some_and(|f| f.repeat == "custom")>
                                    <option value="custom" selected=true>"Personnalisée (conservée)"</option>
                                </Show>
                            </select>
                        </label>
                    </div>
                    <Show when=move || form().is_some_and(|f| !f.repeat.is_empty() && f.repeat != "custom")>
                        <div class="appt-row">
                            <label class="field">
                                <span>"Tous les"</span>
                                <input type="number" min="1" max="999" aria-label="Intervalle" prop:value=move || form().map(|f| f.repeat_every.max(1)).unwrap_or(1)
                                    on:input=move |ev| { let v = event_target_value(&ev).parse().unwrap_or(1); set(&|f| f.repeat_every = v); } />
                            </label>
                            <Show when=move || form().is_some_and(|f| !f.repeat.starts_with("regen_"))>
                            <label class="field">
                                <span>"Fin"</span>
                                <select aria-label="Fin de la répétition" on:change=move |ev| { let v = event_target_value(&ev); set(&|f| f.repeat_end = v.clone()); }>
                                    {REPEAT_ENDS.iter().map(|&(v, l)| view! { <option value=v selected=move || form().is_some_and(|f| f.repeat_end == v || (v == "never" && f.repeat_end.is_empty()))>{l}</option> }).collect_view()}
                                </select>
                            </label>
                            <Show when=move || form().is_some_and(|f| f.repeat_end == "count")>
                                <label class="field">
                                    <span>"Nombre de fois"</span>
                                    <input type="number" min="1" max="9999" aria-label="Nombre d'occurrences" prop:value=move || form().map(|f| f.repeat_count).unwrap_or(5)
                                        on:input=move |ev| { let v = event_target_value(&ev).parse().unwrap_or(1); set(&|f| f.repeat_count = v); } />
                                </label>
                            </Show>
                            <Show when=move || form().is_some_and(|f| f.repeat_end == "until")>
                                <label class="field">
                                    <span>"Dernier jour"</span>
                                    <input type="date" aria-label="Dernier jour" prop:value=move || form().map(|f| f.repeat_until).unwrap_or_default()
                                        on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.repeat_until = v.clone()); } />
                                </label>
                            </Show>
                            </Show>
                        </div>
                    </Show>
                    <label class="field">
                        <span>"Catégories (séparées par des virgules)"</span>
                        <input type="text" aria-label="Catégories" prop:value=move || form().map(|f| f.categories).unwrap_or_default()
                            on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.categories = v.clone()); } />
                    </label>
                    <label class="field">
                        <span>"Description"</span>
                        <textarea class="ar-text" aria-label="Description" rows="6" prop:value=move || form().map(|f| f.body).unwrap_or_default()
                            on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.body = v.clone()); }></textarea>
                    </label>
                    {move || note.get().map(|n| view! { <p class="dialog-note" role="alert">{n}</p> })}
                    <div class="dialog-buttons">
                        <button class="btn primary" prop:disabled=move || busy.get() on:click=move |_| save()>"Enregistrer et fermer"</button>
                        <button class="btn" on:click=move |_| close()>"Annuler"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
