//! Liste des messages virtualisée : seules les lignes visibles existent dans le DOM, et le cœur ne
//! les envoie que par tranches. Pivots Tous / Non lus, tri, groupes repliables, sélection au clavier.

use std::collections::HashSet;

use leptos::{prelude::*, task::spawn_local};
use thunderlook_model::{FlagState, Importance, ListGroup, ListView, MessageSummary, SortField, settings::Density};

use crate::{
    icons::*,
    ipc,
    menu::{MenuButton, MenuEntry},
    state::{Ui, use_ui},
};

/// Hauteur d'un en-tête de groupe (px), identique à `.group-head`.
const GROUP_H: f64 = 28.0;
/// Lignes demandées au cœur par appel.
pub const PAGE: u32 = 100;
/// Lignes rendues au-delà de la zone visible, de chaque côté.
const OVERSCAN: f64 = 6.0;

/// Hauteur d'une ligne de message (px), identique à `--msg-h`.
pub fn row_height(density: Density) -> f64 {
    match density {
        Density::Comfortable => 80.0,
        Density::Compact => 66.0,
    }
}

fn msg_dom_id(pos: u32) -> String {
    format!("msg-{pos}")
}

/// Disposition verticale d'une vue : groupes (ou un seul bloc sans en-tête) et groupes repliés.
#[derive(Clone, PartialEq)]
pub struct Layout {
    groups: Vec<ListGroup>,
    headers: bool,
    collapsed: HashSet<String>,
    row_h: f64,
}

#[derive(Clone, Debug, PartialEq)]
enum Entry {
    Header { index: usize, top: f64 },
    Row { pos: u32, top: f64 },
}

impl Layout {
    pub fn new(view: &ListView, collapsed: HashSet<String>, row_h: f64) -> Self {
        let headers = !view.groups.is_empty();
        let groups = if headers {
            view.groups.clone()
        } else {
            vec![ListGroup { label: String::new(), start: 0, count: view.count }]
        };
        Self { groups, headers, collapsed, row_h }
    }

    fn open(&self, g: &ListGroup) -> bool {
        !self.headers || !self.collapsed.contains(&g.label)
    }

    fn head_h(&self) -> f64 {
        if self.headers { GROUP_H } else { 0.0 }
    }

    fn group_h(&self, g: &ListGroup) -> f64 {
        self.head_h() + if self.open(g) { g.count as f64 * self.row_h } else { 0.0 }
    }

    pub fn height(&self) -> f64 {
        self.groups.iter().map(|g| self.group_h(g)).sum()
    }

    /// Éléments qui recoupent `[top, top + height]`, avec une marge.
    fn visible(&self, top: f64, height: f64) -> Vec<Entry> {
        let (lo, hi) = (top - OVERSCAN * self.row_h, top + height + OVERSCAN * self.row_h);
        let mut out = Vec::new();
        let mut y = 0.0;
        for (index, g) in self.groups.iter().enumerate() {
            let h = self.group_h(g);
            if y + h >= lo && y <= hi {
                if self.headers {
                    out.push(Entry::Header { index, top: y });
                }
                if self.open(g) {
                    let first_y = y + self.head_h();
                    let from = ((lo - first_y) / self.row_h).floor().max(0.0) as u32;
                    let to = (((hi - first_y) / self.row_h).ceil().max(0.0) as u32).min(g.count);
                    for i in from..to {
                        out.push(Entry::Row { pos: g.start + i, top: first_y + i as f64 * self.row_h });
                    }
                }
            }
            y += h;
            if y > hi {
                break;
            }
        }
        out
    }

    /// Ordonnée d'une ligne, si son groupe est ouvert.
    pub fn row_top(&self, pos: u32) -> Option<f64> {
        let mut y = 0.0;
        for g in &self.groups {
            if pos >= g.start && pos < g.start + g.count {
                return self.open(g).then(|| y + self.head_h() + (pos - g.start) as f64 * self.row_h);
            }
            y += self.group_h(g);
        }
        None
    }

    fn open_groups(&self) -> impl Iterator<Item = &ListGroup> {
        self.groups.iter().filter(|g| self.open(g) && g.count > 0)
    }

    /// Position atteinte en se déplaçant de `delta` lignes visibles depuis `pos`, bornée à la liste.
    /// Une position dans un groupe replié compte comme le début du groupe ouvert suivant.
    pub fn step(&self, pos: Option<u32>, delta: i64) -> Option<u32> {
        let total: i64 = self.open_groups().map(|g| g.count as i64).sum();
        if total == 0 {
            return None;
        }
        let rank = pos.map(|p| {
            let mut rank = 0i64;
            for g in self.open_groups() {
                if p < g.start {
                    return rank;
                }
                if p < g.start + g.count {
                    return rank + (p - g.start) as i64;
                }
                rank += g.count as i64;
            }
            total - 1
        });
        let mut r = rank.map_or(0, |r| (r + delta).clamp(0, total - 1));
        for g in self.open_groups() {
            if r < g.count as i64 {
                return Some(g.start + r as u32);
            }
            r -= g.count as i64;
        }
        None
    }

