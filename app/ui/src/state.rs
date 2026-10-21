//! État partagé de l'interface (fourni par contexte à tous les composants).

use std::collections::{HashMap, HashSet};

use leptos::{html, prelude::*};
use thunderlook_model::{
    CategoryInfo, Draft, calendar::{CalendarEvent, CalendarView}, FolderTree, IndexStatus, ListView, MessageSummary, SearchScope, SortField, quicksteps::QuickStep, settings::Settings,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Module {
    Mail,
    Calendar,
    Contacts,
    Tasks,
}

impl Module {
    pub const ALL: [Module; 4] = [Module::Mail, Module::Calendar, Module::Contacts, Module::Tasks];

    pub fn label(self) -> &'static str {
        match self {
            Module::Mail => "Courrier",
            Module::Calendar => "Calendrier",
            Module::Contacts => "Contacts",
            Module::Tasks => "Tâches",
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            Module::Mail => "Ctrl+1",
            Module::Calendar => "Ctrl+2",
            Module::Contacts => "Ctrl+3",
            Module::Tasks => "Ctrl+4",
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
    /// « Règles et alertes », ou directement « Exécuter les règles maintenant » (J6).
    Rules { run: bool },
    /// « Gérer les actions rapides » (`edit` : `None`), ou l'éditeur d'une action rapide (indice ; la longueur de
    /// la liste pour une nouvelle). `first_use` : ouverte à l'exécution d'une action rapide à configurer.
    QuickSteps { edit: Option<usize>, first_use: bool },
    /// « Paramètres du compte » (J7).
    Accounts,
    /// « Réponses automatiques » (J7).
    AutoReply,
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
    pub cal_selected: RwSignal<Option<CalendarEvent>>,
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
            cal_selected: RwSignal::new(None),
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
