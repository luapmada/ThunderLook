//! Boîte « Créer une règle » (clic droit sur un message › Règles), comme la boîte simplifiée d'Outlook : conditions
//! préremplies d'après le message (expéditeur, objet, destinataire), déplacement vers un dossier, exécution
//! immédiate facultative sur le dossier du message. La règle rejoint le jeu de règles du compte du message.

use leptos::{html, prelude::*, task::spawn_local};
use thunderlook_model::{NewRule, RuleDraft, RuleRunRequest, RunFilter};
use wasm_bindgen::JsCast;

use crate::{
    actions,
    dialogs::flat_folders,
    ipc,
    state::{Dialog, use_ui},
};

/// Section du volet des dossiers (`s<source>`) qui contient le dossier `folder`.
fn section_of(ui: crate::state::Ui, folder: &str) -> Option<String> {
    fn has(folders: &[thunderlook_model::Folder], id: &str) -> bool {
        folders.iter().any(|f| f.id == id || has(&f.children, id))
    }
    ui.tree.with_untracked(|t| {
        t.as_ref()?.sections.iter().filter(|s| s.id.starts_with('s') && s.id != "searches").find(|s| has(&s.folders, folder)).map(|s| s.id.clone())
    })
}

/// « Nom <adresse> », ou ce qui est connu des deux.
fn who(name: &str, addr: &str) -> String {
    match (name.trim(), addr.trim()) {
        ("", "") => "(inconnu)".into(),
        (n, "") => n.into(),
        ("", a) => a.into(),
        (n, a) if n.eq_ignore_ascii_case(a) => a.into(),
        (n, a) => format!("{n} <{a}>"),
    }
}

