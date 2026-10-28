//! Module Calendrier (J9, option A : lecture seule). La vue est calculée par le cœur (`calendar_view`) ; ici, on
//! l'affiche : navigateur de dates et calendriers dans le volet de gauche, période au centre, détail en bas.
//! Lot AZ : « 7 prochains jours », jours fériés (identifiants `h<date>`, rien à ouvrir), fuseaux horaires.

use leptos::{html, prelude::*, task::spawn_local};
use serde::Serialize;
use wasm_bindgen::{JsCast, prelude::*};
use thunderlook_model::calendar::{CalendarDay, CalendarEvent, CalendarQuery, CalendarView, CalendarViewKind, TimedEvent, WEEKDAYS_SHORT};

use crate::{
    icons::*,
    ipc,
    item_folders::{FolderHost, FolderKind, FolderUi},
    state::{ContextMenuAt, ContextTarget, Module, Ui, use_ui},
};

/// Hauteur d'une heure dans la grille (px), comme la référence en zoom normal.
const HOUR_PX: f64 = 44.0;
/// Largeur de la colonne des heures (px), comme `grid-template-columns` de la grille.
const HOURS_W: f64 = 56.0;
/// Déplacement minimal du pointeur (px) avant qu'un appui devienne un glissement.
const DRAG_PX: f64 = 4.0;
/// Type de données du glisser-déposer d'un rendez-vous entre deux jours de la vue Mois : `identifiant|jour d'origine`.
const MONTH_DRAG: &str = "application/x-thunderlook-event";

