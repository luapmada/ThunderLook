//! Boîte « Options du courrier indésirable » : expéditeurs bloqués et approuvés (adresses ou « @domaine »).

use leptos::{html, prelude::*, task::spawn_local};
use thunderlook_model::JunkLists;

use crate::{
    icons::*,
    ipc,
    state::{Dialog, use_ui},
};

#[component]
pub fn JunkOptionsDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let lists = RwSignal::new(JunkLists::default());
    let entry = RwSignal::new(String::new());

    Effect::new(move |_| {
        if ui.dialog.get() != Some(Dialog::JunkOptions) {
            return;
        }
        // Une adresse tapée puis abandonnée (Annuler) ne reparaît pas à l'ouverture suivante.
        entry.set(String::new());
        spawn_local(async move {
            match ipc::junk_lists().await {
                Ok(l) => lists.set(l),
                Err(e) => ui.error.set(Some(e)),
            }
        });
    });

    let close = move || {
        ui.dialog.set(None);
        if let Some(l) = ui.refs.list.get_untracked() {
            let _ = l.focus();
        }
    };
    let save = move || {
        let l = lists.get_untracked();
        spawn_local(async move {
            match ipc::set_junk_lists(&l).await {
                Ok(()) => {
                    ui.status.set(Some(format!("Courrier indésirable : {} bloqué(s), {} approuvé(s)", l.blocked.len(), l.safe.len())));
                    close();
                }
                Err(e) => ui.error.set(Some(e)),
            }
        });
    };
    let add = move |blocked: bool| {
        let e = entry.get_untracked();
        if e.trim().is_empty() {
            return;
        }
        lists.update(|l| l.add(&e, blocked));
        entry.set(String::new());
    };
    let column = move |title: &'static str, blocked: bool| {
        view! {
            <section class="junk-col" aria-label=title>
                <h3>{title}</h3>
                <ul class="junk-list">
                    {move || {
                        let items = lists.with(|l| if blocked { l.blocked.clone() } else { l.safe.clone() });
                        if items.is_empty() {
                            return view! { <li class="junk-empty">"(aucun)"</li> }.into_any();
                        }
                        items
                            .into_iter()
                            .map(|x| {
                                let gone = x.clone();
                                view! {
                                    <li>
                                        <span>{x.clone()}</span>
                                        <button class="icon-btn small" aria-label=format!("Retirer {x}") title="Retirer" on:click=move |_| {
                                            let g = gone.clone();
                                            lists.update(|l| {
                                                if blocked { l.blocked.retain(|y| *y != g) } else { l.safe.retain(|y| *y != g) }
                                            });
                                        }>
                                            <Svg icon=X size=12 />
                                        </button>
                                    </li>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }}
                </ul>
            </section>
        }
    };

    view! {
        <Show when=move || ui.dialog.get() == Some(Dialog::JunkOptions)>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog wide" role="dialog" aria-modal="true" aria-labelledby="junk-title" node_ref=root
                    on:keydown=move |ev| {
                        if ev.key() == "Escape" {
                            ev.prevent_default();
                            close();
                        }
                    }>
                    <h2 id="junk-title">"Options du courrier indésirable"</h2>
                    <div class="junk-cols">
                        {column("Expéditeurs bloqués", true)}
                        {column("Expéditeurs approuvés", false)}
                    </div>
                    <label class="field">
                        <span>"Adresse ou domaine (@exemple.fr)"</span>
                        <input type="text" aria-label="Adresse ou domaine" prop:value=move || entry.get() on:input=move |ev| entry.set(event_target_value(&ev)) />
                    </label>
                    <div class="junk-add">
                        <button class="btn" on:click=move |_| add(true)>"Bloquer"</button>
                        <button class="btn" on:click=move |_| add(false)>"Approuver"</button>
                    </div>
                    <p class="dialog-note">"Les nouveaux messages d'un expéditeur bloqué vont dans le Courrier indésirable de leur compte, sans réponse automatique ni règle. Un expéditeur approuvé n'est jamais bloqué."</p>
                    <div class="dialog-buttons">
                        <button class="btn primary" on:click=move |_| save()>"OK"</button>
                        <button class="btn" on:click=move |_| close()>"Annuler"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
