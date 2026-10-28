//! « Personnalisé… » du menu Assurer un suivi : échéance libre de l'indicateur et, au choix, un rappel (date et heure)
//! gardé par ThunderLook (`reminders.json`, commandes `mail_reminder_*`) ; l'indicateur lui-même est posé par
//! `FlagAction::DueOn`. Le rappel ne vaut que pour le message affiché.

use leptos::{html, prelude::*, task::spawn_local};
use serde::Serialize;
use thunderlook_model::{FlagAction, MessageOp};
use wasm_bindgen::{JsCast, JsValue};

use crate::{
    actions,
    ipc,
    state::{Ui, use_ui},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IdArg<'a> {
    message_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetArg<'a> {
    message_id: &'a str,
    when_ms: i64,
}

const DEFAULT_TIME: &str = "09:00";

pub fn open(ui: Ui) {
    if ui.selected_pos.get_untracked().is_some() || !ui.sel_ranges.with_untracked(Vec::is_empty) {
        ui.flag_dialog.set(true);
    }
}

/// « AAAA-MM-JJ » du jour local de `ms`.
fn date_value(ms: f64) -> String {
    let d = js_sys::Date::new(&JsValue::from_f64(ms));
    format!("{:04}-{:02}-{:02}", d.get_full_year(), d.get_month() + 1, d.get_date())
}

fn time_value(ms: f64) -> String {
    let d = js_sys::Date::new(&JsValue::from_f64(ms));
    format!("{:02}:{:02}", d.get_hours(), d.get_minutes())
}

/// Instant local (ms Unix) de « AAAA-MM-JJ » et « HH:MM ».
fn local_ms(date: &str, time: &str) -> Option<i64> {
    let mut d = date.split('-').map(|p| p.parse::<i32>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.split(':').map(|p| p.parse::<i32>().ok());
    let (h, min) = (t.next()??, t.next()??);
    let ms = js_sys::Date::new_with_year_month_day_hr_min_sec(u32::try_from(y).ok()?, m - 1, day, h, min, 0).get_time();
    ms.is_finite().then_some(ms as i64)
}

/// La touche vient d'un bouton (qui garde son propre Entrée).
pub(crate) fn on_button(ev: &web_sys::KeyboardEvent) -> bool {
    ev.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()).is_some_and(|t| t.tag_name() == "BUTTON")
}

fn several_selected(ui: Ui) -> bool {
    ui.sel_ranges.with_untracked(|r| r.iter().map(|(a, b)| b.saturating_sub(*a)).sum::<u32>() > 1)
}