#[component]
pub fn CreateRuleDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let draft = RwSignal::new(None::<RuleDraft>);
    let name = RwSignal::new(String::new());
    let from_on = RwSignal::new(false);
    let subject_on = RwSignal::new(false);
    let subject = RwSignal::new(String::new());
    let to_on = RwSignal::new(false);
    let move_on = RwSignal::new(false);
    let folder = RwSignal::new(String::new());
    let run_now = RwSignal::new(false);
    let note = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    // À chaque ouverture : valeurs proposées d'après le message sélectionné, cases décochées comme dans Outlook.
    Effect::new(move |_| {
        if ui.dialog.get() != Some(Dialog::CreateRule) {
            return;
        }
        draft.set(None);
        note.set(None);
        for s in [from_on, subject_on, to_on, move_on, run_now] {
            s.set(false);
        }
        folder.set(String::new());
        let Some(id) = ui.selected.get_untracked() else {
            ui.dialog.set(None);
            ui.error.set(Some("Sélectionnez un message.".into()));
            return;
        };
        spawn_local(async move {
            let result = ipc::rule_draft(&id).await;
            // Boîte fermée, ou rouverte sur un autre message, pendant la lecture : la réponse est périmée (elle
            // aurait rempli la boîte avec l'ancien message).
            if ui.dialog.get_untracked() != Some(Dialog::CreateRule) || ui.selected.get_untracked().as_deref() != Some(id.as_str()) {
                return;
            }
            match result {
                Ok(d) => {
                    let default_name = [&d.from_name, &d.from_addr, &d.subject].into_iter().find(|s| !s.trim().is_empty()).cloned().unwrap_or_default();
                    name.set(default_name);
                    subject.set(d.subject.clone());
                    draft.set(Some(d));
                    request_animation_frame(move || {
                        if let Some(r) = root.get_untracked() {
                            if let Ok(Some(el)) = r.query_selector("input") {
                                if let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() {
                                    let _ = el.focus();
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    ui.dialog.set(None);
                    ui.error.set(Some(e));
                }
            }
        });
    });

    let close = move || {
        ui.dialog.set(None);
        if let Some(l) = ui.refs.list.get_untracked() {
            let _ = l.focus();
        }
    };

    let submit = move || {
        if busy.get_untracked() {
            return;
        }
        let Some(d) = draft.get_untracked() else { return };
        if !move_on.get_untracked() || folder.get_untracked().is_empty() {
            note.set(Some("Choisissez le dossier vers lequel déplacer les éléments.".into()));
            return;
        }
        let rule = NewRule {
            message_id: d.message_id.clone(),
            name: name.get_untracked(),
            from: from_on.get_untracked(),
            subject: subject_on.get_untracked().then(|| subject.get_untracked()),
            sent_to: to_on.get_untracked(),
            folder_id: folder.get_untracked(),
        };
        let run = run_now.get_untracked();
        busy.set(true);
        spawn_local(async move {
            let id = match ipc::create_rule(&rule).await {
                Ok(id) => id,
                Err(e) => {
                    note.set(Some(e));
                    busy.set(false);
                    return;
                }
            };
            let label = rule.name.trim().to_string();
            if run {
                let req = RuleRunRequest {
                    rule_ids: vec![id],
                    folder_id: d.folder_id.clone(),
                    subfolders: false,
                    filter: RunFilter::All,
                    apply: true,
                    only: Vec::new(),
                    confirm_moves: None,
                };
                match ipc::run_rules(&req).await {
                    Ok(r) if r.needs_confirmation => ui.error.set(Some(format!(
                        "Règle « {label} » créée, mais pas exécutée : {} messages changeraient de dossier (plus de {}). Utilisez Règles et alertes › Exécuter les règles pour un aperçu.",
                        r.moves,
                        thunderlook_model::RULE_MOVES_LIMIT
                    ))),
                    Ok(r) => {
                        ui.status.set(Some(format!("Règle « {label} » créée et exécutée : {} élément(s) déplacé(s)", r.changed)));
                        if let Some(e) = r.errors.into_iter().next() {
                            ui.error.set(Some(e));
                        }
                        actions::reopen_view(ui);
                        actions::refresh_tree(ui);
                    }
                    Err(e) => ui.error.set(Some(format!("Règle « {label} » créée ; exécution impossible : {e}"))),
                }
            } else {
                ui.status.set(Some(format!("Règle « {label} » créée")));
            }
            busy.set(false);
            close();
        });
    };

    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        match ev.key().as_str() {
            "Escape" => close(),
            // Entrée valide, sauf sur une liste déroulante ou un bouton (qui garde son propre effet).
            "Enter" if ev.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()).is_none_or(|t| !matches!(t.tag_name().as_str(), "SELECT" | "BUTTON")) => {
                submit()
            }
            "Tab" => {
                // Le focus reste dans la boîte.
                if let Some(r) = root.get_untracked() {
                    // OK grisé (chargement, enregistrement) exclu : `focus()` y échoue et Tab restait bloqué.
                    let Ok(list) = r.query_selector_all("input:not([disabled]), select:not([disabled]), button:not([disabled])") else { return };
                    let els: Vec<web_sys::HtmlElement> = (0..list.length()).filter_map(|i| list.item(i)?.dyn_into().ok()).collect();
                    let active = document().active_element();
                    let pos = els.iter().position(|e| active.as_ref().is_some_and(|a| a == e.unchecked_ref::<web_sys::Element>()));
                    let n = els.len() as i32;
                    if n > 0 {
                        // Focus hors de la liste (après un retrait de ligne) : premier ou dernier contrôle, sans en sauter un.
                        let next = match pos {
                            Some(p) => (p as i32 + if ev.shift_key() { -1 } else { 1 }).rem_euclid(n),
                            None if ev.shift_key() => n - 1,
                            None => 0,
                        };
                        let _ = els[next as usize].focus();
                    }
                }
            }
            _ => return,
        }
        ev.prevent_default();
        ev.stop_propagation();
    };

    let body = move || {
        let Some(d) = draft.get() else { return view! { <p class="dialog-note">"Chargement…"</p> }.into_any() };
        let from_label = format!("De {}", who(&d.from_name, &d.from_addr));
        let to_label = format!("Envoyé à {}", who(&d.to_name, &d.to_addr));
        let run_label = format!("Exécuter cette règle maintenant sur les messages du dossier actuel ({})", d.folder_name);
        view! {
            <label class="field">
                <span>"Nom de la règle"</span>
                <input type="text" prop:value=move || name.get() on:input=move |ev| name.set(event_target_value(&ev)) />
            </label>
            <p class="dialog-note">"Lorsque je reçois un courrier électronique avec toutes les conditions sélectionnées"</p>
            <label class="check">
                <input type="checkbox" prop:checked=move || from_on.get() on:change=move |ev| from_on.set(event_target_checked(&ev)) />
                <span>{from_label}</span>
            </label>
            <label class="check">
                <input type="checkbox" prop:checked=move || subject_on.get() on:change=move |ev| subject_on.set(event_target_checked(&ev)) />
                <span>"L'objet contient"</span>
            </label>
            <input
                type="text"
                aria-label="Texte de l'objet"
                disabled=move || !subject_on.get()
                prop:value=move || subject.get()
                on:input=move |ev| subject.set(event_target_value(&ev))
            />
            <label class="check">
                <input type="checkbox" prop:checked=move || to_on.get() on:change=move |ev| to_on.set(event_target_checked(&ev)) />
                <span>{to_label}</span>
            </label>
            <p class="dialog-note">"Exécuter les actions suivantes"</p>
            <label class="check">
                <input type="checkbox" prop:checked=move || move_on.get() on:change=move |ev| move_on.set(event_target_checked(&ev)) />
                <span>"Déplacer l'élément vers le dossier :"</span>
            </label>
            <select
                aria-label="Dossier de destination"
                disabled=move || !move_on.get()
                on:change=move |ev| folder.set(event_target_value(&ev))
            >
                <option value="" selected=move || folder.get().is_empty()>"Choisir un dossier…"</option>
                {flat_folders(ui)
                    .into_iter()
                    .map(|(id, fname, depth, section)| {
                        let label = format!("{}{fname}  ({section})", "\u{a0}\u{a0}".repeat(depth));
                        let value = id.clone();
                        view! { <option value=value selected=move || folder.get() == id>{label}</option> }
                    })
                    .collect_view()}
            </select>
            <label class="check">
                <input type="checkbox" prop:checked=move || run_now.get() on:change=move |ev| run_now.set(event_target_checked(&ev)) />
                <span>{run_label}</span>
            </label>
            // Règle d'un compte qui déplace hors du compte (PST, autre compte) : comme Outlook en IMAP, le message
            // quitte le serveur ; on le dit avant l'enregistrement.
            {
                let account = d.account_section.clone();
                move || {
                    let target = folder.get();
                    let away = move_on.get()
                        && !target.is_empty()
                        && account.as_ref().is_some_and(|a| section_of(ui, &target).as_ref() != Some(a));
                    away.then(|| view! {
                        <p class="rules-warn" role="note">
                            "ⓘ Ce dossier n'est pas dans le compte : comme dans Outlook, les messages déplacés par la règle seront retirés du serveur, et leur seule copie sera celle de ThunderLook."
                        </p>
                    })
                }
            }
            <p class="dialog-note">{format!("Jeu de règles : {}", d.rule_set)}</p>
            {d.note.map(|n| view! { <p class="dialog-note">{format!("ⓘ {n}")}</p> })}
        }
        .into_any()
    };

    view! {
        <Show when=move || ui.dialog.get() == Some(Dialog::CreateRule)>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="create-rule-title" node_ref=root on:keydown=on_key>
                    <h2 id="create-rule-title">"Créer une règle"</h2>
                    {body}
                    {move || note.get().map(|n| view! { <p class="rules-warn" role="alert">{n}</p> })}
                    <div class="dialog-buttons">
                        <button class="btn primary" disabled=move || busy.get() || draft.with(Option::is_none) on:click=move |_| submit()>"OK"</button>
                        <button class="btn" on:click=move |_| close()>"Annuler"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
