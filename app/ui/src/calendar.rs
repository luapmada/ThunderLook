//! Module Calendrier (J9, option A : lecture seule). La vue est calculée par le cœur (`calendar_view`) ; ici, on
//! l'affiche : navigateur de dates et calendriers dans le volet de gauche, période au centre, détail en bas.

use leptos::{html, prelude::*, task::spawn_local};
use wasm_bindgen::JsCast;
use thunderlook_model::calendar::{CalendarDay, CalendarEvent, CalendarQuery, CalendarView, CalendarViewKind, TimedEvent, WEEKDAYS_SHORT};

use crate::{
    icons::*,
    ipc,
    state::{ContextMenuAt, ContextTarget, Module, Ui, use_ui},
};

/// Hauteur d'une heure dans la grille (px), comme la référence en zoom normal.
const HOUR_PX: f64 = 44.0;
/// Heure affichée en haut de la grille à l'ouverture.
const FIRST_HOUR: f64 = 7.5;

/// Recharge la vue quand le module, le type de vue, la date ou les calendriers affichés changent.
pub fn watch(ui: Ui) {
    // Calendrier commun avec Outlook : relu à chaque ouverture du module ; « calendar-changed » recharge la vue.
    ipc::on_calendar_changed(move || ui.cal_epoch.update(|e| *e += 1));
    Effect::new(move |_| {
        if ui.module.get() == Module::Calendar {
            spawn_local(async move {
                let _ = ipc::outlook_sync().await;
            });
        }
    });
    // Toute autre navigation (jour, flèches, Aujourd'hui, type de vue) quitte le défilement de la vue Mois.
    Effect::new(move |_| {
        ui.cal_anchor.track();
        ui.settings.with(|s| s.calendar_view);
        ui.cal_month_start.set(None);
    });
    Effect::new(move |_| {
        if ui.module.get() != Module::Calendar || !ui.settings_loaded.get() {
            return;
        }
        let kind = ui.settings.with(|s| s.calendar_view);
        let anchor = ui.cal_anchor.get();
        let start = ui.cal_month_start.get();
        ui.cal_epoch.track();
        spawn_local(async move {
            match ipc::calendar_view(&CalendarQuery { kind, anchor: anchor.clone(), start: start.clone() }).await {
                Ok(v) => {
                    // Une réponse arrivée après un autre changement est ignorée.
                    if ui.cal_anchor.get_untracked() == anchor && ui.cal_month_start.get_untracked() == start && ui.settings.with_untracked(|s| s.calendar_view) == kind {
                        if ui.cal_anchor.get_untracked().is_empty() {
                            ui.cal_anchor.set(v.anchor.clone());
                        }
                        ui.cal_view.set(Some(v));
                    }
                }
                Err(e) => ui.error.set(Some(e)),
            }
        });
    });
}

pub fn set_kind(ui: Ui, kind: CalendarViewKind) {
    ui.update_settings(|s| s.calendar_view = kind);
}

