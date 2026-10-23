//! Boîte « Options » (Fichier › Options, barre d'état) : réglages enregistrés dans `settings.json`.
//! Chaque changement s'applique et s'enregistre tout de suite (comme « Réglages de l'affichage »).

use leptos::{html, prelude::*, task::spawn_local};
use thunderlook_model::{
    MessageOp, Selection,
    settings::{ReadingPane, Settings, Theme},
};
use wasm_bindgen::JsCast;

use crate::{
    ipc,
    state::{Dialog, Ui, use_ui},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    General,
    Calendar,
    Mail,
    TasksNotes,
    Reminders,
}

impl Tab {
    const ALL: [Tab; 5] = [Tab::General, Tab::Calendar, Tab::Mail, Tab::TasksNotes, Tab::Reminders];

    fn label(self) -> &'static str {
        match self {
            Tab::General => "Général",
            Tab::Calendar => "Calendrier",
            Tab::Mail => "Courrier",
            Tab::TasksNotes => "Tâches et notes",
            Tab::Reminders => "Rappels",
        }
    }
}

const DAYS: [&str; 7] = ["Lundi", "Mardi", "Mercredi", "Jeudi", "Vendredi", "Samedi", "Dimanche"];
/// Premiers jours de la semaine proposés : (0 dimanche … 6 samedi, libellé).
const FIRST_DAYS: [(u8, &str); 3] = [(1, "Lundi"), (0, "Dimanche"), (6, "Samedi")];
const NOTE_COLORS: [(i32, &str); 5] = [(0, "Bleue"), (1, "Verte"), (2, "Rose"), (3, "Jaune"), (4, "Blanche")];
const DURATIONS: [u32; 7] = [15, 30, 45, 60, 90, 120, 240];
const REMINDERS: [(i32, &str); 8] = [(0, "0 minute"), (5, "5 minutes"), (15, "15 minutes"), (30, "30 minutes"), (60, "1 heure"), (120, "2 heures"), (1_440, "1 jour"), (10_080, "1 semaine")];

fn hm(minutes: u16) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

fn parse_hm(text: &str) -> Option<u16> {
    let (h, m) = text.split_once(':')?;
    Some(h.parse::<u16>().ok()? * 60 + m.parse::<u16>().ok()?)
}

/// Applique les réglages qui touchent le document entier (thème, zoom) et la minuterie « marquer comme lu ».
/// À monter une fois, dans `App`.
#[component]
pub fn OptionsEffects() -> impl IntoView {
    let ui = use_ui();

    Effect::new(move |_| {
        let (theme, zoom) = ui.settings.with(|s| (s.theme, s.zoom));
        if let Some(root) = document().document_element() {
            match theme {
                Theme::System => {
                    let _ = root.remove_attribute("data-theme");
                }
                Theme::Light => {
                    let _ = root.set_attribute("data-theme", "light");
                }
                Theme::Dark => {
                    let _ = root.set_attribute("data-theme", "dark");
                }
            }
            if let Ok(el) = root.dyn_into::<web_sys::HtmlElement>() {
                let _ = el.style().set_property("zoom", &format!("{}", f64::from(zoom) / 100.0));
            }
        }
    });

    // Marquer comme lu après N secondes d'affichage (0 : pas de minuterie).
    Effect::new(move |_| {
        let Some(id) = ui.selected.get() else { return };
        let delay = ui.settings.with_untracked(|s| s.mark_read_delay);
        if delay == 0 {
            return;
        }
        set_timeout(move || mark_read_if_still_shown(ui, id), std::time::Duration::from_secs(u64::from(delay)));
    });
}

fn mark_read_if_still_shown(ui: Ui, id: String) {
    if ui.selected.get_untracked().as_ref() != Some(&id) {
        return;
    }
    let unread = ui.rows.with_untracked(|r| r.values().find(|m| m.id == id).map(|m| (m.pos, m.unread)));
    if let Some((pos, true)) = unread {
        ui.rows.update(|r| {
            if let Some(m) = r.get_mut(&pos) {
                m.unread = false;
            }
        });
        spawn_local(async move {
            match ipc::apply_op(&Selection::ids(vec![id]), &MessageOp::Seen).await {
                Ok(_) => crate::actions::refresh_tree(ui),
                Err(e) => ui.error.set(Some(e)),
            }
        });
    }
}

