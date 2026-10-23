//! Annulation (Ctrl+Z) des suppressions et modifications des rendez-vous, contacts, tâches et notes, et écran
//! « Sauvegardes » (menu Fichier). Le travail est dans `thunderlook-safety` ; ici, les appels et l'affichage.

use leptos::{html, prelude::*, task::spawn_local};
use serde::Deserialize;
use thunderlook_model::format::size;
use wasm_bindgen::JsCast;

use crate::{
    ipc,
    state::{Ui, use_ui},
};

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRow {
    pub name: String,
    pub weekly: bool,
    pub size: u64,
    pub count: usize,
    pub created: String,
}

/// Les listes des quatre modules se rechargent.
fn reload_lists(ui: Ui) {
    ui.contacts_epoch.update(|e| *e += 1);
    ui.tasks_epoch.update(|e| *e += 1);
    ui.notes_epoch.update(|e| *e += 1);
    ui.cal_epoch.update(|e| *e += 1);
}

/// Ctrl+Z et bouton « Annuler » hors du courrier : défait la dernière suppression ou modification.
pub fn undo(ui: Ui) {
    spawn_local(async move {
        match ipc::safety_undo().await {
            Ok(msg) => {
                ui.status.set(Some(msg));
                reload_lists(ui);
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

#[component]
pub fn BackupsDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let rows = RwSignal::new(Vec::<BackupRow>::new());
    let selected = RwSignal::new(Option::<String>::None);
    let note = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let refresh = move || {
        spawn_local(async move {
            match ipc::backups_list().await {
                Ok(r) => {
                    selected.update(|s| {
                        if s.as_ref().is_none_or(|n| !r.iter().any(|b| &b.name == n)) {
                            *s = r.first().map(|b| b.name.clone());
                        }
                    });
                    rows.set(r);
                }
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };

    Effect::new(move |_| {
        if !ui.backups.get() {
            return;
        }
        note.set(String::new());
        refresh();
        request_animation_frame(move || {
            if let Some(Ok(el)) = root.get_untracked().and_then(|r| r.query_selector("button").ok().flatten()).map(|el| el.dyn_into::<web_sys::HtmlElement>()) {
                let _ = el.focus();
            }
        });
    });

    let close = move || {
        ui.backups.set(false);
        if let Some(l) = ui.refs.list.get_untracked() {
            let _ = l.focus();
        }
    };
    let restore = move |mode: &'static str| {
        let Some(name) = selected.get_untracked() else { return };
        busy.set(true);
        spawn_local(async move {
            match ipc::backup_restore(&name, mode).await {
                Ok(m) => {
                    ui.status.set(Some(m.clone()));
                    note.set(m);
                    reload_lists(ui);
                }
                Err(e) => ui.error.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let now = move || {
        busy.set(true);
        spawn_local(async move {
            match ipc::backup_now().await {
                Ok(m) => note.set(m),
                Err(e) => ui.error.set(Some(e)),
            }
            refresh();
            busy.set(false);
        });
    };
    let open_folder = move || {
        spawn_local(async move {
            if let Err(e) = ipc::backups_open_folder().await {
                ui.error.set(Some(e));
            }
        });
    };

    view! {
        <Show when=move || ui.backups.get()>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog wide" role="dialog" aria-modal="true" aria-labelledby="bk-title" node_ref=root
                    on:keydown=move |ev| {
                        if ev.key() == "Escape" {
                            ev.prevent_default();
                            close();
                        }
                    }>
                    <h2 id="bk-title">"Sauvegardes"</h2>
                    <p class="dialog-note">"Une sauvegarde par jour au lancement (14 gardées) et une par semaine (8 gardées) : rendez-vous, contacts, tâches et notes, jamais le courrier. Une suppression ou une modification se défait aussi avec Ctrl+Z pendant 60 jours."</p>
                    <Show when=move || rows.with(Vec::is_empty)>
                        <p class="dialog-note">"Aucune sauvegarde pour l'instant."</p>
                    </Show>
                    <ul role="listbox" aria-label="Sauvegardes" style="max-height: 16em; overflow: auto; list-style: none; padding: 0; margin: 0.5em 0;">
                        {move || {
                            rows.get()
                                .into_iter()
                                .map(|b| {
                                    let pick = b.name.clone();
                                    let name = b.name.clone();
                                    let is_selected = Memo::new(move |_| selected.with(|s| s.as_deref() == Some(name.as_str())));
                                    let label = format!(
                                        "{} — {} — {} élément(s), {}",
                                        b.created.replace('T', " "),
                                        if b.weekly { "copie hebdomadaire" } else { "quotidienne" },
                                        b.count,
                                        size(b.size)
                                    );
                                    view! {
                                        <li role="option" tabindex="0" aria-selected=move || is_selected.get().to_string()
                                            style=move || if is_selected.get() { "padding: 0.25em 0.5em; cursor: pointer; font-weight: 600;" } else { "padding: 0.25em 0.5em; cursor: pointer;" }
                                            on:click={
                                                let pick = pick.clone();
                                                move |_| selected.set(Some(pick.clone()))
                                            }
                                            on:keydown={
                                                let pick = pick.clone();
                                                move |ev| {
                                                    if ev.key() == "Enter" || ev.key() == " " {
                                                        ev.prevent_default();
                                                        selected.set(Some(pick.clone()));
                                                    }
                                                }
                                            }>
                                            {label}
                                        </li>
                                    }
                                })
                                .collect_view()
                        }}
                    </ul>
                    <div class="dialog-buttons">
                        <button class="btn primary" prop:disabled=move || busy.get() || selected.with(Option::is_none) on:click=move |_| restore("merge")>"Fusionner les éléments manquants"</button>
                        <button class="btn" prop:disabled=move || busy.get() || selected.with(Option::is_none) on:click=move |_| restore("all")>"Tout remettre"</button>
                    </div>
                    <p class="dialog-note">"« Fusionner » ne remet que ce qui n'existe plus. « Tout remettre » redonne à chaque élément son contenu de la sauvegarde ; la version actuelle est gardée 60 jours dans la corbeille interne. Rien n'est supprimé."</p>
                    <div class="dialog-buttons">
                        <button class="btn" prop:disabled=move || busy.get() on:click=move |_| now()>"Sauvegarder maintenant"</button>
                        <button class="btn" on:click=move |_| open_folder()>"Ouvrir le dossier"</button>
                    </div>
                    <p class="dialog-note" aria-live="polite">{move || note.get()}</p>
                    <div class="dialog-buttons">
                        <button class="btn" on:click=move |_| close()>"Fermer"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
