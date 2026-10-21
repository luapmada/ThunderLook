//! Volet de rédaction (J5), à la place du volet de lecture comme la réponse intégrée d'Outlook 2021.
//!
//! Le corps se modifie dans un `iframe` isolé : `sandbox` sans `allow-scripts` (aucun script du message cité ne
//! s'exécute), CSP sans contenu distant, `designMode` activé par l'interface. Enregistrement : Ctrl+S, bouton,
//! et automatiquement chaque minute si le message a changé. L'envoi viendra avec SMTP (J7).

use leptos::{html, prelude::*, task::spawn_local};
use thunderlook_model::{ComposeKind, Draft, Signature};
use wasm_bindgen::{JsCast, closure::Closure};

use crate::{
    actions,
    icons::*,
    ipc,
    menu::{MenuButton, MenuEntry},
    state::{Ui, use_ui},
};

/// Délai de l'enregistrement automatique (ms).
const AUTOSAVE_MS: i32 = 60_000;

fn editor_doc(html: &str, styles: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:\">\
         <style>html{{background:#fff;color:#000;color-scheme:light}}\
         body{{margin:12px 16px;font-family:Calibri,'Segoe UI',Arial,sans-serif;font-size:11pt;line-height:1.35;overflow-wrap:break-word;min-height:200px}}\
         p.MsoNormal{{margin:0}}img{{max-width:100%}}</style><style>{styles}</style></head><body>{html}</body></html>"
    )
}

fn frame_doc(frame: NodeRef<html::Iframe>) -> Option<web_sys::HtmlDocument> {
    // Pas de `dyn_into` : le document de l'iframe appartient à une autre fenêtre, et le test `instanceof`
    // contre le `HTMLDocument` de la fenêtre principale échouerait. C'est bien un document HTML (srcdoc).
    Some(frame.get_untracked()?.content_document()?.unchecked_into())
}

/// Corps courant de l'éditeur.
fn editor_html(frame: NodeRef<html::Iframe>) -> Option<String> {
    frame_doc(frame)?.body().map(|b| b.inner_html())
}

/// Brouillon courant, corps de l'éditeur compris.
fn current(ui: Ui, frame: NodeRef<html::Iframe>) -> Option<Draft> {
    let mut d = ui.compose.get_untracked()?;
    if let Some(h) = editor_html(frame) {
        d.html = h;
    }
    Some(d)
}

