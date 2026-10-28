//! Éditeur de signatures (lot BE, menu Fichier → « Signatures… ») : liste, nom, corps modifiable, et par compte la signature
//! des nouveaux messages et celle des réponses/transferts. Le stockage (`signatures.json`) et le repli sur le dossier
//! d'Outlook sont dans `thunderlook-mailbox` ; ici, les appels et l'affichage.
//!
//! Le corps se modifie dans un `iframe` isolé comme celui de la rédaction : `sandbox` sans `allow-scripts`, CSP sans contenu
//! distant, `designMode` activé par l'interface (une signature copiée d'Outlook ne peut donc rien exécuter).

use leptos::{html, prelude::*, task::spawn_local};
use serde::{Deserialize, Serialize};
use thunderlook_model::{AccountSummary, Signature};
use wasm_bindgen::JsCast;

use crate::{
    icons::*,
    ipc,
    state::use_ui,
};

/// Même forme JSON que `thunderlook_mailbox::StoredSignature` (la crate `mailbox` ne se compile pas pour le navigateur).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stored {
    pub name: String,
    pub html: String,
    #[serde(default)]
    pub for_new: Vec<String>,
    #[serde(default)]
    pub for_reply: Vec<String>,
}

fn editor_doc(html: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:\">\
         <style>html{{background:#fff;color:#000;color-scheme:light}}\
         body{{margin:8px 12px;font-family:Calibri,'Segoe UI',Arial,sans-serif;font-size:11pt;line-height:1.35;overflow-wrap:break-word;min-height:120px}}\
         p.MsoNormal{{margin:0}}img{{max-width:100%}}</style></head><body>{html}</body></html>"
    )
}

fn frame_doc(frame: NodeRef<html::Iframe>) -> Option<web_sys::HtmlDocument> {
    // Pas de `dyn_into` : document d'une autre fenêtre (voir `compose.rs`).
    Some(frame.get_untracked()?.content_document()?.unchecked_into())
}

fn toggle(list: &mut Vec<String>, addr: &str, on: bool) {
    list.retain(|a| !a.eq_ignore_ascii_case(addr));
    if on {
        list.push(addr.to_ascii_lowercase());
    }
}

