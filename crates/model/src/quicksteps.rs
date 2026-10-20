//! Actions rapides (J6) : une suite d'actions appliquée d'un clic aux éléments sélectionnés. Enregistrées par le
//! cœur dans `quicksteps.json` (dossier de configuration de l'application), à côté des réglages.
//!
//! Défauts d'Outlook classique, d'après la page d'aide française de Microsoft (« Automatiser les tâches courantes
//! ou répétitives avec les étapes rapides ») : Déplacer vers, Au responsable, E-mail de l'équipe, Terminé, Répondre
//! et supprimer ; « Créer » ouvre l'éditeur. Comme dans Outlook, une action rapide dont un paramètre manque (dossier,
//! destinataire) s'ouvre dans l'éditeur à la première utilisation.

use serde::{Deserialize, Serialize};

use crate::FlagAction;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum QuickAction {
    /// Dossier `f<id>` ; `None` tant qu'il n'est pas choisi.
    MoveTo { folder_id: Option<String> },
    CopyTo { folder_id: Option<String> },
    Delete,
    DeletePermanently,
    MarkRead,
    MarkUnread,
    Category { name: String },
    ClearCategories,
    Flag { action: FlagAction },
    Reply,
    ReplyAll,
    /// Transférer à ces destinataires (envoi au J7 : le message s'ouvre en rédaction).
    Forward { to: String },
    /// Nouveau message à ces destinataires.
    NewMessage { to: String },
}

impl QuickAction {
    /// Une action de chaque sorte, dans l'ordre de la liste « Choisir une action » de l'éditeur.
    pub fn kinds() -> Vec<QuickAction> {
        vec![
            Self::MoveTo { folder_id: None },
            Self::CopyTo { folder_id: None },
            Self::Delete,
            Self::DeletePermanently,
            Self::MarkRead,
            Self::MarkUnread,
            Self::Category { name: String::new() },
            Self::ClearCategories,
            Self::Flag { action: FlagAction::Today },
            Self::Reply,
            Self::ReplyAll,
            Self::Forward { to: String::new() },
            Self::NewMessage { to: String::new() },
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::MoveTo { .. } => "Déplacer vers le dossier",
            Self::CopyTo { .. } => "Copier dans le dossier",
            Self::Delete => "Supprimer le message",
            Self::DeletePermanently => "Supprimer définitivement le message",
            Self::MarkRead => "Marquer comme lu",
            Self::MarkUnread => "Marquer comme non lu",
            Self::Category { .. } => "Classer le message",
            Self::ClearCategories => "Effacer les catégories",
            Self::Flag { .. } => "Indicateur",
            Self::Reply => "Répondre",
            Self::ReplyAll => "Répondre à tous",
            Self::Forward { .. } => "Transférer",
            Self::NewMessage { .. } => "Nouveau message",
        }
    }

    /// Même sorte d'action (paramètres ignorés).
    pub fn same_kind(&self, other: &QuickAction) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// Paramètre manquant : l'éditeur s'ouvre avant l'exécution.
    pub fn is_configured(&self) -> bool {
        match self {
            Self::MoveTo { folder_id } | Self::CopyTo { folder_id } => folder_id.as_deref().is_some_and(|f| !f.is_empty()),
            Self::Category { name } => !name.trim().is_empty(),
            Self::Forward { to } | Self::NewMessage { to } => !to.trim().is_empty(),
            _ => true,
        }
    }

    /// Ouvre la rédaction (une seule par action rapide : la première l'emporte).
    pub fn composes(&self) -> bool {
        matches!(self, Self::Reply | Self::ReplyAll | Self::Forward { .. } | Self::NewMessage { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickStep {
    pub name: String,
    pub actions: Vec<QuickAction>,
}

impl QuickStep {
    pub fn is_configured(&self) -> bool {
        !self.actions.is_empty() && self.actions.iter().all(QuickAction::is_configured)
    }
}

/// Actions rapides par défaut.
pub fn default_quick_steps() -> Vec<QuickStep> {
    let step = |name: &str, actions: Vec<QuickAction>| QuickStep { name: name.into(), actions };
    vec![
        step("Déplacer vers", vec![QuickAction::MoveTo { folder_id: None }, QuickAction::MarkRead]),
        step("Au responsable", vec![QuickAction::Forward { to: String::new() }]),
        step("E-mail de l'équipe", vec![QuickAction::NewMessage { to: String::new() }]),
        step(
            "Terminé",
            vec![QuickAction::MoveTo { folder_id: None }, QuickAction::Flag { action: FlagAction::Complete }, QuickAction::MarkRead],
        ),
        step("Répondre et supprimer", vec![QuickAction::Reply, QuickAction::Delete]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_need_setup_where_outlook_asks() {
        let d = default_quick_steps();
        let configured: Vec<bool> = d.iter().map(QuickStep::is_configured).collect();
        assert_eq!(configured, [false, false, false, false, true]);
        assert_eq!(QuickAction::kinds().len(), 13);
    }

    #[test]
    fn json_round_trip() {
        let d = default_quick_steps();
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains(r#""kind":"moveTo","folderId":null"#), "{json}");
        assert_eq!(serde_json::from_str::<Vec<QuickStep>>(&json).unwrap(), d);
    }
}
