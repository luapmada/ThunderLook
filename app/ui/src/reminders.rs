//! Rappels : compteur « Rappels : N » dans la barre d'état et panneau des rappels dus (Ouvrir, Reporter, Ignorer).
//! Le calcul et l'état « ignoré / reporté » sont dans `thunderlook-reminders`, côté cœur.

use leptos::{prelude::*, task::spawn_local};
use serde::Deserialize;
use thunderlook_model::SearchScope;
use wasm_bindgen::JsValue;

use crate::{
    global_search,
    ipc,
    state::{Module, Ui, use_ui},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Appointment,
    Task,
    Mail,
}

/// Même forme que `thunderlook_reminders::Reminder` (la crate n'est pas compilable en WebAssembly).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Reminder {
    /// `a<rendez-vous>@<AAAA-MM-JJ>`, `t<tâche>` ou `m<message>`.
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub when_ms: i64,
    pub event_start_ms: i64,
    pub location: Option<String>,
}

const MINUTE_MS: f64 = 60_000.0;

/// Fin du report : 5 min, 1 h, ou demain 8 h (heure locale).
fn snooze_until(choice: &str) -> Option<f64> {
    let now = js_sys::Date::now();
    match choice {
        "5m" => Some(now + 5.0 * MINUTE_MS),
        "1h" => Some(now + 60.0 * MINUTE_MS),
        "tomorrow" => {
            let d = js_sys::Date::new(&JsValue::from_f64(now));
            d.set_date(d.get_date() + 1);
            d.set_hours(8);
            d.set_minutes(0);
            d.set_seconds(0);
            d.set_milliseconds(0);
            Some(d.get_time())
        }
        _ => None,
    }
}

/// « 09:30 » aujourd'hui, « 24/10 09:30 » un autre jour (heure locale).
fn when_label(ms: i64) -> String {
    let (d, now) = (js_sys::Date::new(&JsValue::from_f64(ms as f64)), js_sys::Date::new_0());
    let clock = format!("{:02}:{:02}", d.get_hours(), d.get_minutes());
    if (d.get_full_year(), d.get_month(), d.get_date()) == (now.get_full_year(), now.get_month(), now.get_date()) {
        clock
    } else {
        format!("{:02}/{:02} {clock}", d.get_date(), d.get_month() + 1)
    }
}

/// Écoute `reminders-due` et charge les rappels déjà échus au démarrage.
pub fn watch(ui: Ui) {
    ipc::on_reminders_due(move |fresh| {
        ui.reminders.update(|list| {
            for r in fresh {
                if !list.iter().any(|x| x.id == r.id) {
                    list.push(r);
                }
            }
            list.sort_by_key(|r| r.when_ms);
        });
    });
    reload(ui);
}

