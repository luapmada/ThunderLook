//! Barre de commandes à onglets : mode compact (une rangée, défaut), complet (groupes nommés)
//! ou onglets seuls. Les commandes sont décrites par un registre statique ; celles des jalons
//! suivants sont présentes mais désactivées.

use icondata_core::Icon;
use leptos::prelude::*;
use thunderlook_model::{
    FolderOp, MessageOp,
    settings::{BarMode, Density, ReadingPane},
};

use crate::{
    actions::{self, folder_name, real_folder},
    icons::*,
    menu::{MenuButton, MenuEntry},
    menus,
    state::{ConfirmAction, Dialog, Module, PickPurpose, Tab, Ui, use_ui},
};

/// Comportement d'une commande.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Prévue, pas encore disponible.
    Pending,
    /// Menu de position du volet de lecture.
    ReadingPaneMenu,
    /// Menu du volet des dossiers.
    FolderPaneMenu,
    /// Bascule de densité compacte.
    DensityToggle,
    // J3 : organisation.
    Delete,
    Archive,
    ToggleRead,
    MoveMenu,
    CategoriesMenu,
    FlagMenu,
    NewFolder,
    RenameFolder,
    MoveFolder,
    DeleteFolder,
    MarkAllRead,
    EmptyFolder,
    Favorite,
}

impl Action {
    fn is_menu(self) -> bool {
        matches!(self, Self::ReadingPaneMenu | Self::FolderPaneMenu | Self::MoveMenu | Self::CategoriesMenu | Self::FlagMenu)
    }
}

/// Exécute une commande simple (sans menu).
fn perform(ui: Ui, action: Action) {
    let folder = real_folder(&ui.folder.get_untracked());
    let plain = ui.tree.with_untracked(|t| {
        t.as_ref()
            .and_then(|t| t.find_folder(&folder))
            .is_some_and(|f| matches!(f.role, thunderlook_model::FolderRole::Folder | thunderlook_model::FolderRole::Archive))
    });
    let name = folder_name(ui, &folder);
    match action {
        Action::Delete => actions::delete(ui, false),
        Action::Archive => actions::run(ui, MessageOp::Archive),
        Action::ToggleRead => actions::run(ui, MessageOp::ToggleRead),
        Action::DensityToggle => ui.update_settings(|s| {
            s.density = if s.density == Density::Compact { Density::Comfortable } else { Density::Compact }
        }),
        Action::NewFolder => ui.dialog.set(Some(Dialog::NewFolder { parent: folder })),
        Action::RenameFolder if plain => ui.dialog.set(Some(Dialog::Rename { id: folder, name })),
        Action::MoveFolder if plain => {
            ui.tree_focus.set(folder);
            ui.dialog.set(Some(Dialog::PickFolder { purpose: PickPurpose::MoveFolder }));
        }
        Action::DeleteFolder if plain => ui.dialog.set(Some(Dialog::Confirm {
            text: format!("Supprimer le dossier « {name} » et tout son contenu ?"),
            action: ConfirmAction::DeleteFolder(folder),
        })),
        Action::RenameFolder | Action::MoveFolder | Action::DeleteFolder => {
            ui.error.set(Some(format!("« {name} » est un dossier spécial : action impossible.")));
        }
        Action::MarkAllRead => actions::run_folder(ui, FolderOp::MarkAllRead { id: folder }),
        Action::EmptyFolder => ui.dialog.set(Some(Dialog::Confirm {
            text: format!("Supprimer tous les éléments du dossier « {name} » ?"),
            action: ConfirmAction::EmptyFolder(folder),
        })),
        Action::Favorite => {
            let op = if menus::is_favorite(ui, &folder) { FolderOp::RemoveFavorite { id: folder } } else { FolderOp::AddFavorite { id: folder } };
            actions::run_folder(ui, op);
        }
        _ => {}
    }
}

