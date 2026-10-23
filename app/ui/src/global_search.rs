//! Recherche globale : menu de portée de la barre de recherche, et panneau des résultats groupés par type.
//! Le courrier passe par l'index plein texte existant (`ipc::search`) ; le reste par `thunderlook-search`.

use leptos::{prelude::*, task::spawn_local};
use serde::Deserialize;
use thunderlook_model::{MessageSummary, SearchQuery, SearchScope, SortField};

use crate::{
    actions::real_folder,
    ipc,
    state::{Module, Ui, use_ui},
};

/// Résultats de courrier affichés dans le panneau (le reste s'ouvre dans la liste de messages).
const MAIL_SHOWN: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Contact,
    Appointment,
    Task,
    Note,
}

impl Kind {
    const ORDER: [Kind; 4] = [Kind::Appointment, Kind::Contact, Kind::Task, Kind::Note];

    fn label(self) -> &'static str {
        match self {
            Kind::Contact => "Contacts",
            Kind::Appointment => "Rendez-vous",
            Kind::Task => "Tâches",
            Kind::Note => "Notes",
        }
    }
}

/// Même forme que `thunderlook_search::Hit` (la crate n'est pas compilable en WebAssembly).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub kind: Kind,
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub when: String,
}

/// Recherche en cours ou terminée, affichée dans le panneau.
#[derive(Clone, Debug, Default)]
pub struct Results {
    pub text: String,
    pub loading: bool,
    pub error: Option<String>,
    pub mail: Vec<MessageSummary>,
    pub mail_total: u32,
    pub hits: Vec<Hit>,
}

/// Sélection à faire quand le module visé a chargé ses données.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    /// Rendez-vous `a<id>` (toute occurrence) le jour `AAAA-MM-JJ`.
    Appointment { base: String, date: String },
    /// Contact `p<id>`, avec les dossiers déjà parcourus pour le trouver.
    Contact { id: String, tried: Vec<String> },
}

pub fn close(ui: Ui) {
    ui.global.set(None);
}

/// Lance la recherche de `text` dans tous les modules (texte vide : ferme le panneau).
pub fn run(ui: Ui, text: String) {
    if text.is_empty() {
        close(ui);
        return;
    }
    ui.global.set(Some(Results { text: text.clone(), loading: true, ..Results::default() }));
    let current = move |text: &str| ui.global.with_untracked(|g| g.as_ref().is_some_and(|g| g.text == text));
    let t = text.clone();
    spawn_local(async move {
        let hits = ipc::global_search(&t).await;
        if !current(&t) {
            return;
        }
        ui.global.update(|g| {
            if let Some(g) = g {
                match hits {
                    Ok(h) => g.hits = h,
                    Err(e) => g.error = Some(e),
                }
            }
        });
    });
    let folder = real_folder(&ui.folder.get_untracked());
    spawn_local(async move {
        // Pas de dossier encore (démarrage) : pas de courrier à chercher.
        let mail = if folder.is_empty() {
            Ok((0, Vec::new()))
        } else {
            let q = SearchQuery { text: text.clone(), scope: SearchScope::All, folder_id: folder };
            match ipc::search(&q, SortField::Date, true).await {
                Ok(v) => ipc::list_rows(v.token, 0, MAIL_SHOWN).await.map(|rows| (v.count, rows)),
                Err(e) => Err(e),
            }
        };
        if !current(&text) {
            return;
        }
        ui.global.update(|g| {
            if let Some(g) = g {
                g.loading = false;
                match mail {
                    Ok((total, rows)) => (g.mail_total, g.mail) = (total, rows),
                    Err(e) => g.error = Some(e),
                }
            }
        });
    });
}

/// Ouvre le message dans la liste de courrier, via la même recherche (le dossier d'un résultat n'est qu'un libellé).
fn open_mail(ui: Ui, id: Option<String>) {
    let Some(text) = ui.global.with_untracked(|g| g.as_ref().map(|g| g.text.clone())) else { return };
    close(ui);
    ui.module.set(Module::Mail);
    ui.search_scope.set(SearchScope::All);
    ui.pending_message.set(id);
    if ui.search_text.get_untracked() == text {
        ui.view_epoch.update(|e| *e += 1);
    } else {
        ui.search_text.set(text);
    }
}

/// Bascule sur le calendrier, au jour `date` (`AAAA-MM-JJ`), et sélectionne le rendez-vous `a<id>`.
pub fn open_appointment(ui: Ui, id: &str, date: &str) {
    let base = id.split('@').next().unwrap_or(id).to_string();
    ui.module.set(Module::Calendar);
    ui.cal_anchor.set(date.to_string());
    ui.pending_open.set(Some(Pending::Appointment { base, date: date.to_string() }));
}