    pub fn first(&self) -> Option<u32> {
        self.step(None, 0)
    }

    pub fn last(&self) -> Option<u32> {
        self.open_groups().last().map(|g| g.start + g.count - 1)
    }
}

/// Demande au cœur les tranches de lignes manquantes pour les positions données.
pub fn ensure_rows(ui: Ui, positions: impl IntoIterator<Item = u32>, requested: StoredValue<HashSet<(u64, u32)>>) {
    let Some(token) = ui.view.with_untracked(|v| v.as_ref().map(|v| v.token)) else { return };
    let mut pages: Vec<u32> = positions.into_iter().map(|p| p / PAGE).collect();
    pages.sort_unstable();
    pages.dedup();
    for page in pages {
        let loaded = ui.rows.with_untracked(|r| r.contains_key(&(page * PAGE)));
        if loaded || !requested.try_update_value(|s| s.insert((token, page))).unwrap_or(false) {
            continue;
        }
        spawn_local(async move {
            match ipc::list_rows(token, page * PAGE, PAGE).await {
                Ok(rows) => {
                    if ui.view.with_untracked(|v| v.as_ref().map(|v| v.token)) == Some(token) {
                        ui.rows.update(|r| r.extend(rows.into_iter().map(|m| (m.pos, m))));
                    }
                }
                Err(e) => {
                    requested.update_value(|s| {
                        s.remove(&(token, page));
                    });
                    ui.error.set(Some(e));
                }
            }
        });
    }
}

