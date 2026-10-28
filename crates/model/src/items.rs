//! Tâches et notes : listes calculées par le cœur, formulaires de saisie.

use serde::{Deserialize, Serialize};

/// Dossier de tâches ou de notes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemFolder {
    /// Identifiant du dossier (`f<id>`).
    pub id: String,
    pub name: String,
    pub section: String,
    pub count: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRow {
    /// `t<message>`.
    pub id: String,
    pub title: String,
    /// `AAAA-MM-JJ`, vide sans échéance.
    pub due: String,
    pub complete: bool,
    /// 0 pas commencée, 1 en cours, 2 terminée, 3 en attente, 4 reportée.
    pub status: i32,
    /// 0 basse, 1 normale, 2 haute.
    pub importance: i32,
    pub categories: Vec<String>,
    pub folder: String,
    /// `AAAA-MM-JJ` du début, vide sans début.
    #[serde(default)]
    pub start: String,
    /// Tâche périodique.
    #[serde(default)]
    pub recurring: bool,
    /// % d'avancement (0..=100).
    #[serde(default)]
    pub percent: i32,
}

/// Vue et tri demandés à la liste des tâches.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskQuery {
    /// "all" (défaut), "today", "overdue", "week", "done" ou "folder".
    pub view: String,
    /// Dossier `f<id>` pour la vue "folder".
    pub folder: String,
    /// "due" (défaut), "importance" ou "title".
    pub sort: String,
    /// Texte cherché dans l'objet.
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskList {
    pub rows: Vec<TaskRow>,
    pub folders: Vec<ItemFolder>,
}

/// Tâche saisie (création ou modification).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskForm {
    /// `t<message>` ; absent pour une nouvelle tâche.
    pub id: Option<String>,
    /// Dossier `f<id>`.
    pub folder_id: String,
    pub subject: String,
    /// `AAAA-MM-JJ`.
    pub start: String,
    pub due: String,
    pub status: i32,
    /// % d'avancement (0..=100) ; rendu cohérent avec `status` par [`task_progress`] à l'enregistrement.
    pub percent: i32,
    pub importance: i32,
    pub body: String,
    /// Catégories séparées par des virgules.
    pub categories: String,
    /// Rappel `AAAA-MM-JJTHH:MM` (heure de Paris) ; `None` : aucun.
    pub reminder: Option<String>,
    /// "" (aucune), "daily", "weekly", "monthly" ou "yearly". "custom" : périodicité relue qui n'entre pas dans ces champs
    /// (jours multiples, n-ième jour…), conservée telle quelle à l'enregistrement.
    pub repeat: String,
    /// Tous les N jours, semaines, mois ou ans.
    pub repeat_every: u32,
    /// "never", "count" ou "until".
    pub repeat_end: String,
    pub repeat_count: u32,
    /// Dernier jour `AAAA-MM-JJ`.
    pub repeat_until: String,
}

/// Statut et % d'avancement cohérents (`status` 0..=4, `percent` 0..=100) : terminée ⇔ 100 % ; « pas commencée » avec
/// un avancement passe « en cours ». Choix de ThunderLook calqué sur la boîte Tâche d'Outlook [à vérifier].
pub fn task_progress(status: i32, percent: i32) -> (i32, i32) {
    let (status, percent) = (status.clamp(0, 4), percent.clamp(0, 100));
    match (status, percent) {
        (2, _) | (_, 100) => (2, 100),
        (0, 1..) => (1, percent),
        _ => (status, percent),
    }
}

/// Changement du statut dans la boîte Tâche : terminée → 100 %, pas commencée → 0 %, sortie de « terminée » → 0 %.
pub fn task_set_status(f: &mut TaskForm, status: i32) {
    let status = status.clamp(0, 4);
    if status == 0 || (status != 2 && f.percent >= 100) {
        f.percent = 0;
    }
    (f.status, f.percent) = task_progress(status, f.percent);
}

/// Changement du % dans la boîte Tâche : 100 % → terminée ; moins de 100 % sur une terminée → en cours (pas commencée à 0 %).
pub fn task_set_percent(f: &mut TaskForm, percent: i32) {
    let percent = percent.clamp(0, 100);
    let status = match (f.status, percent) {
        (2, 0) => 0,
        (2, _) => 1,
        (s, _) => s,
    };
    (f.status, f.percent) = task_progress(status, percent);
}

/// Libellé d'un statut ([MS-OXOTASK] `PidLidTaskStatus`).
pub fn task_status_label(status: i32) -> &'static str {
    match status {
        1 => "En cours",
        2 => "Terminée",
        3 => "En attente de quelqu'un d'autre",
        4 => "Reportée",
        _ => "Pas commencée",
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteRow {
    /// `n<message>`.
    pub id: String,
    pub title: String,
    pub text: String,
    /// 0 bleue, 1 verte, 2 rose, 3 jaune, 4 blanche.
    pub color: i32,
    /// `AAAA-MM-JJ` de la dernière modification.
    pub date: String,
    pub folder: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteList {
    pub rows: Vec<NoteRow>,
    pub folders: Vec<ItemFolder>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NoteForm {
    pub id: Option<String>,
    pub folder_id: String,
    pub body: String,
    pub color: i32,
}

/// Recherche et tri de la liste des notes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NoteQuery {
    /// Texte cherché dans le titre et le contenu (sans tenir compte des majuscules).
    pub text: String,
    /// "recent" (défaut) ou "alpha".
    pub sort: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progression_coherente() {
        assert_eq!(task_progress(2, 30), (2, 100));
        assert_eq!(task_progress(1, 100), (2, 100));
        assert_eq!(task_progress(0, 40), (1, 40));
        assert_eq!(task_progress(3, 40), (3, 40));
        assert_eq!(task_progress(9, -5), (4, 0));
        let mut f = TaskForm::default();
        task_set_percent(&mut f, 50);
        assert_eq!((f.status, f.percent), (1, 50));
        task_set_status(&mut f, 4);
        assert_eq!((f.status, f.percent), (4, 50));
        task_set_percent(&mut f, 100);
        assert_eq!((f.status, f.percent), (2, 100));
        task_set_percent(&mut f, 75);
        assert_eq!((f.status, f.percent), (1, 75));
        task_set_status(&mut f, 2);
        task_set_status(&mut f, 3);
        assert_eq!((f.status, f.percent), (3, 0));
        task_set_status(&mut f, 1);
        task_set_percent(&mut f, 20);
        task_set_status(&mut f, 0);
        assert_eq!((f.status, f.percent), (0, 0));
        assert_eq!(task_status_label(3), "En attente de quelqu'un d'autre");
    }
}
