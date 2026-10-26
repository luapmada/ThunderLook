//! État partagé de l'interface (fourni par contexte à tous les composants).

use std::collections::{HashMap, HashSet};

use leptos::{html, prelude::*};
use thunderlook_model::{
    CategoryInfo, Draft, calendar::{AppointmentForm, CalendarEvent, CalendarView}, contacts::{ContactCard, ContactFolder, ContactList}, FolderTree, IndexStatus, ListView, MessageSummary, SearchScope, SortField, quicksteps::QuickStep, settings::Settings,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Module {
    Mail,
    Calendar,
    Contacts,
    Tasks,
    Notes,
}

impl Module {
    pub const ALL: [Module; 5] = [Module::Mail, Module::Calendar, Module::Contacts, Module::Tasks, Module::Notes];

    pub fn label(self) -> &'static str {
        match self {
            Module::Mail => "Courrier",
            Module::Calendar => "Calendrier",
            Module::Contacts => "Contacts",
            Module::Tasks => "Tâches",
            Module::Notes => "Notes",
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            Module::Mail => "Ctrl+1",
            Module::Calendar => "Ctrl+2",
            Module::Contacts => "Ctrl+3",
            Module::Tasks => "Ctrl+4",
            Module::Notes => "Ctrl+5",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Home,
    SendReceive,
    Folder,
    View,
    Help,
    /// Onglet contextuel, présent pendant une recherche.
    Search,
}

impl Tab {
    pub const ALL: [Tab; 5] = [Tab::Home, Tab::SendReceive, Tab::Folder, Tab::View, Tab::Help];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Home => "Accueil",
            Tab::SendReceive => "Envoyer/Recevoir",
            Tab::Folder => "Dossier",
            Tab::View => "Affichage",
            Tab::Help => "Aide",
            Tab::Search => "Rechercher",
        }
    }

    pub fn dom_id(self) -> &'static str {
        match self {
            Tab::Home => "tab-home",
            Tab::SendReceive => "tab-sendreceive",
            Tab::Folder => "tab-folder",
            Tab::View => "tab-view",
            Tab::Help => "tab-help",
            Tab::Search => "tab-search",
        }
    }
}

