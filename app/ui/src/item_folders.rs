//! Dossiers de calendriers, de contacts, de tâches et de notes : menu clic droit (Nouveau, Renommer, Couleur ›, Supprimer),
//! sélecteur de dossier avec « Nouveau dossier ». Le travail est fait par `folders_cmds.rs` (`thunderlook_folders`).

use leptos::{html, prelude::*, task::spawn_local};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;

use crate::{
    menu::{MenuEntry, entry_body, move_focus},
    state::{Ui, use_ui},
    tasks::call,
};

/// Sortes de dossiers (même noms que `thunderlook_folders::Kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderKind {
    Calendar,
    Contact,
    Task,
    Note,
}

impl FolderKind {
    fn new_label(self) -> &'static str {
        match self {
            FolderKind::Calendar => "Nouveau calendrier",
            FolderKind::Contact | FolderKind::Task | FolderKind::Note => "Nouveau dossier",
        }
    }

    fn noun(self) -> &'static str {
        match self {
            FolderKind::Calendar => "calendrier",
            FolderKind::Contact | FolderKind::Task | FolderKind::Note => "dossier",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderRow {
    pub id: String,
    pub source_id: i64,
    /// Nom affiché.
    pub name: String,
    /// Nom enregistré (renommage).
    #[serde(default)]
    pub raw_name: String,
    pub section: String,
    pub count: u32,
    /// Teinte du fichier de données (0 à 7).
    #[serde(default)]
    pub account: u8,
    pub color: Option<u8>,
}

/// Pastille de couleur propre à un compte, pour les distinguer dans les sélecteurs.
pub fn account_dot(account: u8) -> &'static str {
    ["🔵", "🟠", "🟢", "🟣", "🔴", "🟡", "🟤", "⚪"][usize::from(account % 8)]
}

/// Palette des 25 couleurs de catégorie d'Outlook (`notes/outlook-couleurs-menus.md`), dans l'ordre 0 à 24.
const PALETTE: [(&str, &str); 25] = [
    ("Rouge", "#B10E1C"), ("Orange", "#C33400"), ("Pêche", "#E69A3E"), ("Jaune", "#E3CC00"), ("Vert", "#009C4E"),
    ("Sarcelle", "#00A3AE"), ("Olive", "#7EAC00"), ("Bleu", "#006CBE"), ("Violet", "#756CC8"), ("Canneberge", "#CC007E"),
    ("Acier", "#919DA1"), ("Acier foncé", "#005265"), ("Gris", "#8C8E83"), ("Gris foncé", "#5D6C70"), ("Noir", "#3E3E3E"),
    ("Rouge foncé", "#6A0A1A"), ("Orange foncé", "#B5490F"), ("Pêche foncé", "#814E29"), ("Jaune foncé", "#AE8E00"),
    ("Vert foncé", "#0A600A"), ("Sarcelle foncé", "#02767A"), ("Olive foncé", "#427505"), ("Bleu foncé", "#00345C"),
    ("Violet foncé", "#684697"), ("Canneberge foncé", "#8C0059"),
];

/// Même valeur que `folders_cmds::CUSTOM_COLOR_BASE` : `CalendarInfo::color` = 8 + indice de palette.
const CUSTOM_COLOR_BASE: usize = 8;

/// Classes `.cal-c8` à `.cal-c32` des calendriers colorés (les huit premières sont dans `app.css`).
fn palette_css() -> String {
    PALETTE.iter().enumerate().map(|(i, (_, hex))| format!(".cal-c{} {{ --c: {hex}; }}\n", CUSTOM_COLOR_BASE + i)).collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KindArg {
    kind: FolderKind,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateArg<'a> {
    kind: FolderKind,
    source_id: Option<i64>,
    name: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RenameArg<'a> {
    kind: FolderKind,
    id: &'a str,
    name: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "mode", content = "folder")]
enum Content {
    Refuse,
    MoveTo(i64),
    Delete,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteArg<'a> {
    kind: FolderKind,
    id: &'a str,
    content: Content,
}

#[derive(Serialize)]
struct ColorArg<'a> {
    id: &'a str,
    color: Option<u8>,
}