/// « Ouvrir un calendrier » (.ics, CSV d'Outlook) : ajouté aux calendriers importés, en lecture seule.
pub fn import(ui: Ui) {
    spawn_local(async move {
        match ipc::calendar_import().await {
            Ok(Some(msg)) => {
                ui.status.set(Some(msg));
                ui.cal_epoch.update(|e| *e += 1);
            }
            Ok(None) => {}
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

/// Export des calendriers affichés : `.ics` (tous leurs éléments) ou CSV d'Outlook (période affichée).
pub fn export(ui: Ui, csv: bool) {
    let query = CalendarQuery { kind: ui.settings.with_untracked(|s| s.calendar_view), anchor: ui.cal_anchor.get_untracked(), start: None };
    spawn_local(async move {
        match ipc::calendar_export(if csv { "csv" } else { "ics" }, &query).await {
            Ok(Some(msg)) => ui.status.set(Some(msg)),
            Ok(None) => {}
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

pub fn go_today(ui: Ui) {
    let today = ui.cal_view.with_untracked(|v| v.as_ref().map(|v| v.today.clone())).unwrap_or_default();
    ui.cal_anchor.set(today);
}

fn go(ui: Ui, next: bool) {
    if let Some(d) = ui.cal_view.with_untracked(|v| v.as_ref().map(|v| if next { v.next.clone() } else { v.prev.clone() })) {
        ui.cal_anchor.set(d);
    }
}

thread_local! {
    /// Défilement accumulé (pixels) : un pavé tactile envoie beaucoup de petits pas, une roulette des crans de ~100.
    static WHEEL: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// Un cran de roulette (+1 vers le bas, -1 vers le haut) quand le défilement accumulé atteint 50 pixels.
fn wheel_step(ev: &web_sys::WheelEvent) -> Option<i32> {
    let dy = ev.delta_y() * if ev.delta_mode() == 1 { 40.0 } else { 1.0 };
    WHEEL.with(|w| {
        let acc = w.get() + dy;
        if acc.abs() >= 50.0 {
            w.set(0.0);
            Some(if acc > 0.0 { 1 } else { -1 })
        } else {
            w.set(acc);
            None
        }
    })
}

/// `AAAA-MM-JJ` décalé de `days` jours.
fn shift_days(date: &str, days: i32) -> String {
    let n = |r: std::ops::Range<usize>| date.get(r).and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0);
    let ms = js_sys::Date::utc(n(0..4), n(5..7) - 1.0) + (n(8..10) - 1.0 + f64::from(days)) * 86_400_000.0;
    let d = js_sys::Date::new(&ms.into());
    format!("{:04}-{:02}-{:02}", d.get_utc_full_year(), d.get_utc_month() + 1, d.get_utc_date())
}

/// Premier jour du mois précédent ou suivant celui d'une date `AAAA-MM-JJ`.
fn shift_month(date: &str, delta: i32) -> String {
    let y: i32 = date.get(0..4).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let m: i32 = date.get(5..7).and_then(|s| s.parse().ok()).unwrap_or(1);
    let k = y * 12 + (m - 1) + delta;
    format!("{:04}-{:02}-01", k.div_euclid(12), k.rem_euclid(12) + 1)
}

/// Raccourcis du module : Ctrl+Alt+1 à 4 (types de vue), Alt+Origine (aujourd'hui).
pub fn key(ui: Ui, ev: &web_sys::KeyboardEvent) -> bool {
    let (ctrl, alt, shift) = (ev.ctrl_key(), ev.alt_key(), ev.shift_key());
    let kind = match (ctrl, alt, shift, ev.key().as_str()) {
        (true, true, false, "1") => Some(CalendarViewKind::Day),
        (true, true, false, "2") => Some(CalendarViewKind::WorkWeek),
        (true, true, false, "3") => Some(CalendarViewKind::Week),
        (true, true, false, "4") => Some(CalendarViewKind::Month),
        _ => None,
    };
    if let Some(k) = kind {
        set_kind(ui, k);
        return true;
    }
    if ctrl && !alt && !shift && ev.key().eq_ignore_ascii_case("n") {
        crate::appointment::open_new(ui);
        return true;
    }
    false
}

#[component]
fn Chip(event: CalendarEvent, #[prop(default = false)] month: bool) -> impl IntoView {
    let ui = use_ui();
    let id = event.id.clone();
    let selected = {
        let id = id.clone();
        move || ui.cal_selected.with(|s| s.as_ref().map(|e| &e.id) == Some(&id))
    };
    let label = format!("{}, {}{}", event.subject, event.when, event.location.as_ref().map(|l| format!(", {l}")).unwrap_or_default());
    let ev = event.clone();
    let ctx_ev = event.clone();
    let open_id = event.id.clone();
    let shared = ui.cal_view.with_untracked(|v| v.as_ref().is_some_and(|v| v.calendars.iter().any(|c| c.id == event.calendar_id && c.shared)));
    // Comme Outlook : un rendez-vous classé prend la couleur de sa première catégorie.
    let first_category = event.categories.first().cloned();
    let category_color = move || {
        let name = first_category.as_ref()?.to_lowercase();
        ui.categories.with(|cs| cs.iter().find(|c| c.name.to_lowercase() == name).and_then(|c| c.color.clone()))
    };
    let cat_style = category_color.clone();
    view! {
        <button
            class=format!("cal-ev cal-c{} busy-{}", event.color, event.busy)
            class:cal-cat=move || category_color().is_some()
            style=move || cat_style().map(|c| format!("--c: {c}")).unwrap_or_default()
            class:selected=selected
            class:continued=event.continued
            aria-label=label.clone()
            title=label
            on:click=move |e| {
                e.stop_propagation();
                ui.cal_selected.set(Some(ev.clone()));
            }
            on:contextmenu=move |e: leptos::ev::MouseEvent| {
                e.prevent_default();
                e.stop_propagation();
                ui.cal_selected.set(Some(ctx_ev.clone()));
                ui.context_menu.set(Some(ContextMenuAt { x: f64::from(e.client_x()), y: f64::from(e.client_y()), target: ContextTarget::CalEvent(ctx_ev.id.clone()) }));
            }
            // Double clic : ouvrir le rendez-vous (calendrier commun avec Outlook).
            on:dblclick=move |e| {
                e.stop_propagation();
                if shared {
                    crate::appointment::open_edit(ui, open_id.clone());
                }
            }
        >
            {(month && !event.start_label.is_empty()).then(|| view! { <span class="cal-time">{event.start_label.clone()}</span> })}
            <span class="cal-subject">{event.subject.clone()}</span>
            {(!month).then(|| event.location.clone().map(|l| view! { <span class="cal-loc">{l}</span> }))}
            {event.recurring.then(|| view! { <span class="cal-flag" title="Périodique"><Svg icon=REFRESH size=11 /></span> })}
        </button>
    }
}

#[component]
fn Timed(item: TimedEvent) -> impl IntoView {
    let top = f64::from(item.start_min) * HOUR_PX / 60.0;
    let height = (f64::from(item.end_min.saturating_sub(item.start_min)) * HOUR_PX / 60.0).max(18.0);
    let width = 100.0 / f64::from(item.columns.max(1));
    let style = format!("top:{top}px;height:{height}px;left:calc({}% + 1px);width:calc({width}% - 3px)", f64::from(item.column) * width);
    view! {
        <div class="cal-timed" style=style>
            <Chip event=item.event />
        </div>
    }
}

/// Clic droit sur un créneau vide : menu « Nouveau… » à la date (et à l'heure, dans la grille horaire) cliquée.
fn slot_menu(ui: Ui, e: &leptos::ev::MouseEvent, date: &str) {
    e.prevent_default();
    e.stop_propagation();
    let mut start = date.to_string();
    if let Some(el) = e.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
        let h = ((f64::from(e.client_y()) - el.get_bounding_client_rect().top()) / HOUR_PX).floor();
        if (0.0..24.0).contains(&h) && el.class_list().contains("cal-col") {
            start = format!("{date}T{:02}:00", h as u32);
        }
    }
    ui.context_menu.set(Some(ContextMenuAt { x: f64::from(e.client_x()), y: f64::from(e.client_y()), target: ContextTarget::CalSlot(start) }));
}

#[component]
fn TimeGrid(view: CalendarView) -> impl IntoView {
    let scroller = NodeRef::<html::Div>::new();
    let n = view.days.len();
    let cols = format!("grid-template-columns: 56px repeat({n}, minmax(0, 1fr))");
    // Ouverture vers 7 h 30, comme la référence.
    Effect::new(move |_| {
        if let Some(el) = scroller.get() {
            el.set_scroll_top((FIRST_HOUR * HOUR_PX) as i32);
        }
    });
    let ui = use_ui();
    let now = view.now_min;
    let days = view.days.clone();
    view! {
        <div class="cal-grid">
            <div class="cal-dayheads" style=cols.clone()>
                <div></div>
                {days
                    .iter()
                    .map(|d| view! { <div class="cal-dayhead" class:today=d.today>{d.label.clone()}</div> })
                    .collect_view()}
            </div>
            <div class="cal-allday" style=cols.clone()>
                <div class="cal-allday-label">"Journée"</div>
                {days
                    .iter()
                    .map(|d| {
                        view! {
                            <div class="cal-allday-cell" class:today=d.today>
                                {d.all_day.iter().cloned().map(|e| view! { <Chip event=e /> }).collect_view()}
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
            <div class="cal-scroll" node_ref=scroller>
                <div class="cal-body" style=format!("{cols};height:{}px", 24.0 * HOUR_PX)>
                    <div class="cal-hours">
                        {(0..24).map(|h| view! { <div class="cal-hour" style=format!("top:{}px", f64::from(h) * HOUR_PX)>{format!("{h:02}:00")}</div> }).collect_view()}
                    </div>
                    {days
                        .into_iter()
                        .map(|d: CalendarDay| {
                            let date = d.date.clone();
                            view! {
                                <div class="cal-col" class:today=d.today role="group" aria-label=d.label.clone() on:contextmenu=move |e: leptos::ev::MouseEvent| slot_menu(ui, &e, &date)>
                                    {d.timed.into_iter().map(|t| view! { <Timed item=t /> }).collect_view()}
                                    {(d.today).then(|| now.map(|m| view! { <div class="cal-now" style=format!("top:{}px", f64::from(m) * HOUR_PX / 60.0)></div> }))}
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
            </div>
        </div>
    }
}

#[component]
fn MonthGrid(view: CalendarView) -> impl IntoView {
    let ui = use_ui();
    const SHOWN: usize = 3;
    // Roulette : une semaine plus tôt ou plus tard, comme la vue Mois d'Outlook.
    let first = view.days.first().map(|d| d.date.clone()).unwrap_or_default();
    let on_wheel = move |ev: leptos::ev::WheelEvent| {
        if let Some(step) = wheel_step(&ev) {
            ev.prevent_default();
            ui.cal_month_start.set(Some(shift_days(&first, 7 * step)));
        }
    };
    view! {
        <div class="cal-month" on:wheel=on_wheel>
            <div class="cal-month-head">{WEEKDAYS_SHORT.iter().map(|w| view! { <div>{*w}</div> }).collect_view()}</div>
            <div class="cal-month-body">
                {view
                    .days
                    .into_iter()
                    .map(|d| {
                        let date = d.date.clone();
                        let mut items: Vec<CalendarEvent> = d.all_day.clone();
                        items.extend(d.timed.iter().map(|t| t.event.clone()));
                        let extra = items.len().saturating_sub(SHOWN);
                        let cell_date = d.date.clone();
                        view! {
                            <div class="cal-cell" class:today=d.today class:other=!d.in_month role="group" aria-label=d.date.clone() on:contextmenu=move |e: leptos::ev::MouseEvent| slot_menu(ui, &e, &cell_date)>
                                <button
                                    class="cal-cell-day"
                                    title="Afficher ce jour"
                                    on:click=move |_| {
                                        ui.cal_anchor.set(date.clone());
                                        set_kind(ui, CalendarViewKind::Day);
                                    }
                                >
                                    {d.label.clone()}
                                </button>
                                {items.into_iter().take(SHOWN).map(|e| view! { <Chip event=e month=true /> }).collect_view()}
                                {(extra > 0).then(|| view! { <span class="cal-more">{format!("+ {extra} autre{}", if extra > 1 { "s" } else { "" })}</span> })}
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}

#[component]
fn Details() -> impl IntoView {
    let ui = use_ui();
    let shared = move |id: &str| ui.cal_view.with(|v| v.as_ref().is_some_and(|v| v.calendars.iter().any(|c| c.id == id && c.shared)));
    let calendar_name = move |id: &str| {
        ui.cal_view.with(|v| v.as_ref().and_then(|v| v.calendars.iter().find(|c| c.id == id).map(|c| c.name.clone())).unwrap_or_default())
    };
    move || {
        ui.cal_selected.get().map(|e| {
            let busy = match e.busy.as_str() {
                "free" => "Disponible",
                "tentative" => "Provisoire",
                "oof" => "Absent(e) du bureau",
                "elsewhere" => "Travail ailleurs",
                _ => "Occupé(e)",
            };
            view! {
                <section class="cal-details" aria-label="Détail du rendez-vous">
                    <div class="cal-details-head">
                        <span class=format!("cal-swatch cal-c{}", e.color)></span>
                        <h3>{e.subject.clone()}</h3>
                        <button class="icon-btn" aria-label="Fermer le détail" title="Fermer (Échap)" on:click=move |_| ui.cal_selected.set(None)>
                            <Svg icon=X />
                        </button>
                    </div>
                    <dl>
                        <dt>"Quand"</dt>
                        <dd>{e.when.clone()}{e.recurring.then_some(if e.exception { " — occurrence modifiée d'une série" } else { " — série périodique" })}</dd>
                        {e.location.clone().map(|l| view! { <dt>"Lieu"</dt><dd>{l}</dd> })}
                        <dt>"Calendrier"</dt>
                        <dd>{calendar_name(&e.calendar_id)}</dd>
                        <dt>"Afficher comme"</dt>
                        <dd>{busy}{e.private.then_some(" — privé")}</dd>
                        {e.reminder_minutes.map(|m| view! { <dt>"Rappel"</dt><dd>{reminder(m)}</dd> })}
                        {(!e.categories.is_empty()).then(|| view! { <dt>"Catégories"</dt><dd>{e.categories.join(", ")}</dd> })}
                        {e.is_meeting.then(|| view! {
                            <dt>"Organisateur"</dt><dd>{e.organizer.clone()}</dd>
                            <dt>"Participants"</dt><dd>{e.attendees.clone()}</dd>
                        })}
                    </dl>
                    {shared(&e.calendar_id).then(|| {
                        let (edit_id, del_id, series_id) = (e.id.clone(), e.id.clone(), e.id.clone());
                        let confirm = RwSignal::new(false);
                        let recurring = e.recurring;
                        view! {
                            <div class="cal-details-actions">
                                <button class="btn" on:click=move |_| crate::appointment::open_edit(ui, edit_id.clone())>"Ouvrir"</button>
                                <Show when=move || !confirm.get() fallback=move || {
                                    let (del_id, series_id) = (del_id.clone(), series_id.clone());
                                    view! {
                                        <span class="confirm-text">{if recurring { "Supprimer dans Outlook :" } else { "Supprimer dans Outlook ?" }}</span>
                                        <button class="btn danger" on:click=move |_| crate::appointment::delete(ui, del_id.clone(), false)>
                                            {if recurring { "Cette occurrence" } else { "Supprimer" }}
                                        </button>
                                        {recurring.then(|| {
                                            let series_id = series_id.clone();
                                            view! { <button class="btn danger" on:click=move |_| crate::appointment::delete(ui, series_id.clone(), true)>"Toute la série"</button> }
                                        })}
                                        <button class="btn" on:click=move |_| confirm.set(false)>"Annuler"</button>
                                    }
                                }>
                                    <button class="btn" on:click=move |_| confirm.set(true)>"Supprimer…"</button>
                                </Show>
                            </div>
                        }
                    })}
                </section>
            }
        })
    }
}

fn reminder(m: i64) -> String {
    match m {
        0 => "à l'heure du début".into(),
        m if m % 10_080 == 0 => format!("{} semaine(s) avant", m / 10_080),
        m if m % 1_440 == 0 => format!("{} jour(s) avant", m / 1_440),
        m if m % 60 == 0 => format!("{} heure(s) avant", m / 60),
        m => format!("{m} minutes avant"),
    }
}

/// Partie centrale du module.
#[component]
pub fn CalendarPane() -> impl IntoView {
    let ui = use_ui();
    let kind = move || ui.settings.with(|s| s.calendar_view);
    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        match ev.key().as_str() {
            "PageUp" => go(ui, false),
            "PageDown" => go(ui, true),
            "Escape" if ui.cal_selected.get_untracked().is_some() => ui.cal_selected.set(None),
            _ => return,
        }
        ev.prevent_default();
    };
    view! {
        <section class="pane calendar" aria-label="Calendrier" tabindex="0" on:keydown=on_key>
            <header class="cal-head">
                <button class="btn" on:click=move |_| go_today(ui)>"Aujourd'hui"</button>
                <button class="icon-btn" aria-label="Période précédente" title="Période précédente (Pg.Préc)" on:click=move |_| go(ui, false)>
                    <Svg icon=CHEVRON_LEFT />
                </button>
                <button class="icon-btn" aria-label="Période suivante" title="Période suivante (Pg.Suiv)" on:click=move |_| go(ui, true)>
                    <Svg icon=CHEVRON_RIGHT />
                </button>
                <h2 class="cal-title" aria-live="polite">{move || ui.cal_view.with(|v| v.as_ref().map(|v| v.title.clone()).unwrap_or_default())}</h2>
                <div class="cal-kinds" role="radiogroup" aria-label="Affichage">
                    {CalendarViewKind::ALL
                        .into_iter()
                        .map(|k| {
                            view! {
                                <button
                                    role="radio"
                                    class="cal-kind"
                                    aria-checked=move || (kind() == k).to_string()
                                    title=format!("{} ({})", k.label(), k.shortcut())
                                    aria-keyshortcuts=k.shortcut().replace("Ctrl", "Control")
                                    on:click=move |_| set_kind(ui, k)
                                >
                                    {k.label()}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
            </header>
            {move || ui.cal_view.with(|v| v.as_ref().and_then(|v| v.source_note.clone())).map(|n| view! { <p class="cal-note">{n}</p> })}
            <div class="cal-main" on:click=move |_| ui.cal_selected.set(None)>
                {move || match ui.cal_view.get() {
                    None => view! { <div class="placeholder">"Chargement du calendrier…"</div> }.into_any(),
                    Some(v) if v.kind == CalendarViewKind::Month => view! { <MonthGrid view=v /> }.into_any(),
                    Some(v) => view! { <TimeGrid view=v /> }.into_any(),
                }}
            </div>
            <Details />
        </section>
    }
}

/// Volet de gauche du module : navigateur de dates et liste des calendriers.
#[component]
pub fn CalendarNav() -> impl IntoView {
    let ui = use_ui();
    let toggle = move |id: String, shown: bool| {
        spawn_local(async move {
            match ipc::set_calendar_shown(&id, shown).await {
                Ok(()) => ui.cal_epoch.update(|e| *e += 1),
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };
    view! {
        <div class="cal-nav">
            {move || {
                ui.cal_view.get().map(|v| {
                    let months = v.navigator.clone();
                    let first = months.first().and_then(|m| m.weeks.iter().flatten().find(|d| d.in_month).map(|d| d.date.clone())).unwrap_or_default();
                    let (prev, next) = (shift_month(&first, -1), shift_month(&first, 1));
                    let (wheel_prev, wheel_next) = (prev.clone(), next.clone());
                    // Roulette sur le navigateur de dates : mois précédent ou suivant.
                    let on_wheel = move |ev: leptos::ev::WheelEvent| {
                        if let Some(step) = wheel_step(&ev) {
                            ev.prevent_default();
                            ui.cal_anchor.set(if step > 0 { wheel_next.clone() } else { wheel_prev.clone() });
                        }
                    };
                    view! {
                        <div class="nav-months" on:wheel=on_wheel>
                            {months
                                .into_iter()
                                .enumerate()
                                .map(|(i, m)| {
                                    let (prev, next) = (prev.clone(), next.clone());
                                    view! {
                                        <div class="nav-head">
                                                {(i == 0).then(|| view! {
                                                    <button class="icon-btn small" aria-label="Mois précédent" on:click=move |_| ui.cal_anchor.set(prev.clone())>
                                                        <Svg icon=CHEVRON_LEFT size=14 />
                                                    </button>
                                                })}
                                                <span class="nav-title">{m.title.clone()}</span>
                                                {(i == 0).then(|| view! {
                                                    <button class="icon-btn small" aria-label="Mois suivant" on:click=move |_| ui.cal_anchor.set(next.clone())>
                                                        <Svg icon=CHEVRON_RIGHT size=14 />
                                                    </button>
                                                })}
                                        </div>
                                        <table class="nav-month" aria-label=m.title.clone()>
                                            <thead>
                                                <tr>{["L", "M", "M", "J", "V", "S", "D"].iter().map(|d| view! { <th scope="col">{*d}</th> }).collect_view()}</tr>
                                            </thead>
                                            <tbody>
                                                {m.weeks
                                                    .into_iter()
                                                    .map(|w| {
                                                        view! {
                                                            <tr>
                                                                {w.into_iter()
                                                                    .map(|d| {
                                                                        let date = d.date.clone();
                                                                        view! {
                                                                            <td>
                                                                                <button
                                                                                    class="nav-day"
                                                                                    class:other=!d.in_month
                                                                                    class:today=d.today
                                                                                    class:selected=d.selected
                                                                                    class:busy=d.busy
                                                                                    aria-label=d.date.clone()
                                                                                    on:click=move |_| ui.cal_anchor.set(date.clone())
                                                                                >
                                                                                    {d.day}
                                                                                </button>
                                                                            </td>
                                                                        }
                                                                    })
                                                                    .collect_view()}
                                                            </tr>
                                                        }
                                                    })
                                                    .collect_view()}
                                            </tbody>
                                        </table>
                                    }
                                })
                                .collect_view()}
                        </div>
                        <div class="cal-list" role="group" aria-label="Mes calendriers">
                            {let mut section = String::new();
                            v.calendars
                                .into_iter()
                                .map(|c| {
                                    let head = (c.section != section).then(|| {
                                        section = c.section.clone();
                                        view! { <div class="cal-section">{c.section.clone()}</div> }
                                    });
                                    let (id, shown) = (c.id.clone(), c.shown);
                                    let imported = c.id.starts_with("c-");
                                    let (remove_id, remove_name) = (c.id.clone(), c.name.clone());
                                    view! {
                                        {head}
                                        <label class="cal-item">
                                            <input type="checkbox" prop:checked=shown on:change=move |_| toggle(id.clone(), !shown) />
                                            <span class=format!("cal-swatch cal-c{}", c.color)></span>
                                            <span class="name">{c.name.clone()}</span>
                                            <span class="count">{c.count}</span>
                                            {imported.then(|| view! {
                                                <button
                                                    class="icon-btn small"
                                                    aria-label=format!("Retirer le calendrier importé « {remove_name} »")
                                                    title="Retirer ce calendrier importé"
                                                    on:click=move |ev| {
                                                        ev.prevent_default();
                                                        let id = remove_id.clone();
                                                        spawn_local(async move {
                                                            match ipc::calendar_remove(&id).await {
                                                                Ok(()) => ui.cal_epoch.update(|e| *e += 1),
                                                                Err(e) => ui.error.set(Some(e)),
                                                            }
                                                        });
                                                    }
                                                >
                                                    <Svg icon=X size=12 />
                                                </button>
                                            })}
                                        </label>
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                })
            }}
        </div>
    }
}
