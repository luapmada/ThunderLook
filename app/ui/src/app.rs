//! Racine de l'interface : état partagé, chargement, raccourcis globaux, disposition des volets.

use leptos::{prelude::*, task::spawn_local};
use thunderlook_model::settings::{Density, ReadingPane, Settings};
use wasm_bindgen::JsCast;

use crate::{
    commandbar::CommandBar,
    folders::FolderPane,
    ipc,
    list::MessageList,
    reading::ReadingPane as ReadingPaneView,
    splitter::Splitter,
    state::{Module, Ui, use_ui},
    statusbar::StatusBar,
    topbar::TopBar,
};

/// Raccourcis réservés à l'application : on empêche WebView2 de les traiter
/// (rechargement, recherche dans la page, impression…). Ils seront câblés aux jalons suivants.
fn reserved(ev: &web_sys::KeyboardEvent) -> bool {
    let key = ev.key().to_lowercase();
    let ctrl = ev.ctrl_key();
    matches!(key.as_str(), "f5" | "f9" | "f7" | "f3" | "f1" | "f12")
        || (ctrl && matches!(key.as_str(), "r" | "f" | "n" | "p" | "u" | "q" | "s" | "m" | "g" | "j" | "h" | "o" | "t" | "k" | "e" | "y" | "d" | "l" | "b" | "+" | "-" | "=" | "0"))
}

/// Cycle F6 / Maj+F6 : recherche → onglets → dossiers → liste → lecture.
fn cycle_focus(ui: Ui, backward: bool) {
    let r = ui.refs;
    let targets: Vec<web_sys::HtmlElement> = [
        r.search.get_untracked().map(|e| e.unchecked_into()),
        document().get_element_by_id(ui.tab.get_untracked().dom_id()).and_then(|e| e.dyn_into().ok()),
        r.tree.get_untracked().map(|e| e.unchecked_into()),
        r.list.get_untracked().map(|e| e.unchecked_into()),
        r.reading.get_untracked().map(|e| e.unchecked_into()),
    ]
    .into_iter()
    .flatten()
    .collect();
    if targets.is_empty() {
        return;
    }
    let active = document().active_element();
    let current = active.and_then(|a| targets.iter().position(|t| t.contains(Some(&a))));
    let n = targets.len() as i32;
    let next = match (current, backward) {
        (Some(i), false) => (i as i32 + 1).rem_euclid(n),
        (Some(i), true) => (i as i32 - 1).rem_euclid(n),
        (None, false) => 0,
        (None, true) => n - 1,
    };
    let _ = targets[next as usize].focus();
}

#[component]
pub fn App() -> impl IntoView {
    let ui = Ui::new();
    provide_context(ui);

    // Chargement initial : réglages puis instantané du courrier.
    spawn_local(async move {
        match ipc::load_settings().await {
            Ok(s) => ui.settings.set(s.clamped()),
            Err(e) => ui.error.set(Some(e)),
        }
        ui.settings_loaded.set(true);
        match ipc::mail_snapshot().await {
            Ok(s) => ui.snapshot.set(Some(s)),
            Err(e) => ui.error.set(Some(e)),
        }
    });

    // Enregistrement des réglages à chaque modification, une fois chargés.
    Effect::new(move |_| {
        let s: Settings = ui.settings.get();
        if ui.settings_loaded.get_untracked() {
            spawn_local(async move {
                if let Err(e) = ipc::save_settings(s).await {
                    ui.error.set(Some(e));
                }
            });
        }
    });

    // Densité appliquée à la racine du document (variables CSS).
    Effect::new(move |_| {
        let compact = ui.settings.get().density == Density::Compact;
        if let Some(root) = document().document_element() {
            let _ = if compact { root.set_attribute("data-density", "compact") } else { root.remove_attribute("data-density") };
        }
    });

    let keys = window_event_listener(leptos::ev::keydown, move |ev| {
        let key = ev.key();
        let (ctrl, alt, shift) = (ev.ctrl_key(), ev.alt_key(), ev.shift_key());
        let module = match (ctrl, alt, key.as_str()) {
            (true, false, "1") => Some(Module::Mail),
            (true, false, "2") => Some(Module::Calendar),
            (true, false, "3") => Some(Module::Contacts),
            (true, false, "4") => Some(Module::Tasks),
            _ => None,
        };
        if let Some(m) = module {
            ev.prevent_default();
            ui.module.set(m);
            return;
        }
        if key == "F6" {
            ev.prevent_default();
            cycle_focus(ui, shift);
            return;
        }
        if (ctrl && !alt && key.eq_ignore_ascii_case("e")) || key == "F3" {
            ev.prevent_default();
            if let Some(s) = ui.refs.search.get_untracked() {
                let _ = s.focus();
                s.select();
            }
            return;
        }
        if key == "Escape" && ui.bar_popup.get_untracked() {
            ui.bar_popup.set(false);
            return;
        }
        if reserved(&ev) {
            ev.prevent_default();
        }
    });
    on_cleanup(move || keys.remove());

    view! {
        <div class="app">
            <TopBar />
            <CommandBar />
            <Main />
            <StatusBar />
        </div>
    }
}