pub async fn list(kind: FolderKind) -> Result<Vec<FolderRow>, String> {
    call("item_folders", &KindArg { kind }).await
}

/// Dossiers de contacts, vides compris, pour le volet de gauche.
pub async fn contact_folders() -> Result<Vec<thunderlook_model::contacts::ContactFolder>, String> {
    Ok(list(FolderKind::Contact)
        .await?
        .into_iter()
        .map(|f| thunderlook_model::contacts::ContactFolder { id: f.id, name: f.name, section: f.section, count: f.count })
        .collect())
}

/// `c12` / `k12` / `f12` → 12.
fn num(id: &str) -> i64 {
    id.get(1..).and_then(|n| n.parse().ok()).unwrap_or(0)
}

#[derive(Clone, Debug, PartialEq)]
enum Dlg {
    /// Nouveau dossier (dans le fichier de données `source`), ou renommage.
    Name { rename: Option<FolderRow>, source: Option<i64> },
    Delete(FolderRow),
}

/// État d'un module : sorte de dossiers, liste (vides compris), menu et boîte ouverts.
#[derive(Clone, Copy)]
pub struct FolderUi {
    pub kind: FolderKind,
    pub list: RwSignal<Vec<FolderRow>>,
    menu: RwSignal<Option<(f64, f64, Option<FolderRow>)>>,
    dialog: RwSignal<Option<Dlg>>,
}

impl FolderUi {
    pub fn new(kind: FolderKind) -> Self {
        Self { kind, list: RwSignal::new(Vec::new()), menu: RwSignal::new(None), dialog: RwSignal::new(None) }
    }

    /// Relit la liste des dossiers.
    pub fn refresh(self) {
        let ui = use_ui();
        spawn_local(async move {
            match list(self.kind).await {
                Ok(l) => self.list.set(l),
                Err(e) => ui.error.set(Some(e)),
            }
        });
    }

    /// Clic droit : menu du dossier `id` (`c…`, `k…`, `f…`), ou seulement « Nouveau » si `id` est vide.
    pub fn open_menu(self, ev: &leptos::ev::MouseEvent, id: &str) {
        ev.prevent_default();
        ev.stop_propagation();
        let at = (f64::from(ev.client_x()), f64::from(ev.client_y()));
        self.show(at, id);
    }

    /// Ouvre le menu à une position donnée (bouton « … » du sélecteur, touche Menu).
    pub fn show(self, at: (f64, f64), id: &str) {
        let id = id.to_string();
        let ui = use_ui();
        spawn_local(async move {
            match list(self.kind).await {
                Ok(l) => {
                    let folder = l.iter().find(|f| f.id == id).cloned();
                    self.list.set(l);
                    self.menu.set(Some((at.0, at.1, folder)));
                }
                Err(e) => ui.error.set(Some(e)),
            }
        });
    }
}

