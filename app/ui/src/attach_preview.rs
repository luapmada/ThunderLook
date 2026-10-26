//! Aperçu d'une pièce jointe (image, texte, PDF) sous la liste des pièces jointes, et glisser vers l'Explorateur.
//!
//! Lecture seule. Le glisser utilise le format `DownloadURL` de Chromium (WebView2) : le fichier est déposé par
//! l'Explorateur à partir d'une URL `data:` préchargée au survol (le `dragstart` doit rester synchrone).

use std::{cell::RefCell, collections::HashMap};

use leptos::{prelude::*, task::spawn_local};
use serde::Deserialize;

use crate::{icons::*, ipc, state::use_ui};

#[derive(Deserialize, Clone, Debug, Default)]
pub struct AttachPreview {
    pub kind: String,
    pub mime: String,
    pub name: String,
    pub data_uri: String,
    pub text: String,
    pub truncated: bool,
    pub note: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct AttachDrag {
    pub name: String,
    pub mime: String,
    pub data_uri: String,
}

const CSS: &str = ".att-preview { margin: 8px 0 0; padding: 8px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface, transparent); } \
.att-preview-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin-bottom: 8px; } \
.att-preview-img { display: block; max-width: 100%; max-height: 60vh; margin: 0 auto; object-fit: contain; } \
.att-preview-pdf { width: 100%; height: 60vh; border: 0; } \
.att-preview-text { margin: 0; max-height: 40vh; overflow: auto; white-space: pre-wrap; word-break: break-word; font-family: ui-monospace, Consolas, monospace; font-size: var(--fs-sm); } \
.att-preview-note { margin: 0; color: var(--text-2); }";

thread_local! {
    /// Contenus déjà demandés pour le glisser, par (message, index).
    static DRAG_CACHE: RefCell<HashMap<(String, u32), Option<AttachDrag>>> = RefCell::new(HashMap::new());
}

/// À appeler au survol / focus d'une pièce jointe : charge son contenu pour que le glisser soit prêt.
pub fn prefetch_drag(id: &str, index: u32) {
    let key = (id.to_string(), index);
    if DRAG_CACHE.with(|c| c.borrow().contains_key(&key)) {
        return;
    }
    // Réserve la clé tout de suite : pas de double chargement.
    DRAG_CACHE.with(|c| c.borrow_mut().insert(key.clone(), None));
    let id = id.to_string();
    spawn_local(async move {
        match ipc::attachment_drag(&id, index).await {
            Ok(d) => DRAG_CACHE.with(|c| {
                c.borrow_mut().insert(key, d);
            }),
            // Réessayera au prochain survol.
            Err(_) => DRAG_CACHE.with(|c| {
                c.borrow_mut().remove(&key);
            }),
        }
    });
}

/// `dragstart` : synchrone, donc ne sert le fichier que s'il a été préchargé ; sinon le glisser est annulé.
pub fn drag_start(ev: &web_sys::DragEvent, id: &str, index: u32) {
    let ready = DRAG_CACHE.with(|c| c.borrow().get(&(id.to_string(), index)).cloned().flatten());
    match (ready, ev.data_transfer()) {
        (Some(d), Some(dt)) => {
            dt.set_effect_allowed("copy");
            let _ = dt.set_data("DownloadURL", &format!("{}:{}:{}", d.mime, d.name, d.data_uri));
        }
        _ => {
            ev.prevent_default();
            prefetch_drag(id, index);
        }
    }
}

/// Panneau d'aperçu ; `on_close` referme le panneau.
#[component]
pub fn AttachPreviewPane(id: String, index: u32, name: String, on_close: Callback<()>) -> impl IntoView {
    let ui = use_ui();
    let preview = RwSignal::new(None::<AttachPreview>);
    {
        let id = id.clone();
        spawn_local(async move {
            match ipc::attachment_preview(&id, index).await {
                Ok(p) => preview.set(Some(p)),
                Err(e) => ui.error.set(Some(e)),
            }
        });
    }
    let aria = format!("Aperçu de {name}");
    view! {
        <style>{CSS}</style>
        <section class="att-preview" aria-label=aria>
            <div class="att-preview-head">
                <strong>{name.clone()}</strong>
                <button class="icon-btn" aria-label="Fermer l'aperçu" title="Fermer l'aperçu" on:click=move |_| on_close.run(())>
                    <Svg icon=X size=14 />
                </button>
            </div>
            {move || match preview.get() {
                None => view! { <p class="att-preview-note">"Chargement…"</p> }.into_any(),
                Some(p) => match p.kind.as_str() {
                    "image" => view! { <img class="att-preview-img" src=p.data_uri alt=p.name /> }.into_any(),
                    "pdf" => view! { <iframe class="att-preview-pdf" title=p.name src=p.data_uri></iframe> }.into_any(),
                    "text" => view! {
                        <pre class="att-preview-text">{p.text}</pre>
                        {p.truncated.then(|| view! { <p class="att-preview-note">"Aperçu tronqué : début du fichier seulement."</p> })}
                    }
                    .into_any(),
                    _ => view! { <p class="att-preview-note">{p.note}</p> }.into_any(),
                },
            }}
        </section>
    }
}