fn menu_entries(ui: Ui, action: Action) -> Vec<MenuEntry> {
    match action {
        Action::MoveMenu => menus::move_entries(ui),
        Action::CategoriesMenu => menus::category_entries(ui),
        Action::FlagMenu => menus::flag_entries(ui),
        Action::ReadingPaneMenu => {
            let current = ui.settings.get().reading_pane;
            [(ReadingPane::Right, "À droite"), (ReadingPane::Bottom, "En bas"), (ReadingPane::Off, "Désactivé")]
                .into_iter()
                .map(|(v, label)| MenuEntry::radio(label, current == v, move || ui.update_settings(|s| s.reading_pane = v)))
                .collect()
        }
        Action::FolderPaneMenu => {
            let collapsed = ui.settings.get().folder_pane_collapsed;
            vec![
                MenuEntry::radio("Normal", !collapsed, move || ui.update_settings(|s| s.folder_pane_collapsed = false)),
                MenuEntry::radio("Réduit", collapsed, move || ui.update_settings(|s| s.folder_pane_collapsed = true)),
            ]
        }
        _ => Vec::new(),
    }
}

pub struct Cmd {
    pub label: &'static str,
    pub icon: Icon,
    pub shortcut: Option<&'static str>,
    /// Ouvre un menu (flèche ▾).
    pub menu: bool,
    /// Visible dans la barre compacte (sinon dans « Plus de commandes »).
    pub compact: bool,
    /// Grand bouton dans la barre complète.
    pub large: bool,
    pub action: Action,
}

const fn cmd(label: &'static str, icon: Icon, shortcut: Option<&'static str>, menu: bool, compact: bool, large: bool) -> Cmd {
    Cmd { label, icon, shortcut, menu, compact, large, action: Action::Pending }
}

const fn act(label: &'static str, icon: Icon, action: Action, menu: bool) -> Cmd {
    Cmd { label, icon, shortcut: None, menu, compact: true, large: true, action }
}

/// Commande active, avec raccourci et visibilité.
const fn wired(label: &'static str, icon: Icon, shortcut: Option<&'static str>, compact: bool, large: bool, action: Action) -> Cmd {
    Cmd { label, icon, shortcut, menu: action.is_menu_const(), compact, large, action }
}

impl Action {
    const fn is_menu_const(self) -> bool {
        matches!(self, Self::ReadingPaneMenu | Self::FolderPaneMenu | Self::MoveMenu | Self::CategoriesMenu | Self::FlagMenu)
    }
}

pub struct Group {
    pub label: &'static str,
    pub cmds: &'static [Cmd],
}

const MAIL_HOME: &[Group] = &[
    Group {
        label: "Nouveau",
        cmds: &[
            cmd("Nouveau message", MAIL_PLUS, Some("Ctrl+N"), false, true, true),
            cmd("Nouveaux éléments", PLUS, None, true, false, true),
        ],
    },
    Group {
        label: "Supprimer",
        cmds: &[
            cmd("Ignorer la conversation", MESSAGE_OFF, Some("Ctrl+Suppr"), false, false, false),
            cmd("Élaguer la conversation", SCISSORS, None, true, false, false),
            cmd("Indésirables", BAN, None, true, false, false),
            wired("Supprimer", TRASH, Some("Suppr"), true, true, Action::Delete),
            wired("Archiver", ARCHIVE, None, true, true, Action::Archive),
        ],
    },
    Group {
        label: "Répondre",
        cmds: &[
            cmd("Répondre", REPLY, Some("Ctrl+R"), false, true, true),
            cmd("Répondre à tous", REPLY_ALL, Some("Ctrl+Maj+R"), false, true, true),
            cmd("Transférer", FORWARD, Some("Ctrl+F"), false, true, true),
            cmd("Réunion", CALENDAR_PLUS, Some("Ctrl+Alt+R"), false, false, false),
        ],
    },
    Group { label: "Actions enregistrées", cmds: &[cmd("Actions enregistrées", ZAP, None, true, true, true)] },
    Group {
        label: "Déplacer",
        cmds: &[
            wired("Déplacer vers", FOLDER_INPUT, Some("Ctrl+Maj+V"), true, true, Action::MoveMenu),
            cmd("Règles", LIST_FILTER, None, true, false, true),
        ],
    },
    Group {
        label: "Indicateurs",
        cmds: &[
            wired("Lu/Non lu", MAIL_OPEN, Some("Ctrl+Q / Ctrl+U"), true, true, Action::ToggleRead),
            wired("Catégories", TAGS, None, true, true, Action::CategoriesMenu),
            wired("Suivi", FLAG, Some("Inser"), true, true, Action::FlagMenu),
        ],
    },
    Group {
        label: "Rechercher",
        cmds: &[
            cmd("Carnet d'adresses", BOOK_USER, Some("Ctrl+Maj+B"), false, true, false),
            cmd("Filtrer le courrier", FUNNEL, None, true, true, false),
        ],
    },
    Group {
        label: "Envoyer/Recevoir",
        cmds: &[cmd("Tout envoyer et recevoir", REFRESH, Some("F9"), false, true, true)],
    },
];

