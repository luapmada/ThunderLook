//! Liste des messages : pivots Tous / Non lus, groupes par date repliables, sélection au clavier.

use leptos::prelude::*;
use thunderlook_model::{DateGroup, MessageSummary};

use crate::{icons::*, state::use_ui};

fn msg_dom_id(id: &str) -> String {
    format!("msg-{id}")
}

#[component]
pub fn MessageList() -> impl IntoView {
    let ui = use_ui();
    let messages = Memo::new(move |_| ui.visible_messages());

    // Groupes dans l'ordre d'affichage, avec leurs messages.
    let grouped = Memo::new(move |_| {
        let mut groups: Vec<(DateGroup, Vec<MessageSummary>)> = Vec::new();
        for m in messages.get() {
            match groups.iter_mut().find(|(g, _)| *g == m.group) {
                Some((_, v)) => v.push(m),
                None => groups.push((m.group, vec![m])),
            }
        }
        groups.sort_by_key(|(g, _)| *g);
        groups
    });

    // Messages atteignables au clavier (groupes dépliés seulement).
    let navigable = Memo::new(move |_| {
        let collapsed = ui.collapsed_groups.get();
        grouped
            .get()
            .into_iter()
            .filter(|(g, _)| !collapsed.contains(g))
            .flat_map(|(_, v)| v.into_iter().map(|m| m.id))
            .collect::<Vec<_>>()
    });

    // Sélection par défaut : le plus récent, à l'ouverture d'un dossier.
    Effect::new(move |_| {
        let ids = navigable.get();
        let current = ui.selected.get_untracked();
        if current.as_ref().is_none_or(|c| !ids.contains(c)) {
            ui.selected.set(ids.first().cloned());
        }
    });

    let select = move |id: String| {
        if let Some(el) = document().get_element_by_id(&msg_dom_id(&id)) {
            el.scroll_into_view_with_bool(false);
        }
        ui.selected.set(Some(id));
    };

    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        let ids = navigable.get_untracked();
        if ids.is_empty() {
            return;
        }
        let pos = ui.selected.get_untracked().and_then(|s| ids.iter().position(|i| *i == s)).unwrap_or(0);
        let last = ids.len() - 1;
        let target = match ev.key().as_str() {
            "ArrowDown" => (pos + 1).min(last),
            "ArrowUp" => pos.saturating_sub(1),
            "Home" => 0,
            "End" => last,
            "PageDown" => (pos + 10).min(last),
            "PageUp" => pos.saturating_sub(10),
            _ => return,
        };
        ev.prevent_default();
        select(ids[target].clone());
    };

    let folder_name = move || {
        let id = ui.folder.get();
        ui.snapshot.with(|s| s.as_ref().and_then(|s| s.find_folder(&id)).map(|f| f.name.clone()).unwrap_or_default())
    };

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
                <button class="cmd sort" aria-disabled="true" title="Trier par — pas encore disponible" tabindex="-1">
                    "Par date"
                    <Svg icon=CHEVRON_DOWN size=12 />
                </button>
                <button class="icon-btn" aria-disabled="true" aria-label="Inverser le tri" title="Inverser le tri — pas encore disponible" tabindex="-1">
                    <Svg icon=ARROW_UP />
                </button>
            </div>
            <div
                class="listbox"
                role="listbox"
                tabindex="0"
                aria-label=move || format!("Messages — {}", folder_name())
                node_ref=ui.refs.list
                aria-activedescendant=move || ui.selected.get().map(|s| msg_dom_id(&s))
                on:keydown=on_key
            >
                <Show when=move || messages.with(|m| m.is_empty())>
                    <p class="list-empty">"Aucun élément à afficher."</p>
                </Show>
                {move || {
                    let collapsed = ui.collapsed_groups.get();
                    grouped
                        .get()
                        .into_iter()
                        .map(|(group, items)| {
                            let open = !collapsed.contains(&group);
                            let head_id = format!("grp-{group:?}");
                            let labelled_by = head_id.clone();
                            view! {
                                <div role="group" aria-labelledby=labelled_by>
                                    <button
                                        class="group-head"
                                        id=head_id
                                        tabindex="-1"
                                        aria-expanded=open.to_string()
                                        on:click=move |_| {
                                            ui.collapsed_groups.update(|c| {
                                                if !c.remove(&group) {
                                                    c.insert(group);
                                                }
                                            })
                                        }
                                    >
                                        {if open { view! { <Svg icon=CHEVRON_DOWN size=14 /> } } else { view! { <Svg icon=CHEVRON_RIGHT size=14 /> } }}
                                        {group.label()}
                                        <span class="group-count">{items.len()}</span>
                                    </button>
                                    {open.then(|| items.into_iter().map(|m| view! { <Row m=m /> }).collect_view())}
                                </div>
                            }
                        })
                        .collect_view()
                }}
            </div>
        </section>
    }
}

#[component]
fn Row(m: MessageSummary) -> impl IntoView {
    let ui = use_ui();
    let id = m.id.clone();
    let selected = {
        let id = id.clone();
        move || ui.selected.get().as_deref() == Some(id.as_str())
    };
    let status = [
        m.unread.then_some("non lu"),
        m.flagged.then_some("avec suivi"),
        m.has_attachments.then_some("pièce jointe"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ");
    let label = format!("{}, {}, {}{}", m.from, m.subject, m.time_label, if status.is_empty() { String::new() } else { format!(", {status}") });
    let click_id = id.clone();
    view! {
        <div
            role="option"
            id=msg_dom_id(&id)
            class="msg"
            class:unread=m.unread
            class:focused=selected.clone()
            aria-selected=move || selected().to_string()
            aria-label=label
            on:click=move |_| {
                ui.selected.set(Some(click_id.clone()));
                if let Some(l) = ui.refs.list.get_untracked() {
                    let _ = l.focus();
                }
            }
        >
            <span class="from">{m.from.clone()}</span>
            <span class="meta">
                {m.categories.iter().map(|c| view! { <span class="cat" style=format!("background:{}", c.css_var())></span> }).collect_view()}
                {m.has_attachments.then(|| view! { <Svg icon=PAPERCLIP size=14 /> })}
                {m.flagged.then(|| view! { <span class="flag"><Svg icon=FLAG size=14 /></span> })}
                <span class="time">{m.time_label.clone()}</span>
            </span>
            <span class="subject">{m.subject.clone()}</span>
            <span class="preview">{m.preview.clone()}</span>
        </div>
    }
}
