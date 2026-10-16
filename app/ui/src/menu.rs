//! Bouton à menu déroulant accessible (role="menu"), au clavier comme à la souris.

use icondata_core::Icon;
use leptos::{html, prelude::*};
use wasm_bindgen::JsCast;

use crate::icons::{CHEVRON_DOWN, Svg};

#[derive(Clone)]
pub struct MenuEntry {
    pub label: &'static str,
    /// `Some(_)` : élément à cocher (menuitemradio).
    pub checked: Option<bool>,
    pub on_select: Option<Callback<()>>,
}

impl MenuEntry {
    pub fn action(label: &'static str, on_select: impl Fn() + Send + Sync + 'static) -> Self {
        Self { label, checked: None, on_select: Some(Callback::new(move |_| on_select())) }
    }

    pub fn radio(label: &'static str, checked: bool, on_select: impl Fn() + Send + Sync + 'static) -> Self {
        Self { label, checked: Some(checked), on_select: Some(Callback::new(move |_| on_select())) }
    }

    /// Commande prévue mais pas encore disponible (affichée désactivée).
    pub fn pending(label: &'static str) -> Self {
        Self { label, checked: None, on_select: None }
    }
}

fn items(menu: &web_sys::HtmlDivElement) -> Vec<web_sys::HtmlElement> {
    let Ok(list) = menu.query_selector_all("[role^=menuitem]:not([aria-disabled=true])") else { return vec![] };
    (0..list.length()).filter_map(|i| list.item(i)?.dyn_into::<web_sys::HtmlElement>().ok()).collect()
}

fn move_focus(menu: &web_sys::HtmlDivElement, step: i32) {
    let items = items(menu);
    if items.is_empty() {
        return;
    }
    let active = document().active_element();
    let pos = items.iter().position(|el| active.as_ref().is_some_and(|a| a == el.unchecked_ref::<web_sys::Element>()));
    let n = items.len() as i32;
    let next = match pos {
        Some(p) => (p as i32 + step).rem_euclid(n),
        None if step > 0 => 0,
        None => n - 1,
    };
    let _ = items[next as usize].focus();
}

#[component]
pub fn MenuButton(
    label: &'static str,
    icon: Icon,
    #[prop(into)] entries: Signal<Vec<MenuEntry>>,
    /// Affiche le libellé à côté de l'icône.
    #[prop(default = true)]
    show_label: bool,
    #[prop(default = "")] title: &'static str,
    #[prop(default = "cmd")] class: &'static str,
) -> impl IntoView {
    let open = RwSignal::new(false);
    let wrap: NodeRef<html::Div> = NodeRef::new();
    let button: NodeRef<html::Button> = NodeRef::new();
    let menu: NodeRef<html::Div> = NodeRef::new();

    // Fermeture au clic hors du menu.
    let outside = window_event_listener(leptos::ev::pointerdown, move |ev| {
        if !open.get_untracked() {
            return;
        }
        let inside = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
            .zip(wrap.get_untracked())
            .is_some_and(|(t, w)| w.contains(Some(&t)));
        if !inside {
            open.set(false);
        }
    });
    on_cleanup(move || outside.remove());

    // À l'ouverture, le focus va au premier élément actif.
    Effect::new(move |_| {
        if open.get() {
            if let Some(m) = menu.get() {
                move_focus(&m, 1);
            }
        }
    });

    let close = move || {
        open.set(false);
        if let Some(b) = button.get_untracked() {
            let _ = b.focus();
        }
    };

    let on_menu_key = move |ev: leptos::ev::KeyboardEvent| {
        let Some(m) = menu.get_untracked() else { return };
        match ev.key().as_str() {
            "ArrowDown" => {
                ev.prevent_default();
                move_focus(&m, 1);
            }
            "ArrowUp" => {
                ev.prevent_default();
                move_focus(&m, -1);
            }
            "Home" => {
                ev.prevent_default();
                if let Some(first) = items(&m).first() {
                    let _ = first.focus();
                }
            }
            "End" => {
                ev.prevent_default();
                if let Some(last) = items(&m).last() {
                    let _ = last.focus();
                }
            }
            "Escape" => {
                ev.prevent_default();
                ev.stop_propagation();
                close();
            }
            "Tab" => open.set(false),
            _ => {}
        }
    };

    let tooltip = if title.is_empty() { label } else { title };

    view! {
        <div class="menu-wrap" node_ref=wrap>
            <button
                class=class
                node_ref=button
                aria-haspopup="menu"
                aria-expanded=move || open.get().to_string()
                aria-label=label
                title=tooltip
                on:click=move |_| open.update(|o| *o = !*o)
                on:keydown=move |ev| {
                    if ev.key() == "ArrowDown" {
                        ev.prevent_default();
                        open.set(true);
                    }
                }
            >
                <Svg icon=icon />
                {show_label.then(|| view! { <span>{label}</span> })}
                <Svg icon=CHEVRON_DOWN size=12 />
            </button>
            <Show when=move || open.get()>
                <div class="menu" role="menu" aria-label=label node_ref=menu on:keydown=on_menu_key>
                    {move || {
                        entries
                            .get()
                            .into_iter()
                            .map(|entry| {
                                let disabled = entry.on_select.is_none();
                                let role = if entry.checked.is_some() { "menuitemradio" } else { "menuitem" };
                                let checked = entry.checked.unwrap_or(false);
                                let on_select = entry.on_select;
                                view! {
                                    <button
                                        role=role
                                        tabindex="-1"
                                        aria-checked=entry.checked.map(|c| c.to_string())
                                        aria-disabled=disabled.then_some("true")
                                        on:click=move |_| {
                                            if let Some(cb) = on_select {
                                                cb.run(());
                                                close();
                                            }
                                        }
                                    >
                                        <span class="check">{checked.then(|| view! { <Svg icon=crate::icons::CHECK size=14 /> })}</span>
                                        {entry.label}
                                    </button>
                                }
                            })
                            .collect_view()
                    }}
                </div>
            </Show>
        </div>
    }
}