const SEND_RECEIVE: &[Group] = &[
    Group {
        label: "Envoyer et recevoir",
        cmds: &[
            cmd("Tout envoyer et recevoir", REFRESH, Some("F9"), false, true, true),
            cmd("Mettre à jour le dossier", REFRESH, None, false, false, false),
            cmd("Tout envoyer", SEND, None, false, true, false),
            cmd("Groupes d'envoi/réception", ROWS_3, None, true, false, false),
        ],
    },
    Group {
        label: "Téléchargement",
        cmds: &[
            cmd("Afficher la progression", DOWNLOAD, None, false, true, false),
            cmd("Tout annuler", BAN, None, false, false, false),
        ],
    },
    Group {
        label: "Serveur",
        cmds: &[
            cmd("Télécharger les en-têtes", DOWNLOAD, None, false, false, false),
            cmd("Marquer pour téléchargement", CHECK, Some("Ctrl+Alt+M"), true, false, false),
            cmd("Retirer les marques", MINUS, Some("Ctrl+Alt+U"), true, false, false),
        ],
    },
    Group {
        label: "Préférences",
        cmds: &[cmd("Travailler hors connexion", CLOUD_OFF, None, false, true, true)],
    },
];

const FOLDER: &[Group] = &[
    Group {
        label: "Nouveau",
        cmds: &[
            wired("Nouveau dossier", FOLDER_PLUS, Some("Ctrl+Maj+E"), true, true, Action::NewFolder),
            cmd("Nouveau dossier virtuel", FOLDER_SEARCH, None, false, true, true),
        ],
    },
    Group {
        label: "Actions",
        cmds: &[
            wired("Renommer le dossier", PENCIL, Some("F2"), true, false, Action::RenameFolder),
            wired("Déplacer le dossier", FOLDER_INPUT, None, false, false, Action::MoveFolder),
            wired("Supprimer le dossier", TRASH, None, false, false, Action::DeleteFolder),
        ],
    },
    Group {
        label: "Nettoyer",
        cmds: &[
            wired("Tout marquer comme lu", CHECK_CHECK, None, true, false, Action::MarkAllRead),
            cmd("Exécuter les règles", LIST_CHECKS, None, false, false, false),
            cmd("Trier les dossiers de A à Z", SORT_AZ, None, false, false, false),
            wired("Vider le dossier", ERASER, None, false, false, Action::EmptyFolder),
        ],
    },
    Group { label: "Favoris", cmds: &[wired("Ajouter aux favoris", STAR, None, true, true, Action::Favorite)] },
    Group { label: "Propriétés", cmds: &[cmd("Propriétés du dossier", INFO, None, false, false, true)] },
];

