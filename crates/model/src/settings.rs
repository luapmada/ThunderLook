//! Réglages d'affichage, enregistrés par le cœur dans le dossier de configuration de l'application.

use serde::{Deserialize, Serialize};

use crate::calendar::CalendarViewKind;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    /// Suit le système (comportement d'origine).
    #[default]
    System,
    Light,
    Dark,
}

/// Message d'origine dans une réponse (Options › Courrier).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReplyQuote {
    /// Texte du message d'origine sous la réponse (comportement actuel).
    #[default]
    Include,
    /// Réponse vide : le message d'origine n'est pas repris.
    None,
}

/// Police par défaut de la rédaction ; `Interface` : celle de l'application (comportement actuel).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComposeFont {
    #[default]
    Interface,
    Arial,
    Calibri,
    Verdana,
    Tahoma,
    Georgia,
    TimesNewRoman,
    CourierNew,
}

impl ComposeFont {
    pub const ALL: [ComposeFont; 8] = [
        ComposeFont::Interface,
        ComposeFont::Arial,
        ComposeFont::Calibri,
        ComposeFont::Verdana,
        ComposeFont::Tahoma,
        ComposeFont::Georgia,
        ComposeFont::TimesNewRoman,
        ComposeFont::CourierNew,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ComposeFont::Interface => "Police de l'interface",
            ComposeFont::Arial => "Arial",
            ComposeFont::Calibri => "Calibri",
            ComposeFont::Verdana => "Verdana",
            ComposeFont::Tahoma => "Tahoma",
            ComposeFont::Georgia => "Georgia",
            ComposeFont::TimesNewRoman => "Times New Roman",
            ComposeFont::CourierNew => "Courier New",
        }
    }

    /// Valeur de `font-family` ; `None` : ne rien imposer.
    pub fn css(self) -> Option<&'static str> {
        match self {
            ComposeFont::Interface => None,
            ComposeFont::Arial => Some("Arial, sans-serif"),
            ComposeFont::Calibri => Some("Calibri, Carlito, sans-serif"),
            ComposeFont::Verdana => Some("Verdana, sans-serif"),
            ComposeFont::Tahoma => Some("Tahoma, sans-serif"),
            ComposeFont::Georgia => Some("Georgia, serif"),
            ComposeFont::TimesNewRoman => Some("\"Times New Roman\", Times, serif"),
            ComposeFont::CourierNew => Some("\"Courier New\", monospace"),
        }
    }
}