pub fn reload(ui: Ui) {
    spawn_local(async move {
        match ipc::reminders_due().await {
            Ok(list) => ui.reminders.set(list),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

fn remove(ui: Ui, id: &str) {
    ui.reminders.update(|l| l.retain(|r| r.id != id));
    if ui.reminders.with_untracked(Vec::is_empty) {
        ui.reminders_open.set(false);
    }
}

fn dismiss(ui: Ui, id: String) {
    remove(ui, &id);
    spawn_local(async move {
        if let Err(e) = ipc::reminder_dismiss(&id).await {
            ui.error.set(Some(e));
            reload(ui);
        }
    });
}

fn snooze(ui: Ui, id: String, choice: &str) {
    let Some(until) = snooze_until(choice) else { return };
    remove(ui, &id);
    spawn_local(async move {
        if let Err(e) = ipc::reminder_snooze(&id, until as i64).await {
            ui.error.set(Some(e));
            reload(ui);
        }
    });
}

/// Ouvre l'élément dans son module : rendez-vous → jour et sélection ; tâche → sélection.
fn open(ui: Ui, r: &Reminder) {
    ui.reminders_open.set(false);
    match r.kind {
        Kind::Appointment => match r.id.split_once('@') {
            Some((id, date)) => global_search::open_appointment(ui, id, date),
            None => global_search::open_appointment(ui, &r.id, ""),
        },
        Kind::Task => global_search::open_task(ui, &r.id),
        Kind::Mail => open_mail(ui, &r.id),
    }
}

/// Affiche le message suivi : retour au courrier, recherche vide sur « Tous les éléments », message sélectionné.
fn open_mail(ui: Ui, id: &str) {
    ui.module.set(Module::Mail);
    ui.search_scope.set(SearchScope::All);
    ui.pending_message.set(Some(id.to_string()));
    ui.search_text.set(String::new());
    ui.view_epoch.update(|e| *e += 1);
}

const STYLE: &str = "
.rem-count { background: transparent; border: 0; border-radius: var(--radius); color: inherit; font: inherit; padding: 0 calc(var(--u) * 2); height: 24px; }
.rem-count:hover, .rem-count:focus-visible { background: var(--hover); }
.rem-count.has { color: var(--accent-text); font-weight: 600; }
.rem-panel { position: fixed; z-index: 21; left: var(--u); bottom: calc(var(--target) + var(--u)); width: min(460px, calc(100vw - 16px)); max-height: 60vh; overflow-y: auto; padding: var(--u); background: var(--bg); border: 1px solid var(--border-strong); border-radius: var(--radius); box-shadow: 0 8px 24px rgb(0 0 0 / 16%); color: var(--text); }
.rem-panel h2 { margin: var(--u) calc(var(--u) * 2); font-size: var(--fs); }
.rem-row { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; gap: var(--u) calc(var(--u) * 2); padding: var(--u) calc(var(--u) * 2); border-top: 1px solid var(--border); }
.rem-row .rem-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.rem-row .rem-sub { grid-column: 1; color: var(--text-2); font-size: var(--fs-sm); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.rem-row .rem-actions { grid-column: 2; grid-row: 1 / span 2; display: flex; gap: var(--u); align-items: center; }
.rem-row .rem-actions button, .rem-row .rem-actions select { height: 24px; font-size: var(--fs-sm); background: var(--surface); color: var(--text); border: 1px solid var(--border); border-radius: var(--radius); padding: 0 calc(var(--u) * 2); }
.rem-row .rem-actions button:hover { background: var(--hover); }
";

/// Compteur « Rappels : N » (barre d'état) ; un clic ouvre ou ferme le panneau.
#[component]
pub fn StatusCount() -> impl IntoView {
    let ui = use_ui();
    let count = move || ui.reminders.with(Vec::len);
    view! {
        <style>{STYLE}</style>
        <button
            class="rem-count"
            class:has=move || ui.reminders.with(|l| !l.is_empty())
            aria-expanded=move || ui.reminders_open.get().to_string()
            title="Rappels dus"
            on:click=move |_| ui.reminders_open.update(|o| *o = !*o && ui.reminders.with_untracked(|l| !l.is_empty()))
        >
            {move || format!("Rappels : {}", count())}
        </button>
    }
}

#[component]
pub fn RemindersPanel() -> impl IntoView {
    let ui = use_ui();
    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Escape" {
            ev.prevent_default();
            ui.reminders_open.set(false);
        }
    };
    view! {
        <Show when=move || ui.reminders_open.get()>
            <div class="gs-backdrop" on:pointerdown=move |_| ui.reminders_open.set(false)></div>
            <div class="rem-panel" role="region" aria-label="Rappels" on:keydown=on_key>
                <h2>"Rappels"</h2>
                <Show when=move || ui.reminders.with(Vec::is_empty)>
                    <p class="rem-sub">"Aucun rappel."</p>
                </Show>
                <For each=move || ui.reminders.get() key=|r| (r.id.clone(), r.when_ms) let:r>
                    {
                        let (open_r, snooze_id, dismiss_id) = (r.clone(), r.id.clone(), r.id.clone());
                        let title = if r.title.trim().is_empty() { "(aucun objet)".to_string() } else { r.title.clone() };
                        let sub = match r.location.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
                            Some(l) => format!("{} · {l}", when_label(r.event_start_ms)),
                            None => when_label(r.event_start_ms),
                        };
                        view! {
                            <div class="rem-row">
                                <span class="rem-title">{title}</span>
                                <span class="rem-sub">{sub}</span>
                                <div class="rem-actions">
                                    <button on:click=move |_| open(ui, &open_r)>"Ouvrir"</button>
                                    <select
                                        aria-label="Reporter"
                                        on:change=move |ev| {
                                            snooze(ui, snooze_id.clone(), &event_target_value(&ev));
                                        }
                                    >
                                        <option value="" selected>"Reporter"</option>
                                        <option value="5m">"5 minutes"</option>
                                        <option value="1h">"1 heure"</option>
                                        <option value="tomorrow">"Demain"</option>
                                    </select>
                                    <button on:click=move |_| dismiss(ui, dismiss_id.clone())>"Ignorer"</button>
                                </div>
                            </div>
                        }
                    }
                </For>
            </div>
        </Show>
    }
}