const VIEW: &[Group] = &[
    Group {
        label: "Affichage actuel",
        cmds: &[
            cmd("Changer d'affichage", EYE, None, true, true, true),
            cmd("Réglages de l'affichage", SETTINGS, None, false, false, true),
            cmd("Rétablir l'affichage", UNDO, None, false, false, false),
        ],
    },
    Group {
        label: "Messages",
        cmds: &[cmd("Afficher en conversations", ROWS_2, None, false, false, true)],
    },
    Group {
        label: "Disposition",
        cmds: &[
            act("Volet des dossiers", PANEL_LEFT_CLOSE, Action::FolderPaneMenu, true),
            act("Volet de lecture", PANEL_RIGHT, Action::ReadingPaneMenu, true),
            act("Espacement plus étroit", ROWS_3, Action::DensityToggle, false),
        ],
    },
    Group { label: "Fenêtre", cmds: &[cmd("Fenêtre des rappels", CLOCK, None, false, false, true)] },
];

const HELP: &[Group] = &[Group {
    label: "Aide",
    cmds: &[cmd("Aide", HELP_ICON, Some("F1"), false, true, true), cmd("Raccourcis clavier", INFO, None, false, true, true)],
}];

use crate::icons::HELP as HELP_ICON;

const CALENDAR_HOME: &[Group] = &[
    Group {
        label: "Nouveau",
        cmds: &[
            cmd("Nouveau rendez-vous", CALENDAR_PLUS, Some("Ctrl+N"), false, true, true),
            cmd("Nouvelle réunion", USERS, Some("Ctrl+Maj+Q"), true, true, true),
        ],
    },
    Group {
        label: "Atteindre",
        cmds: &[
            cmd("Aujourd'hui", CALENDAR, None, false, true, true),
            cmd("7 prochains jours", CALENDAR_DAYS, None, false, true, false),
        ],
    },
    Group {
        label: "Organiser",
        cmds: &[
            cmd("Jour", CALENDAR, Some("Ctrl+Alt+1"), false, true, false),
            cmd("Semaine de travail", CALENDAR_DAYS, Some("Ctrl+Alt+2"), false, true, false),
            cmd("Semaine", CALENDAR_DAYS, Some("Ctrl+Alt+3"), false, true, false),
            cmd("Mois", CALENDAR, Some("Ctrl+Alt+4"), false, true, false),
        ],
    },
];

const CONTACTS_HOME: &[Group] = &[
    Group { label: "Nouveau", cmds: &[cmd("Nouveau contact", PLUS, Some("Ctrl+N"), true, true, true)] },
    Group { label: "Supprimer", cmds: &[cmd("Supprimer", TRASH, Some("Suppr"), false, true, true)] },
    Group { label: "Communiquer", cmds: &[cmd("Nouveau message", MAIL_PLUS, None, false, true, true)] },
    Group { label: "Indicateurs", cmds: &[cmd("Catégories", TAGS, None, true, true, false), cmd("Suivi", FLAG, None, true, true, false)] },
];

const TASKS_HOME: &[Group] = &[
    Group {
        label: "Nouveau",
        cmds: &[
            cmd("Nouvelle tâche", SQUARE_CHECK, Some("Ctrl+N"), false, true, true),
            cmd("Nouveau message", MAIL_PLUS, Some("Ctrl+Maj+M"), false, true, false),
        ],
    },
    Group {
        label: "Gérer",
        cmds: &[
            cmd("Marquer comme terminée", CHECK, None, false, true, true),
            cmd("Retirer de la liste", MINUS, None, false, true, false),
        ],
    },
];

pub fn groups(module: Module, tab: Tab) -> &'static [Group] {
    match (tab, module) {
        (Tab::Home, Module::Mail) => MAIL_HOME,
        (Tab::Home, Module::Calendar) => CALENDAR_HOME,
        (Tab::Home, Module::Contacts) => CONTACTS_HOME,
        (Tab::Home, Module::Tasks) => TASKS_HOME,
        (Tab::SendReceive, _) => SEND_RECEIVE,
        (Tab::Folder, _) => FOLDER,
        (Tab::View, _) => VIEW,
        (Tab::Help, _) => HELP,
    }
}