/// Recharge la vue quand le module, le type de vue, la date ou les calendriers affichés changent.
pub fn watch(ui: Ui) {
    // Calendrier commun avec Outlook : relu à chaque ouverture du module ; « calendar-changed » recharge la vue.
    ipc::on_calendar_changed(move || ui.cal_epoch.update(|e| *e += 1));
    // Type de vue seul : suivre tous les réglages relançait la lecture (et quittait le défilement du mois) à chaque
    // réglage modifié ailleurs (largeur de colonne, densité…).
    let view_kind = Memo::new(move |_| ui.settings.with(|s| s.calendar_view));
    // Toute autre navigation (jour, flèches, Aujourd'hui, type de vue) quitte le défilement de la vue Mois.
    Effect::new(move |_| {
        ui.cal_anchor.track();
        view_kind.track();
        ui.cal_month_start.set(None);
    });
    // Dernière lecture demandée : deux rechargements de mêmes paramètres (enregistrement, puis « calendar-changed »)
    // peuvent répondre dans le désordre, et l'ancien ne doit pas remplacer le nouveau.
    let seq = StoredValue::new(0u32);
    Effect::new(move |_| {
        if ui.module.get() != Module::Calendar || !ui.settings_loaded.get() {
            return;
        }
        let kind = view_kind.get();
        let anchor = ui.cal_anchor.get();
        let start = ui.cal_month_start.get();
        ui.cal_epoch.track();
        let me = seq.get_value().wrapping_add(1);
        seq.set_value(me);
        spawn_local(async move {
            match ipc::calendar_view(&CalendarQuery { kind, anchor: anchor.clone(), start: start.clone() }).await {
                Ok(v) => {
                    // Une réponse arrivée après un autre changement est ignorée.
                    if seq.try_get_value() == Some(me) && ui.cal_anchor.get_untracked() == anchor && ui.cal_month_start.get_untracked() == start && ui.settings.with_untracked(|s| s.calendar_view) == kind {
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
    // « 7 prochains jours » part d'aujourd'hui, comme Outlook.
    if kind == CalendarViewKind::NextSevenDays {
        go_today(ui);
    }
    ui.update_settings(|s| s.calendar_view = kind);
}

/// Jour férié ajouté par le cœur (`h<AAAA-MM-JJ>`) : ni ouverture, ni menu, ni suppression.
fn is_holiday(id: &str) -> bool {
    id.starts_with('h')
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ZoneItem {
    id: String,
    label: String,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ZoneSettings {
    zones: Vec<ZoneItem>,
    primary: String,
    second: Option<String>,
}

#[derive(Serialize)]
struct NoArgs {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ZonesArg {
    primary: String,
    second: Option<String>,
}

/// Bouton « Fuseaux horaires » de l'en-tête : fuseau de l'utilisateur et second fuseau affiché à côté des heures.
#[component]
fn ZonePicker() -> impl IntoView {
    let ui = use_ui();
    let open = RwSignal::new(false);
    let data = RwSignal::new(None::<ZoneSettings>);
    let load = move || {
        spawn_local(async move {
            match ipc::call::<_, ZoneSettings>("time_zones", &NoArgs {}).await {
                Ok(z) => data.set(Some(z)),
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };
    let save = move |primary: String, second: Option<String>| {
        spawn_local(async move {
            match ipc::call::<_, ()>("set_time_zones", &ZonesArg { primary, second }).await {
                Ok(()) => {
                    ui.cal_epoch.update(|e| *e += 1);
                    load();
                }
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };
    view! {
        <div style="position:relative">
            <button
                class="btn"
                aria-expanded=move || open.get().to_string()
                title="Fuseau horaire et second fuseau"
                on:click=move |_| {
                    if !open.get_untracked() {
                        load();
                    }
                    open.update(|o| *o = !*o);
                }
            >
                "Fuseaux horaires"
            </button>
            {move || open.get().then(|| data.get().map(|z| {
                let options = |selected: Option<String>| {
                    z.zones
                        .iter()
                        .map(|o| view! { <option value=o.id.clone() selected=selected.as_deref() == Some(o.id.as_str())>{o.label.clone()}</option> })
                        .collect_view()
                };
                let (primary, second) = (z.primary.clone(), z.second.clone());
                let (p2, s2) = (primary.clone(), second.clone());
                view! {
                    <div
                        class="cal-details"
                        role="dialog"
                        aria-label="Fuseaux horaires"
                        style="position:absolute;right:0;top:100%;z-index:20;min-width:22rem;display:grid;gap:0.5rem"
                    >
                        <label>
                            "Fuseau horaire"
                            <select on:change=move |e| save(event_target_value(&e), s2.clone())>{options(Some(primary.clone()))}</select>
                        </label>
                        <label>
                            "Second fuseau (à côté des heures)"
                            <select on:change=move |e| {
                                let v = event_target_value(&e);
                                save(p2.clone(), (!v.is_empty()).then_some(v));
                            }>
                                <option value="" selected=second.is_none()>"Aucun"</option>
                                {options(second.clone())}
                            </select>
                        </label>
                        <p class="cal-note">"Les fichiers échangés (.ics, téléphone, CSV) restent en heure de Paris."</p>
                        <button class="btn" on:click=move |_| open.set(false)>"Fermer"</button>
                    </div>
                }
            }))}
        </div>
    }
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

/// Nombre de jours de `from` à `to` (`AAAA-MM-JJ`).
fn day_diff(from: &str, to: &str) -> i64 {
    let n = |d: &str| {
        let f = |r: std::ops::Range<usize>| d.get(r).and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0);
        js_sys::Date::utc(f(0..4), f(5..7) - 1.0) + (f(8..10) - 1.0) * 86_400_000.0
    };
    ((n(to) - n(from)) / 86_400_000.0).round() as i64
}

/// Premier jour du mois précédent ou suivant celui d'une date `AAAA-MM-JJ`.
fn shift_month(date: &str, delta: i32) -> String {
    let y: i32 = date.get(0..4).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let m: i32 = date.get(5..7).and_then(|s| s.parse().ok()).unwrap_or(1);
    let k = y * 12 + (m - 1) + delta;
    format!("{:04}-{:02}-01", k.div_euclid(12), k.rem_euclid(12) + 1)
}

/// Une boîte modale est ouverte (rendez-vous, tâche, contact…) : elle garde ses touches.
fn modal_open() -> bool {
    document().query_selector("[aria-modal='true']").ok().flatten().is_some()
}

/// Raccourcis du module : Ctrl+Alt+1 à 4 (types de vue), Alt+Origine (aujourd'hui).
pub fn key(ui: Ui, ev: &web_sys::KeyboardEvent) -> bool {
    // Pendant la saisie dans une boîte, Ctrl+N remplaçait le rendez-vous en cours par un nouveau (saisie perdue) et
    // Ctrl+P imprimait le calendrier caché derrière ; Ctrl+Alt (AltGr du pavé numérique) changeait de vue.
    if modal_open() {
        return false;
    }
    let (ctrl, alt, shift) = (ev.ctrl_key(), ev.alt_key(), ev.shift_key());
    // Touche physique (`code`) : sur AZERTY, Ctrl+Alt+& ne donne pas « 1 » (voir `app::shortcut_digit`).
    let kind = match (ctrl, alt, shift, crate::app::shortcut_digit(&ev.code(), &ev.key())) {
        (true, true, false, Some(1)) => Some(CalendarViewKind::Day),
        (true, true, false, Some(2)) => Some(CalendarViewKind::WorkWeek),
        (true, true, false, Some(3)) => Some(CalendarViewKind::Week),
        (true, true, false, Some(4)) => Some(CalendarViewKind::Month),
        _ => None,
    };
    if let Some(k) = kind {
        set_kind(ui, k);
        return true;
    }
    if ctrl && !alt && !shift && ev.key().eq_ignore_ascii_case("p") {
        crate::print::run(ui, "page");
        return true;
    }
    if ctrl && !alt && !shift && ev.key().eq_ignore_ascii_case("n") {
        crate::appointment::open_new(ui);
        return true;
    }
    false
}

#[component]
fn Chip(event: CalendarEvent, #[prop(default = false)] month: bool, #[prop(default = String::new())] date: String) -> impl IntoView {
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
    // Vue Mois : glisser un rendez-vous d'un jour à l'autre (comme Outlook) ; l'heure ne change pas.
    let draggable = (month && shared && !event.continued).then_some("true");
    let drag_id = event.id.clone();
    view! {
        <button
            draggable=draggable
            on:dragstart=move |e: web_sys::DragEvent| {
                if let Some(dt) = e.data_transfer() {
                    let _ = dt.set_data(MONTH_DRAG, &format!("{drag_id}|{date}"));
                    dt.set_effect_allowed("move");
                }
            }
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
                if is_holiday(&ctx_ev.id) {
                    return;
                }
                ui.context_menu.set(Some(ContextMenuAt { x: f64::from(e.client_x()), y: f64::from(e.client_y()), target: ContextTarget::CalEvent(ctx_ev.id.clone()) }));
            }
            // Double clic : ouvrir le rendez-vous (calendrier commun avec Outlook).
            on:dblclick=move |e| {
                e.stop_propagation();
                if shared && !is_holiday(&open_id) {
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

/// Geste à la souris dans la grille horaire : tirer dans une colonne vide (créer), glisser un rendez-vous (déplacer) ou
/// sa poignée du bas (redimensionner).
#[derive(Clone)]
enum Gesture {
    Create,
    Move { id: String, subject: String, start: i64, end: i64 },
    Resize { id: String, subject: String, start: i64, end: i64 },
}

#[derive(Clone)]
struct Drag {
    gesture: Gesture,
    /// Le pointeur a assez bougé : sinon c'est un simple clic.
    active: bool,
    x0: f64,
    y0: f64,
    col0: usize,
    /// Minute du jour sous le pointeur à l'appui, puis maintenant.
    m0: f64,
    col: usize,
    m: f64,
}

/// Ce que donnerait le geste s'il s'arrêtait ici : colonne, début et fin (minutes dans la journée), décalages à écrire.
struct Outcome {
    col: usize,
    start: i64,
    end: i64,
    shift_start: i64,
    shift_end: i64,
}

fn snap(m: f64, step: i64) -> i64 {
    (m / step as f64).round() as i64 * step
}

/// `step` : pas de la grille en minutes (réglage « Calendrier » des Options).
fn outcome(d: &Drag, step: i64) -> Outcome {
    match &d.gesture {
        Gesture::Create => {
            let floor = |m: f64| (m.min(1439.0) / step as f64).floor() as i64 * step;
            let (a, b) = (floor(d.m0), floor(d.m));
            Outcome { col: d.col0, start: a.min(b), end: (a.max(b) + step).min(1440), shift_start: 0, shift_end: 0 }
        }
        Gesture::Move { start, end, .. } => {
            let dur = end - start;
            let new_start = snap(*start as f64 + d.m - d.m0, step).clamp(0, (1440 - dur).max(0));
            let shift = (d.col as i64 - d.col0 as i64) * 1440 + new_start - start;
            Outcome { col: d.col, start: new_start, end: new_start + dur, shift_start: shift, shift_end: shift }
        }
        Gesture::Resize { start, end, .. } => {
            let new_end = snap(d.m, step).clamp(start + step, 1440);
            Outcome { col: d.col0, start: *start, end: new_end, shift_start: 0, shift_end: new_end - end }
        }
    }
}

/// Colonne et minute du jour sous le pointeur, dans le corps de la grille (défilement compris).
fn locate(body: &web_sys::Element, columns: usize, x: f64, y: f64) -> (usize, f64) {
    let r = body.get_bounding_client_rect();
    let w = ((r.width() - HOURS_W) / columns as f64).max(1.0);
    let col = (((x - r.left() - HOURS_W) / w).floor().max(0.0) as usize).min(columns.saturating_sub(1));
    (col, ((y - r.top()) / HOUR_PX * 60.0).clamp(0.0, 1440.0))
}

fn clock(min: i64) -> String {
    format!("{:02}:{:02}", min / 60, min % 60)
}

/// `AAAA-MM-JJTHH:MM` ; 24:00 devient minuit du lendemain.
fn stamp(date: &str, min: i64) -> String {
    if min >= 1440 { format!("{}T00:00", shift_days(date, 1)) } else { format!("{date}T{}", clock(min)) }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShiftArg<'a> {
    id: &'a str,
    start_shift_min: i64,
    end_shift_min: i64,
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch, js_name = invoke)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

/// Déplace ou redimensionne (`calendar_cmds::appointment_shift`) ; la vue se recharge sur « calendar-changed ».
fn shift(ui: Ui, id: String, start_shift_min: i64, end_shift_min: i64) {
    spawn_local(async move {
        let args = match serde_wasm_bindgen::to_value(&ShiftArg { id: &id, start_shift_min, end_shift_min }) {
            Ok(a) => a,
            Err(e) => return ui.error.set(Some(e.to_string())),
        };
        match tauri_invoke("appointment_shift", args).await {
            Ok(m) => {
                if let Some(m) = m.as_string() {
                    ui.status.set(Some(m));
                }
                ui.cal_selected.set(None);
            }
            Err(e) => ui.error.set(Some(e.as_string().unwrap_or_else(|| "déplacement impossible".into()))),
        }
    });
}

/// Nouveau rendez-vous sur un créneau tiré à la souris (début et fin `AAAA-MM-JJTHH:MM`).
fn open_new_range(ui: Ui, start: String, end: String) {
    let calendar = ui.cal_view.with_untracked(|v| {
        let v = v.as_ref()?;
        v.calendars.iter().find(|c| c.outlook_default).or_else(|| v.calendars.iter().find(|c| c.shown && c.shared)).or_else(|| v.calendars.iter().find(|c| c.shared)).map(|c| c.id.clone())
    });
    let Some(calendar) = calendar else {
        ui.error.set(Some("Aucun calendrier modifiable.".into()));
        return;
    };
    spawn_local(async move {
        match ipc::appointment_form(None, Some(&calendar), Some(&start)).await {
            Ok(mut f) => {
                f.end = end;
                ui.appointment.set(Some(f));
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

#[component]
fn Timed(item: TimedEvent, col: usize, drag: RwSignal<Option<Drag>>, body: NodeRef<html::Div>, columns: usize) -> impl IntoView {
    let ui = use_ui();
    let top = f64::from(item.start_min) * HOUR_PX / 60.0;
    // Au moins un quart d'heure, comme la mise en colonnes du cœur : deux rendez-vous d'une même colonne ne se recouvrent pas.
    let height = (f64::from(item.end_min.saturating_sub(item.start_min)) * HOUR_PX / 60.0).max(HOUR_PX / 4.0);
    let width = 100.0 / f64::from(item.columns.max(1));
    let style = format!("top:{top}px;height:{height}px;left:calc({}% + 1px);width:calc({width}% - 3px)", f64::from(item.column) * width);
    let (start, end) = (i64::from(item.start_min), i64::from(item.end_min));
    let writable = ui.cal_view.with_untracked(|v| v.as_ref().is_some_and(|v| v.calendars.iter().any(|c| c.id == item.event.calendar_id && c.shared)));
    // Une suite d'un rendez-vous de la veille ou un rendez-vous qui passe minuit se règle dans la boîte « Rendez-vous ».
    let movable = writable && !item.event.continued && end < 1440;
    let (id, subject) = (item.event.id.clone(), item.event.subject.clone());
    let begin = move |e: web_sys::PointerEvent, gesture: Gesture| {
        if e.button() != 0 || !movable {
            return;
        }
        e.stop_propagation();
        let Some(body) = body.get_untracked() else { return };
        let (x, y) = (f64::from(e.client_x()), f64::from(e.client_y()));
        let (_, m) = locate(&body, columns, x, y);
        drag.set(Some(Drag { gesture, active: false, x0: x, y0: y, col0: col, m0: m, col, m }));
    };
    let (move_id, move_subject) = (id.clone(), subject.clone());
    let on_down = move |e: web_sys::PointerEvent| begin(e, Gesture::Move { id: move_id.clone(), subject: move_subject.clone(), start, end });
    let (resize_id, resize_subject) = (id.clone(), subject.clone());
    let on_resize = move |e: web_sys::PointerEvent| begin(e, Gesture::Resize { id: resize_id.clone(), subject: resize_subject.clone(), start, end });
    let dragged = move || drag.with(|d| d.as_ref().is_some_and(|d| d.active && matches!(&d.gesture, Gesture::Move { id: i, .. } | Gesture::Resize { id: i, .. } if *i == id)));
    view! {
        <div class="cal-timed" class:short=height < 22.0 class:dragging=dragged style=style on:pointerdown=on_down>
            <Chip event=item.event />
            {movable.then(|| view! { <div class="cal-resize" title="Changer l'heure de fin" on:pointerdown=on_resize></div> })}
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
    // Ouverture au début de la journée de travail (7 h 30 par défaut, comme la référence).
    Effect::new(move |_| {
        if let Some(el) = scroller.get() {
            let first_hour = use_ui().settings.with_untracked(|s| f64::from(s.work_start) / 60.0);
            el.set_scroll_top((first_hour * HOUR_PX) as i32);
        }
    });
    let ui = use_ui();
    let now = view.now_min;
    let second = view.second_zone.clone();
    let days = view.days.clone();
    let columns = n;
    let dates: Vec<String> = days.iter().map(|d| d.date.clone()).collect();
    let body = NodeRef::<html::Div>::new();
    let drag = RwSignal::new(None::<Drag>);
    // Appui dans une colonne vide : début d'un créneau à tirer.
    let on_down = move |e: web_sys::PointerEvent| {
        if e.button() != 0 {
            return;
        }
        let on_event = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()).is_some_and(|t| t.closest(".cal-timed").ok().flatten().is_some());
        let Some(body) = body.get_untracked() else { return };
        let (x, y) = (f64::from(e.client_x()), f64::from(e.client_y()));
        if on_event || x - body.get_bounding_client_rect().left() < HOURS_W {
            return;
        }
        let (col, m) = locate(&body, columns, x, y);
        drag.set(Some(Drag { gesture: Gesture::Create, active: false, x0: x, y0: y, col0: col, m0: m, col, m }));
    };
    let handles = [
        window_event_listener(leptos::ev::pointermove, move |e| {
            let Some(mut d) = drag.get_untracked() else { return };
            let (x, y) = (f64::from(e.client_x()), f64::from(e.client_y()));
            if !d.active && (x - d.x0).hypot(y - d.y0) < DRAG_PX {
                return;
            }
            let Some(body) = body.get_untracked() else { return };
            let (col, m) = locate(&body, columns, x, y);
            d.active = true;
            d.m = m;
            // Seul un déplacement change de jour ; créer et redimensionner restent dans leur colonne.
            d.col = if matches!(d.gesture, Gesture::Move { .. }) { col } else { d.col0 };
            drag.set(Some(d));
        }),
        window_event_listener(leptos::ev::pointerup, {
            let dates = dates.clone();
            move |_| {
                let Some(d) = drag.get_untracked() else { return };
                drag.set(None);
                if !d.active {
                    return;
                }
                let o = outcome(&d, i64::from(use_ui().settings.with_untracked(|s| s.snap_minutes)));
                match d.gesture {
                    Gesture::Create => {
                        if let Some(date) = dates.get(o.col) {
                            open_new_range(ui, stamp(date, o.start), stamp(date, o.end));
                        }
                    }
                    Gesture::Move { id, .. } | Gesture::Resize { id, .. } => {
                        if o.shift_start != 0 || o.shift_end != 0 {
                            shift(ui, id, o.shift_start, o.shift_end);
                        }
                    }
                }
            }
        }),
        window_event_listener(leptos::ev::pointercancel, move |_| drag.set(None)),
        // Échap annule le geste en cours (le détail du rendez-vous reste affiché).
        window_event_listener(leptos::ev::keydown, move |e| {
            if e.key() == "Escape" && drag.get_untracked().is_some() {
                drag.set(None);
                e.stop_propagation();
            }
        }),
    ];
    on_cleanup(move || handles.into_iter().for_each(|h| h.remove()));
    view! {
        <div class="cal-grid">
            <div class="cal-dayheads" style=cols.clone()>
                <div class="cal-hour-zones" style="font-size:var(--fs-sm);text-align:right;padding-right:var(--u)">
                    {second.as_ref().map(|z| view! { <span title=format!("Second fuseau : {}", z.label)>{z.label.clone()}</span> })}
                </div>
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
                <div class="cal-body" class:dragging=move || drag.with(|d| d.as_ref().is_some_and(|d| d.active)) node_ref=body style=format!("{cols};height:{}px", 24.0 * HOUR_PX) on:pointerdown=on_down>
                    <div class="cal-hours">
                        {(0..24)
                            .map(|h| {
                                // Second fuseau : heure correspondante, en plus petit, sous l'heure locale.
                                let other = second.as_ref().map(|z| {
                                    let m = (h * 60 + z.delta_min).rem_euclid(1_440);
                                    view! { <span style="display:block;opacity:0.7;font-size:0.85em">{format!("{:02}:{:02}", m / 60, m % 60)}</span> }
                                });
                                view! { <div class="cal-hour" style=format!("top:{}px", f64::from(h) * HOUR_PX)>{format!("{h:02}:00")}{other}</div> }
                            })
                            .collect_view()}
                    </div>
                    {days
                        .into_iter()
                        .enumerate()
                        .map(|(idx, d): (usize, CalendarDay)| {
                            let date = d.date.clone();
                            view! {
                                <div class="cal-col" class:today=d.today role="group" aria-label=d.label.clone() on:contextmenu=move |e: leptos::ev::MouseEvent| slot_menu(ui, &e, &date)>
                                    {d.timed.into_iter().map(|t| view! { <Timed item=t col=idx drag=drag body=body columns=columns /> }).collect_view()}
                                    {move || {
                                        let d = drag.get().filter(|d| d.active)?;
                                        let o = outcome(&d, i64::from(use_ui().settings.with_untracked(|s| s.snap_minutes)));
                                        (o.col == idx).then(|| {
                                            let label = match &d.gesture {
                                                Gesture::Create => String::new(),
                                                Gesture::Move { subject, .. } | Gesture::Resize { subject, .. } => subject.clone(),
                                            };
                                            let style = format!("top:{}px;height:{}px", o.start as f64 * HOUR_PX / 60.0, (o.end - o.start) as f64 * HOUR_PX / 60.0);
                                            view! { <div class="cal-ghost" style=style><span class="cal-time">{format!("{} – {}", clock(o.start), clock(o.end))}</span>{label}</div> }
                                        })
                                    }}
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
                        let new_date = d.date.clone();
                        let more_date = d.date.clone();
                        let drop_date = d.date.clone();
                        let chip_date = d.date.clone();
                        view! {
                            <div
                                class="cal-cell"
                                class:today=d.today
                                class:other=!d.in_month
                                role="group"
                                aria-label=d.date.clone()
                                on:contextmenu=move |e: leptos::ev::MouseEvent| slot_menu(ui, &e, &cell_date)
                                on:dragover=move |e: web_sys::DragEvent| {
                                    if e.data_transfer().is_some_and(|dt| dt.types().iter().any(|t| t.as_string().as_deref() == Some(MONTH_DRAG))) {
                                        e.prevent_default();
                                    }
                                }
                                on:drop=move |e: web_sys::DragEvent| {
                                    let Some(data) = e.data_transfer().map(|dt| dt.get_data(MONTH_DRAG).unwrap_or_default()) else { return };
                                    let Some((id, from)) = data.split_once('|') else { return };
                                    e.prevent_default();
                                    let days = day_diff(from, &drop_date);
                                    if days != 0 {
                                        shift(ui, id.to_string(), days * 1440, days * 1440);
                                    }
                                }
                                // Double clic sur un jour (hors rendez-vous) : nouveau rendez-vous ce jour-là.
                                on:dblclick=move |_| crate::appointment::open_new_at(ui, Some(new_date.clone()), false)
                            >
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
                                {items.into_iter().take(SHOWN).map(|e| view! { <Chip event=e month=true date=chip_date.clone() /> }).collect_view()}
                                {(extra > 0).then(|| view! {
                                    <button
                                        class="cal-more"
                                        title="Afficher ce jour"
                                        on:click=move |e| {
                                            e.stop_propagation();
                                            ui.cal_anchor.set(more_date.clone());
                                            set_kind(ui, CalendarViewKind::Day);
                                        }
                                    >
                                        {format!("+ {extra} autre{}", if extra > 1 { "s" } else { "" })}
                                    </button>
                                })}
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}

/// Partie centrale du module.
#[component]
pub fn CalendarPane() -> impl IntoView {
    let ui = use_ui();
    let kind = move || ui.settings.with(|s| s.calendar_view);
    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        // Pg.Préc/Pg.Suiv d'une liste (fuseaux horaires) ou d'une case : la touche reste au champ.
        let in_field = ev.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()).is_some_and(|t| matches!(t.tag_name().as_str(), "INPUT" | "SELECT" | "TEXTAREA"));
        if in_field {
            return;
        }
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
                <ZonePicker />
            </header>
            {move || ui.cal_view.with(|v| v.as_ref().and_then(|v| v.source_note.clone())).map(|n| view! { <p class="cal-note">{n}</p> })}
            <div class="cal-main" on:click=move |_| ui.cal_selected.set(None)>
                {move || match ui.cal_view.get() {
                    None => view! { <div class="placeholder">"Chargement du calendrier…"</div> }.into_any(),
                    Some(v) if v.kind == CalendarViewKind::Month => view! { <MonthGrid view=v /> }.into_any(),
                    Some(v) => view! { <TimeGrid view=v /> }.into_any(),
                }}
            </div>
        </section>
    }
}

/// Volet de gauche du module : navigateur de dates et liste des calendriers.
#[component]
pub fn CalendarNav() -> impl IntoView {
    let ui = use_ui();
    let fu = FolderUi::new(FolderKind::Calendar);
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
            <FolderHost f=fu on_change=move |()| ui.cal_epoch.update(|e| *e += 1) />
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
                        <div class="cal-list" role="group" aria-label="Mes calendriers" on:contextmenu=move |ev| fu.open_menu(&ev, "")>
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
                                    let menu_id = c.id.clone();
                                    view! {
                                        {head}
                                        <label class="cal-item" on:contextmenu=move |ev| fu.open_menu(&ev, &menu_id)>
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
