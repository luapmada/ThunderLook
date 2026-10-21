//! Racine de l'interface : état partagé, chargement, raccourcis globaux, disposition des volets.

use std::collections::HashSet;

use leptos::{prelude::*, task::spawn_local};
use thunderlook_model::{
    ComposeKind, FlagAction, ListQuery, MessageOp, SearchQuery, Selection,
    settings::{Density, ReadingPane, Settings},
};
use wasm_bindgen::JsCast;

use crate::{
    actions::{self, real_folder, select_single},
    commandbar::CommandBar,
    compose::ComposePane,
    accounts::AccountsDialog,
    autoreply::AutoReplyDialog,
    calendar::{self, CalendarPane},
    dialogs::Dialogs,
    quicksteps::QuickStepsDialog,
    rules::RulesDialog,
    menus::ContextMenu,
    folders::FolderPane,
    ipc,
    list::{Layout, MessageList, PAGE, ensure_rows, row_height},
    reading::ReadingPane as ReadingPaneView,
    splitter::Splitter,
    state::{Dialog, Module, Tab, Ui, use_ui},
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

/// Volet de droite (ou du bas) : la rédaction en cours, sinon le message sélectionné.
#[component]
fn ReadingOrCompose() -> impl IntoView {
    let ui = use_ui();
    let composing = Memo::new(move |_| ui.compose.with(Option::is_some));
    move || {
        ui.compose_epoch.track();
        if composing.get() { view! { <ComposePane /> }.into_any() } else { view! { <ReadingPaneView /> }.into_any() }
    }
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

/// Le focus est dans un champ de saisie (les raccourcis d'édition lui reviennent).
fn typing() -> bool {
    document().active_element().is_some_and(|a| matches!(a.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT"))
}

/// Raccourcis valables partout, y compris dans le corps d'un message (relayés depuis l'iframe).
pub fn global_key(ui: Ui, ev: &web_sys::KeyboardEvent) {
    let key = ev.key();
    let (ctrl, alt, shift) = (ev.ctrl_key(), ev.alt_key(), ev.shift_key());
    // Une boîte de dialogue ou un menu contextuel garde ses touches.
    if ui.dialog.get_untracked().is_some() || ui.context_menu.get_untracked().is_some() {
        return;
    }
    // Organisation (J3), hors saisie de texte.
    if !typing() && ui.module.get_untracked() == Module::Mail {
        let op = match (ctrl, alt, shift, key.as_str()) {
            (true, false, false, "q" | "Q") => Some(MessageOp::Read { read: true }),
            (true, false, false, "u" | "U") => Some(MessageOp::Read { read: false }),
            (false, false, false, "Insert") => Some(MessageOp::Flag { action: FlagAction::Toggle }),
            _ => None,
        };
        if let Some(op) = op {
            ev.prevent_default();
            actions::run(ui, op);
            return;
        }
        // Rédaction (J5).
        let kind = match (ctrl, alt, shift, key.to_ascii_lowercase().as_str()) {
            (true, false, false, "n") => Some(ComposeKind::New),
            (true, false, false, "r") => Some(ComposeKind::Reply),
            (true, false, true, "r") => Some(ComposeKind::ReplyAll),
            (true, false, false, "f") => Some(ComposeKind::Forward),
            _ => None,
        };
        if let Some(kind) = kind {
            ev.prevent_default();
            actions::compose(ui, kind);
            return;
        }
        if key == "F9" && !ctrl && !alt && !shift {
            ev.prevent_default();
            actions::sync_now(ui);
            return;
        }
        if ctrl && !alt && !shift && key.eq_ignore_ascii_case("z") {
            ev.prevent_default();
            actions::undo(ui);
            return;
        }
        if ctrl && shift && key.eq_ignore_ascii_case("e") {
            ev.prevent_default();
            ui.dialog.set(Some(Dialog::NewFolder { parent: real_folder(&ui.folder.get_untracked()) }));
            return;
        }
        // Suppr depuis le volet de lecture (la liste et l'arborescence ont leur propre Suppr).
        let in_reading = ui
            .refs
            .reading
            .get_untracked()
            .zip(document().active_element())
            .is_some_and(|(r, a)| r.contains(Some(&a)));
        if key == "Delete" && (in_reading || ev.target().is_some_and(|t| t.dyn_ref::<web_sys::Node>().is_some_and(|n| n.owner_document() != Some(document())))) {
            ev.prevent_default();
            actions::delete(ui, shift);
            return;
        }
    }
    if ui.module.get_untracked() == Module::Calendar && calendar::key(ui, ev) {
        ev.prevent_default();
        return;
    }
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
    // Élément suivant / précédent, d'où que vienne le focus.
    if ctrl && !alt && (key == "." || key == ",") {
        ev.prevent_default();
        let row_h = row_height(ui.settings.get_untracked().density);
        let collapsed = ui.collapsed_groups.get_untracked();
        let delta = if key == "." { 1 } else { -1 };
        let target = ui
            .view
            .with_untracked(|v| v.as_ref().and_then(|v| Layout::new(v, collapsed, row_h).step(ui.selected_pos.get_untracked(), delta)));
        if let Some(p) = target {
            select_single(ui, p);
        }
        return;
    }
    if key == "Escape" && ui.bar_popup.get_untracked() {
        ui.bar_popup.set(false);
        return;
    }
    if reserved(ev) {
        ev.prevent_default();
    }
}

fn now_ms() -> f64 {
    window().performance().map(|p| p.now()).unwrap_or_default()
}

#[component]
pub fn App() -> impl IntoView {
    let ui = Ui::new();
    provide_context(ui);
    let requested = StoredValue::new(HashSet::<(u64, u32)>::new());
    provide_context(requested);

    actions::load_categories(ui);
    calendar::watch(ui);

    // Envoi et réception (J7) : barre d'état, puis liste et arborescence rafraîchies à la fin d'un passage.
    ipc::on_sync(move |e| {
        ui.status.set(Some(e.message.clone()));
        if !e.running {
            if e.error.is_some() {
                ui.error.set(e.error.clone());
            }
            if e.sent > 0 || e.received > 0 {
                actions::refresh_tree(ui);
                actions::reopen_view(ui);
            }
        }
    });

    // Chargement initial : réglages puis arborescence ; ouverture de la Boîte de réception.
    spawn_local(async move {
        match ipc::load_settings().await {
            Ok(s) => ui.settings.set(s.clamped()),
            Err(e) => ui.error.set(Some(e)),
        }
        ui.settings_loaded.set(true);
        match ipc::load_quick_steps().await {
            Ok(q) => ui.quick_steps.set(q),
            Err(e) => ui.error.set(Some(e)),
        }
        match ipc::folder_tree().await {
            Ok(t) => {
                if let Some(e) = &t.error {
                    ui.error.set(Some(e.clone()));
                }
                if let Some(f) = t.default_folder() {
                    ui.folder.set(f.id.clone());
                    ui.tree_focus.set(f.id.clone());
                }
                ui.tree.set(Some(t));
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });

    // Seul ce réglage rouvre la vue (pas la largeur des volets, par exemple).
    let conversations_on = Memo::new(move |_| ui.settings.with(|s| s.conversations));

    // Ouverture d'une vue à chaque changement de dossier, de filtre ou de tri. La première tranche
    // est demandée tout de suite ; la mesure couvre les deux allers-retours et l'affichage.
    Effect::new(move |_| {
        let folder = ui.folder.get();
        let unread_only = ui.unread_only.get();
        let (sort, descending) = ui.sort.get();
        let conversations = conversations_on.get();
        ui.view_epoch.track();
        let search_text = ui.search_text.get();
        let scope = ui.search_scope.get();
        if folder.is_empty() {
            return;
        }
        let search = (!search_text.trim().is_empty()).then(|| SearchQuery { text: search_text.clone(), scope, folder_id: folder.clone() });
        let query = ListQuery { folder_id: folder, unread_only, sort, descending, conversations };
        let t0 = now_ms();
        spawn_local(async move {
            let q = query.clone();
            let s = search.clone();
            let current = move || {
                ui.folder.get_untracked() == q.folder_id
                    && ui.unread_only.get_untracked() == q.unread_only
                    && ui.sort.get_untracked() == (q.sort, q.descending)
                    && ui.settings.with_untracked(|s| s.conversations) == q.conversations
                    && ui.search_text.get_untracked() == s.as_ref().map(|s| s.text.clone()).unwrap_or_default()
                    && s.as_ref().is_none_or(|s| ui.search_scope.get_untracked() == s.scope)
            };
            let opened = match &search {
                Some(s) => ipc::search(s, query.sort, query.descending).await,
                None => ipc::open_list(&query).await,
            };
            let view = match opened {
                Ok(v) => v,
                Err(e) => {
                    ui.error.set(Some(e));
                    return;
                }
            };
            if !current() {
                return;
            }
            let first = match ipc::list_rows(view.token, 0, PAGE).await {
                Ok(r) => r,
                Err(e) => {
                    ui.error.set(Some(e));
                    Vec::new()
                }
            };
            if !current() {
                return;
            }
            let count = view.count;
            // Même dossier rouvert après une opération : groupes repliés et position gardés.
            let same = ui.view.with_untracked(|v| v.is_some()) && ui.pending_select.get_untracked().is_some();
            requested.set_value(HashSet::from([(view.token, 0)]));
            if !same {
                ui.collapsed_groups.set(HashSet::new());
            }
            ui.rows.set(first.into_iter().map(|m| (m.pos, m)).collect());
            ui.view.set(Some(view));
            let pos = ui.pending_select.get_untracked().map(|p| p.min(count.saturating_sub(1))).unwrap_or(0);
            ui.pending_select.set(None);
            if count > 0 {
                select_single(ui, pos);
            } else {
                ui.sel_ranges.set(Vec::new());
                ui.selected_pos.set(None);
            }
            request_animation_frame(move || {
                let ms = now_ms() - t0;
                let label = format!("ouverture {} : {count} éléments, tri {:?}, non lus {}", query.folder_id, query.sort, query.unread_only);
                spawn_local(async move { ipc::perf_mark(&label, ms).await });
            });
        });
    });

    // Changer de dossier ferme la recherche, comme la référence.
    Effect::new(move |prev: Option<String>| {
        let f = ui.folder.get();
        if prev.is_some_and(|p| p != f) && !ui.search_text.get_untracked().is_empty() {
            ui.search_text.set(String::new());
            if let Some(s) = ui.refs.search.get_untracked() {
                s.set_value("");
            }
        }
        f
    });

    // Onglet contextuel « Rechercher » pendant une recherche.
    Effect::new(move |_| {
        let active = !ui.search_text.get().is_empty();
        if active {
            ui.tab.set(Tab::Search);
        } else if ui.tab.get_untracked() == Tab::Search {
            ui.tab.set(Tab::Home);
        }
    });

    // Suivi de l'indexation (toutes les 2 s tant qu'elle tourne).
    let poll = StoredValue::new_local(None::<IntervalHandle>);
    let check_index = move || {
        spawn_local(async move {
            if let Ok(s) = ipc::index_status().await {
                let was = ui.index.get_untracked().running;
                ui.index.set(s);
                if !s.running && was {
                    // Fin de l'indexation : une recherche en cours est relancée sur l'index complet.
                    if !ui.search_text.get_untracked().is_empty() {
                        actions::reopen_view(ui);
                    }
                }
            }
        });
    };
    if let Ok(h) = set_interval_with_handle(check_index, std::time::Duration::from_secs(2)) {
        poll.set_value(Some(h));
    }
    on_cleanup(move || {
        poll.update_value(|h| {
            if let Some(h) = h.take() {
                h.clear();
            }
        })
    });

    // Identifiant du message sélectionné, dès que sa ligne est reçue (chargée au besoin).
    Effect::new(move |_| {
        let pos = ui.selected_pos.get();
        let id = pos.and_then(|p| ui.rows.with(|r| r.get(&p).map(|m| m.id.clone())));
        if let (Some(p), None) = (pos, &id) {
            ensure_rows(ui, [p], requested);
        }
        if ui.selected.get_untracked() != id {
            ui.selected.set(id);
        }
    });

    // Marquer comme lu l'élément que la sélection quitte (réglage `mark_read_on_leave`).
    Effect::new(move |prev: Option<Option<String>>| {
        let current = ui.selected.get();
        if let Some(Some(left)) = prev
            && Some(&left) != current.as_ref()
            && ui.settings.get_untracked().mark_read_on_leave
        {
            let unread = ui.rows.with_untracked(|r| r.values().find(|m| m.id == left).map(|m| (m.pos, m.unread)));
            if let Some((pos, true)) = unread {
                ui.rows.update(|r| {
                    if let Some(m) = r.get_mut(&pos) {
                        m.unread = false;
                    }
                });
                spawn_local(async move {
                    match ipc::apply_op(&Selection::ids(vec![left]), &MessageOp::Seen).await {
                        Ok(_) => actions::refresh_tree(ui),
                        Err(e) => ui.error.set(Some(e)),
                    }
                });
            }
        }
        current
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

    let keys = window_event_listener(leptos::ev::keydown, move |ev| global_key(ui, &ev));
    on_cleanup(move || keys.remove());

    view! {
        <div class="app">
            <TopBar />
            <CommandBar />
            <Main />
            <StatusBar />
            <ContextMenu />
            <Dialogs />
            <RulesDialog />
            <QuickStepsDialog />
            <AccountsDialog />
            <AutoReplyDialog />
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
                    <ReadingOrCompose />
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
                        <ReadingOrCompose />
                    </div>
                }
                .into_any(),
                (Module::Mail, ReadingPane::Off) => view! { <MessageList /> }.into_any(),
                (Module::Calendar, _) => view! { <CalendarPane /> }.into_any(),
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
