//! Opérations d'organisation (J3) vues de l'interface : sélection multiple, envoi au cœur, rafraîchissement
//! de la liste et de l'arborescence, annulation, confirmations.

use std::collections::BTreeSet;

use leptos::{prelude::*, task::spawn_local};
use thunderlook_model::{FolderOp, MessageOp, OpResult, Selection};

use crate::{
    ipc,
    list::PAGE,
    state::{ConfirmAction, Dialog, Ui},
};

/// Plages de positions `[début, fin[`, triées et disjointes.
pub mod ranges {
    pub fn contains(r: &[(u32, u32)], p: u32) -> bool {
        r.iter().any(|&(a, b)| p >= a && p < b)
    }

    pub fn single(p: u32) -> Vec<(u32, u32)> {
        vec![(p, p + 1)]
    }

    pub fn span(a: u32, b: u32) -> Vec<(u32, u32)> {
        vec![(a.min(b), a.max(b) + 1)]
    }

    pub fn count(r: &[(u32, u32)]) -> u32 {
        r.iter().map(|(a, b)| b.saturating_sub(*a)).sum()
    }

    fn normalize(mut v: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
        v.retain(|(a, b)| b > a);
        v.sort_unstable();
        let mut out: Vec<(u32, u32)> = Vec::with_capacity(v.len());
        for (a, b) in v {
            match out.last_mut() {
                Some(last) if a <= last.1 => last.1 = last.1.max(b),
                _ => out.push((a, b)),
            }
        }
        out
    }

    /// Ctrl+clic : ajoute ou retire une position.
    pub fn toggle(r: &[(u32, u32)], p: u32) -> Vec<(u32, u32)> {
        if contains(r, p) {
            let mut out = Vec::with_capacity(r.len() + 1);
            for &(a, b) in r {
                if p >= a && p < b {
                    out.push((a, p));
                    out.push((p + 1, b));
                } else {
                    out.push((a, b));
                }
            }
            normalize(out)
        } else {
            let mut v = r.to_vec();
            v.push((p, p + 1));
            normalize(v)
        }
    }
}

/// Éléments visés : la sélection, ou à défaut l'élément courant.
pub fn selection(ui: Ui) -> Option<Selection> {
    let token = ui.view.with_untracked(|v| v.as_ref().map(|v| v.token))?;
    let mut r = ui.sel_ranges.get_untracked();
    if r.is_empty() {
        r = ranges::single(ui.selected_pos.get_untracked()?);
    }
    Some(Selection { token, ranges: r, ids: Vec::new() })
}

pub fn select_single(ui: Ui, pos: u32) {
    ui.sel_ranges.set(ranges::single(pos));
    ui.anchor.set(Some(pos));
    ui.selected_pos.set(Some(pos));
}

