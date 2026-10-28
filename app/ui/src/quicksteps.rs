//! Actions rapides (J6) : exécution sur la sélection, et boîte « Gérer les actions rapides » avec son éditeur
//! (nom, suite d'actions). Une action rapide à configurer s'ouvre dans l'éditeur à sa première utilisation.

use leptos::{html, prelude::*, task::spawn_local};
use thunderlook_model::{
    ComposeKind, FlagAction, MessageOp,
    quicksteps::{QuickAction, QuickStep, default_quick_steps},
};
use wasm_bindgen::JsCast;

use crate::{
    actions,
    dialogs::flat_folders,
    ipc,
    state::{Dialog, Ui, use_ui},
};

const FLAGS: [FlagAction; 7] =
    [FlagAction::Today, FlagAction::Tomorrow, FlagAction::ThisWeek, FlagAction::NextWeek, FlagAction::NoDate, FlagAction::Complete, FlagAction::Clear];

/// Libellé dans la galerie : « : ? » tant qu'un paramètre manque, comme Outlook.
pub fn step_label(step: &QuickStep) -> String {
    if step.is_configured() { step.name.clone() } else { format!("{} : ?", step.name) }
}

pub fn save(ui: Ui) {
    let steps = ui.quick_steps.get_untracked();
    spawn_local(async move {
        if let Err(e) = ipc::save_quick_steps(&steps).await {
            ui.error.set(Some(e));
        }
    });
}

/// Exécute l'action rapide `index` sur la sélection : modifications, puis rédaction éventuelle (qui lit le
/// message d'origine), puis copies, puis le déplacement ou la suppression (une seule destination, la dernière).
pub fn run(ui: Ui, index: usize) {
    let Some(step) = ui.quick_steps.with_untracked(|v| v.get(index).cloned()) else { return };
    if !step.is_configured() {
        ui.dialog.set(Some(Dialog::QuickSteps { edit: Some(index), first_use: true }));
        return;
    }
    let compose = step.actions.iter().find(|a| a.composes()).cloned();
    let needs_message = step.actions.iter().any(|a| !matches!(a, QuickAction::NewMessage { .. }));
    let sel = actions::selection(ui);
    if needs_message && sel.is_none() {
        ui.error.set(Some("Sélectionnez d'abord un message.".into()));
        return;
    }
    if compose.is_some() && ui.compose.with_untracked(Option::is_some) && ui.compose_dirty.get_untracked() {
        ui.error.set(Some("Un message est déjà en cours de rédaction : enregistrez-le ou ignorez-le d'abord.".into()));
        return;
    }
    let mut edits = Vec::new();
    let mut copies = Vec::new();
    let mut destination = None;
    for a in &step.actions {
        match a {
            QuickAction::MarkRead => edits.push(MessageOp::Read { read: true }),
            QuickAction::MarkUnread => edits.push(MessageOp::Read { read: false }),
            QuickAction::Category { name } => edits.push(MessageOp::Category { name: name.trim().to_string(), on: true }),
            QuickAction::ClearCategories => edits.push(MessageOp::ClearCategories),
            QuickAction::Flag { action } => edits.push(MessageOp::Flag { action: *action }),
            QuickAction::CopyTo { folder_id: Some(f) } => copies.push(MessageOp::Copy { folder_id: f.clone() }),
            QuickAction::MoveTo { folder_id: Some(f) } => destination = Some(MessageOp::Move { folder_id: f.clone() }),
            QuickAction::Delete => destination = Some(MessageOp::Delete),
            QuickAction::DeletePermanently => destination = Some(MessageOp::DeletePermanently),
            _ => {}
        }
    }
    let source = ui.selected.get_untracked();
    spawn_local(async move {
        let mut changed = false;
        let mut structural = false;
        let mut category = false;
        let ops: Vec<MessageOp> = edits.into_iter().chain(copies).chain(destination).collect();
        // Modifications d'abord : elles portent sur les éléments encore en place.
        let (before, after): (Vec<_>, Vec<_>) = ops.into_iter().partition(|op| !matches!(op, MessageOp::Copy { .. } | MessageOp::Move { .. } | MessageOp::Delete | MessageOp::DeletePermanently));
        if let Some(sel) = &sel {
            for op in &before {
                category |= matches!(op, MessageOp::Category { .. } | MessageOp::ClearCategories);
                if let Err(e) = ipc::apply_op(sel, op).await {
                    ui.error.set(Some(e));
                    return;
                }
                changed = true;
            }
        }
        if let Some(c) = compose {
            let (kind, to) = match c {
                QuickAction::Reply => (ComposeKind::Reply, String::new()),
                QuickAction::ReplyAll => (ComposeKind::ReplyAll, String::new()),
                QuickAction::Forward { to } => (ComposeKind::Forward, to),
                _ => (ComposeKind::New, match c {
                    QuickAction::NewMessage { to } => to,
                    _ => String::new(),
                }),
            };
            let src = if kind == ComposeKind::New { None } else { source.clone() };
            match ipc::compose(kind, src.as_deref(), Some(ui.folder.get_untracked().as_str())).await {
                Ok(mut d) => {
                    if !to.trim().is_empty() {
                        d.to = if d.to.trim().is_empty() { to } else { format!("{}; {to}", d.to) };
                    }
                    ui.compose_dirty.set(false);
                    ui.compose.set(Some(d));
                    ui.compose_epoch.update(|e| *e += 1);
                }
                Err(e) => {
                    ui.error.set(Some(e));
                    return;
                }
            }
        }
        if let Some(sel) = &sel {
            for op in &after {
                if let Err(e) = ipc::apply_op(sel, op).await {
                    ui.error.set(Some(e));
                    break;
                }
                changed = true;
                structural = true;
            }
        }
        ui.status.set(Some(format!("Action rapide « {} » exécutée", step.name)));
        if changed {
            if structural {
                actions::reopen_view(ui);
            } else {
                actions::reload_rows(ui);
            }
            actions::refresh_tree(ui);
        }
        if category {
            actions::load_categories(ui);
        }
    });
}