#[component]
pub fn MessageList() -> impl IntoView {
    let ui = use_ui();
    let requested = expect_context::<StoredValue<HashSet<(u64, u32)>>>();
    let scroll_top = RwSignal::new(0.0f64);
    let viewport_h = RwSignal::new(600.0f64);

    let layout = Memo::new(move |_| {
        let row_h = row_height(ui.settings.get().density);
        let collapsed = ui.collapsed_groups.get();
        ui.view.with(|v| v.as_ref().map(|v| Layout::new(v, collapsed, row_h)))
    });

    let measure = move || {
        if let Some(el) = ui.refs.list.get_untracked() {
            viewport_h.set(el.client_height() as f64);
            scroll_top.set(el.scroll_top() as f64);
        }
    };
    let resize = window_event_listener(leptos::ev::resize, move |_| measure());
    on_cleanup(move || resize.remove());
    // Mesure après montage et après chaque changement de disposition des volets.
    Effect::new(move |_| {
        ui.settings.track();
        request_animation_frame(measure);
    });

    // Nouvelle vue : retour en haut.
    Effect::new(move |prev: Option<Option<u64>>| {
        let token = ui.view.with(|v| v.as_ref().map(|v| v.token));
        if prev.is_some_and(|p| p != token) {
            if let Some(el) = ui.refs.list.get_untracked() {
                el.set_scroll_top(0);
            }
            scroll_top.set(0.0);
        }
        token
    });

    let entries = Memo::new(move |_| {
        layout.with(|l| l.as_ref().map(|l| l.visible(scroll_top.get(), viewport_h.get())).unwrap_or_default())
    });

    // Lignes visibles à charger.
    Effect::new(move |_| {
        let positions: Vec<u32> = entries.with(|e| {
            e.iter().filter_map(|x| if let Entry::Row { pos, .. } = x { Some(*pos) } else { None }).collect()
        });
        ensure_rows(ui, positions, requested);
    });

    // La sélection reste visible quand elle change (clavier, Ctrl+. / Ctrl+,).
    Effect::new(move |_| {
        let Some(pos) = ui.selected_pos.get() else { return };
        let (Some(el), Some(top)) = (ui.refs.list.get_untracked(), layout.with_untracked(|l| l.as_ref().and_then(|l| l.row_top(pos)))) else {
            return;
        };
        let row_h = row_height(ui.settings.get_untracked().density);
        let (view_top, h) = (el.scroll_top() as f64, el.client_height() as f64);
        // Un en-tête de groupe peut masquer la ligne : on garde sa hauteur en marge au-dessus.
        if top - GROUP_H < view_top {
            el.set_scroll_top((top - GROUP_H).max(0.0) as i32);
        } else if top + row_h > view_top + h {
            el.set_scroll_top((top + row_h - h) as i32);
        }
    });

    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        let Some(l) = layout.get_untracked() else { return };
        let page = ((viewport_h.get_untracked() / row_height(ui.settings.get_untracked().density)).floor() as i64 - 1).max(1);
        let cur = ui.selected_pos.get_untracked();
        let target = match ev.key().as_str() {
            "ArrowDown" => l.step(cur, 1),
            "ArrowUp" => l.step(cur, -1),
            "PageDown" => l.step(cur, page),
            "PageUp" => l.step(cur, -page),
            "Home" => l.first(),
            "End" => l.last(),
            _ => return,
        };
        ev.prevent_default();
        if target.is_some() {
            ui.selected_pos.set(target);
        }
    };

    let folder_name = move || {
        let id = ui.folder.get();
        ui.tree.with(|t| t.as_ref().and_then(|t| t.find_folder(&id)).map(|f| f.name.clone()).unwrap_or_default())
    };

    let sort_entries = Signal::derive(move || {
        let (field, desc) = ui.sort.get();
        let mut v: Vec<MenuEntry> = SortField::ALL
            .into_iter()
            .map(|f| MenuEntry::radio(f.label(), f == field, move || ui.sort.set((f, f.default_descending()))))
            .collect();
        v.push(MenuEntry::radio("Ordre croissant", !desc, move || ui.sort.update(|s| s.1 = false)));
        v.push(MenuEntry::radio("Ordre décroissant", desc, move || ui.sort.update(|s| s.1 = true)));
        v
    });

    let empty = move || ui.view.with(|v| v.as_ref().is_some_and(|v| v.count == 0));

    view! {
        <section class="pane list" aria-label="Liste des messages">
            <div class="list-head">
                <div role="group" aria-label="Filtre">
                    <button class="pivot" aria-pressed=move || (!ui.unread_only.get()).to_string() on:click=move |_| ui.unread_only.set(false)>
                        "Tous"
                    </button>
                    <button class="pivot" aria-pressed=move || ui.unread_only.get().to_string() on:click=move |_| ui.unread_only.set(true)>
                        "Non lus"
                    </button>
                </div>
                <span class="sort-current">{move || format!("Par {}", ui.sort.get().0.label().to_lowercase())}</span>
                <MenuButton label="Trier par" icon=SORT_AZ entries=sort_entries show_label=false class="icon-btn" />
                <button
                    class="icon-btn"
                    aria-label=move || if ui.sort.get().1 { "Ordre décroissant (inverser)" } else { "Ordre croissant (inverser)" }
                    title="Inverser le tri"
                    on:click=move |_| ui.sort.update(|s| s.1 = !s.1)
                >
                    {move || if ui.sort.get().1 { view! { <Svg icon=ARROW_DOWN /> } } else { view! { <Svg icon=ARROW_UP /> } }}
                </button>
            </div>
            <div
                class="listbox"
                role="listbox"
                tabindex="0"
                aria-label=move || format!("Messages — {}", folder_name())
                node_ref=ui.refs.list
                aria-activedescendant=move || ui.selected_pos.get().map(msg_dom_id)
                on:keydown=on_key
                on:scroll=move |_| measure()
            >
                <Show when=empty>
                    <p class="list-empty">"Aucun élément à afficher."</p>
                </Show>
                <div class="list-canvas" style=move || format!("height: {}px", layout.with(|l| l.as_ref().map(Layout::height).unwrap_or(0.0)))>
                    {move || {
                        let total = ui.view.with(|v| v.as_ref().map(|v| v.count).unwrap_or(0));
                        entries
                            .get()
                            .into_iter()
                            .map(|e| match e {
                                Entry::Header { index, top } => {
                                    let (label, count) = layout.with_untracked(|l| {
                                        l.as_ref().map(|l| (l.groups[index].label.clone(), l.groups[index].count)).unwrap_or_default()
                                    });
                                    let open = !ui.collapsed_groups.with_untracked(|c| c.contains(&label));
                                    let toggle = label.clone();
                                    view! {
                                        <button
                                            class="group-head"
                                            style=format!("top: {top}px")
                                            tabindex="-1"
                                            aria-expanded=open.to_string()
                                            on:click=move |_| {
                                                ui.collapsed_groups.update(|c| {
                                                    if !c.remove(&toggle) {
                                                        c.insert(toggle.clone());
                                                    }
                                                })
                                            }
                                        >
                                            {if open { view! { <Svg icon=CHEVRON_DOWN size=14 /> } } else { view! { <Svg icon=CHEVRON_RIGHT size=14 /> } }}
                                            <span class="group-label">{label}</span>
                                            <span class="group-count">{count}</span>
                                        </button>
                                    }
                                    .into_any()
                                }
                                Entry::Row { pos, top } => {
                                    let m = ui.rows.with(|r| r.get(&pos).cloned());
                                    view! { <Row pos=pos top=top total=total m=m /> }.into_any()
                                }
                            })
                            .collect_view()
                    }}
                </div>
            </div>
        </section>
    }
}

