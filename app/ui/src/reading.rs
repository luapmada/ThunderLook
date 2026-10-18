//! Volet de lecture : en-têtes, barres d'information, pièces jointes et corps.
//!
//! Le HTML s'affiche dans un `iframe` isolé (`sandbox` sans `allow-scripts`, donc aucun script ne s'exécute ;
//! sa politique CSP bloque le contenu distant). `allow-same-origin` sert seulement au cœur de l'interface :
//! ajuster la hauteur du cadre à son contenu et relayer les raccourcis clavier tapés dans le message.

use leptos::{prelude::*, task::spawn_local};
use thunderlook_model::{BodyView, FlagState, Importance, MessageView, initials};
use wasm_bindgen::{JsCast, closure::Closure};

use crate::{app::global_key, icons::*, ipc, state::use_ui};

/// Ajuste la hauteur de l'iframe à son document et y relaie les raccourcis globaux.
fn wire_frame(frame: &web_sys::HtmlIFrameElement, relay: &js_sys::Function) {
    let Some(doc) = frame.content_document() else { return };
    if let Some(root) = doc.document_element() {
        let h = root.scroll_height().max(doc.body().map(|b| b.scroll_height()).unwrap_or(0));
        let _ = frame.set_attribute("style", &format!("height: {}px", h + 4));
    }
    let _ = doc.add_event_listener_with_callback("keydown", relay);
}

