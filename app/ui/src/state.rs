//! État partagé de l'interface (fourni par contexte à tous les composants).

use std::collections::HashSet;

use leptos::{html, prelude::*};
use thunderlook_model::{DateGroup, MailSnapshot, MessageSummary, settings::Settings};

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
        }
    }

    pub fn dom_id(self) -> &'static str {
        match self {
            Tab::Home => "tab-home",
            Tab::SendReceive => "tab-sendreceive",
            Tab::Folder => "tab-folder",
            Tab::View => "tab-view",
            Tab::Help => "tab-help",
        }
    }
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
    pub snapshot: RwSignal<Option<MailSnapshot>>,
    pub error: RwSignal<Option<String>>,
    pub folder: RwSignal<String>,
    pub tree_focus: RwSignal<String>,
    pub selected: RwSignal<Option<String>>,
    pub unread_only: RwSignal<bool>,
    pub collapsed_groups: RwSignal<HashSet<DateGroup>>,
    pub collapsed_nodes: RwSignal<HashSet<String>>,
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
            snapshot: RwSignal::new(None),
            error: RwSignal::new(None),
            folder: RwSignal::new("inbox".into()),
            tree_focus: RwSignal::new("inbox".into()),
            selected: RwSignal::new(None),
            unread_only: RwSignal::new(false),
            collapsed_groups: RwSignal::new(HashSet::new()),
            collapsed_nodes: RwSignal::new(HashSet::new()),
            refs: Refs {
                search: NodeRef::new(),
                tabs: NodeRef::new(),
                tree: NodeRef::new(),
                list: NodeRef::new(),
                reading: NodeRef::new(),
            },
        }
    }

    /// Messages du dossier courant, filtre « Non lus » appliqué.
    pub fn visible_messages(&self) -> Vec<MessageSummary> {
        let folder = self.folder.get();
        let unread_only = self.unread_only.get();
        self.snapshot.with(|s| {
            s.as_ref()
                .map(|s| s.messages_in(&folder).into_iter().filter(|m| !unread_only || m.unread).collect())
                .unwrap_or_default()
        })
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