pub fn open(ui: Ui) {
    ui.dialog.set(Some(Dialog::Options));
}

pub fn zoom_by(ui: Ui, delta: i32) {
    ui.update_settings(|s| s.zoom = (s.zoom as i32 + delta).clamp(Settings::ZOOM_MIN as i32, Settings::ZOOM_MAX as i32) as u32);
}

#[component]
pub fn OptionsDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let tab = RwSignal::new(Tab::General);
    let s = move || ui.settings.get();
    let set = move |f: &dyn Fn(&mut Settings)| ui.update_settings(|s| f(s));

    Effect::new(move |_| {
        if ui.dialog.get() == Some(Dialog::Options) {
            request_animation_frame(move || {
                if let Some(r) = root.get_untracked()
                    && let Ok(Some(el)) = r.query_selector("[role=tab][aria-selected=true]")
                    && let Ok(el) = el.dyn_into::<web_sys::HtmlElement>()
                {
                    let _ = el.focus();
                }
            });
        }
    });

    let close = move || {
        ui.dialog.set(None);
        if let Some(l) = ui.refs.list.get_untracked() {
            let _ = l.focus();
        }
    };

    let check = move |label: &'static str, get: fn(&Settings) -> bool, put: fn(&mut Settings, bool)| {
        view! {
            <label class="check">
                <input type="checkbox" prop:checked=move || ui.settings.with(get) on:change=move |ev| set(&|s| put(s, event_target_checked(&ev))) />
                <span>{label}</span>
            </label>
        }
    };
    let number = move |label: &'static str, min: u32, max: u32, get: fn(&Settings) -> u32, put: fn(&mut Settings, u32)| {
        view! {
            <label class="field">
                <span>{label}</span>
                <input type="number" min=min max=max aria-label=label prop:value=move || ui.settings.with(get)
                    on:change=move |ev| {
                        if let Ok(v) = event_target_value(&ev).trim().parse::<u32>() {
                            set(&|s| put(s, v));
                        }
                    } />
            </label>
        }
    };
    let time = move |label: &'static str, get: fn(&Settings) -> u16, put: fn(&mut Settings, u16)| {
        view! {
            <label class="field">
                <span>{label}</span>
                <input type="time" aria-label=label prop:value=move || hm(ui.settings.with(get))
                    on:change=move |ev| {
                        if let Some(v) = parse_hm(&event_target_value(&ev)) {
                            set(&|s| put(s, v));
                        }
                    } />
            </label>
        }
    };

    let general = move || view! {
        <label class="field">
            <span>"Thème"</span>
            <select aria-label="Thème" on:change=move |ev| {
                let t = match event_target_value(&ev).as_str() { "light" => Theme::Light, "dark" => Theme::Dark, _ => Theme::System };
                set(&|s| s.theme = t);
            }>
                <option value="system" selected=move || s().theme == Theme::System>"Système"</option>
                <option value="light" selected=move || s().theme == Theme::Light>"Clair"</option>
                <option value="dark" selected=move || s().theme == Theme::Dark>"Sombre"</option>
            </select>
        </label>
        {number("Zoom (%)", Settings::ZOOM_MIN, Settings::ZOOM_MAX, |s| s.zoom, |s, v| s.zoom = v)}
    };

    let calendar = move || view! {
        <label class="field">
            <span>"Premier jour de la semaine"</span>
            <select aria-label="Premier jour de la semaine" on:change=move |ev| {
                let v = event_target_value(&ev).parse().unwrap_or(1);
                set(&|s| s.first_day_of_week = v);
            }>
                {FIRST_DAYS.iter().map(|&(v, l)| view! { <option value=v.to_string() selected=move || s().first_day_of_week == v>{l}</option> }).collect_view()}
            </select>
        </label>
        <fieldset class="field">
            <legend>"Jours ouvrés"</legend>
            <div class="field-row">
                {DAYS.iter().enumerate().map(|(i, name)| view! {
                    <label class="check">
                        <input type="checkbox" aria-label=*name prop:checked=move || s().is_work_day(i as u32)
                            on:change=move |ev| {
                                let on = event_target_checked(&ev);
                                set(&|s| if on { s.work_days |= 1 << i } else { s.work_days &= !(1 << i) });
                            } />
                        <span>{&name[..3]}</span>
                    </label>
                }).collect_view()}
            </div>
        </fieldset>
        <div class="field-row">
            {time("Début de la journée", |s| s.work_start, |s, v| s.work_start = v)}
            {time("Fin de la journée", |s| s.work_end, |s, v| s.work_end = v)}
        </div>
        <div class="field-row">
            <label class="field">
                <span>"Durée d'un nouveau rendez-vous"</span>
                <select aria-label="Durée d'un nouveau rendez-vous" on:change=move |ev| {
                    let v = event_target_value(&ev).parse().unwrap_or(30);
                    set(&|s| s.default_duration = v);
                }>
                    {move || {
                        let cur = s().default_duration;
                        let mut all: Vec<u32> = DURATIONS.to_vec();
                        if !all.contains(&cur) { all.push(cur); all.sort_unstable(); }
                        all.into_iter().map(|m| view! { <option value=m.to_string() selected=move || cur == m>{format!("{m} minutes")}</option> }).collect_view()
                    }}
                </select>
            </label>
            <label class="field">
                <span>"Rappel par défaut"</span>
                <select aria-label="Rappel par défaut" on:change=move |ev| {
                    let v = event_target_value(&ev).parse::<i32>().ok();
                    set(&|s| s.default_reminder = v);
                }>
                    <option value="" selected=move || s().default_reminder.is_none()>"Aucun"</option>
                    {move || {
                        let cur = s().default_reminder;
                        let mut all: Vec<(i32, String)> = REMINDERS.iter().map(|&(m, l)| (m, l.to_string())).collect();
                        if let Some(c) = cur && !all.iter().any(|x| x.0 == c) { all.push((c, format!("{c} minutes"))); all.sort_by_key(|x| x.0); }
                        all.into_iter().map(|(m, l)| view! { <option value=m.to_string() selected=move || cur == Some(m)>{l}</option> }).collect_view()
                    }}
                </select>
            </label>
        </div>
        <label class="field">
            <span>"Pas de la grille"</span>
            <select aria-label="Pas de la grille" on:change=move |ev| {
                let v = event_target_value(&ev).parse().unwrap_or(15);
                set(&|s| s.snap_minutes = v);
            }>
                {[15u32, 30, 60].into_iter().map(|m| view! { <option value=m.to_string() selected=move || s().snap_minutes == m>{format!("{m} minutes")}</option> }).collect_view()}
            </select>
        </label>
        {check("Afficher les numéros de semaine", |s| s.show_week_numbers, |s, v| s.show_week_numbers = v)}
    };

    let mail = move || view! {
        {number("Marquer comme lu après (secondes, 0 : à la sélection suivante)", 0, 600, |s| s.mark_read_delay, |s, v| s.mark_read_delay = v)}
        {check("Marquer comme lu quand la sélection quitte le message", |s| s.mark_read_on_leave, |s, v| s.mark_read_on_leave = v)}
        <label class="field">
            <span>"Volet de lecture"</span>
            <select aria-label="Volet de lecture" on:change=move |ev| {
                let p = match event_target_value(&ev).as_str() { "bottom" => ReadingPane::Bottom, "off" => ReadingPane::Off, _ => ReadingPane::Right };
                set(&|s| s.reading_pane = p);
            }>
                <option value="right" selected=move || s().reading_pane == ReadingPane::Right>"À droite"</option>
                <option value="bottom" selected=move || s().reading_pane == ReadingPane::Bottom>"En bas"</option>
                <option value="off" selected=move || s().reading_pane == ReadingPane::Off>"Masqué"</option>
            </select>
        </label>
    };

    let tasks_notes = move || view! {
        <label class="field">
            <span>"Couleur d'une nouvelle note"</span>
            <select aria-label="Couleur d'une nouvelle note" on:change=move |ev| {
                let v = event_target_value(&ev).parse().unwrap_or(3);
                set(&|s| s.default_note_color = v);
            }>
                {NOTE_COLORS.iter().map(|&(v, l)| view! { <option value=v.to_string() selected=move || s().default_note_color == v>{l}</option> }).collect_view()}
            </select>
        </label>
        <label class="field">
            <span>"Rappel d'une tâche avec échéance"</span>
            <input type="time" aria-label="Rappel d'une tâche avec échéance" prop:value=move || s().default_task_reminder.map(hm).unwrap_or_default()
                on:change=move |ev| {
                    let v = parse_hm(&event_target_value(&ev));
                    set(&|s| s.default_task_reminder = v);
                } />
        </label>
        <p class="dialog-note">"Laisser vide : pas de rappel par défaut."</p>
    };

    let reminders = move || view! {
        {check("Notifications Windows des rappels", |s| s.notifications, |s, v| s.notifications = v)}
        {check("Jouer un son à l'arrivée d'un rappel", |s| s.reminder_sound, |s, v| s.reminder_sound = v)}
    };

    let on_tab_key = move |ev: leptos::ev::KeyboardEvent| {
        let i = Tab::ALL.iter().position(|t| *t == tab.get_untracked()).unwrap_or(0);
        let next = match ev.key().as_str() {
            "ArrowRight" => (i + 1) % Tab::ALL.len(),
            "ArrowLeft" => (i + Tab::ALL.len() - 1) % Tab::ALL.len(),
            "Home" => 0,
            "End" => Tab::ALL.len() - 1,
            _ => return,
        };
        ev.prevent_default();
        tab.set(Tab::ALL[next]);
        request_animation_frame(move || {
            if let Some(r) = root.get_untracked()
                && let Ok(Some(el)) = r.query_selector("[role=tab][aria-selected=true]")
                && let Ok(el) = el.dyn_into::<web_sys::HtmlElement>()
            {
                let _ = el.focus();
            }
        });
    };

    view! {
        <Show when=move || ui.dialog.get() == Some(Dialog::Options)>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog wide" role="dialog" aria-modal="true" aria-labelledby="opt-title" node_ref=root
                    on:keydown=move |ev| {
                        if ev.key() == "Escape" {
                            ev.prevent_default();
                            close();
                        }
                    }>
                    <h2 id="opt-title">"Options"</h2>
                    <div role="tablist" aria-label="Catégories d'options" style="display:flex;gap:4px;flex-wrap:wrap" on:keydown=on_tab_key>
                        {Tab::ALL.into_iter().map(|t| view! {
                            <button role="tab" class="btn" type="button" aria-selected=move || (tab.get() == t).to_string()
                                tabindex=move || if tab.get() == t { "0" } else { "-1" }
                                style=move || if tab.get() == t { "font-weight:600;border-color:var(--accent)" } else { "" }
                                on:click=move |_| tab.set(t)>{t.label()}</button>
                        }).collect_view()}
                    </div>
                    <div role="tabpanel" style="display:flex;flex-direction:column;gap:8px;min-height:260px;padding-top:8px">
                        {move || match tab.get() {
                            Tab::General => general().into_any(),
                            Tab::Calendar => calendar().into_any(),
                            Tab::Mail => mail().into_any(),
                            Tab::TasksNotes => tasks_notes().into_any(),
                            Tab::Reminders => reminders().into_any(),
                        }}
                    </div>
                    <div class="dialog-buttons">
                        <button class="btn primary" on:click=move |_| close()>"Fermer"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