#[component]
pub fn FlagDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let due = RwSignal::new(String::new());
    let remind = RwSignal::new(false);
    let remind_date = RwSignal::new(String::new());
    let remind_time = RwSignal::new(DEFAULT_TIME.to_string());
    let multi = RwSignal::new(false);
    let had_reminder = RwSignal::new(false);
    // Ouverture en cours : la réponse de `mail_reminder_get` d'une ouverture précédente est ignorée.
    let opening = StoredValue::new(0u32);

    // À chaque ouverture : aujourd'hui, rappel décoché, ou le rappel déjà posé sur le message.
    Effect::new(move |_| {
        if !ui.flag_dialog.get() {
            return;
        }
        let today = date_value(js_sys::Date::now());
        due.set(today.clone());
        remind.set(false);
        remind_date.set(today);
        remind_time.set(DEFAULT_TIME.to_string());
        had_reminder.set(false);
        multi.set(several_selected(ui));
        let me = opening.get_value().wrapping_add(1);
        opening.set_value(me);
        if let Some(id) = ui.selected.get_untracked().filter(|_| !multi.get_untracked()) {
            spawn_local(async move {
                let got = ipc::call::<_, Option<i64>>("mail_reminder_get", &IdArg { message_id: &id }).await;
                if opening.try_get_value() != Some(me) || !ui.flag_dialog.get_untracked() {
                    return;
                }
                if let Ok(Some(ms)) = got {
                    remind.set(true);
                    had_reminder.set(true);
                    remind_date.set(date_value(ms as f64));
                    remind_time.set(time_value(ms as f64));
                }
            });
        }
        // Après le rendu : à l'exécution de l'effet, la boîte (dans `Show`) n'est pas encore montée et rien ne prenait le focus.
        request_animation_frame(move || {
            if let Some(el) = root.get_untracked() {
                if let Ok(Some(first)) = el.query_selector("input") {
                    if let Ok(first) = first.dyn_into::<web_sys::HtmlElement>() {
                        let _ = first.focus();
                    }
                }
            }
        });
    });

    let close = move || ui.flag_dialog.set(false);
    let submit = move || {
        let Some(due_ms) = local_ms(&due.get_untracked(), "00:00") else {
            ui.error.set(Some("Date d'échéance invalide.".into()));
            return;
        };
        let reminder = if multi.get_untracked() || !remind.get_untracked() {
            None
        } else {
            match local_ms(&remind_date.get_untracked(), &remind_time.get_untracked()) {
                Some(ms) => Some(ms),
                None => {
                    ui.error.set(Some("Date ou heure du rappel invalide.".into()));
                    return;
                }
            }
        };
        let id = ui.selected.get_untracked().filter(|_| !multi.get_untracked());
        let clear = !remind.get_untracked() && had_reminder.get_untracked();
        close();
        actions::run(ui, MessageOp::Flag { action: FlagAction::DueOn(due_ms) });
        let Some(id) = id else { return };
        spawn_local(async move {
            let res = match (reminder, clear) {
                (Some(when_ms), _) => ipc::call::<_, ()>("mail_reminder_set", &SetArg { message_id: &id, when_ms }).await,
                (None, true) => ipc::call::<_, ()>("mail_reminder_clear", &IdArg { message_id: &id }).await,
                (None, false) => Ok(()),
            };
            match res {
                Ok(()) => crate::reminders::reload(ui),
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };
    let on_key = move |ev: leptos::ev::KeyboardEvent| match ev.key().as_str() {
        "Escape" => {
            ev.prevent_default();
            close();
        }
        // Entrée sur « Annuler » (ou « OK ») active ce bouton, elle ne valide pas la boîte.
        "Enter" if !on_button(&ev) => {
            ev.prevent_default();
            submit();
        }
        _ => {}
    };

    view! {
        <Show when=move || ui.flag_dialog.get()>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="flag-dialog-title" node_ref=root on:keydown=on_key>
                    <h2 id="flag-dialog-title">"Indicateur de suivi personnalisé"</h2>
                    <label>
                        "Échéance : "
                        <input type="date" aria-label="Échéance" prop:value=move || due.get() on:input=move |ev| due.set(event_target_value(&ev)) />
                    </label>
                    <label>
                        <input
                            type="checkbox"
                            prop:checked=move || remind.get() && !multi.get()
                            prop:disabled=move || multi.get()
                            on:change=move |ev| remind.set(event_target_checked(&ev))
                        />
                        " Rappel"
                    </label>
                    <div>
                        <input
                            type="date"
                            aria-label="Date du rappel"
                            prop:disabled=move || !remind.get() || multi.get()
                            prop:value=move || remind_date.get()
                            on:input=move |ev| remind_date.set(event_target_value(&ev))
                        />
                        <input
                            type="time"
                            aria-label="Heure du rappel"
                            prop:disabled=move || !remind.get() || multi.get()
                            prop:value=move || remind_time.get()
                            on:input=move |ev| remind_time.set(event_target_value(&ev))
                        />
                    </div>
                    {move || multi.get().then(|| view! { <p class="dialog-note">"ⓘ Le rappel ne se pose que sur un seul message à la fois."</p> })}
                    <p class="dialog-note">"ⓘ Le rappel reste dans ThunderLook : rien n'est écrit dans Outlook."</p>
                    <div class="dialog-buttons">
                        <button class="btn primary" on:click=move |_| submit()>"OK"</button>
                        <button class="btn" on:click=move |_| close()>"Annuler"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