#[component]
fn Row(pos: u32, top: f64, total: u32, m: Option<MessageSummary>) -> impl IntoView {
    let ui = use_ui();
    let selected = move || ui.selected_pos.get() == Some(pos);
    let style = format!("top: {top}px");
    let pick = move |_| {
        ui.selected_pos.set(Some(pos));
        if let Some(l) = ui.refs.list.get_untracked() {
            let _ = l.focus();
        }
    };
    let Some(m) = m else {
        return view! {
            <div
                role="option"
                id=msg_dom_id(pos)
                class="msg pending"
                style=style
                aria-selected=move || selected().to_string()
                aria-posinset=pos + 1
                aria-setsize=total
                aria-label="Chargement…"
                on:click=pick
            >
                <span class="from">"…"</span>
            </div>
        }
        .into_any();
    };
    let status = [
        m.unread.then_some("non lu"),
        (m.flag == FlagState::Flagged).then_some("avec suivi"),
        (m.flag == FlagState::Done).then_some("suivi terminé"),
        (m.importance == Importance::High).then_some("importance haute"),
        m.has_attachments.then_some("pièce jointe"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ");
    let label = format!("{}, {}, {}{}", m.from, m.subject, m.time_label, if status.is_empty() { String::new() } else { format!(", {status}") });
    let subject = if m.subject.trim().is_empty() { "(aucun objet)".to_string() } else { m.subject.clone() };
    view! {
        <div
            role="option"
            id=msg_dom_id(pos)
            class="msg"
            class:unread=m.unread
            class:focused=selected
            style=style
            aria-selected=move || selected().to_string()
            aria-posinset=pos + 1
            aria-setsize=total
            aria-label=label
            on:click=pick
        >
            <span class="from">{m.from.clone()}</span>
            <span class="meta">
                {match m.importance {
                    Importance::High => Some(view! { <span class="imp-high" title="Importance haute"><Svg icon=CIRCLE_ALERT size=14 /></span> }.into_any()),
                    Importance::Low => Some(view! { <span class="imp-low" title="Importance faible"><Svg icon=ARROW_DOWN size=14 /></span> }.into_any()),
                    Importance::Normal => None,
                }}
                {m.categories
                    .iter()
                    .map(|c| {
                        let style = c.color.as_ref().map(|col| format!("background:{col}")).unwrap_or_default();
                        view! { <span class="cat" class:nocolor=c.color.is_none() style=style title=c.name.clone()></span> }
                    })
                    .collect_view()}
                {m.has_attachments.then(|| view! { <Svg icon=PAPERCLIP size=14 /> })}
                {match m.flag {
                    FlagState::Flagged => Some(view! { <span class="flag" title="Avec suivi"><Svg icon=FLAG size=14 /></span> }.into_any()),
                    FlagState::Done => Some(view! { <span class="flag-done" title="Suivi terminé"><Svg icon=CHECK size=14 /></span> }.into_any()),
                    FlagState::None => None,
                }}
                <span class="time">{m.time_label.clone()}</span>
            </span>
            <span class="subject">{subject}</span>
            <span class="preview">{m.preview.clone()}</span>
        </div>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(groups: Vec<(&str, u32, u32)>, count: u32) -> ListView {
        ListView {
            token: 1,
            count,
            folder_total: count,
            folder_unread: 0,
            groups: groups.into_iter().map(|(l, s, c)| ListGroup { label: l.into(), start: s, count: c }).collect(),
        }
    }

    #[test]
    fn layout_and_steps() {
        let v = view(vec![("A", 0, 2), ("B", 2, 3), ("C", 5, 1)], 6);
        let l = Layout::new(&v, HashSet::new(), 50.0);
        assert_eq!(l.height(), 3.0 * GROUP_H + 6.0 * 50.0);
        assert_eq!(l.row_top(2), Some(2.0 * GROUP_H + 100.0));
        assert_eq!(l.step(Some(1), 1), Some(2));
        assert_eq!(l.step(Some(5), 1), Some(5));
        assert_eq!(l.last(), Some(5));
        let l = Layout::new(&v, HashSet::from(["B".to_string()]), 50.0);
        assert_eq!(l.row_top(3), None);
        assert_eq!(l.step(Some(1), 1), Some(5));
        assert_eq!(l.step(Some(5), -1), Some(1));
        assert_eq!(l.step(Some(3), 0), Some(5));
        let flat = Layout::new(&view(vec![], 1000), HashSet::new(), 50.0);
        assert_eq!(flat.height(), 50_000.0);
        let vis = flat.visible(10_000.0, 500.0);
        assert!(vis.len() < 30);
        assert_eq!(vis.first(), Some(&Entry::Row { pos: 194, top: 9_700.0 }));
    }
}