#[component]
pub fn ReadingPane() -> impl IntoView {
    let ui = use_ui();
    let message = RwSignal::new(None::<MessageView>);
    let loading = RwSignal::new(false);
    let frame = NodeRef::<leptos::html::Iframe>::new();
    // Une seule fermeture pour tous les messages affichés.
    let relay = StoredValue::new_local(Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
        global_key(ui, &ev);
    }));

    // Chargement à la sélection ; une réponse tardive pour un autre message est ignorée.
    Effect::new(move |_| {
        let id = ui.selected.get();
        let allow = id.as_ref().is_some_and(|id| ui.remote_allowed.with(|r| r.contains(id)));
        let Some(id) = id else {
            message.set(None);
            return;
        };
        loading.set(true);
        spawn_local(async move {
            let result = ipc::message_view(&id, allow).await;
            if ui.selected.get_untracked().as_deref() != Some(id.as_str()) {
                return;
            }
            loading.set(false);
            match result {
                Ok(v) => message.set(Some(v)),
                Err(e) => {
                    message.set(None);
                    ui.error.set(Some(e));
                }
            }
        });
    });

    let action = |label: &'static str, icon, shortcut: &'static str| {
        view! {
            <button class="cmd" aria-disabled="true" tabindex="-1" title=format!("{label} ({shortcut}) — prévu au jalon J5")>
                <Svg icon=icon />
                <span>{label}</span>
            </button>
        }
    };

    let open_att = move |id: String, index: u32| {
        spawn_local(async move {
            if let Err(e) = ipc::open_attachment(&id, index).await {
                ui.error.set(Some(e));
            }
        })
    };
    let save_att = move |id: String, index: u32| {
        spawn_local(async move {
            if let Err(e) = ipc::save_attachment(&id, index).await {
                ui.error.set(Some(e));
            }
        })
    };

    view! {
        <section class="pane reading" aria-label="Volet de lecture" tabindex="-1" node_ref=ui.refs.reading>
            {move || match message.get() {
                None if ui.selected.get().is_none() => view! { <p class="reading-empty">"Sélectionnez un élément à lire."</p> }.into_any(),
                None => view! { <p class="reading-empty">{move || if loading.get() { "Chargement…" } else { "" }}</p> }.into_any(),
                Some(m) => {
                    let id = m.id.clone();
                    let allow_id = m.id.clone();
                    view! {
                        <header class="reading-head">
                            <div class="reading-actions" role="toolbar" aria-label="Actions sur le message">
                                {action("Répondre", REPLY, "Ctrl+R")}
                                {action("Répondre à tous", REPLY_ALL, "Ctrl+Maj+R")}
                                {action("Transférer", FORWARD, "Ctrl+F")}
                            </div>
                            <h1 class="reading-subject">{if m.subject.trim().is_empty() { "(aucun objet)".to_string() } else { m.subject.clone() }}</h1>
                            <div class="reading-who">
                                <span class="avatar" aria-hidden="true">{initials(&m.from)}</span>
                                <div class="reading-people">
                                    <div class="reading-from">
                                        {if m.from_address.is_empty() || m.from_address == m.from {
                                            m.from.clone()
                                        } else {
                                            format!("{} <{}>", m.from, m.from_address)
                                        }}
                                    </div>
                                    <div class="reading-to">{format!("À : {}", m.to)}</div>
                                    {(!m.cc.trim().is_empty()).then(|| view! { <div class="reading-to">{format!("Cc : {}", m.cc)}</div> })}
                                </div>
                                <div class="reading-date">{m.date_label.clone()}</div>
                            </div>
                            {(!m.categories.is_empty()).then(|| view! {
                                <ul class="reading-cats" aria-label="Catégories">
                                    {m.categories.iter().map(|c| {
                                        let style = c.color.as_ref().map(|col| format!("background:{col}")).unwrap_or_default();
                                        view! { <li><span class="cat" class:nocolor=c.color.is_none() style=style></span>{c.name.clone()}</li> }
                                    }).collect_view()}
                                </ul>
                            })}
                            {m.item_kind.clone().map(|k| view! {
                                <p class="infobar" role="note"><Svg icon=INFO size=14 />{format!("Élément de type « {k} » : affichage complet prévu avec le module correspondant.")}</p>
                            })}
                            {(m.importance == Importance::High).then(|| view! {
                                <p class="infobar" role="note"><Svg icon=CIRCLE_ALERT size=14 />"Ce message a une importance haute."</p>
                            })}
                            {m.flag_label.clone().map(|f| view! {
                                <p class="infobar" class:done=m.flag == FlagState::Done role="note">
                                    {if m.flag == FlagState::Done { view! { <Svg icon=CHECK size=14 /> } } else { view! { <Svg icon=FLAG size=14 /> } }}
                                    {f}
                                </p>
                            })}
                            {m.remote_blocked.then(|| view! {
                                <p class="infobar" role="note">
                                    <Svg icon=IMAGE_OFF size=14 />
                                    "Le contenu distant de ce message a été bloqué pour protéger votre vie privée."
                                    <button class="link-btn" on:click=move |_| {
                                        let id = allow_id.clone();
                                        ui.remote_allowed.update(|r| {
                                            r.insert(id);
                                        });
                                    }>"Télécharger les images"</button>
                                </p>
                            })}
                            {(!m.attachments.is_empty()).then(|| {
                                let atts = m.attachments.clone();
                                let label = format!("Pièces jointes : {}", atts.len());
                                view! {
                                    <ul class="attachments" aria-label=label>
                                        {atts.into_iter().map(|a| {
                                            let (oid, sid) = (id.clone(), id.clone());
                                            let index = a.index;
                                            let title = if a.blocked {
                                                format!("{} — type bloqué : enregistrer seulement", a.name)
                                            } else {
                                                format!("Ouvrir {}", a.name)
                                            };
                                            view! {
                                                <li class="attachment" class:blocked=a.blocked>
                                                    <button class="att-open" title=title on:click=move |_| open_att(oid.clone(), index)>
                                                        {if a.blocked { view! { <Svg icon=SHIELD_ALERT size=16 /> } } else if a.message { view! { <Svg icon=MAIL size=16 /> } } else { view! { <Svg icon=FILE size=16 /> } }}
                                                        <span class="att-name">{a.name.clone()}</span>
                                                        <span class="att-size">{a.size_label.clone()}</span>
                                                    </button>
                                                    <button class="icon-btn" aria-label=format!("Enregistrer {} sous", a.name) title="Enregistrer sous" on:click=move |_| save_att(sid.clone(), index)>
                                                        <Svg icon=SAVE size=14 />
                                                    </button>
                                                </li>
                                            }
                                        }).collect_view()}
                                    </ul>
                                }
                            })}
                        </header>
                        <div class="reading-body">
                            {match m.body {
                                BodyView::Html { srcdoc } => view! {
                                    <iframe
                                        class="mail-frame"
                                        title="Contenu du message"
                                        sandbox="allow-same-origin allow-popups allow-popups-to-escape-sandbox"
                                        referrerpolicy="no-referrer"
                                        srcdoc=srcdoc
                                        node_ref=frame
                                        on:load=move |_| {
                                            if let Some(f) = frame.get_untracked() {
                                                relay.with_value(|r| wire_frame(&f, r.as_ref().unchecked_ref()));
                                            }
                                        }
                                    ></iframe>
                                }
                                .into_any(),
                                BodyView::Text { text } => view! { <div class="reading-text">{text}</div> }.into_any(),
                                BodyView::Empty => view! { <p class="reading-empty">"(Aucun contenu)"</p> }.into_any(),
                            }}
                        </div>
                    }
                    .into_any()
                }
            }}
        </section>
    }
}