/// Menu et boîtes de dialogue ; `on_change` est appelé après chaque création, renommage, suppression ou changement de couleur.
#[component]
pub fn FolderHost(f: FolderUi, #[prop(into)] on_change: Callback<()>) -> impl IntoView {
    let ui = use_ui();
    let menu_ref: NodeRef<html::Div> = NodeRef::new();
    let sub_colors = RwSignal::new(false);

    let close_menu = move || {
        sub_colors.set(false);
        f.menu.set(None);
    };
    let outside = window_event_listener(leptos::ev::pointerdown, move |ev| {
        if f.menu.get_untracked().is_none() {
            return;
        }
        let inside = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
            .zip(menu_ref.get_untracked())
            .is_some_and(|(t, m)| m.contains(Some(&t)));
        if !inside {
            close_menu();
        }
    });
    on_cleanup(move || outside.remove());
    Effect::new(move |_| {
        f.menu.track();
        sub_colors.track();
        if let Some(m) = menu_ref.get() {
            request_animation_frame(move || move_focus(&m, 1));
        }
    });

    let entries = move || -> Vec<MenuEntry> {
        let Some((_, _, folder)) = f.menu.get() else { return Vec::new() };
        let kind = f.kind;
        if sub_colors.get() {
            let Some(folder) = folder else { return Vec::new() };
            let mut out = vec![MenuEntry::radio("Automatique", folder.color.is_none(), {
                let id = folder.id.clone();
                move || set_color(ui, f, on_change, id.clone(), None)
            })];
            out.extend(PALETTE.iter().enumerate().map(|(i, (label, hex))| {
                let id = folder.id.clone();
                MenuEntry::radio(*label, folder.color == Some(i as u8), move || set_color(ui, f, on_change, id.clone(), Some(i as u8))).with_swatch(Some((*hex).to_string()))
            }));
            return out;
        }
        let source = folder.as_ref().map(|x| x.source_id);
        let mut out = vec![MenuEntry::action(kind.new_label(), move || f.dialog.set(Some(Dlg::Name { rename: None, source })))];
        if let Some(folder) = folder {
            let (r, d) = (folder.clone(), folder.clone());
            out.push(MenuEntry::action("Renommer", move || f.dialog.set(Some(Dlg::Name { rename: Some(r.clone()), source: None }))));
            if kind == FolderKind::Calendar {
                out.push(MenuEntry::submenu("Couleur", Vec::new()));
            }
            out.push(MenuEntry::action("Supprimer", move || f.dialog.set(Some(Dlg::Delete(d.clone())))).separated());
        }
        out
    };

    let style = move || {
        f.menu
            .get()
            .map(|(x, y, _)| {
                let w = window().inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1200.0);
                let h = window().inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(800.0);
                format!("left: {}px; top: {}px", x.min(w - 270.0).max(0.0), y.min(h - 370.0).max(0.0))
            })
            .unwrap_or_default()
    };

    view! {
        <style>{palette_css()}</style>
        <Show when=move || f.menu.with(Option::is_some)>
            <div class="menu context-menu" role="menu" aria-label="Menu du dossier" style=style node_ref=menu_ref
                on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                    let Some(m) = menu_ref.get_untracked() else { return };
                    match ev.key().as_str() {
                        "ArrowDown" => move_focus(&m, 1),
                        "ArrowUp" => move_focus(&m, -1),
                        "ArrowLeft" if sub_colors.get_untracked() => sub_colors.set(false),
                        "Escape" | "Tab" => close_menu(),
                        _ => return,
                    }
                    ev.prevent_default();
                    ev.stop_propagation();
                }>
                {move || {
                    let in_colors = sub_colors.get();
                    let back = in_colors.then(|| view! {
                        <button role="menuitem" tabindex="-1" class="menu-back" on:click=move |_| sub_colors.set(false)>
                            <span class="check"></span>
                            "‹ Couleur"
                        </button>
                        <div class="menu-sep" role="separator"></div>
                    });
                    let list = entries()
                        .into_iter()
                        .map(|entry| {
                            let role = if entry.checked.is_some() { "menuitemradio" } else { "menuitem" };
                            let body = entry_body(&entry);
                            let (on_select, has_children) = (entry.on_select, !entry.children.is_empty() || entry.label == "Couleur");
                            view! {
                                {entry.separator.then(|| view! { <div class="menu-sep" role="separator"></div> })}
                                <button role=role tabindex="-1" aria-checked=entry.checked.map(|c| c.to_string()) aria-haspopup=has_children.then_some("menu")
                                    on:click=move |_| {
                                        if has_children {
                                            sub_colors.set(true);
                                        } else if let Some(cb) = on_select {
                                            cb.run(());
                                            close_menu();
                                        }
                                    }>
                                    {body}
                                    {(has_children && entry.children.is_empty()).then(|| view! { <span class="menu-label">"›"</span> })}
                                </button>
                            }
                        })
                        .collect_view();
                    view! { {back} {list} }
                }}
            </div>
        </Show>
        <NameDialog f=f on_change=on_change />
        <DeleteDialog f=f on_change=on_change />
    }
}

