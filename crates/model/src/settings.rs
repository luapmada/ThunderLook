//! Réglages d'affichage, enregistrés par le cœur dans le dossier de configuration de l'application.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BarMode {
    /// Une rangée, débordement dans « Plus de commandes » (défaut, comme chez l'utilisateur).
    #[default]
    Compact,
    /// Trois rangées avec noms de groupes.
    Full,
    /// Onglets seuls ; la barre s'ouvre au clic sur un onglet.
    TabsOnly,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReadingPane {
    #[default]
    Right,
    Bottom,
    Off,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub bar_mode: BarMode,
    pub reading_pane: ReadingPane,
    pub density: Density,
    pub folder_pane_width: u32,
    pub folder_pane_collapsed: bool,
    pub list_width: u32,
    pub list_height: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bar_mode: BarMode::Compact,
            reading_pane: ReadingPane::Right,
            density: Density::Comfortable,
            folder_pane_width: 260,
            folder_pane_collapsed: false,
            list_width: 440,
            list_height: 360,
        }
    }
}

impl Settings {
    pub const FOLDER_MIN: u32 = 180;
    pub const FOLDER_MAX: u32 = 480;
    pub const LIST_MIN: u32 = 300;
    pub const LIST_MAX: u32 = 900;

    /// Ramène les dimensions dans des bornes utilisables (fichier édité à la main, écran plus petit…).
    pub fn clamped(mut self) -> Self {
        self.folder_pane_width = self.folder_pane_width.clamp(Self::FOLDER_MIN, Self::FOLDER_MAX);
        self.list_width = self.list_width.clamp(Self::LIST_MIN, Self::LIST_MAX);
        self.list_height = self.list_height.clamp(160, 900);
        self
    }
}