pub fn refresh_tree(ui: Ui) {
    spawn_local(async move {
        match ipc::folder_tree().await {
            Ok(t) => ui.tree.set(Some(t)),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

pub fn load_categories(ui: Ui) {
    spawn_local(async move {
        match ipc::categories().await {
            Ok(c) => ui.categories.set(c),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

/// Relit les lignes déjà chargées (l'ordre de la vue ne change pas : lu, indicateurs, catégories).
pub fn reload_rows(ui: Ui) {
    let Some(token) = ui.view.with_untracked(|v| v.as_ref().map(|v| v.token)) else { return };
    let pages: BTreeSet<u32> = ui.rows.with_untracked(|r| r.keys().map(|p| p / PAGE).collect());
    for page in pages {
        spawn_local(async move {
            match ipc::list_rows(token, page * PAGE, PAGE).await {
                Ok(rows) if ui.view.with_untracked(|v| v.as_ref().map(|v| v.token)) == Some(token) => {
                    ui.rows.update(|r| r.extend(rows.into_iter().map(|m| (m.pos, m))));
                }
                Ok(_) => {}
                Err(e) => ui.error.set(Some(e)),
            }
        });
    }
}

/// Rouvre la vue courante en gardant la position de sélection (la liste a changé : éléments partis ou arrivés).
pub fn reopen_view(ui: Ui) {
    let first = ui.sel_ranges.with_untracked(|r| r.first().map(|(a, _)| *a)).or(ui.selected_pos.get_untracked());
    ui.pending_select.set(first);
    ui.view_epoch.update(|e| *e += 1);
}

fn done(ui: Ui, r: OpResult) {
    ui.status.set(Some(r.message));
    ui.undo_label.set(r.undo_label);
    ui.error.set(None);
}

/// Envoie une opération sur les éléments sélectionnés.
pub fn run(ui: Ui, op: MessageOp) {
    let Some(sel) = selection(ui) else { return };
    let structural = matches!(
        op,
        MessageOp::Move { .. } | MessageOp::Copy { .. } | MessageOp::Delete | MessageOp::DeletePermanently | MessageOp::Archive
    );
    if let MessageOp::Move { folder_id } | MessageOp::Copy { folder_id } = &op {
        let id = folder_id.trim_start_matches("fav-").to_string();
        ui.recent_folders.update(|r| {
            r.retain(|x| *x != id);
            r.insert(0, id);
            r.truncate(5);
        });
    }
    let category = matches!(op, MessageOp::Category { .. });
    spawn_local(async move {
        match ipc::apply_op(&sel, &op).await {
            Ok(r) => {
                done(ui, r);
                if structural {
                    reopen_view(ui);
                } else {
                    reload_rows(ui);
                }
                refresh_tree(ui);
                if category {
                    load_categories(ui);
                }
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

pub fn run_folder(ui: Ui, op: FolderOp) {
    spawn_local(async move {
        match ipc::folder_op(&op).await {
            Ok(r) => {
                let created = r.created_id.clone();
                done(ui, r);
                refresh_tree(ui);
                match (&op, created) {
                    (FolderOp::Create { parent_id, .. }, Some(id)) => {
                        // Le parent est déplié pour que le nouveau dossier soit visible et atteignable au clavier.
                        let parent = parent_id.clone();
                        ui.expanded_nodes.update(|e| {
                            e.insert(parent);
                        });
                        ui.folder.set(id.clone());
                        ui.tree_focus.set(id);
                    }
                    (FolderOp::Move { parent_id, .. }, _) => {
                        let parent = parent_id.clone();
                        ui.expanded_nodes.update(|e| {
                            e.insert(parent);
                        });
                        reopen_view(ui);
                    }
                    (FolderOp::SaveSearch { .. }, Some(id)) => {
                        ui.folder.set(id.clone());
                        ui.tree_focus.set(id);
                    }
                    (FolderOp::DeleteSearchFolder { .. }, _) => {
                        if ui.folder.get_untracked().starts_with("s-") {
                            if let Some(f) = ui.tree.with_untracked(|t| t.as_ref().and_then(|t| t.default_folder()).map(|f| f.id.clone())) {
                                ui.folder.set(f);
                            }
                        }
                    }
                    (FolderOp::MarkAllRead { .. }, _) => reload_rows(ui),
                    (FolderOp::Empty { .. } | FolderOp::Delete { .. }, _) => reopen_view(ui),
                    _ => {}
                }
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

pub fn undo(ui: Ui) {
    spawn_local(async move {
        match ipc::undo().await {
            Ok(Some(label)) => {
                ui.status.set(Some(format!("Annulé : {label}")));
                ui.undo_label.set(None);
                reopen_view(ui);
                refresh_tree(ui);
                load_categories(ui);
            }
            Ok(None) => ui.status.set(Some("Rien à annuler".into())),
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

/// Suppr : vers « Éléments supprimés » ; Maj+Suppr : définitif, après confirmation.
pub fn delete(ui: Ui, permanent: bool) {
    if selection(ui).is_none() {
        return;
    }
    if permanent {
        let n = ui.sel_ranges.with_untracked(|r| ranges::count(r)).max(1);
        let text = if n == 1 {
            "Cet élément va être supprimé définitivement. Continuer ?".to_string()
        } else {
            format!("Ces {n} éléments vont être supprimés définitivement. Continuer ?")
        };
        ui.dialog.set(Some(Dialog::Confirm { text, action: ConfirmAction::DeletePermanently }));
    } else {
        run(ui, MessageOp::Delete);
    }
}

pub fn confirm(ui: Ui, action: ConfirmAction) {
    match action {
        ConfirmAction::DeletePermanently => run(ui, MessageOp::DeletePermanently),
        ConfirmAction::DeleteFolder(id) => run_folder(ui, FolderOp::Delete { id }),
        ConfirmAction::EmptyFolder(id) => run_folder(ui, FolderOp::Empty { id }),
    }
}

/// Dossier réel (sans le préfixe des favoris) et son nom.
pub fn folder_name(ui: Ui, id: &str) -> String {
    ui.tree.with_untracked(|t| t.as_ref().and_then(|t| t.find_folder(id)).map(|f| f.name.clone()).unwrap_or_default())
}

pub fn real_folder(id: &str) -> String {
    id.trim_start_matches("fav-").to_string()
}

#[cfg(test)]
mod tests {
    use super::ranges::*;

    #[test]
    fn ranges_ops() {
        assert_eq!(span(5, 2), vec![(2, 6)]);
        let r = toggle(&single(3), 5);
        assert_eq!(r, vec![(3, 4), (5, 6)]);
        let r = toggle(&r, 4);
        assert_eq!(r, vec![(3, 6)]);
        let r = toggle(&r, 4);
        assert_eq!(r, vec![(3, 4), (5, 6)]);
        assert!(contains(&r, 5) && !contains(&r, 4));
        assert_eq!(count(&[(0, 22_188)]), 22_188);
    }
}
