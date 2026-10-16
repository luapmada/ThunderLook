//! Types et logique d'affichage partagés entre le cœur (natif) et l'interface (WebAssembly).
//!
//! Le cœur calcule tout ce qui dépend de l'heure locale (groupes de dates, libellés) ;
//! l'interface ne fait qu'afficher.

pub mod format;
pub mod settings;

#[cfg(feature = "clock")]
pub mod demo;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FolderRole {
    Inbox,
    Drafts,
    Sent,
    Deleted,
    Junk,
    Outbox,
    Archive,
    Virtual,
    Folder,
}

impl FolderRole {
    /// Brouillons, indésirables, boîte d'envoi et dossiers virtuels affichent le **total** entre
    /// crochets ; les autres affichent le nombre de non-lus.
    pub fn shows_total(self) -> bool {
        matches!(self, Self::Drafts | Self::Junk | Self::Outbox | Self::Virtual)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub role: FolderRole,
    pub unread: u32,
    pub total: u32,
    #[serde(default)]
    pub children: Vec<Folder>,
}

impl Folder {
    /// Texte du compteur tel qu'affiché à droite du nom, ou `None` si rien à afficher.
    pub fn badge(&self) -> Option<String> {
        if self.role.shows_total() {
            (self.total > 0).then(|| format!("[{}]", format::thousands(self.total)))
        } else {
            (self.unread > 0).then(|| format::thousands(self.unread))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FolderSection {
    pub id: String,
    pub name: String,
    pub folders: Vec<Folder>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CategoryColor {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl CategoryColor {
    pub fn css_var(self) -> &'static str {
        match self {
            Self::Red => "var(--cat-red)",
            Self::Orange => "var(--cat-orange)",
            Self::Yellow => "var(--cat-yellow)",
            Self::Green => "var(--cat-green)",
            Self::Blue => "var(--cat-blue)",
            Self::Purple => "var(--cat-purple)",
        }
    }
}

/// Groupes de la liste triée par date, dans l'ordre d'affichage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DateGroup {
    Today,
    Yesterday,
    ThisWeek,
    LastWeek,
    Older,
}

impl DateGroup {
    pub fn label(self) -> &'static str {
        match self {
            Self::Today => "Aujourd'hui",
            Self::Yesterday => "Hier",
            Self::ThisWeek => "Cette semaine",
            Self::LastWeek => "La semaine dernière",
            Self::Older => "Plus ancien",
        }
    }
}

/// Ligne de la liste des messages, prête à afficher.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageSummary {
    pub id: String,
    pub folder_id: String,
    pub from: String,
    pub from_address: String,
    pub to: String,
    pub subject: String,
    pub preview: String,
    /// Horodatage RFC 3339 (heure locale avec décalage).
    pub received: String,
    pub group: DateGroup,
    /// Libellé court de la liste (« 09:12 », « lun. 17:05 », « 04/10/2026 »).
    pub time_label: String,
    /// Libellé complet du volet de lecture.
    pub date_label: String,
    pub unread: bool,
    pub flagged: bool,
    pub has_attachments: bool,
    pub categories: Vec<CategoryColor>,
}

/// Corps d'un message, chargé à la demande. J0 : paragraphes de texte brut.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageBody {
    pub id: String,
    pub paragraphs: Vec<String>,
}

/// Instantané du courrier envoyé à l'interface (J0 : démonstration).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MailSnapshot {
    pub sections: Vec<FolderSection>,
    pub messages: Vec<MessageSummary>,
    /// `true` tant que les données sont fictives (avant l'import J1).
    pub demo: bool,
}

impl MailSnapshot {
    /// Messages d'un dossier, du plus récent au plus ancien.
    /// J0 : favoris et dossiers virtuels sont résolus sur la démonstration ; le magasin J1 les remplacera.
    pub fn messages_in(&self, folder_id: &str) -> Vec<MessageSummary> {
        let base = match folder_id {
            "fav-inbox" => "inbox",
            "fav-sent" => "sent",
            other => other,
        };
        let mut out: Vec<MessageSummary> = self
            .messages
            .iter()
            .filter(|m| match base {
                "v-unread" => m.unread,
                "v-flagged" => m.flagged,
                b => m.folder_id == b,
            })
            .cloned()
            .collect();
        out.sort_by(|a, b| b.received.cmp(&a.received));
        out
    }

    /// Recherche d'un dossier par identifiant dans toutes les sections.
    pub fn find_folder(&self, id: &str) -> Option<&Folder> {
        fn walk<'a>(folders: &'a [Folder], id: &str) -> Option<&'a Folder> {
            folders.iter().find_map(|f| if f.id == id { Some(f) } else { walk(&f.children, id) })
        }
        self.sections.iter().find_map(|s| walk(&s.folders, id))
    }
}

/// Initiales pour la pastille de l'expéditeur.
pub fn initials(name: &str) -> String {
    let mut words = name.split_whitespace().filter_map(|w| w.chars().find(|c| c.is_alphanumeric()));
    let s: String = words.by_ref().take(2).flat_map(char::to_uppercase).collect();
    if s.is_empty() { "?".into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_cases() {
        assert_eq!(initials("Camille Martin"), "CM");
        assert_eq!(initials("Équipe ThunderLook"), "ÉT");
        assert_eq!(initials("bibliotheque"), "B");
        assert_eq!(initials("   "), "?");
    }

    #[test]
    fn badge_unread_vs_total() {
        let mut f = Folder { id: "x".into(), name: "x".into(), role: FolderRole::Inbox, unread: 1234, total: 2000, children: vec![] };
        assert_eq!(f.badge().as_deref(), Some("1\u{202f}234"));
        f.role = FolderRole::Drafts;
        assert_eq!(f.badge().as_deref(), Some("[2\u{202f}000]"));
        f.total = 0;
        assert_eq!(f.badge(), None);
    }
}