/// Module affiché au lancement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StartModule {
    #[default]
    Mail,
    Calendar,
    Contacts,
    Tasks,
    Notes,
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
    /// Marquer un élément comme lu quand la sélection le quitte (défaut de la référence).
    pub mark_read_on_leave: bool,
    /// « Afficher en tant que conversations », pour tous les dossiers.
    pub conversations: bool,
    /// Vue du calendrier (J9).
    pub calendar_view: CalendarViewKind,
    /// Thème clair, sombre ou celui du système.
    pub theme: Theme,
    /// Zoom de l'interface, en pour cent.
    pub zoom: u32,
    /// Premier jour de la semaine : 0 dimanche, 1 lundi… 6 samedi.
    pub first_day_of_week: u8,
    /// Jours ouvrés : un bit par jour, bit 0 lundi … bit 6 dimanche.
    pub work_days: u8,
    /// Début et fin de la journée de travail, en minutes depuis minuit.
    pub work_start: u16,
    pub work_end: u16,
    /// Durée d'un nouveau rendez-vous, en minutes.
    pub default_duration: u32,
    /// Rappel d'un nouveau rendez-vous, en minutes avant le début ; `None` : aucun.
    pub default_reminder: Option<i32>,
    /// Pas de la grille du calendrier (déplacement, création), en minutes : 15, 30 ou 60.
    pub snap_minutes: u32,
    pub show_week_numbers: bool,
    /// Marquer comme lu après ce nombre de secondes d'affichage ; 0 : pas de minuterie.
    pub mark_read_delay: u32,
    /// Couleur des nouvelles notes (numéro de couleur du module Notes ; 3 : jaune).
    pub default_note_color: i32,
    /// Heure de rappel d'une nouvelle tâche avec échéance, en minutes depuis minuit ; `None` : aucun rappel.
    pub default_task_reminder: Option<u16>,
    /// Notifications Windows des rappels.
    pub notifications: bool,
    /// Son à l'arrivée d'un rappel.
    pub reminder_sound: bool,
    /// Réponses : reprise ou non du message d'origine.
    pub reply_quote: ReplyQuote,
    /// Police et taille (points) du texte d'un nouveau message.
    pub compose_font: ComposeFont,
    pub compose_font_size: u8,
    /// Enregistrement automatique d'un message en cours de rédaction, en minutes ; 0 : jamais.
    pub draft_autosave_minutes: u32,
    /// Insérer la signature par défaut dans les nouveaux messages / dans les réponses et transferts.
    pub signature_on_new: bool,
    pub signature_on_reply: bool,
    /// Module affiché au lancement.
    pub start_module: StartModule,
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
            mark_read_on_leave: true,
            conversations: false,
            calendar_view: CalendarViewKind::Week,
            theme: Theme::System,
            zoom: 100,
            first_day_of_week: 1,
            work_days: 0b001_1111,
            work_start: 450,
            work_end: 1080,
            default_duration: 30,
            default_reminder: Some(15),
            snap_minutes: 15,
            show_week_numbers: false,
            mark_read_delay: 0,
            default_note_color: 3,
            default_task_reminder: None,
            notifications: true,
            reminder_sound: false,
            reply_quote: ReplyQuote::Include,
            compose_font: ComposeFont::Interface,
            compose_font_size: 11,
            draft_autosave_minutes: 0,
            signature_on_new: true,
            signature_on_reply: true,
            start_module: StartModule::Mail,
        }
    }
}

impl Settings {
    pub const FOLDER_MIN: u32 = 180;
    pub const FOLDER_MAX: u32 = 480;
    pub const LIST_MIN: u32 = 300;
    pub const LIST_MAX: u32 = 900;
    pub const ZOOM_MIN: u32 = 70;
    pub const ZOOM_MAX: u32 = 200;

    /// Ramène les dimensions dans des bornes utilisables (fichier édité à la main, écran plus petit…).
    pub fn clamped(mut self) -> Self {
        self.folder_pane_width = self.folder_pane_width.clamp(Self::FOLDER_MIN, Self::FOLDER_MAX);
        self.list_width = self.list_width.clamp(Self::LIST_MIN, Self::LIST_MAX);
        self.list_height = self.list_height.clamp(160, 900);
        self.zoom = self.zoom.clamp(Self::ZOOM_MIN, Self::ZOOM_MAX);
        self.first_day_of_week %= 7;
        self.work_days &= 0b111_1111;
        self.work_start = self.work_start.min(1380);
        self.work_end = self.work_end.clamp(self.work_start + 60, 1440);
        self.default_duration = self.default_duration.clamp(5, 1440);
        self.default_reminder = self.default_reminder.map(|m| m.clamp(0, 20_160));
        if !matches!(self.snap_minutes, 15 | 30 | 60) {
            self.snap_minutes = 15;
        }
        self.mark_read_delay = self.mark_read_delay.min(600);
        self.default_note_color = self.default_note_color.clamp(0, 4);
        self.default_task_reminder = self.default_task_reminder.map(|m| m.min(1439));
        self.compose_font_size = self.compose_font_size.clamp(8, 36);
        self.draft_autosave_minutes = self.draft_autosave_minutes.min(120);
        self
    }

    /// Le jour (0 lundi … 6 dimanche) est-il ouvré ?
    pub fn is_work_day(&self, monday_based: u32) -> bool {
        self.work_days >> (monday_based % 7) & 1 == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fichier écrit avant le lot M : aucun des nouveaux champs.
    const OLD: &str = r#"{"barMode":"full","readingPane":"bottom","density":"compact","folderPaneWidth":300,
        "folderPaneCollapsed":true,"listWidth":500,"listHeight":400,"markReadOnLeave":false,"conversations":true,"calendarView":"week"}"#;