#[component]
fn Main() -> impl IntoView {
    let ui = use_ui();
    let s = move || ui.settings.get();

    let set_folder_w = Callback::new(move |w| ui.update_settings(|s| s.folder_pane_width = w));
    let set_list_w = Callback::new(move |w| ui.update_settings(|s| s.list_width = w));
    let set_list_h = Callback::new(move |h| ui.update_settings(|s| s.list_height = h));

    let columns = move || {
        let st = s();
        let folder = if st.folder_pane_collapsed { "44px".to_string() } else { format!("{}px", st.folder_pane_width) };
        let mail = ui.module.get() == Module::Mail;
        let splitter = if st.folder_pane_collapsed { "0px" } else { "1px" };
        match (mail, st.reading_pane) {
            (true, ReadingPane::Right) => format!("grid-template-columns: {folder} {splitter} {}px 1px minmax(0, 1fr)", st.list_width),
            _ => format!("grid-template-columns: {folder} {splitter} minmax(0, 1fr)"),
        }
    };

    view! {
        <main class="main" style=columns on:pointerdown=move |_| ui.bar_popup.set(false)>
            <FolderPane />
            <Show when=move || !s().folder_pane_collapsed fallback=|| view! { <div></div> }>
                <Splitter
                    label="Redimensionner le volet des dossiers"
                    vertical=true
                    value=Signal::derive(move || s().folder_pane_width)
                    min=Settings::FOLDER_MIN
                    max=Settings::FOLDER_MAX
                    on_change=set_folder_w
                />
            </Show>
            {move || match (ui.module.get(), s().reading_pane) {
                (Module::Mail, ReadingPane::Right) => view! {
                    <MessageList />
                    <Splitter
                        label="Redimensionner la liste des messages"
                        vertical=true
                        value=Signal::derive(move || s().list_width)
                        min=Settings::LIST_MIN
                        max=Settings::LIST_MAX
                        on_change=set_list_w
                    />
                    <ReadingPaneView />
                }
                .into_any(),
                (Module::Mail, ReadingPane::Bottom) => view! {
                    <div class="stack" style=move || format!("grid-template-rows: {}px 1px minmax(0, 1fr)", s().list_height)>
                        <MessageList />
                        <Splitter
                            label="Redimensionner la liste des messages"
                            vertical=false
                            value=Signal::derive(move || s().list_height)
                            min=160
                            max=900
                            on_change=set_list_h
                        />
                        <ReadingPaneView />
                    </div>
                }
                .into_any(),
                (Module::Mail, ReadingPane::Off) => view! { <MessageList /> }.into_any(),
                (other, _) => view! {
                    <section class="pane placeholder" aria-label=other.label()>
                        <div>
                            <p><strong>{other.label()}</strong></p>
                            <p>"Ce module viendra après le courrier (jalons J9 et suivants)."</p>
                        </div>
                    </section>
                }
                .into_any(),
            }}
        </main>
    }
}
