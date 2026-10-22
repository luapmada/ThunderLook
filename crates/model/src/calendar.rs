//! Calendrier (J9) : vue calculée par le cœur (jours, positions, chevauchements, navigateur de dates), que l'interface
//! affiche telle quelle. Dates au format `AAAA-MM-JJ`, heures locales en minutes depuis minuit.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CalendarViewKind {
    Day,
    /// Lundi à vendredi.
    WorkWeek,
    #[default]
    Week,
    Month,
}

impl CalendarViewKind {
    pub const ALL: [CalendarViewKind; 4] = [Self::Day, Self::WorkWeek, Self::Week, Self::Month];

    pub fn label(self) -> &'static str {
        match self {
            Self::Day => "Jour",
            Self::WorkWeek => "Semaine de travail",
            Self::Week => "Semaine",
            Self::Month => "Mois",
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            Self::Day => "Ctrl+Alt+1",
            Self::WorkWeek => "Ctrl+Alt+2",
            Self::Week => "Ctrl+Alt+3",
            Self::Month => "Ctrl+Alt+4",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarQuery {
    pub kind: CalendarViewKind,
    /// Jour de référence ; vide = aujourd'hui.
    pub anchor: String,
    /// Vue Mois défilée à la roulette (semaine par semaine, comme Outlook) : lundi de la première semaine affichée.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
}

/// Un calendrier (dossier de rendez-vous) et son affichage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarInfo {
    pub id: String,
    pub name: String,
    /// Fichier de données ou compte qui le contient.
    pub section: String,
    /// Indice de couleur (0-7), stable pour un calendrier donné.
    pub color: u8,
    pub count: u32,
    pub shown: bool,
    /// Calendrier commun avec Outlook : modifiable dans ThunderLook (écrit par Outlook).
    #[serde(default)]
    pub shared: bool,
    /// Calendrier par défaut d'Outlook : proposé pour un nouveau rendez-vous.
    #[serde(default)]
    pub outlook_default: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    /// Identifiant de l'occurrence (`a<id>` ou `a<id>@<date d'origine>` pour une série).
    pub id: String,
    pub calendar_id: String,
    pub color: u8,
    pub subject: String,
    pub location: Option<String>,
    /// « 14:00 – 15:30 », « Toute la journée », « 3 jours »…
    pub when: String,
    pub start_label: String,
    pub recurring: bool,
    pub exception: bool,
    pub private: bool,
    /// free, tentative, busy, oof, elsewhere.
    pub busy: String,
    pub categories: Vec<String>,
    pub reminder_minutes: Option<i64>,
    pub organizer: String,
    pub attendees: String,
    pub is_meeting: bool,
    /// Ce jour n'est pas le premier de l'élément (suite d'un événement sur plusieurs jours).
    pub continued: bool,
}

/// Rendez-vous placé dans la grille horaire d'un jour.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimedEvent {
    pub event: CalendarEvent,
    pub start_min: u32,
    pub end_min: u32,
    /// Colonne et nombre de colonnes du groupe de rendez-vous qui se chevauchent.
    pub column: u8,
    pub columns: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarDay {
    pub date: String,
    /// « mardi 21 », ou « 21 » dans la vue Mois (« 1 nov. » pour le premier du mois).
    pub label: String,
    pub today: bool,
    /// Vue Mois : jour du mois affiché (sinon grisé).
    pub in_month: bool,
    pub all_day: Vec<CalendarEvent>,
    pub timed: Vec<TimedEvent>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavDay {
    pub date: String,
    pub day: u32,
    pub in_month: bool,
    pub today: bool,
    /// Dans la période affichée.
    pub selected: bool,
    /// Au moins un rendez-vous dans les calendriers affichés (gras, comme la référence).
    pub busy: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavMonth {
    pub title: String,
    /// Six semaines de sept jours, du lundi au dimanche.
    pub weeks: Vec<Vec<NavDay>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarView {
    pub kind: CalendarViewKind,
    pub anchor: String,
    pub title: String,
    pub prev: String,
    pub next: String,
    pub today: String,
    /// Colonnes (Jour, Semaine) ou 42 cases (Mois).
    pub days: Vec<CalendarDay>,
    /// Minutes depuis minuit de l'heure actuelle si aujourd'hui est affiché (ligne de l'heure).
    pub now_min: Option<u32>,
    pub calendars: Vec<CalendarInfo>,
    pub navigator: Vec<NavMonth>,
    /// Données lues à partir d'une copie de fichier de données : date de cette copie (option A du J9).
    pub source_note: Option<String>,
}

pub const WEEKDAYS_SHORT: [&str; 7] = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."];
pub const WEEKDAYS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
pub const MONTHS: [&str; 12] = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];
pub const MONTHS_SHORT: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];

/// Rendez-vous dans la boîte « Rendez-vous » (créer, modifier). Dates locales `AAAA-MM-JJTHH:MM`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppointmentForm {
    /// `a<id>` ; absent pour un nouveau rendez-vous.
    pub id: Option<String>,
    /// Calendrier (`c<dossier>`).
    pub calendar_id: String,
    pub subject: String,
    pub location: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
    /// Minutes avant le début ; `None` : aucun rappel.
    pub reminder: Option<i32>,
    /// 0 disponible, 1 provisoire, 2 occupé(e), 3 absent(e) du bureau, 4 travail ailleurs.
    pub busy: i32,
    pub body: String,
    /// Catégories séparées par des virgules.
    pub categories: String,
    pub private: bool,
    /// Série périodique : heures et périodicité restent celles d'Outlook.
    pub recurring: bool,
}
