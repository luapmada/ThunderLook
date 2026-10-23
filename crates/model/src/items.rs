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