/// Boîte de dialogue ouverte (une seule à la fois).
#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
    /// Nouveau dossier sous `parent`.
    NewFolder { parent: String },
    Rename { id: String, name: String },
    /// Choix d'un dossier de destination.
    PickFolder { purpose: PickPurpose },
    /// Confirmation d'une action destructrice.
    Confirm { text: String, action: ConfirmAction },
    NewCategory,
    /// Enregistrer la recherche courante comme dossier de recherche.
    SaveSearch,
    /// « Réglages de l'affichage » : classement, conversations, espacement, lecture (J3b).
    ViewSettings,
    /// « Options » (Fichier › Options) : thème, calendrier, courrier, tâches et notes, rappels.
    Options,
    /// « Règles et alertes », ou directement « Exécuter les règles maintenant » (J6).
    Rules { run: bool },
    /// « Gérer les actions rapides » (`edit` : `None`), ou l'éditeur d'une action rapide (indice ; la longueur de
    /// la liste pour une nouvelle). `first_use` : ouverte à l'exécution d'une action rapide à configurer.
    QuickSteps { edit: Option<usize>, first_use: bool },
    /// « Paramètres du compte » (J7).
    Accounts,
    /// « Réponses automatiques » (J7).
    AutoReply,
    /// Expéditeurs bloqués et approuvés.
    JunkOptions,
    /// « Créer une règle » d'après le message sélectionné (clic droit › Règles).
    CreateRule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickPurpose {
    MoveMessages,
    CopyMessages,
    MoveFolder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfirmAction {
    DeletePermanently,
    DeleteFolder(String),
    EmptyFolder(String),
}

/// Menu contextuel ouvert à une position de la fenêtre.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextMenuAt {
    pub x: f64,
    pub y: f64,
    pub target: ContextTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextTarget {
    Messages,
    Folder(String),
    /// Clic droit sur l'indicateur d'une ligne : menu « Assurer un suivi » directement.
    Flag,
    /// Clic droit sur les catégories d'une ligne : menu « Catégoriser » directement.
    Categories,
    /// Clic droit sur un rendez-vous du calendrier (identifiant de l'occurrence).
    CalEvent(String),
    /// Clic droit sur un créneau vide : `AAAA-MM-JJ` ou `AAAA-MM-JJTHH:00`.
    CalSlot(String),
}

/// Points d'arrêt du cycle F6 / Maj+F6.
#[derive(Clone, Copy)]
pub struct Refs {
    pub search: NodeRef<html::Input>,
    pub tabs: NodeRef<html::Div>,
    pub tree: NodeRef<html::Div>,
    pub list: NodeRef<html::Div>,
    pub reading: NodeRef<html::Section>,
}

#[derive(Clone, Copy)]
pub struct Ui {
    pub module: RwSignal<Module>,
    pub tab: RwSignal<Tab>,
    pub settings: RwSignal<Settings>,
    pub settings_loaded: RwSignal<bool>,
    /// Mode « onglets seuls » : barre déroulée temporairement.
    pub bar_popup: RwSignal<bool>,
    pub tree: RwSignal<Option<FolderTree>>,
    pub error: RwSignal<Option<String>>,
    /// Dossier affiché (identifiant de l'arborescence, favoris compris).
    pub folder: RwSignal<String>,
    pub tree_focus: RwSignal<String>,
    pub unread_only: RwSignal<bool>,
    /// Tri de la liste : champ et sens décroissant.
    pub sort: RwSignal<(SortField, bool)>,
    /// Vue ouverte sur le dossier courant, et lignes déjà reçues (par position).
    pub view: RwSignal<Option<ListView>>,
    pub rows: RwSignal<HashMap<u32, MessageSummary>>,
    /// Position sélectionnée dans la vue, et identifiant du message une fois la ligne reçue.
    pub selected_pos: RwSignal<Option<u32>>,
    pub selected: RwSignal<Option<String>>,
    pub collapsed_groups: RwSignal<HashSet<String>>,
    /// Sections repliées, et dossiers dépliés (les dossiers à sous-dossiers sont repliés par défaut).
    pub collapsed_nodes: RwSignal<HashSet<String>>,
    pub expanded_nodes: RwSignal<HashSet<String>>,
    /// Messages dont le contenu distant a été autorisé pendant la session.
    pub remote_allowed: RwSignal<HashSet<String>>,
    /// Sélection multiple : plages `[début, fin[` de positions, et point d'ancrage des Maj+clic.
    pub sel_ranges: RwSignal<Vec<(u32, u32)>>,
    pub anchor: RwSignal<Option<u32>>,
    /// Incrémenté pour rouvrir la vue courante après une opération.
    pub view_epoch: RwSignal<u64>,
    /// Position à sélectionner à la prochaine ouverture de vue (au lieu de la première).
    pub pending_select: RwSignal<Option<u32>>,
    /// Message à sélectionner à la prochaine ouverture de vue (clic sur une notification), s'il y figure.
    pub pending_message: RwSignal<Option<String>>,
    /// Dernier résultat d'opération (barre d'état) et ce que Ctrl+Z annulera.
    pub status: RwSignal<Option<String>>,
    pub undo_label: RwSignal<Option<String>>,
    pub categories: RwSignal<Vec<CategoryInfo>>,
    /// Dossiers récents de « Déplacer vers ».
    pub recent_folders: RwSignal<Vec<String>>,
    pub dialog: RwSignal<Option<Dialog>>,
    pub context_menu: RwSignal<Option<ContextMenuAt>>,
    /// Recherche validée (vide : pas de recherche) et son étendue.
    pub search_text: RwSignal<String>,
    pub search_scope: RwSignal<SearchScope>,
    pub index: RwSignal<IndexStatus>,
    /// Message en rédaction (J5), affiché à la place du volet de lecture ; modifié depuis le dernier enregistrement.
    pub compose: RwSignal<Option<Draft>>,
    pub compose_dirty: RwSignal<bool>,
    /// Incrémenté à chaque ouverture de rédaction : le volet est alors recréé avec le nouveau message.
    pub compose_epoch: RwSignal<u64>,
    /// Actions rapides (J6), dans l'ordre de la galerie.
    pub quick_steps: RwSignal<Vec<QuickStep>>,
    /// « Travailler hors connexion » (J7).
    pub offline: RwSignal<bool>,
    /// Calendrier (J9) : vue reçue du cœur, jour de référence (vide = aujourd'hui), rechargement, élément ouvert.
    pub cal_view: RwSignal<Option<CalendarView>>,
    pub cal_anchor: RwSignal<String>,
    pub cal_epoch: RwSignal<u64>,
    /// Incrémenté quand la synchronisation avec Outlook a changé des contacts.
    pub contacts_epoch: RwSignal<u64>,
    /// Vue Mois défilée à la roulette : lundi de la première semaine (`None` : mois de `cal_anchor`).
    pub cal_month_start: RwSignal<Option<String>>,
    pub cal_selected: RwSignal<Option<CalendarEvent>>,
    /// Boîte « Rendez-vous » ouverte (calendrier commun avec Outlook).
    pub appointment: RwSignal<Option<AppointmentForm>>,
    /// Contacts (J10) : dossiers, dossier affiché (`k<id>`), liste, contact sélectionné (`p<id>`), sa fiche, filtre.
    pub contact_folders: RwSignal<Vec<ContactFolder>>,
    pub contact_folder: RwSignal<String>,
    pub contact_list: RwSignal<Option<ContactList>>,
    pub contact_selected: RwSignal<Option<String>>,
    pub contact_card: RwSignal<Option<ContactCard>>,
    /// Boîte « Contact » ouverte (création ou modification).
    pub contact_form: RwSignal<Option<thunderlook_model::contacts::ContactForm>>,
    /// Boîte « Nettoyer les doublons » ouverte.
    pub dedupe_open: RwSignal<bool>,
    /// Tâches et notes : listes, élément sélectionné, boîtes ouvertes, filtre et rechargement.
    pub tasks: RwSignal<Option<thunderlook_model::items::TaskList>>,
    pub task_selected: RwSignal<Option<String>>,
    pub task_form: RwSignal<Option<thunderlook_model::items::TaskForm>>,
    pub task_filter: RwSignal<String>,
    pub tasks_epoch: RwSignal<u64>,
    pub notes: RwSignal<Option<thunderlook_model::items::NoteList>>,
    pub note_form: RwSignal<Option<thunderlook_model::items::NoteForm>>,
    pub notes_epoch: RwSignal<u64>,
    pub contact_filter: RwSignal<String>,
    /// Recherche globale : portée « Tout » choisie, résultats affichés, sélection à faire à l'arrivée dans un module.
    pub search_all: RwSignal<bool>,
    pub global: RwSignal<Option<crate::global_search::Results>>,
    pub pending_open: RwSignal<Option<crate::global_search::Pending>>,
    /// Rappels dus (barre d'état) et panneau ouvert.
    pub reminders: RwSignal<Vec<crate::reminders::Reminder>>,
    pub reminders_open: RwSignal<bool>,
    pub flag_dialog: RwSignal<bool>,
    /// Boîte « Échange avec le téléphone » ouverte (Ctrl+Maj+T).
    pub phone_exchange: RwSignal<bool>,
    /// Écran « Sauvegardes » ouvert (menu Fichier).
    pub backups: RwSignal<bool>,
    /// Aide intégrée ouverte (F1, onglet Aide) : rubrique à atteindre, vide pour le haut.
    pub help: RwSignal<Option<String>>,
    pub refs: Refs,
}

impl Ui {
    pub fn new() -> Self {
        Self {
            module: RwSignal::new(Module::Mail),
            tab: RwSignal::new(Tab::Home),
            settings: RwSignal::new(Settings::default()),
            settings_loaded: RwSignal::new(false),
            bar_popup: RwSignal::new(false),
            tree: RwSignal::new(None),
            error: RwSignal::new(None),
            folder: RwSignal::new(String::new()),
            tree_focus: RwSignal::new(String::new()),
            unread_only: RwSignal::new(false),
            sort: RwSignal::new((SortField::Date, true)),
            view: RwSignal::new(None),
            rows: RwSignal::new(HashMap::new()),
            selected_pos: RwSignal::new(None),
            selected: RwSignal::new(None),
            collapsed_groups: RwSignal::new(HashSet::new()),
            collapsed_nodes: RwSignal::new(HashSet::new()),
            expanded_nodes: RwSignal::new(HashSet::new()),
            remote_allowed: RwSignal::new(HashSet::new()),
            sel_ranges: RwSignal::new(Vec::new()),
            anchor: RwSignal::new(None),
            view_epoch: RwSignal::new(0),
            pending_select: RwSignal::new(None),
            pending_message: RwSignal::new(None),
            status: RwSignal::new(None),
            undo_label: RwSignal::new(None),
            categories: RwSignal::new(Vec::new()),
            recent_folders: RwSignal::new(Vec::new()),
            dialog: RwSignal::new(None),
            context_menu: RwSignal::new(None),
            search_text: RwSignal::new(String::new()),
            search_scope: RwSignal::new(SearchScope::default()),
            index: RwSignal::new(IndexStatus::default()),
            compose: RwSignal::new(None),
            compose_dirty: RwSignal::new(false),
            compose_epoch: RwSignal::new(0),
            quick_steps: RwSignal::new(Vec::new()),
            offline: RwSignal::new(false),
            cal_view: RwSignal::new(None),
            cal_anchor: RwSignal::new(String::new()),
            cal_epoch: RwSignal::new(0),
            contacts_epoch: RwSignal::new(0),
            cal_month_start: RwSignal::new(None),
            cal_selected: RwSignal::new(None),
            appointment: RwSignal::new(None),
            contact_folders: RwSignal::new(Vec::new()),
            contact_folder: RwSignal::new(String::new()),
            contact_list: RwSignal::new(None),
            contact_selected: RwSignal::new(None),
            contact_card: RwSignal::new(None),
            contact_form: RwSignal::new(None),
            dedupe_open: RwSignal::new(false),
            tasks: RwSignal::new(None),
            task_selected: RwSignal::new(None),
            task_form: RwSignal::new(None),
            task_filter: RwSignal::new(String::new()),
            tasks_epoch: RwSignal::new(0),
            notes: RwSignal::new(None),
            note_form: RwSignal::new(None),
            notes_epoch: RwSignal::new(0),
            contact_filter: RwSignal::new(String::new()),
            search_all: RwSignal::new(false),
            global: RwSignal::new(None),
            pending_open: RwSignal::new(None),
            reminders: RwSignal::new(Vec::new()),
            reminders_open: RwSignal::new(false),
            flag_dialog: RwSignal::new(false),
            phone_exchange: RwSignal::new(false),
            backups: RwSignal::new(false),
            help: RwSignal::new(None),
            refs: Refs {
                search: NodeRef::new(),
                tabs: NodeRef::new(),
                tree: NodeRef::new(),
                list: NodeRef::new(),
                reading: NodeRef::new(),
            },
        }
    }

    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) {
        self.settings.update(|s| {
            f(s);
            *s = s.clamped();
        });
    }
}

pub fn use_ui() -> Ui {
    expect_context::<Ui>()
}