pub fn open_task(ui: Ui, id: &str) {
    ui.module.set(Module::Tasks);
    ui.task_filter.set(String::new());
    ui.task_selected.set(Some(id.to_string()));
}

fn open_contact(ui: Ui, id: &str) {
    ui.module.set(Module::Contacts);
    ui.contact_filter.set(String::new());
    ui.pending_open.set(Some(Pending::Contact { id: id.to_string(), tried: Vec::new() }));
}

/// Les notes n'ont pas de sélection accessible d'ici : la note s'ouvre dans sa boîte.
fn open_note(ui: Ui, id: &str) {
    ui.module.set(Module::Notes);
    let id = id.to_string();
    spawn_local(async move {
        match ipc::note_form(Some(&id)).await {
            Ok(f) => ui.note_form.set(Some(f)),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

fn open_hit(ui: Ui, hit: &Hit) {
    close(ui);
    match hit.kind {
        Kind::Appointment => open_appointment(ui, &hit.id, hit.when.get(..10).unwrap_or_default()),
        Kind::Contact => open_contact(ui, &hit.id),
        Kind::Task => open_task(ui, &hit.id),
        Kind::Note => open_note(ui, &hit.id),
    }
}

/// Termine les sélections demandées par [`Pending`] quand les données du module arrivent.
pub fn watch(ui: Ui) {
    Effect::new(move |_| {
        let Some(Pending::Appointment { base, date }) = ui.pending_open.get() else { return };
        if ui.module.get() != Module::Calendar {
            ui.pending_open.set(None);
            return;
        }
        let found = ui.cal_view.with(|v| {
            let day = v.as_ref()?.days.iter().find(|d| d.date == date)?;
            day.all_day.iter().chain(day.timed.iter().map(|t| &t.event)).find(|e| e.id.split('@').next() == Some(base.as_str())).cloned()
        });
        if let Some(e) = found {
            ui.cal_selected.set(Some(e));
            ui.pending_open.set(None);
        }
    });
    Effect::new(move |_| {
        let Some(Pending::Contact { id, tried }) = ui.pending_open.get() else { return };
        if ui.module.get() != Module::Contacts {
            ui.pending_open.set(None);
            return;
        }
        let Some((folder, found)) = ui.contact_list.with(|l| l.as_ref().map(|l| (l.folder_id.clone(), l.rows.iter().any(|r| r.id == id)))) else { return };
        // Liste du dossier précédent, pas encore remplacée.
        if folder != ui.contact_folder.get_untracked() {
            return;
        }
        if found {
            ui.contact_selected.set(Some(id));
            ui.pending_open.set(None);
            return;
        }
        let mut tried = tried;
        tried.push(folder);
        match ui.contact_folders.with(|f| f.iter().find(|f| !tried.contains(&f.id)).map(|f| f.id.clone())) {
            Some(next) => {
                ui.pending_open.set(Some(Pending::Contact { id, tried }));
                ui.contact_folder.set(next);
            }
            // Introuvable dans les listes : la fiche s'affiche quand même.
            None => {
                ui.contact_selected.set(Some(id));
                ui.pending_open.set(None);
            }
        }
    });
}

const STYLE: &str = "
.search input[type=search] { padding-right: 150px; }
.search .gs-scope { position: absolute; right: 4px; height: 24px; max-width: 140px; padding: 0 4px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); color: var(--text); font-size: var(--fs-sm); text-overflow: ellipsis; }
.gs-backdrop { position: fixed; inset: 0; z-index: 20; }
.gs-panel { position: fixed; z-index: 21; top: calc(var(--target) + var(--u) * 3); left: 50%; transform: translateX(-50%); width: min(620px, calc(100vw - 32px)); max-height: 70vh; overflow-y: auto; padding: var(--u); background: var(--bg); border: 1px solid var(--border-strong); border-radius: var(--radius); box-shadow: 0 8px 24px rgb(0 0 0 / 16%); }
.gs-panel h3 { margin: calc(var(--u) * 2) calc(var(--u) * 2) var(--u); font-size: var(--fs-sm); color: var(--text-2); font-weight: 600; }
.gs-panel .gs-empty { padding: calc(var(--u) * 3); color: var(--text-2); }
.gs-item { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 0 calc(var(--u) * 3); width: 100%; padding: var(--u) calc(var(--u) * 2); background: transparent; border: 0; border-radius: var(--radius); text-align: left; }
.gs-item:hover, .gs-item:focus-visible { background: var(--hover); }
.gs-item .gs-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.gs-item .gs-when { color: var(--text-2); font-size: var(--fs-sm); }
.gs-item .gs-snippet { grid-column: 1 / -1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); font-size: var(--fs-sm); }
";

/// Menu de portée, dans la zone de recherche : « Module affiché » (comportement d'origine) ou « Tout ».
#[component]
pub fn ScopeMenu() -> impl IntoView {
    let ui = use_ui();
    let on_change = move |ev: leptos::ev::Event| {
        let all = event_target_value(&ev) == "all";
        ui.search_all.set(all);
        let text = ui.refs.search.get_untracked().map(|s| s.value().trim().to_string()).unwrap_or_default();
        if all {
            ui.search_text.set(String::new());
            run(ui, text);
        } else {
            close(ui);
            ui.search_text.set(text);
        }
    };
    view! {
        <style>{STYLE}</style>
        <select class="gs-scope" aria-label="Portée de la recherche" title="Portée de la recherche" on:change=on_change>
            <option value="module" selected=move || !ui.search_all.get()>"Module affiché"</option>
            <option value="all" selected=move || ui.search_all.get()>"Tout (courrier, rendez-vous, contacts, tâches, notes)"</option>
        </select>
    }
}

#[component]
pub fn GlobalSearchPanel() -> impl IntoView {
    let ui = use_ui();
    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Escape" {
            ev.prevent_default();
            close(ui);
            if let Some(s) = ui.refs.search.get_untracked() {
                let _ = s.focus();
            }
        }
    };
    let body = move || {
        let Some(g) = ui.global.get() else { return ().into_any() };
        let empty = g.mail.is_empty() && g.hits.is_empty();
        let mut out = Vec::new();
        if !g.mail.is_empty() {
            let more = g.mail_total.saturating_sub(g.mail.len() as u32);
            let items = g
                .mail
                .iter()
                .map(|m| {
                    let id = m.id.clone();
                    let subject = if m.subject.trim().is_empty() { "(aucun objet)".to_string() } else { m.subject.clone() };
                    view! {
                        <button class="gs-item" on:click=move |_| open_mail(ui, Some(id.clone()))>
                            <span class="gs-title">{subject}</span>
                            <span class="gs-when">{m.time_label.clone()}</span>
                            <span class="gs-snippet">{m.from.clone()}</span>
                        </button>
                    }
                })
                .collect_view();
            out.push(
                view! {
                    <section aria-label="Courrier">
                        <h3>"Courrier"</h3>
                        {items}
                        {(more > 0).then(|| view! {
                            <button class="gs-item" on:click=move |_| open_mail(ui, None)>
                                <span class="gs-title">{format!("Afficher les {} résultats dans le courrier", g.mail_total)}</span>
                            </button>
                        })}
                    </section>
                }
                .into_any(),
            );
        }
        for kind in Kind::ORDER {
            let hits: Vec<Hit> = g.hits.iter().filter(|h| h.kind == kind).cloned().collect();
            if hits.is_empty() {
                continue;
            }
            let items = hits
                .into_iter()
                .map(|h| {
                    let title = if h.title.trim().is_empty() { "(sans titre)".to_string() } else { h.title.clone() };
                    let (when, snippet) = (h.when.clone(), h.snippet.clone());
                    view! {
                        <button class="gs-item" on:click=move |_| open_hit(ui, &h)>
                            <span class="gs-title">{title}</span>
                            <span class="gs-when">{when}</span>
                            <span class="gs-snippet">{snippet}</span>
                        </button>
                    }
                })
                .collect_view();
            out.push(view! { <section aria-label=kind.label()><h3>{kind.label()}</h3>{items}</section> }.into_any());
        }
        view! {
            {out}
            {g.loading.then(|| view! { <div class="gs-empty" role="status">"Recherche…"</div> })}
            {(!g.loading && empty && g.error.is_none()).then(|| view! { <div class="gs-empty" role="status">"Aucun résultat."</div> })}
            {g.error.map(|e| view! { <div class="gs-empty status-error" role="alert">{e}</div> })}
        }
        .into_any()
    };
    view! {
        <Show when=move || ui.global.with(Option::is_some)>
            <div class="gs-backdrop" on:pointerdown=move |_| close(ui)></div>
            <div class="gs-panel" role="region" aria-label="Résultats de la recherche globale" on:keydown=on_key>
                {body}
            </div>
        </Show>
    }
}
