//! Module Notes : pense-bêtes colorés (jaune par défaut), créés, modifiés et supprimés dans la base de ThunderLook.

use leptos::{html, prelude::*, task::spawn_local};
use thunderlook_model::items::NoteForm;
use wasm_bindgen::JsCast;

use crate::{
    ipc,
    state::{Module, Ui, use_ui},
};

const COLORS: [(i32, &str, &str); 5] = [(0, "Bleue", "#9ec8f0"), (1, "Verte", "#a8dba8"), (2, "Rose", "#f2b3cf"), (3, "Jaune", "#f7e27a"), (4, "Blanche", "#f2f2f2")];

fn color_of(c: i32) -> &'static str {
    COLORS.iter().find(|x| x.0 == c).map_or("#f7e27a", |x| x.2)
}

/// Recharge la liste à chaque ouverture du module et après chaque modification.
pub fn watch(ui: Ui) {
    Effect::new(move |_| {
        ui.notes_epoch.track();
        if ui.module.get() != Module::Notes {
            return;
        }
        spawn_local(async move {
            match ipc::notes_list().await {
                Ok(l) => ui.notes.set(Some(l)),
                Err(e) => ui.error.set(Some(e)),
            }
        });
    });
}

fn changed(ui: Ui) {
    ui.notes_epoch.update(|e| *e += 1);
}

pub fn open_new(ui: Ui) {
    spawn_local(async move {
        match ipc::note_form(None).await {
            Ok(f) => ui.note_form.set(Some(f)),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

fn open_edit(ui: Ui, id: String) {
    spawn_local(async move {
        match ipc::note_form(Some(&id)).await {
            Ok(f) => ui.note_form.set(Some(f)),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

fn delete(ui: Ui, id: String) {
    spawn_local(async move {
        match ipc::note_delete(&id).await {
            Ok(()) => {
                ui.status.set(Some("Note supprimée".into()));
                changed(ui);
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

thread_local! {
    /// Note choisie dans la liste (pour « Supprimer » du ruban).
    static SELECTED: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

pub fn delete_selected(ui: Ui) {
    if let Some(id) = SELECTED.with(|s| s.borrow().clone()) {
        SELECTED.with(|s| *s.borrow_mut() = None);
        delete(ui, id);
    }
}

#[component]
pub fn NotesPane() -> impl IntoView {
    let ui = use_ui();
    let selected = RwSignal::new(None::<String>);
    view! {
        <section class="pane notes" aria-label="Notes">
            {move || match ui.notes.with(Option::is_none) {
                true => view! { <div class="placeholder">"Chargement des notes…"</div> }.into_any(),
                false => {
                    let rows = ui.notes.with(|l| l.as_ref().map(|l| l.rows.clone()).unwrap_or_default());
                    if rows.is_empty() {
                        return view! { <div class="placeholder">"Aucune note."</div> }.into_any();
                    }
                    view! {
                        <div class="note-grid" role="listbox" aria-label="Notes">
                            {rows
                                .into_iter()
                                .map(|n| {
                                    let (sel, aria, pick, edit) = (n.id.clone(), n.id.clone(), n.id.clone(), n.id.clone());
                                    view! {
                                        <article class="note-card" role="option" tabindex="0" style=format!("background:{}", color_of(n.color))
                                            class:selected=move || selected.get().as_deref() == Some(sel.as_str())
                                            aria-selected=move || (selected.get().as_deref() == Some(aria.as_str())).to_string()
                                            on:click=move |_| {
                                                selected.set(Some(pick.clone()));
                                                SELECTED.with(|s| *s.borrow_mut() = Some(pick.clone()));
                                            }
                                            on:dblclick=move |_| open_edit(ui, edit.clone())>
                                            <h3>{n.title.clone()}</h3>
                                            <p>{n.text.clone()}</p>
                                            <footer>{n.date.clone()}</footer>
                                        </article>
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                    .into_any()
                }
            }}
        </section>
    }
}

#[component]
pub fn NoteDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let busy = RwSignal::new(false);
    let note = RwSignal::new(None::<String>);
    let form = move || ui.note_form.get();
    let set = move |f: &dyn Fn(&mut NoteForm)| {
        ui.note_form.update(|c| {
            if let Some(c) = c {
                f(c);
            }
        })
    };
    Effect::new(move |_| {
        if ui.note_form.with(Option::is_some) {
            note.set(None);
            request_animation_frame(move || {
                if let Some(r) = root.get_untracked() {
                    if let Ok(Some(el)) = r.query_selector("textarea") {
                        if let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() {
                            let _ = el.focus();
                        }
                    }
                }
            });
        }
    });
    let close = move || ui.note_form.set(None);
    let save = move || {
        let Some(f) = ui.note_form.get_untracked() else { return };
        busy.set(true);
        spawn_local(async move {
            match ipc::note_save(&f).await {
                Ok(_) => {
                    ui.status.set(Some("Note enregistrée".into()));
                    changed(ui);
                    close();
                }
                Err(e) => note.set(Some(e)),
            }
            busy.set(false);
        });
    };
    view! {
        <Show when=move || ui.note_form.with(Option::is_some)>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog appt" role="dialog" aria-modal="true" aria-labelledby="note-title" node_ref=root
                    on:keydown=move |ev| match ev.key().as_str() {
                        "Escape" => {
                            ev.prevent_default();
                            close();
                        }
                        "s" if ev.ctrl_key() => {
                            ev.prevent_default();
                            save();
                        }
                        _ => {}
                    }>
                    <h2 id="note-title">{move || if form().is_some_and(|f| f.id.is_some()) { "Note" } else { "Nouvelle note" }}</h2>
                    <label class="field">
                        <span>"Texte (la première ligne sert de titre)"</span>
                        <textarea class="ar-text" aria-label="Texte de la note" rows="10" prop:value=move || form().map(|f| f.body).unwrap_or_default()
                            on:input=move |ev| { let v = event_target_value(&ev); set(&|f| f.body = v.clone()); }></textarea>
                    </label>
                    <label class="field">
                        <span>"Couleur"</span>
                        <select aria-label="Couleur" on:change=move |ev| { let v = event_target_value(&ev).parse().unwrap_or(3); set(&|f| f.color = v); }>
                            {COLORS.iter().map(|&(v, l, _)| view! { <option value=v.to_string() selected=move || form().is_some_and(|f| f.color == v)>{l}</option> }).collect_view()}
                        </select>
                    </label>
                    {move || note.get().map(|n| view! { <p class="dialog-note" role="alert">{n}</p> })}
                    <div class="dialog-buttons">
                        <button class="btn primary" prop:disabled=move || busy.get() on:click=move |_| save()>"Enregistrer et fermer"</button>
                        <button class="btn" on:click=move |_| close()>"Annuler"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
