//! « Nettoyer les doublons » (ruban Contacts) : groupes de doublons du dossier affiché, fiches côte à côte, choix de la
//! fiche conservée et des valeurs, aperçu de la fiche fusionnée, fusion, « Ce ne sont pas des doublons » et fusion
//! groupée des groupes sûrs. Tout le calcul est dans `thunderlook_contacts::dedupe` (commandes `dedupe_*`).

use std::collections::BTreeMap;

use leptos::{prelude::*, task::spawn_local};
use serde::Deserialize;
use thunderlook_model::contacts::ContactForm;

use crate::{ipc, state::use_ui};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSummary {
    pub ids: Vec<String>,
    pub titles: Vec<String>,
    pub reason: String,
    pub score: u8,
    pub safe: bool,
    pub chain: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub key: String,
    pub label: String,
    pub values: Vec<String>,
    pub chosen: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub ids: Vec<String>,
    pub titles: Vec<String>,
    pub keep: String,
    pub rows: Vec<Row>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bulk {
    pub groups: usize,
    pub deleted: usize,
}

/// Ouvre la boîte (ruban Contacts › « Nettoyer les doublons »).
pub fn open(ui: crate::state::Ui) {
    ui.dedupe_open.set(true);
}

/// Aperçu de la fiche fusionnée, sur quelques lignes.
fn preview_lines(f: &ContactForm) -> Vec<(&'static str, String)> {
    let join = |p: &[&str]| p.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ");
    let notes: String = f.notes.split_whitespace().collect::<Vec<_>>().join(" ");
    vec![
        ("Nom", join(&[&f.given_name, &f.surname])),
        ("Société", join(&[&f.company, &f.job_title, &f.department])),
        ("Courriels", join(&[&f.email1, &f.email2, &f.email3])),
        ("Téléphones", join(&[&f.mobile, &f.business, &f.home_phone])),
        ("Adresses", join(&[&f.work_street, &f.work_postal, &f.work_city, &f.home_street, &f.home_postal, &f.home_city])),
        ("Catégories", f.categories.clone()),
        ("Notes", notes.chars().take(300).collect()),
    ]
    .into_iter()
    .filter(|(_, v)| !v.is_empty())
    .collect()
}

#[component]
pub fn DedupeDialog() -> impl IntoView {
    let ui = use_ui();
    let groups = RwSignal::new(Vec::<GroupSummary>::new());
    let loading = RwSignal::new(false);
    let current = RwSignal::new(None::<usize>);
    let detail = RwSignal::new(None::<Detail>);
    let keep = RwSignal::new(String::new());
    let choices = RwSignal::new(BTreeMap::<String, String>::new());
    let merged = RwSignal::new(None::<ContactForm>);
    let busy = RwSignal::new(false);
    let recap = RwSignal::new(false);
    let note = RwSignal::new(None::<String>);
    // Numéros de la dernière demande de détail et d'aperçu : un clic rapide sur un autre groupe, ou sur un autre bouton
    // radio, ne doit pas être écrasé par la réponse plus lente de la demande précédente.
    let (detail_seq, preview_seq) = (StoredValue::new(0u32), StoredValue::new(0u32));

    let folder = move || Some(ui.contact_folder.get_untracked()).filter(|f| !f.is_empty());
    let reload = move || {
        loading.set(true);
        current.set(None);
        detail.set(None);
        merged.set(None);
        spawn_local(async move {
            match ipc::dedupe_scan(folder().as_deref()).await {
                Ok(g) => groups.set(g),
                Err(e) => note.set(Some(e)),
            }
            loading.set(false);
        });
    };
    Effect::new(move |_| {
        if ui.dedupe_open.get() {
            note.set(None);
            recap.set(false);
            reload();
        }
    });
    let close = move || ui.dedupe_open.set(false);

    let select = move |i: usize| {
        let Some(g) = groups.with_untracked(|g| g.get(i).cloned()) else { return };
        current.set(Some(i));
        detail.set(None);
        merged.set(None);
        let me = detail_seq.get_value().wrapping_add(1);
        detail_seq.set_value(me);
        spawn_local(async move {
            let res = ipc::dedupe_detail(&g.ids).await;
            if detail_seq.try_get_value() != Some(me) {
                return;
            }
            match res {
                Ok(d) => {
                    keep.set(d.keep.clone());
                    choices.set(d.rows.iter().filter(|r| !r.key.is_empty() && !r.chosen.is_empty()).map(|r| (r.key.clone(), r.chosen.clone())).collect());
                    detail.set(Some(d));
                }
                Err(e) => note.set(Some(e)),
            }
        });
    };
    // Aperçu : recalculé à chaque changement de fiche conservée ou de valeur choisie.
    Effect::new(move |_| {
        let (k, c) = (keep.get(), choices.get());
        let me = preview_seq.get_value().wrapping_add(1);
        preview_seq.set_value(me);
        let Some(d) = detail.get() else { return };
        spawn_local(async move {
            let res = ipc::dedupe_preview(&d.ids, &k, &c).await;
            if preview_seq.try_get_value() != Some(me) {
                return;
            }
            match res {
                Ok(f) => merged.set(Some(f)),
                Err(e) => note.set(Some(e)),
            }
        });
    });

    let merge = move || {
        let Some(d) = detail.get_untracked() else { return };
        let (k, c) = (keep.get_untracked(), choices.get_untracked());
        busy.set(true);
        spawn_local(async move {
            match ipc::dedupe_merge(&d.ids, &k, &c).await {
                Ok(n) => {
                    ui.status.set(Some(format!("Fusionné : {n} fiche(s) supprimée(s)")));
                    reload();
                }
                Err(e) => note.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let not_duplicates = move || {
        let Some(d) = detail.get_untracked() else { return };
        spawn_local(async move {
            match ipc::dedupe_ignore(&d.ids).await {
                Ok(()) => reload(),
                Err(e) => note.set(Some(e)),
            }
        });
    };
    let safe_stats = move || groups.with(|g| g.iter().filter(|g| g.safe).fold((0usize, 0usize), |(n, d), g| (n + 1, d + g.ids.len().saturating_sub(1))));
    let merge_safe = move || {
        busy.set(true);
        spawn_local(async move {
            match ipc::dedupe_merge_safe(folder().as_deref()).await {
                Ok(b) => {
                    ui.status.set(Some(format!("Fusion groupée : {} groupe(s), {} fiche(s) supprimée(s)", b.groups, b.deleted)));
                    recap.set(false);
                    reload();
                }
                Err(e) => note.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let list = move || {
        groups
            .get()
            .into_iter()
            .enumerate()
            .map(|(i, g)| {
                let label = g.titles.join("  =  ");
                let sub = format!("{} · {} %{}", g.reason, g.score, if g.safe { " · sûr" } else { "" });
                let chain = g.chain.clone();
                view! {
                    <li>
                        <button class="btn" style="width:100%;text-align:left;display:block" aria-pressed=move || (current.get() == Some(i)).to_string() on:click=move |_| select(i)>
                            <strong>{label}</strong>
                            <br />
                            <small>{sub}</small>
                            {chain.map(|c| view! { <br /><small>{c}</small> })}
                        </button>
                    </li>
                }
            })
            .collect_view()
    };

    let table = move || {
        let d = detail.get()?;
        let head = d
            .ids
            .iter()
            .zip(&d.titles)
            .map(|(id, t)| {
                let (a, b) = (id.clone(), id.clone());
                view! {
                    <th style="text-align:left">
                        <label>
                            <input type="radio" name="dedupe-keep" prop:checked=move || keep.get() == a on:change=move |_| keep.set(b.clone()) />
                            " Conserver : " {t.clone()}
                        </label>
                    </th>
                }
            })
            .collect_view();
        let body = d
            .rows
            .iter()
            .map(|r| {
                let r = r.clone();
                let cells = r
                    .values
                    .iter()
                    .zip(&d.ids)
                    .map(|(v, id)| {
                        let (key, id, v) = (r.key.clone(), id.clone(), v.clone());
                        let (key2, id2) = (key.clone(), id.clone());
                        view! {
                            <td>
                                {(!key.is_empty() && !v.is_empty())
                                    .then(|| {
                                        view! {
                                            <input type="radio" name=format!("dedupe-{key}") aria-label=format!("Reprendre cette valeur : {v}")
                                                prop:checked=move || choices.with(|c| c.get(&key2) == Some(&id2))
                                                on:change=move |_| choices.update(|c| { c.insert(key.clone(), id.clone()); }) />
                                            " "
                                        }
                                    })}
                                {v}
                            </td>
                        }
                    })
                    .collect_view();
                view! { <tr><th scope="row" style="text-align:left">{r.label.clone()}</th>{cells}</tr> }
            })
            .collect_view();
        Some(view! {
            <table style="width:100%;border-collapse:collapse" aria-label="Fiches à comparer">
                <thead><tr><th></th>{head}</tr></thead>
                <tbody>{body}</tbody>
            </table>
        })
    };

    view! {
        <Show when=move || ui.dedupe_open.get()>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog wide" role="dialog" aria-modal="true" aria-labelledby="dedupe-title" style="max-width:min(1200px,95vw)"
                    on:keydown=move |ev| {
                        if ev.key() == "Escape" {
                            ev.prevent_default();
                            close();
                        }
                    }>
                    <h2 id="dedupe-title">"Nettoyer les doublons"</h2>
                    {move || note.get().map(|n| view! { <p class="dialog-note" role="alert">{n}</p> })}
                    <p class="dialog-note">
                        {move || if loading.get() { "Analyse en cours…".to_string() } else { format!("{} groupe(s) de doublons probables dans le dossier affiché.", groups.with(Vec::len)) }}
                    </p>
                    <div style="display:grid;grid-template-columns:minmax(220px,1fr) 3fr;gap:12px;max-height:60vh">
                        <ul style="list-style:none;margin:0;padding:0;overflow:auto" aria-label="Groupes de doublons">{list}</ul>
                        <div style="overflow:auto">
                            {table}
                            <Show when=move || merged.with(Option::is_some)>
                                <h3>"Fiche fusionnée"</h3>
                                <dl aria-label="Aperçu de la fiche fusionnée">
                                    {move || merged.get().map(|f| preview_lines(&f).into_iter().map(|(l, v)| view! { <dt>{l}</dt><dd>{v}</dd> }).collect_view())}
                                </dl>
                            </Show>
                        </div>
                    </div>
                    <Show when=move || recap.get()>
                        <p class="dialog-note" role="alert">
                            {move || {
                                let (n, d) = safe_stats();
                                format!("{n} groupe(s) sûr(s) (même courriel ET même nom) : {d} fiche(s) seront supprimées, la plus complète de chaque groupe est conservée et reçoit les informations des autres. Chaque suppression reste annulable pendant 60 jours (corbeille interne).")
                            }}
                        </p>
                        <div class="dialog-buttons">
                            <button class="btn primary" prop:disabled=move || busy.get() || safe_stats().0 == 0 on:click=move |_| merge_safe()>"Confirmer la fusion"</button>
                            <button class="btn" on:click=move |_| recap.set(false)>"Annuler"</button>
                        </div>
                    </Show>
                    <div class="dialog-buttons">
                        <button class="btn primary" prop:disabled=move || busy.get() || detail.with(Option::is_none) on:click=move |_| merge()>"Fusionner"</button>
                        <button class="btn" prop:disabled=move || detail.with(Option::is_none) on:click=move |_| not_duplicates()>"Ce ne sont pas des doublons"</button>
                        <button class="btn" prop:disabled=move || busy.get() || safe_stats().0 == 0 on:click=move |_| recap.set(true)>"Fusionner les groupes sûrs (même courriel ET même nom)"</button>
                        <button class="btn" on:click=move |_| close()>"Fermer"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