/// Forme de la suite d'actions : rang de chaque action dans `QuickAction::kinds()`. Les lignes de l'éditeur ne
/// sont recréées que quand elle change (ajout, retrait, autre sorte d'action), jamais pendant la saisie d'un
/// paramètre.
fn shape(actions: &[QuickAction]) -> Vec<usize> {
    let kinds = QuickAction::kinds();
    actions.iter().map(|a| kinds.iter().position(|x| x.same_kind(a)).unwrap_or(0)).collect()
}

/// Contrôle du paramètre d'une action, ou rien.
fn parameter(ui: Ui, i: usize, action: QuickAction, list: RwSignal<Vec<QuickAction>>) -> AnyView {
    let set = move |a: QuickAction| list.update(|v| v[i] = a);
    match action {
        QuickAction::MoveTo { folder_id } | QuickAction::CopyTo { folder_id } => {
            let is_move = matches!(list.get_untracked()[i], QuickAction::MoveTo { .. });
            let current = folder_id.unwrap_or_default();
            view! {
                <select aria-label="Dossier" on:change=move |ev| {
                    let v = event_target_value(&ev);
                    let f = (!v.is_empty()).then_some(v);
                    set(if is_move { QuickAction::MoveTo { folder_id: f } } else { QuickAction::CopyTo { folder_id: f } });
                }>
                    <option value="" selected=current.is_empty()>"Choisir un dossier…"</option>
                    {flat_folders(ui)
                        .into_iter()
                        .map(|(id, name, depth, section)| {
                            let selected = id == current;
                            view! { <option value=id selected=selected>{format!("{}{name}  ({section})", "\u{a0}\u{a0}".repeat(depth))}</option> }
                        })
                        .collect_view()}
                </select>
            }
            .into_any()
        }
        QuickAction::Category { name } => view! {
            <input type="text" aria-label="Catégorie" list="qs-categories" prop:value=name
                on:input=move |ev| set(QuickAction::Category { name: event_target_value(&ev) }) />
            <datalist id="qs-categories">
                {move || ui.categories.get().into_iter().map(|c| view! { <option value=c.name></option> }).collect_view()}
            </datalist>
        }
        .into_any(),
        QuickAction::Flag { action } => view! {
            <select aria-label="Indicateur" on:change=move |ev| {
                let k: usize = event_target_value(&ev).parse().unwrap_or(0);
                set(QuickAction::Flag { action: FLAGS[k.min(FLAGS.len() - 1)] });
            }>
                {FLAGS.iter().enumerate().map(|(k, f)| view! { <option value=k.to_string() selected=*f == action>{f.label()}</option> }).collect_view()}
            </select>
        }
        .into_any(),
        QuickAction::Forward { to } => view! {
            <input type="text" aria-label="À" placeholder="À : adresse ; adresse" prop:value=to
                on:input=move |ev| set(QuickAction::Forward { to: event_target_value(&ev) }) />
        }
        .into_any(),
        QuickAction::NewMessage { to } => view! {
            <input type="text" aria-label="À" placeholder="À : adresse ; adresse" prop:value=to
                on:input=move |ev| set(QuickAction::NewMessage { to: event_target_value(&ev) }) />
        }
        .into_any(),
        _ => ().into_any(),
    }
}