/// Format ARIA des raccourcis (« Control+Shift+R ») à partir du libellé affiché (« Ctrl+Maj+R »).
fn aria_shortcut(display: &str) -> String {
    display
        .split(" / ")
        .map(|combo| {
            combo
                .split('+')
                .map(|k| match k {
                    "Ctrl" => "Control",
                    "Maj" => "Shift",
                    "Suppr" => "Delete",
                    "Inser" => "Insert",
                    other => other,
                })
                .collect::<Vec<_>>()
                .join("+")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn tooltip(c: &Cmd) -> String {
    match c.shortcut {
        Some(s) => format!("{} ({s})", c.label),
        None => c.label.to_string(),
    }
}

/// Une commande rendue selon son action et le mode de barre.
fn command_view(c: &'static Cmd, full: bool) -> AnyView {
    let ui = use_ui();
    // Barre complète : grands boutons pour les commandes principales.
    // Barre compacte : libellé pour les commandes principales, icône seule pour les autres.
    let class = match (full, c.large) {
        (true, true) => "cmd large",
        (false, false) => "cmd icon-only",
        _ => "cmd",
    };
    match c.action {
        Action::Pending => view! {
            <button
                class=class
                aria-disabled="true"
                tabindex="-1"
                aria-label=c.label
                title=format!("{} — pas encore disponible", tooltip(c))
                aria-keyshortcuts=c.shortcut.map(aria_shortcut)
            >
                <Svg icon=c.icon />
                <span>{c.label}</span>
                {c.menu.then(|| view! { <Svg icon=CHEVRON_DOWN size=12 /> })}
            </button>
        }
        .into_any(),
        Action::DensityToggle => view! {
            <button
                class=class
                aria-label=c.label
                aria-pressed=move || (ui.settings.get().density == Density::Compact).to_string()
                title=tooltip(c)
                on:click=move |_| perform(ui, Action::DensityToggle)
            >
                <Svg icon=c.icon />
                <span>{c.label}</span>
            </button>
        }
        .into_any(),
        Action::Favorite => view! {
            <button
                class=class
                aria-label=move || if menus::is_favorite(ui, &ui.folder.get()) { "Retirer des favoris" } else { "Ajouter aux favoris" }
                title=move || if menus::is_favorite(ui, &ui.folder.get()) { "Retirer des favoris" } else { "Ajouter aux favoris" }
                on:click=move |_| perform(ui, Action::Favorite)
            >
                <Svg icon=c.icon />
                <span>{move || if menus::is_favorite(ui, &ui.folder.get()) { "Retirer des favoris" } else { "Ajouter aux favoris" }}</span>
            </button>
        }
        .into_any(),
        a if a.is_menu() => {
            let entries = Signal::derive(move || menu_entries(ui, a));
            view! { <MenuButton label=c.label.to_string() icon=c.icon entries=entries class=class /> }.into_any()
        }
        a => view! {
            <button
                class=class
                aria-label=c.label
                title=tooltip(c)
                aria-keyshortcuts=c.shortcut.map(aria_shortcut)
                on:click=move |_| perform(ui, a)
            >
                <Svg icon=c.icon />
                <span>{c.label}</span>
            </button>
        }
        .into_any(),
    }
}

#[component]
pub fn CommandBar() -> impl IntoView {
    let ui = use_ui();
    let mode = move || ui.settings.get().bar_mode;

    let select_tab = move |tab: Tab| {
        ui.tab.set(tab);
        if mode() == BarMode::TabsOnly {
            ui.bar_popup.set(true);
        }
    };

    let on_tabs_key = move |ev: leptos::ev::KeyboardEvent| {
        let idx = Tab::ALL.iter().position(|t| *t == ui.tab.get_untracked()).unwrap_or(0) as i32;
        let n = Tab::ALL.len() as i32;
        let next = match ev.key().as_str() {
            "ArrowRight" => (idx + 1).rem_euclid(n),
            "ArrowLeft" => (idx - 1).rem_euclid(n),
            "Home" => 0,
            "End" => n - 1,
            _ => return,
        };
        ev.prevent_default();
        let tab = Tab::ALL[next as usize];
        select_tab(tab);
        if let Some(el) = document().get_element_by_id(tab.dom_id()) {
            use wasm_bindgen::JsCast;
            if let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() {
                let _ = el.focus();
            }
        }
    };

    let bar_entries = Signal::derive(move || {
        let current = ui.settings.get().bar_mode;
        [(BarMode::Compact, "Barre compacte"), (BarMode::Full, "Barre complète"), (BarMode::TabsOnly, "Onglets seuls")]
            .into_iter()
            .map(|(v, label)| {
                MenuEntry::radio(label, current == v, move || {
                    ui.update_settings(|s| s.bar_mode = v);
                    ui.bar_popup.set(false);
                })
            })
            .collect::<Vec<_>>()
    });

    let overflow = Signal::derive(move || {
        groups(ui.module.get(), ui.tab.get())
            .iter()
            .flat_map(|g| g.cmds.iter())
            .filter(|c| !c.compact)
            .map(|c| match c.action {
                Action::Pending => MenuEntry::pending(c.label),
                a if a.is_menu() => MenuEntry::pending(c.label),
                a => MenuEntry::action(c.label, move || perform(ui, a)),
            })
            .collect::<Vec<_>>()
    });

    view! {
        <div class="commandbar">
            <div class="tabs">
                <button class="tab file" aria-disabled="true" title="Fichier — pas encore disponible" tabindex="-1">
                    "Fichier"
                </button>
                <div role="tablist" aria-label="Barre de commandes" class="tablist" node_ref=ui.refs.tabs on:keydown=on_tabs_key>
                    {Tab::ALL
                        .into_iter()
                        .map(|tab| {
                            let selected = move || ui.tab.get() == tab;
                            view! {
                                <button
                                    role="tab"
                                    class="tab"
                                    id=tab.dom_id()
                                    aria-selected=move || selected().to_string()
                                    aria-controls="command-panel"
                                    tabindex=move || if selected() { "0" } else { "-1" }
                                    on:click=move |_| select_tab(tab)
                                >
                                    {tab.label()}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
                <div class="tabs-spacer"></div>
                <MenuButton
                    label="Options de la barre de commandes"
                    icon=ROWS_2
                    show_label=false
                    entries=bar_entries
                    class="icon-btn"
                />
            </div>
            <div
                id="command-panel"
                role="tabpanel"
                aria-labelledby=move || ui.tab.get().dom_id()
                class="bar"
                class:compact=move || mode() != BarMode::Full
                class:full=move || mode() == BarMode::Full
                hidden=move || mode() == BarMode::TabsOnly && !ui.bar_popup.get()
            >
                <div class="bar-groups">
                {move || {
                    let full = mode() == BarMode::Full;
                    groups(ui.module.get(), ui.tab.get())
                        .iter()
                        .filter(|g| full || g.cmds.iter().any(|c| c.compact))
                        .map(|g| {
                            view! {
                                <div class="group" role="group" aria-label=g.label>
                                    <div class="group-cmds">
                                        {g.cmds
                                            .iter()
                                            .filter(|c| full || c.compact)
                                            .map(|c| command_view(c, full))
                                            .collect_view()}
                                    </div>
                                    <div class="group-label" aria-hidden="true">{g.label}</div>
                                </div>
                            }
                        })
                        .collect_view()
                }}
                </div>
                <div class="bar-end">
                    <Show when=move || mode() != BarMode::Full && !overflow.get().is_empty()>
                        <MenuButton label="Plus de commandes" icon=ELLIPSIS show_label=false entries=overflow class="icon-btn" />
                    </Show>
                </div>
            </div>
        </div>
    }
}