/// Enregistre le brouillon ; `then` est appelé après succès.
fn save(ui: Ui, frame: NodeRef<html::Iframe>, then: impl FnOnce() + 'static) {
    let Some(d) = current(ui, frame) else { return };
    spawn_local(async move {
        match ipc::save_draft(&d).await {
            Ok(saved) => {
                // Les champs ont pu changer pendant l'aller-retour : on ne reprend que l'identifiant et les pièces jointes.
                ui.compose.update(|c| {
                    if let Some(c) = c {
                        c.id = saved.id.clone();
                        c.attachments = saved.attachments.clone();
                        c.html = d.html.clone();
                    }
                });
                ui.compose_dirty.set(false);
                ui.status.set(Some("Brouillon enregistré dans « Brouillons ».".into()));
                // Le dossier Brouillons et ses compteurs ont changé.
                actions::refresh_tree(ui);
                actions::reopen_view(ui);
                then();
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

/// « Envoyer » (J7) : le message part dans la Boîte d'envoi du compte ; l'envoi suit (événement « sync »).
fn send(ui: Ui, frame: NodeRef<html::Iframe>) {
    let Some(d) = current(ui, frame) else { return };
    spawn_local(async move {
        match ipc::send_draft(&d).await {
            Ok(_) => {
                ui.compose_dirty.set(false);
                ui.compose.set(None);
                ui.status.set(Some("Envoi en cours…".into()));
                actions::refresh_tree(ui);
                actions::reopen_view(ui);
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

fn exec(frame: NodeRef<html::Iframe>, command: &str, value: Option<&str>) {
    if let Some(doc) = frame_doc(frame) {
        let _ = doc.exec_command_with_show_ui_and_value(command, false, value.unwrap_or(""));
        if let Some(w) = frame.get_untracked().and_then(|f| f.content_window()) {
            let _ = w.focus();
        }
    }
}

#[component]
pub fn ComposePane() -> impl IntoView {
    let ui = use_ui();
    let frame = NodeRef::<html::Iframe>::new();
    let show_bcc = RwSignal::new(false);
    let signatures = RwSignal::new(Vec::<Signature>::new());
    spawn_local(async move {
        if let Ok(s) = ipc::signatures().await {
            signatures.set(s);
        }
    });
    show_bcc.set(ui.compose.with_untracked(|d| d.as_ref().is_some_and(|d| !d.bcc.is_empty())));

    // Document de l'éditeur, figé à l'ouverture (les modifications vivent dans l'iframe).
    let srcdoc = ui.compose.with_untracked(|d| d.as_ref().map(|d| editor_doc(&d.html, &d.styles)).unwrap_or_default());
    let kind = ui.compose.with_untracked(|d| d.as_ref().map(|d| d.kind).unwrap_or_default());

    // Ctrl+S dans l'éditeur ; toute frappe le marque modifié.
    let on_key = StoredValue::new_local(Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
        if ev.ctrl_key() && ev.key().eq_ignore_ascii_case("s") {
            ev.prevent_default();
            save(ui, frame, || {});
        }
        if ev.ctrl_key() && ev.key() == "Enter" {
            ev.prevent_default();
            send(ui, frame);
        }
    }));
    let on_input = StoredValue::new_local(Closure::<dyn Fn(web_sys::Event)>::new(move |_| ui.compose_dirty.set(true)));
    let on_load = move |_| {
        let Some(doc) = frame_doc(frame) else { return };
        doc.set_design_mode("on");
        on_key.with_value(|k| {
            let _ = doc.add_event_listener_with_callback("keydown", k.as_ref().unchecked_ref());
        });
        on_input.with_value(|i| {
            let _ = doc.add_event_listener_with_callback("input", i.as_ref().unchecked_ref());
        });
        // Nouveau message : le curseur dans « À » ; réponse : dans le corps.
        if kind == ComposeKind::New || kind == ComposeKind::Forward {
            if let Some(to) = document().get_element_by_id("compose-to").and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok()) {
                let _ = to.focus();
            }
        } else if let Some(w) = frame.get_untracked().and_then(|f| f.content_window()) {
            let _ = w.focus();
        }
    };

    // Enregistrement automatique.
    let timer = StoredValue::new_local(Closure::<dyn Fn()>::new(move || {
        if ui.compose.with_untracked(Option::is_some) && ui.compose_dirty.get_untracked() {
            save(ui, frame, || {});
        }
    }));
    let handle = timer.with_value(|t| {
        window().set_interval_with_callback_and_timeout_and_arguments_0(t.as_ref().unchecked_ref(), AUTOSAVE_MS).ok()
    });
    on_cleanup(move || {
        if let Some(h) = handle {
            window().clear_interval_with_handle(h);
        }
    });

    let field = move |get: fn(&Draft) -> String, set: fn(&mut Draft, String)| {
        (
            move || ui.compose.with(|d| d.as_ref().map(get).unwrap_or_default()),
            move |ev: leptos::ev::Event| {
                let v = event_target_value(&ev);
                ui.compose.update(|d| {
                    if let Some(d) = d {
                        set(d, v);
                    }
                });
                ui.compose_dirty.set(true);
            },
        )
    };
    let (to, set_to) = field(|d| d.to.clone(), |d, v| d.to = v);
    let (cc, set_cc) = field(|d| d.cc.clone(), |d, v| d.cc = v);
    let (bcc, set_bcc) = field(|d| d.bcc.clone(), |d, v| d.bcc = v);
    let (subject, set_subject) = field(|d| d.subject.clone(), |d, v| d.subject = v);
    // Champ « De » : comptes de messagerie ; sans choix, le premier compte.
    let accounts = RwSignal::new(Vec::<thunderlook_model::AccountSummary>::new());
    spawn_local(async move {
        if let Ok(list) = ipc::accounts().await {
            if ui.compose.with_untracked(|d| d.as_ref().is_some_and(|d| d.from_account.is_none())) {
                if let Some(first) = list.first() {
                    let id = first.id.clone();
                    ui.compose.update(|d| {
                        if let Some(d) = d {
                            d.from_account = Some(id);
                        }
                    });
                }
            }
            accounts.set(list);
        }
    });
    let from = move || ui.compose.with(|d| d.as_ref().and_then(|d| d.from_account.clone()).unwrap_or_default());
    let set_from = move |ev: leptos::ev::Event| {
        let v = event_target_value(&ev);
        ui.compose.update(|d| {
            if let Some(d) = d {
                d.from_account = Some(v);
            }
        });
        ui.compose_dirty.set(true);
    };

    let attach = move |_| {
        spawn_local(async move {
            match ipc::pick_attachments().await {
                Ok(files) if !files.is_empty() => {
                    ui.compose.update(|d| {
                        if let Some(d) = d {
                            d.attachments.extend(files);
                        }
                    });
                    ui.compose_dirty.set(true);
                }
                Ok(_) => {}
                Err(e) => ui.error.set(Some(e)),
            }
        })
    };
    let discard = move |_| {
        // Un brouillon déjà enregistré reste dans « Brouillons » ; seules les modifications non enregistrées sont perdues.
        ui.compose.set(None);
        ui.compose_dirty.set(false);
        ui.status.set(Some("Rédaction fermée.".into()));
    };
    let signature_entries = signature_entries(ui, signatures, frame);

    let title = match kind {
        ComposeKind::New => "Nouveau message",
        ComposeKind::Reply => "Répondre",
        ComposeKind::ReplyAll => "Répondre à tous",
        ComposeKind::Forward => "Transférer",
    };

    view! {
        <section
            class="pane reading compose"
            aria-label=format!("Rédaction — {title}")
            node_ref=ui.refs.reading
            on:keydown=move |ev| {
                if ev.ctrl_key() && ev.key().eq_ignore_ascii_case("s") {
                    ev.prevent_default();
                    ev.stop_propagation();
                    save(ui, frame, || {});
                }
                if ev.ctrl_key() && ev.key() == "Enter" {
                    ev.prevent_default();
                    ev.stop_propagation();
                    send(ui, frame);
                }
            }
        >
            <div class="compose-bar" role="toolbar" aria-label="Actions de rédaction">
                <button class="cmd primary" title="Envoyer (Ctrl+Entrée)" aria-keyshortcuts="Control+Enter" on:click=move |_| send(ui, frame)>
                    <Svg icon=SEND />
                    <span>"Envoyer"</span>
                </button>
                <button class="cmd" title="Enregistrer le brouillon (Ctrl+S)" aria-keyshortcuts="Control+S" on:click=move |_| save(ui, frame, || {})>
                    <Svg icon=SAVE />
                    <span>"Enregistrer"</span>
                </button>
                <button class="cmd" title="Joindre un fichier" on:click=attach>
                    <Svg icon=PAPERCLIP />
                    <span>"Joindre"</span>
                </button>
                <MenuButton label="Signature" icon=PENCIL entries=signature_entries class="cmd" />
                <span class="compose-sep" aria-hidden="true"></span>
                <button class="cmd icon-only" title="Gras (Ctrl+G)" aria-label="Gras" on:click=move |_| exec(frame, "bold", None)>
                    <Svg icon=BOLD />
                </button>
                <button class="cmd icon-only" title="Italique (Ctrl+I)" aria-label="Italique" on:click=move |_| exec(frame, "italic", None)>
                    <Svg icon=ITALIC />
                </button>
                <button class="cmd icon-only" title="Souligné (Ctrl+U)" aria-label="Souligné" on:click=move |_| exec(frame, "underline", None)>
                    <Svg icon=UNDERLINE />
                </button>
                <button class="cmd icon-only" title="Liste à puces" aria-label="Liste à puces" on:click=move |_| exec(frame, "insertUnorderedList", None)>
                    <Svg icon=LIST />
                </button>
                <button class="cmd icon-only" title="Liste numérotée" aria-label="Liste numérotée" on:click=move |_| exec(frame, "insertOrderedList", None)>
                    <Svg icon=LIST_ORDERED />
                </button>
                <span class="tabs-spacer"></span>
                <button class="cmd" title="Fermer la rédaction" on:click=discard>
                    <Svg icon=X />
                    <span>"Ignorer"</span>
                </button>
            </div>
            <div class="compose-fields">
                <Show when=move || accounts.with(|a| !a.is_empty())>
                    <label class="compose-field">
                        <span>"De"</span>
                        <select aria-label="De" prop:value=from on:change=set_from>
                            {move || accounts.get().into_iter().map(|a| {
                                let label = if a.sender_name.is_empty() { a.address.clone() } else { format!("{} <{}>", a.sender_name, a.address) };
                                let id = a.id.clone();
                                view! { <option value=a.id.clone() selected=move || from() == id>{label}</option> }
                            }).collect_view()}
                        </select>
                    </label>
                </Show>
                <label class="compose-field">
                    <span>"À"</span>
                    <input id="compose-to" type="text" autocomplete="off" prop:value=to on:input=set_to />
                </label>
                <label class="compose-field">
                    <span>"Cc"</span>
                    <input type="text" autocomplete="off" prop:value=cc on:input=set_cc />
                    <button class="link-btn" type="button" title="Afficher la ligne Cci" hidden=move || show_bcc.get() on:click=move |_| show_bcc.set(true)>
                        "Cci"
                    </button>
                </label>
                <Show when=move || show_bcc.get()>
                    <label class="compose-field">
                        <span>"Cci"</span>
                        <input type="text" autocomplete="off" prop:value=bcc on:input=set_bcc />
                    </label>
                </Show>
                <label class="compose-field">
                    <span>"Objet"</span>
                    <input type="text" autocomplete="off" prop:value=subject on:input=set_subject />
                </label>
                <Show when=move || ui.compose.with(|d| d.as_ref().is_some_and(|d| !d.attachments.is_empty()))>
                    <div class="compose-attachments" role="list" aria-label="Pièces jointes">
                        {move || {
                            ui.compose
                                .with(|d| d.as_ref().map(|d| d.attachments.clone()).unwrap_or_default())
                                .into_iter()
                                .enumerate()
                                .map(|(i, a)| {
                                    view! {
                                        <span class="att-chip" role="listitem">
                                            <Svg icon=PAPERCLIP size=14 />
                                            <span>{format!("{} ({})", a.name, a.size_label)}</span>
                                            <button
                                                class="icon-btn"
                                                aria-label=format!("Retirer {}", a.name)
                                                on:click=move |_| {
                                                    ui.compose.update(|d| {
                                                        if let Some(d) = d {
                                                            if i < d.attachments.len() {
                                                                d.attachments.remove(i);
                                                            }
                                                        }
                                                    });
                                                    ui.compose_dirty.set(true);
                                                }
                                            >
                                                <Svg icon=X size=12 />
                                            </button>
                                        </span>
                                    }
                                })
                                .collect_view()
                        }}
                    </div>
                </Show>
            </div>
            <iframe
                class="compose-body"
                title="Corps du message"
                sandbox="allow-same-origin"
                srcdoc=srcdoc
                node_ref=frame
                on:load=on_load
            ></iframe>
        </section>
    }
}

/// Entrées du menu « Signature » : insertion à la position du curseur.
fn signature_entries(ui: Ui, all: RwSignal<Vec<Signature>>, frame: NodeRef<html::Iframe>) -> Signal<Vec<MenuEntry>> {
    Signal::derive(move || {
        let list = all.get();
        if list.is_empty() {
            return vec![MenuEntry::pending("Aucune signature trouvée")];
        }
        list.into_iter()
            .map(|s| {
                let html = s.html.clone();
                MenuEntry::action(s.name.clone(), move || {
                    exec(frame, "insertHTML", Some(&html));
                    ui.compose_dirty.set(true);
                })
            })
            .collect()
    })
}