#[component]
pub fn SignaturesDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let frame = NodeRef::<html::Iframe>::new();
    let list = RwSignal::new(Vec::<Stored>::new());
    let sel = RwSignal::new(Option::<usize>::None);
    let accounts = RwSignal::new(Vec::<AccountSummary>::new());
    let outlook = RwSignal::new(Vec::<Signature>::new());
    let note = RwSignal::new(Option::<String>::None);
    let busy = RwSignal::new(false);

    // Le corps de l'éditeur est reporté dans la liste avant tout changement de sélection et avant l'enregistrement.
    let commit = move || {
        let Some(i) = sel.get_untracked() else { return };
        if let Some(h) = frame_doc(frame).and_then(|d| d.body()).map(|b| b.inner_html()) {
            list.update(|l| {
                if let Some(s) = l.get_mut(i) {
                    s.html = h;
                }
            });
        }
    };
    let select = move |i: usize| {
        // Signature déjà affichée : ne pas recréer l'éditeur (focus et curseur perdus).
        if sel.get_untracked() == Some(i) {
            return;
        }
        commit();
        sel.set(Some(i));
    };

    Effect::new(move |_| {
        if !ui.signatures_open.get() {
            return;
        }
        note.set(None);
        spawn_local(async move {
            match ipc::signatures_stored().await {
                Ok(l) => {
                    sel.set((!l.is_empty()).then_some(0));
                    list.set(l);
                }
                Err(e) => ui.error.set(Some(e)),
            }
            if let Ok(a) = ipc::accounts().await {
                accounts.set(a);
            }
            if let Ok(o) = ipc::signatures_outlook().await {
                outlook.set(o);
            }
        });
        request_animation_frame(move || {
            if let Some(Ok(el)) = root.get_untracked().and_then(|r| r.query_selector("button").ok().flatten()).map(|el| el.dyn_into::<web_sys::HtmlElement>()) {
                let _ = el.focus();
            }
        });
    });

    let close = move || {
        ui.signatures_open.set(false);
        if let Some(l) = ui.refs.list.get_untracked() {
            let _ = l.focus();
        }
    };
    let unique = move |base: &str| {
        let taken = |n: &str| list.with_untracked(|l| l.iter().any(|s| s.name.eq_ignore_ascii_case(n)));
        let mut name = base.to_string();
        let mut k = 2;
        while taken(&name) {
            name = format!("{base} {k}");
            k += 1;
        }
        name
    };
    let add = move |name: String, html: String| {
        commit();
        let name = unique(&name);
        list.update(|l| l.push(Stored { name, html, ..Stored::default() }));
        sel.set(Some(list.with_untracked(Vec::len) - 1));
    };
    let remove = move || {
        let Some(i) = sel.get_untracked() else { return };
        list.update(|l| {
            l.remove(i);
        });
        let n = list.with_untracked(Vec::len);
        sel.set((n > 0).then(|| i.min(n - 1)));
    };
    let save = move || {
        commit();
        let current = list.get_untracked();
        busy.set(true);
        spawn_local(async move {
            match ipc::signatures_store_save(&current).await {
                Ok(()) => {
                    ui.status.set(Some("Signatures enregistrées.".into()));
                    close();
                }
                Err(e) => note.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let exec = move |command: &'static str| {
        if let Some(doc) = frame_doc(frame) {
            let _ = doc.exec_command_with_show_ui_and_value(command, false, "");
            if let Some(w) = frame.get_untracked().and_then(|f| f.content_window()) {
                let _ = w.focus();
            }
        }
    };
    let assign = move |addr: String, new: bool, on: bool| {
        let Some(i) = sel.get_untracked() else { return };
        list.update(|l| {
            // Un compte n'a qu'une signature par type : cocher ici la retire des autres.
            if on {
                for (j, s) in l.iter_mut().enumerate() {
                    if j != i {
                        toggle(if new { &mut s.for_new } else { &mut s.for_reply }, &addr, false);
                    }
                }
            }
            if let Some(s) = l.get_mut(i) {
                toggle(if new { &mut s.for_new } else { &mut s.for_reply }, &addr, on);
            }
        });
    };

    view! {
        <Show when=move || ui.signatures_open.get()>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog wide" role="dialog" aria-modal="true" aria-labelledby="sig-title" node_ref=root
                    on:keydown=move |ev| {
                        if ev.key() == "Escape" {
                            ev.prevent_default();
                            close();
                        }
                    }>
                    <h2 id="sig-title">"Signatures"</h2>
                    <p class="dialog-note">"Enregistrées dans ThunderLook (fichier signatures.json du dossier de données). Un compte sans signature attribuée garde celle du dossier d'Outlook dont le nom contient son adresse entre parenthèses ; le dossier d'Outlook n'est jamais modifié."</p>
                    <div style="display: flex; gap: 0.5em; flex-wrap: wrap; margin: 0.5em 0;">
                        <button class="btn" on:click=move |_| add("Nouvelle signature".into(), String::new())>"Nouvelle"</button>
                        <button class="btn" disabled=move || sel.get().is_none() on:click=move |_| remove()>"Supprimer"</button>
                        <select aria-label="Copier une signature d'Outlook" on:change=move |ev| {
                            let v = event_target_value(&ev);
                            if let Some(s) = outlook.with_untracked(|o| o.iter().find(|s| s.name == v).cloned()) {
                                add(s.name, s.html);
                                note.set(Some("Copiée d'Outlook : à enregistrer pour la garder.".into()));
                            }
                        } prop:value="">
                            <option value="" selected>"Copier d'Outlook…"</option>
                            {move || outlook.get().into_iter().map(|s| view! { <option value=s.name.clone()>{s.name.clone()}</option> }).collect_view()}
                        </select>
                    </div>
                    <div style="display: flex; gap: 1em; align-items: flex-start;">
                        <ul role="listbox" aria-label="Signatures enregistrées" style="width: 14em; max-height: 22em; overflow: auto; list-style: none; padding: 0; margin: 0;">
                            {move || {
                                if list.with(Vec::is_empty) {
                                    return view! { <li class="dialog-note">"Aucune signature enregistrée."</li> }.into_any();
                                }
                                list.get().into_iter().enumerate().map(|(i, s)| view! {
                                    <li role="option" tabindex="0" aria-selected=move || (sel.get() == Some(i)).to_string()
                                        style=move || if sel.get() == Some(i) { "padding: 0.25em 0.5em; cursor: pointer; font-weight: 600; background: var(--select, #cde6f7);" } else { "padding: 0.25em 0.5em; cursor: pointer;" }
                                        on:click=move |_| select(i)
                                        on:keydown=move |ev| if ev.key() == "Enter" || ev.key() == " " { ev.prevent_default(); select(i) }>
                                        {s.name}
                                    </li>
                                }).collect_view().into_any()
                            }}
                        </ul>
                        <div style="flex: 1; min-width: 0;">
                            {move || sel.get().map(|i| {
                                let name = list.with_untracked(|l| l.get(i).map(|s| s.name.clone()).unwrap_or_default());
                                let doc = editor_doc(&list.with_untracked(|l| l.get(i).map(|s| s.html.clone()).unwrap_or_default()));
                                view! {
                                    <label class="field">
                                        <span>"Nom"</span>
                                        <input type="text" aria-label="Nom de la signature" prop:value=name
                                            on:input=move |ev| {
                                                let v = event_target_value(&ev);
                                                list.update(|l| if let Some(s) = l.get_mut(i) { s.name = v });
                                            } />
                                    </label>
                                    <div role="toolbar" aria-label="Mise en forme" style="display: flex; gap: 0.25em; margin: 0.5em 0 0.25em;">
                                        <button class="cmd icon-only" title="Gras" aria-label="Gras" on:click=move |_| exec("bold")><Svg icon=BOLD /></button>
                                        <button class="cmd icon-only" title="Italique" aria-label="Italique" on:click=move |_| exec("italic")><Svg icon=ITALIC /></button>
                                        <button class="cmd icon-only" title="Souligné" aria-label="Souligné" on:click=move |_| exec("underline")><Svg icon=UNDERLINE /></button>
                                    </div>
                                    <iframe title="Corps de la signature" sandbox="allow-same-origin" srcdoc=doc node_ref=frame
                                        style="width: 100%; height: 12em; border: 1px solid var(--border, #ccc); background: #fff;"
                                        on:load=move |_| if let Some(d) = frame_doc(frame) { d.set_design_mode("on") }></iframe>
                                }
                            })}
                        </div>
                    </div>
                    {move || {
                        let Some(i) = sel.get() else { return ().into_any() };
                        let accs = accounts.get();
                        if accs.is_empty() {
                            return view! { <p class="dialog-note">"Aucun compte de messagerie : attribution impossible."</p> }.into_any();
                        }
                        view! {
                            <table style="margin-top: 0.75em; border-collapse: collapse;" aria-label="Attribution par compte">
                                <thead><tr><th style="text-align: left; padding-right: 1em;">"Compte"</th><th>"Nouveaux messages"</th><th>"Réponses et transferts"</th></tr></thead>
                                <tbody>
                                    {accs.into_iter().map(|a| {
                                        let addr = a.address.to_ascii_lowercase();
                                        let (a1, a2, c1, c2) = (addr.clone(), addr.clone(), addr.clone(), addr.clone());
                                        view! {
                                            <tr>
                                                <td style="padding-right: 1em;">{a.address.clone()}</td>
                                                <td style="text-align: center;">
                                                    <input type="checkbox" aria-label=format!("Nouveaux messages de {}", a.address)
                                                        prop:checked=move || list.with(|l| l.get(i).is_some_and(|s| s.for_new.iter().any(|x| x == &c1)))
                                                        on:change=move |ev| assign(a1.clone(), true, event_target_checked(&ev)) />
                                                </td>
                                                <td style="text-align: center;">
                                                    <input type="checkbox" aria-label=format!("Réponses et transferts de {}", a.address)
                                                        prop:checked=move || list.with(|l| l.get(i).is_some_and(|s| s.for_reply.iter().any(|x| x == &c2)))
                                                        on:change=move |ev| assign(a2.clone(), false, event_target_checked(&ev)) />
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        }.into_any()
                    }}
                    {move || note.get().map(|n| view! { <p class="dialog-note" role="alert">{n}</p> })}
                    <div class="dialog-buttons">
                        <button class="btn primary" disabled=move || busy.get() on:click=move |_| save()>"Enregistrer"</button>
                        <button class="btn" on:click=move |_| close()>"Annuler"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
