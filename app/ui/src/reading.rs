//! Volet de lecture. J0 : en-tête et corps en texte brut ; le rendu HTML isolé arrive au J2.

use leptos::{prelude::*, task::spawn_local};
use thunderlook_model::{MessageBody, initials};

use crate::{icons::*, ipc, state::use_ui};

#[component]
pub fn ReadingPane() -> impl IntoView {
    let ui = use_ui();
    let body = RwSignal::new(None::<MessageBody>);

    // Chargement du corps à la demande ; une réponse tardive pour un autre message est ignorée.
    Effect::new(move |_| {
        let id = ui.selected.get();
        body.set(None);
        if let Some(id) = id {
            spawn_local(async move {
                match ipc::message_body(&id).await {
                    Ok(b) if ui.selected.get_untracked().as_deref() == Some(b.id.as_str()) => body.set(Some(b)),
                    Ok(_) => {}
                    Err(e) => ui.error.set(Some(e)),
                }
            });
        }
    });

    let current = Memo::new(move |_| {
        let id = ui.selected.get()?;
        ui.snapshot.with(|s| s.as_ref()?.messages.iter().find(|m| m.id == id).cloned())
    });

    let action = |label: &'static str, icon, shortcut: &'static str| {
        view! {
            <button class="cmd" aria-disabled="true" tabindex="-1" title=format!("{label} ({shortcut}) — pas encore disponible")>
                <Svg icon=icon />
                <span>{label}</span>
            </button>
        }
    };

    view! {
        <section class="pane reading" aria-label="Volet de lecture" tabindex="-1" node_ref=ui.refs.reading>
            {move || match current.get() {
                None => view! { <p class="reading-empty">"Sélectionnez un élément à lire."</p> }.into_any(),
                Some(m) => view! {
                    <header class="reading-head">
                        <div class="reading-actions" role="toolbar" aria-label="Actions sur le message">
                            {action("Répondre", REPLY, "Ctrl+R")}
                            {action("Répondre à tous", REPLY_ALL, "Ctrl+Maj+R")}
                            {action("Transférer", FORWARD, "Ctrl+F")}
                        </div>
                        <h1 class="reading-subject">{m.subject.clone()}</h1>
                        <div class="reading-who">
                            <span class="avatar" aria-hidden="true">{initials(&m.from)}</span>
                            <div>
                                <div class="reading-from">{format!("{} <{}>", m.from, m.from_address)}</div>
                                <div class="reading-to">{format!("À : {}", m.to)}</div>
                            </div>
                            <div class="reading-date">{m.date_label.clone()}</div>
                        </div>
                    </header>
                    <div class="reading-body">
                        {move || match body.get() {
                            Some(b) => b.paragraphs.into_iter().map(|p| view! { <p>{p}</p> }).collect_view().into_any(),
                            None => view! { <p class="reading-empty">"Chargement…"</p> }.into_any(),
                        }}
                    </div>
                }
                .into_any(),
            }}
        </section>
    }
}
