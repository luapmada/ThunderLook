//! Volet de lecture : en-têtes, barres d'information, pièces jointes et corps.
//!
//! Le HTML s'affiche dans un `iframe` isolé (`sandbox` sans `allow-scripts`, donc aucun script ne s'exécute ;
//! sa politique CSP bloque le contenu distant). `allow-same-origin` sert seulement au cœur de l'interface :
//! ajuster la hauteur du cadre à son contenu et relayer les raccourcis clavier tapés dans le message.

use leptos::{prelude::*, task::spawn_local};
use serde::{Deserialize, Serialize};
use thunderlook_model::{BodyView, ComposeKind, FlagState, Importance, MessageView, initials};
use wasm_bindgen::{JsCast, closure::Closure};

use crate::{actions, app::global_key, appointment, attach_preview, icons::*, ipc, state::use_ui};

/// Impression de la page entière (repli quand le corps n'est pas un cadre) : on ne garde que le volet de lecture.
const PRINT_CSS: &str = "@media print { body * { visibility: hidden !important; } \
.pane.reading, .pane.reading * { visibility: visible !important; } \
.pane.reading { position: absolute !important; left: 0; top: 0; width: 100% !important; height: auto !important; overflow: visible !important; } \
.reading-actions { display: none !important; } }";

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
    // Pièce jointe dont l'aperçu est ouvert : (message, index).
    let preview = RwSignal::new(None::<(String, u32)>);
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

    let action = move |label: &'static str, icon, shortcut: &'static str, kind: ComposeKind| {
        view! {
            <button class="cmd" title=format!("{label} ({shortcut})") on:click=move |_| actions::compose(ui, kind)>
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

    let save_all = move |id: String| {
        spawn_local(async move {
            if let Err(e) = ipc::save_all_attachments(&id).await {
                ui.error.set(Some(e));
            }
        })
    };
    // Impression par le navigateur de la page : le corps HTML (cadre isolé) seul, sinon le volet de lecture
    // (les règles `@media print` ci-dessous ne gardent que lui).
    let print = move |_| {
        let body_frame = frame.get_untracked().and_then(|f| f.content_window());
        let result = match body_frame {
            Some(w) => w.print(),
            None => window().print(),
        };
        if let Err(e) = result {
            ui.error.set(Some(format!("Impression impossible : {e:?}")));
        }
    };

    view! {
        <style>{PRINT_CSS}</style>
        <section class="pane reading" aria-label="Volet de lecture" tabindex="-1" node_ref=ui.refs.reading>
            {move || match message.get() {
                None if ui.selected.get().is_none() => view! { <p class="reading-empty">"Sélectionnez un élément à lire."</p> }.into_any(),
                None => view! { <p class="reading-empty">{move || if loading.get() { "Chargement…" } else { "" }}</p> }.into_any(),
                Some(m) => {
                    let id = m.id.clone();
                    let prev_id = m.id.clone();
                    let m_names: Vec<String> = m.attachments.iter().map(|a| a.name.clone()).collect();
                    let allow_id = m.id.clone();
                    view! {
                        <header class="reading-head">
                            <div class="reading-actions" role="toolbar" aria-label="Actions sur le message">
                                {action("Répondre", REPLY, "Ctrl+R", ComposeKind::Reply)}
                                {action("Répondre à tous", REPLY_ALL, "Ctrl+Maj+R", ComposeKind::ReplyAll)}
                                {action("Transférer", FORWARD, "Ctrl+F", ComposeKind::Forward)}
                                <button class="cmd" title="Imprimer ce message" on:click=print>
                                    <Svg icon=FILE />
                                    <span>"Imprimer"</span>
                                </button>
                                {(m.attachments.len() > 1).then(|| {
                                    let id = m.id.clone();
                                    view! {
                                        <button class="cmd" title="Enregistrer toutes les pièces jointes dans un dossier" on:click=move |_| save_all(id.clone())>
                                            <Svg icon=SAVE />
                                            <span>"Tout enregistrer"</span>
                                        </button>
                                    }
                                })}
                                {m.remote_blocked.then(|| {
                                    let id = m.id.clone();
                                    view! {
                                        <button class="cmd" title="Afficher les images et le contenu distant de ce message" on:click=move |_| {
                                            let id = id.clone();
                                            ui.remote_allowed.update(|r| {
                                                r.insert(id);
                                            });
                                        }>
                                            <Svg icon=DOWNLOAD />
                                            <span>"Télécharger les images"</span>
                                        </button>
                                    }
                                })}
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
                            {m.last_verb_label.clone().map(|t| {
                                let forward = m.last_verb.is_some_and(|v| v.is_forward());
                                view! {
                                    <p class="infobar" role="note">
                                        {if forward { view! { <Svg icon=FORWARD size=14 /> } } else { view! { <Svg icon=REPLY size=14 /> } }}
                                        {t}
                                    </p>
                                }
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
                                    "Images bloquées par ThunderLook (protection de la vie privée)."
                                    <button class="link-btn" on:click=move |_| {
                                        let id = allow_id.clone();
                                        ui.remote_allowed.update(|r| {
                                            r.insert(id);
                                        });
                                    }>"Les afficher"</button>
                                </p>
                            })}
                            {(!m.attachments.is_empty()).then(|| view! { <InviteBar id=m.id.clone() /> })}
                            {(!m.attachments.is_empty()).then(|| {
                                let atts = m.attachments.clone();
                                let label = format!("Pièces jointes : {}", atts.len());
                                view! {
                                    <ul class="attachments" aria-label=label>
                                        {atts.into_iter().map(|a| {
                                            let (oid, sid, pid, hid, did) = (id.clone(), id.clone(), id.clone(), id.clone(), id.clone());
                                            let index = a.index;
                                            let title = if a.blocked {
                                                format!("{} — type bloqué : enregistrer seulement", a.name)
                                            } else {
                                                format!("Ouvrir {}", a.name)
                                            };
                                            view! {
                                                <li class="attachment" class:blocked=a.blocked draggable="true"
                                                    on:mouseenter=move |_| attach_preview::prefetch_drag(&hid, index)
                                                    on:dragstart=move |ev| attach_preview::drag_start(&ev, &did, index)>
                                                    <button class="att-open" title=title on:click=move |_| open_att(oid.clone(), index)>
                                                        {if a.blocked { view! { <Svg icon=SHIELD_ALERT size=16 /> } } else if a.message { view! { <Svg icon=MAIL size=16 /> } } else { view! { <Svg icon=FILE size=16 /> } }}
                                                        <span class="att-name">{a.name.clone()}</span>
                                                        <span class="att-size">{a.size_label.clone()}</span>
                                                    </button>
                                                    <button class="icon-btn" aria-label=format!("Aperçu de {}", a.name) title="Aperçu" on:click=move |_| {
                                                        let key = (pid.clone(), index);
                                                        preview.update(|p| *p = if p.as_ref() == Some(&key) { None } else { Some(key) });
                                                    }>
                                                        <Svg icon=EYE size=14 />
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
                            {move || {
                                let (pid, index) = preview.get().filter(|(pid, _)| *pid == prev_id)?;
                                let name = m_names.get(index as usize).cloned().unwrap_or_default();
                                Some(view! {
                                    <attach_preview::AttachPreviewPane id=pid index=index name=name
                                        on_close=Callback::new(move |_| preview.set(None)) />
                                })
                            }}
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

// ---------------------------------------------------------------- lot AW : invitations de réunion

/// Invitation lue dans le message (voir `invite_cmds::InviteView`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
struct Invite {
    method: String,
    summary: String,
    location: String,
    description: String,
    when_label: String,
    start: String,
    end: String,
    all_day: bool,
    recurring: bool,
    organizer: String,
    attendees: Vec<appointment::Attendee>,
    me: String,
    my_status: String,
    headline: String,
    warnings: Vec<String>,
    tracked: bool,
}

#[derive(Serialize)]
struct InviteIdArg<'a> {
    id: &'a str,
}

#[derive(Serialize)]
struct RespondArg<'a> {
    id: &'a str,
    answer: &'a str,
    comment: &'a str,
}

const INVITE_CSS: &str = ".invite-bar { flex-direction: column; align-items: stretch; } \
.invite-row { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; } \
.invite-title { font-weight: 600; color: var(--text); } \
.invite-people { margin: 0; padding: 0; list-style: none; display: flex; flex-wrap: wrap; gap: 2px 12px; } \
.invite-people .att-accepted { color: var(--success, #107c10); } .invite-people .att-declined { text-decoration: line-through; } \
.invite-comment { flex: 1 1 220px; min-width: 0; }";

fn answer_label(code: &str) -> &'static str {
    match code {
        "accepted" => "Accepter",
        "tentative" => "Provisoire",
        _ => "Refuser",
    }
}

/// Barre d'invitation : Accepter / Provisoire / Refuser pour une invitation (REQUEST), suivi d'une réponse (REPLY),
/// avis d'annulation (CANCEL). Rien ne s'affiche si le message ne porte pas de `text/calendar`.
#[component]
fn InviteBar(id: String) -> impl IntoView {
    let ui = use_ui();
    let invite = RwSignal::new(None::<Invite>);
    // Réponse choisie, en attente de confirmation (commentaire facultatif).
    let pending = RwSignal::new(None::<&'static str>);
    let comment = RwSignal::new(String::new());
    let sending = RwSignal::new(false);
    let done = RwSignal::new(None::<String>);
    let id = StoredValue::new(id);

    spawn_local(async move {
        let mid = id.get_value();
        if let Ok(Some(v)) = ipc::call::<_, Option<Invite>>("invite_read", &InviteIdArg { id: &mid }).await {
            if ui.selected.get_untracked().as_deref() == Some(mid.as_str()) {
                invite.set(Some(v));
            }
        }
    });

    let respond = move |answer: &'static str| {
        sending.set(true);
        spawn_local(async move {
            let mid = id.get_value();
            let text = comment.get_untracked();
            match ipc::call::<_, String>("invite_respond", &RespondArg { id: &mid, answer, comment: &text }).await {
                Ok(m) => {
                    pending.set(None);
                    done.set(Some(m.clone()));
                    invite.update(|i| {
                        if let Some(i) = i {
                            i.my_status = answer.to_string();
                        }
                    });
                    ui.status.set(Some(m));
                }
                Err(e) => ui.error.set(Some(e)),
            }
            sending.set(false);
        });
    };
    let add_to_calendar = move |busy: i32| {
        let Some(v) = invite.get_untracked() else { return };
        let mut body = format!("Organisateur : {}", v.organizer);
        if !v.description.trim().is_empty() {
            body.push_str("\n\n");
            body.push_str(&v.description);
        }
        appointment::open_from_invite(ui, v.summary, v.location, v.start, v.end, v.all_day, body, busy);
    };
    let apply_reply = move |_| {
        spawn_local(async move {
            let mid = id.get_value();
            match ipc::call::<_, String>("invite_apply_reply", &InviteIdArg { id: &mid }).await {
                Ok(m) => {
                    done.set(Some(m.clone()));
                    ui.status.set(Some(m));
                }
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };

    move || {
        let v = invite.get()?;
        let request = v.method == "request";
        let can_answer = request && !v.me.is_empty();
        let mine = v.my_status.clone();
        let answer_btn = move |code: &'static str, icon| {
            let current = mine == code;
            view! {
                <button class="cmd" class:active=current aria-pressed=current.to_string() prop:disabled=move || sending.get()
                    title=format!("{} l'invitation", answer_label(code))
                    on:click=move |_| {
                        comment.set(String::new());
                        pending.set(Some(code));
                    }>
                    <Svg icon=icon />
                    <span>{answer_label(code)}</span>
                </button>
            }
        };
        Some(view! {
            <style>{INVITE_CSS}</style>
            <div class="infobar invite-bar" role="region" aria-label="Invitation de réunion">
                <div class="invite-row">
                    <Svg icon=CALENDAR size=14 />
                    <span>{v.headline.clone()}</span>
                </div>
                <div class="invite-row">
                    <span class="invite-title">{if v.summary.trim().is_empty() { "(sans objet)".to_string() } else { v.summary.clone() }}</span>
                    <span>{v.when_label.clone()}</span>
                    {(!v.location.trim().is_empty()).then(|| view! { <span>{format!("Lieu : {}", v.location)}</span> })}
                    {v.recurring.then(|| view! { <span>"(réunion périodique)"</span> })}
                </div>
                {(!v.attendees.is_empty()).then(|| view! {
                    <ul class="invite-people" aria-label="Participants">
                        {v.attendees.iter().map(|a| {
                            let who = if a.name.trim().is_empty() { a.email.clone() } else { a.name.clone() };
                            view! { <li class=format!("att-{}", a.status) title=a.email.clone()>{format!("{who} — {}", a.status_label)}</li> }
                        }).collect_view()}
                    </ul>
                })}
                {v.warnings.iter().map(|w| view! { <div class="invite-row"><Svg icon=CIRCLE_ALERT size=14 />{w.clone()}</div> }).collect_view()}
                {can_answer.then(|| view! {
                    <div class="invite-row" role="toolbar" aria-label="Répondre à l'invitation">
                        {answer_btn("accepted", CHECK)}
                        {answer_btn("tentative", HELP)}
                        {answer_btn("declined", X)}
                        <button class="cmd" title="Ajouter au calendrier sans répondre" on:click=move |_| add_to_calendar(2)>
                            <Svg icon=CALENDAR_PLUS />
                            <span>"Ajouter au calendrier"</span>
                        </button>
                    </div>
                })}
                {(request && v.me.is_empty()).then(|| view! {
                    <div class="invite-row">
                        "Vous ne figurez pas nommément parmi les participants : pas de réponse possible depuis ThunderLook."
                        <button class="link-btn" on:click=move |_| add_to_calendar(2)>"Ajouter au calendrier"</button>
                    </div>
                })}
                {move || pending.get().map(|code| view! {
                    <div class="invite-row">
                        <input class="invite-comment" type="text" aria-label="Commentaire joint à la réponse (facultatif)"
                            placeholder="Commentaire (facultatif)" prop:value=move || comment.get()
                            on:input=move |ev| comment.set(event_target_value(&ev)) />
                        <button class="btn primary" prop:disabled=move || sending.get() on:click=move |_| respond(code)>
                            {format!("Envoyer « {} »", answer_label(code))}
                        </button>
                        <button class="btn" on:click=move |_| pending.set(None)>"Annuler"</button>
                    </div>
                })}
                {move || done.get().map(|m| {
                    let accepted = matches!(invite.with(|i| i.as_ref().map(|i| i.my_status.clone())).as_deref(), Some("accepted" | "tentative"));
                    let busy = if invite.with(|i| i.as_ref().is_some_and(|i| i.my_status == "tentative")) { 1 } else { 2 };
                    view! {
                        <div class="invite-row" role="status">
                            <Svg icon=CHECK size=14 />
                            {m}
                            {(accepted && request).then(|| view! {
                                <button class="link-btn" on:click=move |_| add_to_calendar(busy)>"Ajouter au calendrier"</button>
                            })}
                        </div>
                    }
                })}
                {(v.method == "reply" && v.tracked).then(|| view! {
                    <div class="invite-row">
                        <button class="link-btn" on:click=apply_reply>"Mettre à jour le suivi des réponses"</button>
                    </div>
                })}
                {(v.method == "reply" && !v.tracked).then(|| view! {
                    <div class="invite-row">"Réunion non envoyée depuis ThunderLook : pas de suivi à mettre à jour."</div>
                })}
            </div>
        })
    }
}