#[component]
pub fn QuickStepsDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let current = RwSignal::new(0usize);
    // Édition : indice de l'action rapide (= longueur pour une nouvelle), nom et actions en cours.
    let editing = RwSignal::new(None::<usize>);
    let first_use = RwSignal::new(false);
    let from_manage = RwSignal::new(false);
    let name = RwSignal::new(String::new());
    let list = RwSignal::new(Vec::<QuickAction>::new());
    let note = RwSignal::new(None::<String>);

    let start_edit = move |i: usize| {
        let step = ui.quick_steps.with_untracked(|v| v.get(i).cloned()).unwrap_or(QuickStep {
            name: "Nouvelle action rapide".into(),
            actions: vec![QuickAction::MoveTo { folder_id: None }],
        });
        name.set(step.name);
        list.set(step.actions);
        note.set(None);
        editing.set(Some(i));
    };

    Effect::new(move |_| {
        let Some(Dialog::QuickSteps { edit, first_use: first }) = ui.dialog.get() else { return };
        first_use.set(first);
        from_manage.set(edit.is_none());
        current.set(0);
        match edit {
            Some(i) => start_edit(i),
            None => editing.set(None),
        }
        request_animation_frame(move || {
            if let Some(r) = root.get_untracked() {
                if let Ok(Some(el)) = r.query_selector("input, [data-autofocus], select, button") {
                    if let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() {
                        let _ = el.focus();
                    }
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

    let commit = move || {
        let Some(i) = editing.get_untracked() else { return };
        let step = QuickStep { name: name.get_untracked().trim().to_string(), actions: list.get_untracked() };
        if step.name.is_empty() {
            note.set(Some("Donnez un nom à l'action rapide.".into()));
            return;
        }
        if let Some(bad) = step.actions.iter().find(|a| !a.is_configured()) {
            note.set(Some(format!("« {} » : paramètre à renseigner.", bad.label())));
            return;
        }
        if step.actions.is_empty() {
            note.set(Some("Ajoutez au moins une action.".into()));
            return;
        }
        let run_after = first_use.get_untracked();
        ui.quick_steps.update(|v| {
            if i < v.len() {
                v[i] = step;
            } else {
                v.push(step);
            }
        });
        save(ui);
        if from_manage.get_untracked() {
            editing.set(None);
            current.set(i);
        } else {
            close();
            if run_after {
                // Première utilisation : l'action rapide s'exécute une fois configurée, comme dans Outlook.
                run(ui, i);
            }
        }
    };

    let cancel = move || {
        if editing.get_untracked().is_some() && from_manage.get_untracked() {
            editing.set(None);
        } else {
            close();
        }
    };

    let move_by = move |delta: i32| {
        let i = current.get_untracked();
        let n = ui.quick_steps.with_untracked(Vec::len);
        let j = i as i32 + delta;
        if j < 0 || j as usize >= n {
            return;
        }
        ui.quick_steps.update(|v| v.swap(i, j as usize));
        current.set(j as usize);
        save(ui);
    };

    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        match ev.key().as_str() {
            "Escape" => cancel(),
            "Enter" if editing.get_untracked().is_some() && ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()).is_some() => commit(),
            "ArrowDown" | "ArrowUp" if editing.get_untracked().is_none() => {
                let n = ui.quick_steps.with_untracked(Vec::len);
                if n > 0 {
                    let cur = current.get_untracked();
                    current.set(if ev.key() == "ArrowDown" { (cur + 1).min(n - 1) } else { cur.saturating_sub(1) });
                }
            }
            "Tab" => {
                if let Some(r) = root.get_untracked() {
                    // Contrôles grisés exclus : `focus()` y échoue et Tab restait bloqué.
                    let Ok(found) = r.query_selector_all("input:not([disabled]), select:not([disabled]), button:not([disabled]), [tabindex='0']") else { return };
                    let els: Vec<web_sys::HtmlElement> = (0..found.length()).filter_map(|i| found.item(i)?.dyn_into().ok()).collect();
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

    let manage = move || {
        view! {
            <p class="dialog-note">"Actions rapides (dans l'ordre de la galerie) :"</p>
            <div class="rules-list" role="listbox" aria-label="Actions rapides" tabindex="0" data-autofocus
                aria-activedescendant=move || format!("qs-{}", current.get())>
                {move || ui.quick_steps.get().into_iter().enumerate().map(|(i, s)| view! {
                    <div role="option" id=format!("qs-{i}") class="rules-item" aria-selected=move || (current.get() == i).to_string()
                        on:click=move |_| current.set(i) on:dblclick=move |_| start_edit(i)>
                        <span class="rules-name">{step_label(&s)}</span>
                    </div>
                }).collect_view()}
            </div>
            <p class="dialog-note">"Actions :"</p>
            <div class="rules-desc">
                {move || ui.quick_steps.with(|v| v.get(current.get()).map(|s| s.actions.iter().map(|a| {
                    let p = match a {
                        QuickAction::MoveTo { folder_id: Some(f) } | QuickAction::CopyTo { folder_id: Some(f) } => format!(" : {}", actions::folder_name(ui, f)),
                        QuickAction::Category { name } => format!(" : {name}"),
                        QuickAction::Flag { action } => format!(" : {}", action.label()),
                        QuickAction::Forward { to } | QuickAction::NewMessage { to } if !to.is_empty() => format!(" : {to}"),
                        a if !a.is_configured() => " : ?".into(),
                        _ => String::new(),
                    };
                    view! { <p>{format!("{}{p}", a.label())}</p> }
                }).collect_view()))}
            </div>
            <div class="rules-toolbar">
                <button class="btn" on:click=move |_| start_edit(current.get_untracked())>"Modifier…"</button>
                <button class="btn" on:click=move |_| start_edit(ui.quick_steps.with_untracked(Vec::len))>"Nouveau…"</button>
                <button class="btn" on:click=move |_| move_by(-1)>"Monter"</button>
                <button class="btn" on:click=move |_| move_by(1)>"Descendre"</button>
                <button class="btn" on:click=move |_| {
                    let i = current.get_untracked();
                    ui.quick_steps.update(|v| { if i < v.len() { v.remove(i); } });
                    current.set(i.saturating_sub(1));
                    save(ui);
                }>"Supprimer"</button>
                <button class="btn" on:click=move |_| {
                    ui.quick_steps.set(default_quick_steps());
                    current.set(0);
                    save(ui);
                }>"Rétablir les valeurs par défaut"</button>
            </div>
        }
    };

    // Avant : les lignes suivaient `list`, que chaque frappe dans « Catégorie » ou « À » modifie ; le champ était
    // recréé et perdait le focus après chaque caractère.
    let rows = Memo::new(move |_| list.with(|v| shape(v)));
    let editor = move || {
        view! {
            <Show when=move || first_use.get()>
                <p class="dialog-note">"Première utilisation : renseignez les paramètres de cette action rapide."</p>
            </Show>
            <label class="field">
                <span>"Nom"</span>
                <input type="text" prop:value=move || name.get() on:input=move |ev| name.set(event_target_value(&ev)) />
            </label>
            <p class="dialog-note">"Actions :"</p>
            <div class="qs-actions">
                {move || rows.get().into_iter().enumerate().map(|(i, k)| {
                    let kinds = QuickAction::kinds();
                    let a = list.with_untracked(|v| v[i].clone());
                    view! {
                        <div class="qs-row">
                            <select aria-label=format!("Action {}", i + 1) on:change=move |ev| {
                                let j: usize = event_target_value(&ev).parse().unwrap_or(0);
                                list.update(|v| v[i] = QuickAction::kinds()[j].clone());
                                // La ligne est recréée (autre paramètre) : le focus revient sur sa liste.
                                request_animation_frame(move || {
                                    if let Some(r) = root.get_untracked() {
                                        if let Ok(Some(el)) = r.query_selector(&format!("select[aria-label='Action {}']", i + 1)) {
                                            if let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() {
                                                let _ = el.focus();
                                            }
                                        }
                                    }
                                });
                            }>
                                {kinds.iter().enumerate().map(|(j, x)| view! { <option value=j.to_string() selected=j == k>{x.label()}</option> }).collect_view()}
                            </select>
                            {parameter(ui, i, a, list)}
                            <button class="btn" aria-label=format!("Retirer l'action {}", i + 1) on:click=move |_| list.update(|v| { v.remove(i); })>"Retirer"</button>
                        </div>
                    }
                }).collect_view()}
            </div>
            <div class="rules-toolbar">
                <button class="btn" on:click=move |_| list.update(|v| v.push(QuickAction::MarkRead))>"Ajouter une action"</button>
            </div>
            {move || note.get().map(|n| view! { <p class="rules-warn" role="alert">{n}</p> })}
        }
    };

    view! {
        <Show when=move || matches!(ui.dialog.get(), Some(Dialog::QuickSteps { .. }))>
            <div class="modal-backdrop">
                <div class="dialog wide" role="dialog" aria-modal="true" aria-labelledby="qs-title" node_ref=root on:keydown=on_key>
                    <h2 id="qs-title">{move || match editing.get() {
                        None => "Gérer les actions rapides".to_string(),
                        Some(i) if i >= ui.quick_steps.with(Vec::len) => "Créer une action rapide".to_string(),
                        Some(_) => "Modifier l'action rapide".to_string(),
                    }}</h2>
                    {move || if editing.get().is_some() { editor().into_any() } else { manage().into_any() }}
                    <div class="dialog-buttons">
                        {move || if editing.get().is_some() {
                            view! {
                                <button class="btn primary" on:click=move |_| commit()>{move || if first_use.get() { "Enregistrer et exécuter" } else { "Enregistrer" }}</button>
                                <button class="btn" on:click=move |_| cancel()>"Annuler"</button>
                            }.into_any()
                        } else {
                            view! { <button class="btn primary" on:click=move |_| close()>"OK"</button> }.into_any()
                        }}
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forme_inchangee_pendant_la_saisie() {
        let before = [QuickAction::Category { name: "Cl".into() }, QuickAction::Forward { to: String::new() }];
        let after = [QuickAction::Category { name: "Client".into() }, QuickAction::Forward { to: "a@b.fr".into() }];
        assert_eq!(shape(&before), shape(&after));
        // Autre sorte d'action : la ligne est recréée.
        assert_ne!(shape(&before), shape(&[QuickAction::Category { name: "Cl".into() }, QuickAction::Reply]));
        assert_eq!(shape(&[QuickAction::MoveTo { folder_id: None }, QuickAction::NewMessage { to: String::new() }]), vec![0, 12]);
    }
}
