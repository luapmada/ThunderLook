//! Boîte « Rendez-vous » du calendrier commun avec Outlook (J9, étape 4) : créer, modifier ; suppression depuis le
//! détail. Tout passe par Outlook (le cœur écrit par son modèle objet, puis relit le calendrier).

use leptos::{html, prelude::*, task::spawn_local};
use thunderlook_model::calendar::AppointmentForm;
use wasm_bindgen::JsCast;

use crate::{ipc, state::{Ui, use_ui}};

/// Rappels proposés (minutes), comme la liste d'Outlook.
const REMINDERS: [(i32, &str); 11] = [
    (0, "0 minute"),
    (5, "5 minutes"),
    (10, "10 minutes"),
    (15, "15 minutes"),
    (30, "30 minutes"),
    (60, "1 heure"),
    (120, "2 heures"),
    (720, "0,5 jour"),
    (1_080, "18 heures"),
    (1_440, "1 jour"),
    (10_080, "1 semaine"),
];

const BUSY: [(i32, &str); 5] = [(0, "Disponible"), (4, "Travail ailleurs"), (1, "Provisoire"), (2, "Occupé(e)"), (3, "Absent(e) du bureau")];

/// Calendrier commun où créer : celui du rendez-vous sélectionné s'il l'est, sinon le calendrier par défaut d'Outlook,
/// sinon le premier affiché.
pub(crate) fn target_calendar(ui: Ui) -> Option<String> {
    ui.cal_view.with_untracked(|v| {
        let v = v.as_ref()?;
        let selected = ui.cal_selected.with_untracked(|s| s.as_ref().map(|e| e.calendar_id.clone()));
        selected
            .filter(|id| v.calendars.iter().any(|c| &c.id == id && c.shared))
            .or_else(|| v.calendars.iter().find(|c| c.outlook_default).map(|c| c.id.clone()))
            .or_else(|| v.calendars.iter().find(|c| c.shown && c.shared).map(|c| c.id.clone()))
            .or_else(|| v.calendars.iter().find(|c| c.shared).map(|c| c.id.clone()))
    })
}

/// Nouveau rendez-vous (Ctrl+N, ruban), le jour affiché.
pub fn open_new(ui: Ui) {
    // Jour affiché (vide : aujourd'hui), à l'heure pleine suivante ; le cœur complète.
    let day = ui.cal_anchor.get_untracked();
    let now = js_sys::Date::new_0();
    let start = (!day.is_empty()).then(|| format!("{day}T{:02}:00", (now.get_hours() + 1).min(23)));
    open_new_at(ui, start, false);
}