fn set_color(ui: Ui, f: FolderUi, on_change: Callback<()>, id: String, color: Option<u8>) {
    spawn_local(async move {
        match call::<_, ()>("calendar_folder_color", &ColorArg { id: &id, color }).await {
            Ok(()) => {
                f.refresh();
                on_change.run(());
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

#[component]
fn NameDialog(f: FolderUi, on_change: Callback<()>) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let input: NodeRef<html::Input> = NodeRef::new();
    let dlg = Memo::new(move |_| match f.dialog.get() {
        Some(d @ Dlg::Name { .. }) => Some(d),
        _ => None,
    });
    Effect::new(move |_| {
        if let Some(Dlg::Name { rename, .. }) = dlg.get() {
            name.set(rename.map(|r| if r.raw_name.is_empty() { r.name } else { r.raw_name }).unwrap_or_default());
            error.set(None);
            request_animation_frame(move || {
                if let Some(i) = input.get() {
                    let _ = i.focus();
                    i.select();
                }
            });
        }
    });
    let close = move || f.dialog.set(None);
    let submit = move || {
        let Some(Dlg::Name { rename, source }) = dlg.get_untracked() else { return };
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        let text = name.get_untracked();
        spawn_local(async move {
            let res = match &rename {
                Some(r) => call::<_, ()>("item_folder_rename", &RenameArg { kind: f.kind, id: &r.id, name: &text }).await,
                None => call::<_, String>("item_folder_create", &CreateArg { kind: f.kind, source_id: source, name: &text }).await.map(|_| ()),
            };
            busy.set(false);
            match res {
                Ok(()) => {
                    f.dialog.set(None);
                    f.refresh();
                    on_change.run(());
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };
    view! {
        <Show when=move || dlg.with(Option::is_some)>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="folder-name-title"
                    on:keydown=move |ev| match ev.key().as_str() {
                        "Escape" => {
                            ev.prevent_default();
                            close();
                        }
                        "Enter" => {
                            ev.prevent_default();
                            submit();
                        }
                        _ => {}
                    }>
                    <h2 id="folder-name-title">
                        {move || match dlg.get() {
                            Some(Dlg::Name { rename: Some(_), .. }) => format!("Renommer le {}", f.kind.noun()),
                            _ => f.kind.new_label().to_string(),
                        }}
                    </h2>
                    <label class="field">
                        <span>"Nom"</span>
                        <input type="text" node_ref=input maxlength="255" prop:value=move || name.get() on:input=move |ev| name.set(event_target_value(&ev)) />
                    </label>
                    {move || error.get().map(|e| view! { <p class="dialog-note" role="alert">{e}</p> })}
                    <div class="dialog-buttons">
                        <button class="btn primary" prop:disabled=move || busy.get() on:click=move |_| submit()>"OK"</button>
                        <button class="btn" on:click=move |_| close()>"Annuler"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[component]
fn DeleteDialog(f: FolderUi, on_change: Callback<()>) -> impl IntoView {
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    // "move" ou "delete" ; destination : identifiant numérique du dossier.
    let mode = RwSignal::new("move".to_string());
    let target = RwSignal::new(0_i64);
    let dlg = Memo::new(move |_| match f.dialog.get() {
        Some(Dlg::Delete(r)) => Some(r),
        _ => None,
    });
    // Autres dossiers du même fichier de données, utilisables comme destination.
    let others = Memo::new(move |_| match dlg.get() {
        Some(r) => f.list.with(|l| l.iter().filter(|x| x.source_id == r.source_id && x.id != r.id).cloned().collect::<Vec<_>>()),
        None => Vec::new(),
    });
    Effect::new(move |_| {
        if dlg.get().is_some() {
            error.set(None);
            mode.set("move".into());
            target.set(others.with_untracked(|o| o.first().map_or(0, |x| num(&x.id))));
        }
    });
    let close = move || f.dialog.set(None);
    let confirm = move || {
        let Some(r) = dlg.get_untracked() else { return };
        if busy.get_untracked() {
            return;
        }
        let content = match (r.count, mode.get_untracked().as_str()) {
            (0, _) => Content::Refuse,
            (_, "delete") => Content::Delete,
            _ if target.get_untracked() > 0 => Content::MoveTo(target.get_untracked()),
            _ => {
                error.set(Some("Choisissez le dossier de destination.".into()));
                return;
            }
        };
        busy.set(true);
        spawn_local(async move {
            let res = call::<_, u32>("item_folder_delete", &DeleteArg { kind: f.kind, id: &r.id, content }).await;
            busy.set(false);
            match res {
                Ok(_) => {
                    f.dialog.set(None);
                    f.refresh();
                    on_change.run(());
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };
    view! {
        <Show when=move || dlg.with(Option::is_some)>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="folder-del-title"
                    on:keydown=move |ev| {
                        if ev.key() == "Escape" {
                            ev.prevent_default();
                            close();
                        }
                    }>
                    <h2 id="folder-del-title">{move || format!("Supprimer le {}", f.kind.noun())}</h2>
                    {move || {
                        let r = dlg.get()?;
                        Some(if r.count == 0 {
                            view! { <p>{format!("Supprimer « {} » ?", r.name)}</p> }.into_any()
                        } else {
                            let empty_others = others.with(Vec::is_empty);
                            view! {
                                <p>{format!("« {} » contient {} élément(s).", r.name, r.count)}</p>
                                <label class="field">
                                    <input type="radio" name="folder-del" prop:checked=move || mode.get() == "move" prop:disabled=empty_others
                                        on:change=move |_| mode.set("move".into()) />
                                    " Déplacer le contenu vers… "
                                    <select aria-label="Dossier de destination" prop:disabled=empty_others
                                        on:change=move |ev| target.set(event_target_value(&ev).parse().unwrap_or(0))>
                                        {others.get().into_iter().map(|o| view! { <option value=num(&o.id).to_string()>{o.name}</option> }).collect_view()}
                                    </select>
                                </label>
                                <label class="field">
                                    <input type="radio" name="folder-del" prop:checked=move || mode.get() == "delete" on:change=move |_| mode.set("delete".into()) />
                                    " Supprimer aussi le contenu (gardé 60 jours dans la corbeille interne)"
                                </label>
                            }
                            .into_any()
                        })
                    }}
                    {move || error.get().map(|e| view! { <p class="dialog-note" role="alert">{e}</p> })}
                    <div class="dialog-buttons">
                        <button class="btn primary" prop:disabled=move || busy.get() on:click=move |_| confirm()>"Supprimer"</button>
                        <button class="btn" on:click=move |_| close()>"Annuler"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

/// Sélecteur de dossier avec « Nouveau dossier… » ; le bouton « … » ouvre le menu du dossier choisi (renommer, supprimer).
/// `value` : identifiant du dossier choisi (vide = `all_label`), `on_pick` : appelé avec le nouvel identifiant.
#[component]
pub fn FolderPicker(
    f: FolderUi,
    #[prop(into)] value: Signal<String>,
    #[prop(into)] on_pick: Callback<String>,
    /// Libellé de l'entrée « tous les dossiers » ; absent = pas d'entrée.
    #[prop(optional)]
    all_label: Option<&'static str>,
) -> impl IntoView {
    const NEW: &str = "\u{1}new";
    let select: NodeRef<html::Select> = NodeRef::new();
    view! {
        <select aria-label="Dossier" node_ref=select on:change=move |ev| {
            let v = event_target_value(&ev);
            if v == NEW {
                // Rend la sélection précédente en attendant la création.
                if let Some(s) = select.get_untracked() {
                    s.set_value(&value.get_untracked());
                }
                f.dialog.set(Some(Dlg::Name { rename: None, source: None }));
            } else {
                on_pick.run(v);
            }
        }>
            {all_label.map(|l| view! { <option value="" selected=move || value.get().is_empty()>{l}</option> })}
            {move || f.list.get().into_iter().map(|d| {
                let id = d.id.clone();
                view! { <option value=d.id.clone() selected=move || value.get() == id>{format!("{} {} — {}", account_dot(d.account), d.section, d.name)}</option> }
            }).collect_view()}
            <option value=NEW>{format!("＋ {}…", f.kind.new_label())}</option>
        </select>
        <button class="icon-btn small" aria-label="Menu du dossier" title="Renommer ou supprimer ce dossier"
            on:click=move |ev| f.show((f64::from(ev.client_x()), f64::from(ev.client_y())), &value.get_untracked())>
            "…"
        </button>
    }
}
