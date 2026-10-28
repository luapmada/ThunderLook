//! Bouton à menu déroulant accessible (role="menu"), au clavier comme à la souris.

use icondata_core::Icon;
use leptos::{html, prelude::*};
use wasm_bindgen::JsCast;

use crate::icons::{CHEVRON_DOWN, Svg};

#[derive(Clone)]
pub struct MenuEntry {
    pub label: String,
    /// `Some(_)` : élément à cocher (menuitemradio).
    pub checked: Option<bool>,
    pub on_select: Option<Callback<()>>,
    /// Sous-menu (menus contextuels) : remplace le menu courant, avec un retour.
    pub children: Vec<MenuEntry>,
    /// Pastille de couleur (catégories).
    pub swatch: Option<String>,
    /// Trait de séparation au-dessus.
    pub separator: bool,
}

impl MenuEntry {
    fn new(label: impl Into<String>, checked: Option<bool>, on_select: Option<Callback<()>>) -> Self {
        Self { label: label.into(), checked, on_select, children: Vec::new(), swatch: None, separator: false }
    }

    pub fn action(label: impl Into<String>, on_select: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(label, None, Some(Callback::new(move |_| on_select())))
    }

    pub fn radio(label: impl Into<String>, checked: bool, on_select: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(label, Some(checked), Some(Callback::new(move |_| on_select())))
    }

    /// Commande prévue mais pas encore disponible (affichée désactivée).
    pub fn pending(label: impl Into<String>) -> Self {
        Self::new(label, None, None)
    }

    pub fn submenu(label: impl Into<String>, children: Vec<MenuEntry>) -> Self {
        Self { children, ..Self::new(label, None, None) }
    }

    pub fn with_swatch(mut self, color: Option<String>) -> Self {
        self.swatch = Some(color.unwrap_or_default());
        self
    }

    pub fn separated(mut self) -> Self {
        self.separator = true;
        self
    }

    pub fn is_enabled(&self) -> bool {
        self.on_select.is_some() || !self.children.is_empty()
    }
}

/// Contenu d'un élément de menu : coche, pastille, libellé.
pub fn entry_body(entry: &MenuEntry) -> impl IntoView + use<> {
    let checked = entry.checked.unwrap_or(false);
    let swatch = entry.swatch.clone().map(|c| {
        let style = if c.is_empty() { String::new() } else { format!("background:{c}") };
        let empty = c.is_empty();
        view! { <span class="cat" class:nocolor=empty style=style></span> }
    });
    view! {
        <span class="check">{checked.then(|| view! { <Svg icon=crate::icons::CHECK size=14 /> })}</span>
        {swatch}
        <span class="menu-label">{entry.label.clone()}</span>
        {(!entry.children.is_empty()).then(|| view! { <Svg icon=crate::icons::CHEVRON_RIGHT size=12 /> })}
    }
}

/// Ce qu'un menu affiche (libellés, coches, pastilles, état, séparateurs, sous-menus). Un menu ouvert n'est reconstruit
/// que quand cela change : sinon un rafraîchissement de l'état (synchro IDLE, arborescence relue) recréait ses boutons
/// et le focus clavier tombait sur la page.
pub fn shape(entries: &[MenuEntry]) -> String {
    let mut out = String::new();
    for e in entries {
        out.push_str(&format!(
            "{}|{:?}|{}|{}|{:?}[{}]\u{1e}",
            e.label,
            e.checked,
            e.is_enabled(),
            e.separator,
            e.swatch,
            shape(&e.children)
        ));
    }
    out
}

pub fn items(menu: &web_sys::HtmlDivElement) -> Vec<web_sys::HtmlElement> {
    let Ok(list) = menu.query_selector_all("[role^=menuitem]:not([aria-disabled=true])") else { return vec![] };
    (0..list.length()).filter_map(|i| list.item(i)?.dyn_into::<web_sys::HtmlElement>().ok()).collect()
}

pub fn move_focus(menu: &web_sys::HtmlDivElement, step: i32) {
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
    #[prop(into)] label: Signal<String>,
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
    let drawn = Memo::new(move |_| entries.with(|e| shape(e)));

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

    let tooltip = move || if title.is_empty() { label.get() } else { title.to_string() };

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
                {show_label.then(|| view! { <span>{move || label.get()}</span> })}
                <Svg icon=CHEVRON_DOWN size=12 />
            </button>
            <Show when=move || open.get()>
                <div class="menu" role="menu" aria-label=label node_ref=menu on:keydown=on_menu_key>
                    {move || {
                        drawn.track();
                        entries
                            .get_untracked()
                            .into_iter()
                            .map(|entry| {
                                let disabled = entry.on_select.is_none();
                                let role = if entry.checked.is_some() { "menuitemradio" } else { "menuitem" };
                                let on_select = entry.on_select;
                                let body = entry_body(&entry);
                                view! {
                                    {entry.separator.then(|| view! { <div class="menu-sep" role="separator"></div> })}
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
                                        {body}
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forme_du_menu() {
        let a = vec![MenuEntry::pending("Trier"), MenuEntry::submenu("Déplacer", vec![MenuEntry::pending("Boîte de réception")]).separated()];
        let b = vec![MenuEntry::pending("Trier"), MenuEntry::submenu("Déplacer", vec![MenuEntry::pending("Boîte de réception")]).separated()];
        // Mêmes entrées recalculées : pas de reconstruction.
        assert_eq!(shape(&a), shape(&b));
        let c = vec![MenuEntry::pending("Trier"), MenuEntry::submenu("Déplacer", vec![MenuEntry::pending("Archives")]).separated()];
        assert_ne!(shape(&a), shape(&c));
        assert_ne!(shape(&a), shape(&a[..1]));
    }
}