/// Nouveau rendez-vous à un créneau (`AAAA-MM-JJ` ou `AAAA-MM-JJTHH:MM`), éventuellement sur la journée entière.
pub fn open_new_at(ui: Ui, start: Option<String>, all_day: bool) {
    let Some(calendar) = target_calendar(ui) else {
        ui.error.set(Some("Aucun calendrier commun avec Outlook : ouvrez Outlook, puis le calendrier de ThunderLook.".into()));
        return;
    };
    let start = start.map(|s| if s.contains('T') { s } else { format!("{s}T09:00") });
    spawn_local(async move {
        match ipc::appointment_form(None, Some(&calendar), start.as_deref()).await {
            Ok(mut f) => {
                f.all_day = all_day;
                ui.appointment.set(Some(f));
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

/// Modifie un champ d'un rendez-vous existant sans ouvrir la boîte (clic droit : Privé, Afficher en tant que, Classer).
pub fn update(ui: Ui, id: String, change: impl FnOnce(&mut AppointmentForm) + 'static) {
    spawn_local(async move {
        let result = match ipc::appointment_form(Some(&id), None, None).await {
            Ok(mut f) => {
                change(&mut f);
                ipc::appointment_save(&f).await
            }
            Err(e) => Err(e),
        };
        match result {
            Ok(m) => {
                ui.cal_selected.set(None);
                ui.status.set(Some(m));
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

/// Ouvre un rendez-vous existant (double clic, « Modifier »).
pub fn open_edit(ui: Ui, id: String) {
    spawn_local(async move {
        match ipc::appointment_form(Some(&id), None, None).await {
            Ok(f) => ui.appointment.set(Some(f)),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

/// Supprime dans Outlook l'occurrence affichée, ou toute la série.
pub fn delete(ui: Ui, id: String, series: bool) {
    spawn_local(async move {
        match ipc::appointment_delete(&id, series).await {
            Ok(m) => {
                ui.cal_selected.set(None);
                ui.status.set(Some(m));
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

#[component]
pub fn AppointmentDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let busy_saving = RwSignal::new(false);
    let note = RwSignal::new(None::<String>);
    let form = move || ui.appointment.get();
    let set = move |f: &dyn Fn(&mut AppointmentForm)| ui.appointment.update(|a| {
        if let Some(a) = a {
            f(a);
        }
    });

    // Seulement à l'ouverture : suivre `appointment` relancerait l'effet à chaque frappe (focus volé).
    let open = Memo::new(move |_| ui.appointment.with(Option::is_some));
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

    let close = move || ui.appointment.set(None);
    let save = move || {
        let Some(f) = ui.appointment.get_untracked() else { return };
        busy_saving.set(true);
        note.set(Some("Enregistrement dans Outlook…".into()));
        spawn_local(async move {
            match ipc::appointment_save(&f).await {
                Ok(m) => {
                    ui.status.set(Some(m));
                    ui.cal_selected.set(None);
                    close();
                }
                Err(e) => note.set(Some(e)),
            }
            busy_saving.set(false);
        });
    };
    // Date et heure séparées (champs du navigateur), recomposées en `AAAA-MM-JJTHH:MM`.
    let date_time = move |label: &'static str, get: fn(&AppointmentForm) -> String, put: fn(&mut AppointmentForm, String)| {
        // Série entière : les dates (premier jour) ne se modifient pas ; une occurrence se déplace librement.
        let disabled = move || form().is_some_and(|f| f.recurring && f.scope != "occurrence");
        let date_disabled = disabled;
        let time_disabled = move || form().is_some_and(|f| f.all_day);
        view! {
            <div class="appt-when">
                <span class="appt-label">{label}</span>
                <input type="date" aria-label=format!("{label} (date)") prop:disabled=date_disabled
                    prop:value=move || form().map(|f| get(&f).split('T').next().unwrap_or_default().to_string()).unwrap_or_default()
                    on:input=move |ev| {
                        let d = event_target_value(&ev);
                        set(&|f| {
                            let t = get(f).split_once('T').map(|(_, t)| t.to_string()).unwrap_or_else(|| "09:00".into());
                            put(f, format!("{d}T{t}"));
                        });
                    } />
                <input type="time" aria-label=format!("{label} (heure)")
                    prop:disabled=time_disabled
                    prop:value=move || form().map(|f| get(&f).split_once('T').map(|(_, t)| t.to_string()).unwrap_or_default()).unwrap_or_default()
                    on:input=move |ev| {
                        let t = event_target_value(&ev);
                        set(&|f| {
                            let d = get(f).split('T').next().unwrap_or_default().to_string();
                            put(f, format!("{d}T{t}"));
                        });
                    } />
            </div>
        }
    };
    let calendars = move || ui.cal_view.with(|v| v.as_ref().map(|v| v.calendars.iter().filter(|c| c.shared).map(|c| (c.id.clone(), c.name.clone())).collect::<Vec<_>>()).unwrap_or_default());

    view! {
        <Show when=move || ui.appointment.with(Option::is_some)>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog wide appt" role="dialog" aria-modal="true" aria-labelledby="appt-title" node_ref=root
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
                    <h2 id="appt-title">{move || if form().is_some_and(|f| f.id.is_some()) { "Rendez-vous" } else { "Nouveau rendez-vous" }}</h2>
                    <label class="field">
                        <span>"Objet"</span>
                        <input type="text" aria-label="Objet" prop:value=move || form().map(|f| f.subject).unwrap_or_default()
                            on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.subject = v.clone()); } />
                    </label>
                    <label class="field">
                        <span>"Emplacement"</span>
                        <input type="text" aria-label="Emplacement" prop:value=move || form().map(|f| f.location).unwrap_or_default()
                            on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.location = v.clone()); } />
                    </label>
                    {date_time("Début", |f| f.start.clone(), |f, v| f.start = v)}
                    {date_time("Fin", |f| f.end.clone(), |f, v| f.end = v)}
                    <label class="check">
                        <input type="checkbox" prop:checked=move || form().is_some_and(|f| f.all_day)
                            prop:disabled=move || form().is_some_and(|f| f.recurring)
                            on:change=move |ev| { let on = event_target_checked(&ev); set(&|f| f.all_day = on); } />
                        <span>"Journée entière"</span>
                    </label>
                    {move || form().is_some_and(|f| f.recurring && f.id.is_some()).then(|| view! {
                        <label class="field">
                            <span>"Appliquer à"</span>
                            <select aria-label="Appliquer à" on:change=move |ev| { let v = event_target_value(&ev); set(&|f| f.scope = v.clone()); }>
                                {move || form().is_some_and(|f| !f.occurrence.is_empty()).then(|| view! {
                                    <option value="occurrence" selected=move || form().is_some_and(|f| f.scope == "occurrence")>"Cette occurrence seulement"</option>
                                })}
                                <option value="series" selected=move || form().is_some_and(|f| f.scope != "occurrence")>"Toute la série"</option>
                            </select>
                        </label>
                    })}
                    {move || form().is_some_and(|f| f.id.is_none() || (f.recurring && f.scope != "occurrence")).then(|| view! {
                        <div class="appt-row">
                            <label class="field">
                                <span>"Répéter"</span>
                                <select aria-label="Répéter" on:change=move |ev| { let v = event_target_value(&ev); set(&|f| f.repeat = v.clone()); }>
                                    <option value="" selected=move || form().is_some_and(|f| f.repeat.is_empty())>{move || if form().is_some_and(|f| f.id.is_some()) { "Inchangée" } else { "Jamais" }}</option>
                                    <option value="daily" selected=move || form().is_some_and(|f| f.repeat == "daily")>"Tous les jours"</option>
                                    <option value="weekly" selected=move || form().is_some_and(|f| f.repeat == "weekly")>"Toutes les semaines"</option>
                                    <option value="monthly" selected=move || form().is_some_and(|f| f.repeat == "monthly")>"Tous les mois"</option>
                                    <option value="yearly" selected=move || form().is_some_and(|f| f.repeat == "yearly")>"Tous les ans"</option>
                                </select>
                            </label>
                            {move || form().is_some_and(|f| !f.repeat.is_empty()).then(|| view! {
                                <label class="field">
                                    <span>"Tous les"</span>
                                    <input type="number" min="1" max="99" aria-label="Intervalle de répétition" prop:value=move || form().map(|f| f.repeat_every).unwrap_or(1)
                                        on:input=move |ev| { let v = event_target_value(&ev).parse().unwrap_or(1); set(&|f| f.repeat_every = v); } />
                                </label>
                                <label class="field">
                                    <span>"Fin"</span>
                                    <select aria-label="Fin de la répétition" on:change=move |ev| { let v = event_target_value(&ev); set(&|f| f.repeat_end = v.clone()); }>
                                        <option value="never" selected=move || form().is_some_and(|f| f.repeat_end != "count" && f.repeat_end != "until")>"Jamais"</option>
                                        <option value="count" selected=move || form().is_some_and(|f| f.repeat_end == "count")>"Après N occurrences"</option>
                                        <option value="until" selected=move || form().is_some_and(|f| f.repeat_end == "until")>"Le…"</option>
                                    </select>
                                </label>
                                {move || form().is_some_and(|f| f.repeat_end == "count").then(|| view! {
                                    <label class="field">
                                        <span>"Occurrences"</span>
                                        <input type="number" min="1" max="999" aria-label="Nombre d'occurrences" prop:value=move || form().map(|f| f.repeat_count).unwrap_or(10)
                                            on:input=move |ev| { let v = event_target_value(&ev).parse().unwrap_or(1); set(&|f| f.repeat_count = v); } />
                                    </label>
                                })}
                                {move || form().is_some_and(|f| f.repeat_end == "until").then(|| view! {
                                    <label class="field">
                                        <span>"Dernier jour"</span>
                                        <input type="date" aria-label="Dernier jour de la répétition" prop:value=move || form().map(|f| f.repeat_until).unwrap_or_default()
                                            on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.repeat_until = v.clone()); } />
                                    </label>
                                })}
                            })}
                        </div>
                    })}
                    <div class="appt-row">
                        <label class="field">
                            <span>"Rappel"</span>
                            <select aria-label="Rappel" on:change=move |ev| {
                                let v = event_target_value(&ev);
                                set(&|f| f.reminder = v.parse().ok());
                            }>
                                <option value="" selected=move || form().is_some_and(|f| f.reminder.is_none())>"Aucun"</option>
                                {REMINDERS.iter().map(|&(m, l)| view! {
                                    <option value=m.to_string() selected=move || form().is_some_and(|f| f.reminder == Some(m))>{l}</option>
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="field">
                            <span>"Afficher comme"</span>
                            <select aria-label="Afficher comme" on:change=move |ev| {
                                let v = event_target_value(&ev);
                                set(&|f| f.busy = v.parse().unwrap_or(2));
                            }>
                                {BUSY.iter().map(|&(b, l)| view! {
                                    <option value=b.to_string() selected=move || form().is_some_and(|f| f.busy == b)>{l}</option>
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="field">
                            <span>"Calendrier"</span>
                            <select aria-label="Calendrier" prop:disabled=move || form().is_some_and(|f| f.id.is_some()) on:change=move |ev| {
                                let v = event_target_value(&ev);
                                set(&|f| f.calendar_id = v.clone());
                            }>
                                {move || calendars().into_iter().map(|(id, name)| {
                                    let sel = id.clone();
                                    view! { <option value=id selected=move || form().is_some_and(|f| f.calendar_id == sel)>{name}</option> }
                                }).collect_view()}
                            </select>
                        </label>
                    </div>
                    <div class="appt-row">
                        <label class="field grow">
                            <span>"Catégories (séparées par des virgules)"</span>
                            <input type="text" aria-label="Catégories" prop:value=move || form().map(|f| f.categories).unwrap_or_default()
                                on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.categories = v.clone()); } />
                        </label>
                        <label class="check">
                            <input type="checkbox" prop:checked=move || form().is_some_and(|f| f.private)
                                on:change=move |ev| { let on = event_target_checked(&ev); set(&|f| f.private = on); } />
                            <span>"Privé"</span>
                        </label>
                    </div>
                    <label class="field">
                        <span>"Description"</span>
                        <textarea class="ar-text" aria-label="Description" rows="6" prop:value=move || form().map(|f| f.body).unwrap_or_default()
                            on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.body = v.clone()); }></textarea>
                    </label>
                    {move || note.get().map(|n| view! { <p class="dialog-note" role="status">{n}</p> })}
                    <div class="dialog-buttons">
                        <button class="btn primary" prop:disabled=move || busy_saving.get() on:click=move |_| save()>"Enregistrer et fermer"</button>
                        <button class="btn" on:click=move |_| close()>"Annuler"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