    #[test]
    fn old_file_keeps_values_and_gets_defaults() {
        let s: Settings = serde_json::from_str(OLD).unwrap();
        let d = Settings::default();
        assert_eq!((s.bar_mode, s.reading_pane, s.density, s.folder_pane_width), (BarMode::Full, ReadingPane::Bottom, Density::Compact, 300));
        assert!(!s.mark_read_on_leave && s.conversations);
        assert_eq!((s.theme, s.zoom, s.first_day_of_week, s.work_days), (d.theme, d.zoom, d.first_day_of_week, d.work_days));
        assert_eq!((s.default_duration, s.default_reminder, s.snap_minutes), (30, Some(15), 15));
        assert_eq!((s.work_start, s.work_end, s.notifications), (450, 1080, true));
    }

    #[test]
    fn empty_file_is_default() {
        assert_eq!(serde_json::from_str::<Settings>("{}").unwrap(), Settings::default());
    }

    #[test]
    fn round_trip() {
        let s = Settings { theme: Theme::Dark, zoom: 125, default_reminder: None, default_task_reminder: Some(540), ..Settings::default() };
        assert_eq!(serde_json::from_str::<Settings>(&serde_json::to_string(&s).unwrap()).unwrap(), s);
    }

    #[test]
    fn clamped_repairs_hand_edited_values() {
        let s = Settings { zoom: 900, first_day_of_week: 9, work_start: 2000, work_end: 10, default_duration: 0, snap_minutes: 7, mark_read_delay: 99_999, default_note_color: 42, default_task_reminder: Some(5000), ..Settings::default() }.clamped();
        assert_eq!((s.zoom, s.first_day_of_week, s.default_duration, s.snap_minutes, s.mark_read_delay), (Settings::ZOOM_MAX, 2, 5, 15, 600));
        assert!(s.work_start < s.work_end && s.work_end <= 1440);
        assert_eq!((s.default_note_color, s.default_task_reminder), (4, Some(1439)));
        assert_eq!(Settings::default().clamped(), Settings::default());
    }

    #[test]
    fn mail_options_default_to_current_behaviour() {
        let s = Settings::default();
        assert_eq!((s.reply_quote, s.compose_font, s.compose_font_size, s.draft_autosave_minutes), (ReplyQuote::Include, ComposeFont::Interface, 11, 0));
        assert!(s.signature_on_new && s.signature_on_reply);
        assert_eq!(s.start_module, StartModule::Mail);
        assert!(ComposeFont::Interface.css().is_none() && ComposeFont::ALL.iter().skip(1).all(|f| f.css().is_some()));
    }

    #[test]
    fn mail_options_round_trip_and_old_file() {
        let s = Settings { reply_quote: ReplyQuote::None, compose_font: ComposeFont::TimesNewRoman, compose_font_size: 14, draft_autosave_minutes: 5, signature_on_new: false, signature_on_reply: false, start_module: StartModule::Calendar, ..Settings::default() };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"composeFont\":\"timesNewRoman\"") && json.contains("\"startModule\":\"calendar\""));
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
        let old: Settings = serde_json::from_str(OLD).unwrap();
        assert_eq!((old.reply_quote, old.start_module, old.draft_autosave_minutes), (ReplyQuote::Include, StartModule::Mail, 0));
    }

    #[test]
    fn mail_options_are_clamped() {
        let s = Settings { compose_font_size: 0, draft_autosave_minutes: 9_999, ..Settings::default() }.clamped();
        assert_eq!((s.compose_font_size, s.draft_autosave_minutes), (8, 120));
        assert_eq!(Settings { compose_font_size: 200, ..Settings::default() }.clamped().compose_font_size, 36);
    }

    #[test]
    fn work_days_default_is_monday_to_friday() {
        let s = Settings::default();
        assert!((0..5).all(|d| s.is_work_day(d)) && !s.is_work_day(5) && !s.is_work_day(6));
    }
}
