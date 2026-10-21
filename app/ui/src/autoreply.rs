//! Boîte « Réponses automatiques » (J7) : activation, période facultative, texte de la réponse.

use leptos::{html, prelude::*, task::spawn_local};
use thunderlook_model::AutoReply;
use wasm_bindgen::JsCast;

use crate::{
    ipc,
    state::{Dialog, use_ui},
};

#[component]
pub fn AutoReplyDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let cfg = RwSignal::new(AutoReply::default());

    Effect::new(move |_| {
        if ui.dialog.get() != Some(Dialog::AutoReply) {
            return;
        }
        spawn_local(async move {
            match ipc::get_auto_reply().await {
                Ok(c) => cfg.set(c),
                Err(e) => ui.error.set(Some(e)),
            }
            request_animation_frame(move || {
                if let Some(r) = root.get_untracked() {
                    if let Ok(Some(el)) = r.query_selector("input") {
                        if let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() {
                            let _ = el.focus();
                        }
                    }
                }
            });
        });
    });

    let close = move || {
        ui.dialog.set(None);
        if let Some(l) = ui.refs.list.get_untracked() {
            let _ = l.focus();
        }
    };
    let save = move || {
        let c = cfg.get_untracked();
        spawn_local(async move {
            match ipc::set_auto_reply(&c).await {
                Ok(()) => {
                    ui.status.set(Some(if c.enabled { "Réponses automatiques activées".into() } else { "Réponses automatiques désactivées".into() }));
                    close();
                }
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };
    let date = move |label: &'static str, get: fn(&AutoReply) -> Option<String>, set: fn(&mut AutoReply, Option<String>)| {
        view! {
            <label class="field">
                <span>{label}</span>
                <input
                    type="date"
                    aria-label=label
                    prop:value=move || cfg.with(|c| get(c).unwrap_or_default())
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        cfg.update(|c| set(c, (!v.is_empty()).then_some(v)));
                    }
                />
            </label>
        }
    };

    view! {
        <Show when=move || ui.dialog.get() == Some(Dialog::AutoReply)>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog wide" role="dialog" aria-modal="true" aria-labelledby="ar-title" node_ref=root
                    on:keydown=move |ev| {
                        if ev.key() == "Escape" {
                            ev.prevent_default();
                            close();
                        }
                    }>
                    <h2 id="ar-title">"Réponses automatiques"</h2>
                    <label class="check">
                        <input type="checkbox" prop:checked=move || cfg.with(|c| c.enabled)
                            on:change=move |ev| cfg.update(|c| c.enabled = event_target_checked(&ev)) />
                        <span>"Envoyer des réponses automatiques"</span>
                    </label>
                    {date("Du (facultatif)", |c| c.start.clone(), |c, v| c.start = v)}
                    {date("Au (facultatif)", |c| c.end.clone(), |c, v| c.end = v)}
                    <label class="field">
                        <span>"Message"</span>
                        <textarea class="ar-text" aria-label="Message" rows="6"
                            prop:value=move || cfg.with(|c| c.text.clone())
                            on:input=move |ev| cfg.update(|c| c.text = event_target_value(&ev))></textarea>
                    </label>
                    <p class="dialog-note">"Une réponse par expéditeur pendant la période ; jamais aux listes de diffusion, aux réponses automatiques ni aux adresses « noreply ». ThunderLook doit être ouvert pour répondre."</p>
                    <div class="dialog-buttons">
                        <button class="btn primary" on:click=move |_| save()>"OK"</button>
                        <button class="btn" on:click=move |_| close()>"Annuler"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
