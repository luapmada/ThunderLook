//! Volet de rédaction (J5), à la place du volet de lecture comme la réponse intégrée d'Outlook 2021.
//!
//! Le corps se modifie dans un `iframe` isolé : `sandbox` sans `allow-scripts` (aucun script du message cité ne
//! s'exécute), CSP sans contenu distant, `designMode` activé par l'interface. Enregistrement : Ctrl+S, bouton,
//! et automatiquement chaque minute si le message a changé. L'envoi viendra avec SMTP (J7).
//!
//! Mise en forme (lot AS) : `execCommand` dans l'iframe (police, taille, couleur, surlignage, alignement, lien) ;
//! image insérée en URI `data:` (le cœur la transforme en pièce intégrée `cid:` à l'enregistrement). Importance et
//! confirmation de lecture vivent dans le brouillon et deviennent des en-têtes du message.

use leptos::{html, prelude::*, task::spawn_local};
use icondata_lu::{
    LuAlignCenter as ALIGN_CENTER, LuAlignLeft as ALIGN_LEFT, LuAlignRight as ALIGN_RIGHT, LuArrowBigDown as IMPORTANCE_LOW,
    LuArrowBigUp as IMPORTANCE_HIGH, LuBaseline as FONT_COLOR, LuHighlighter as HIGHLIGHT, LuImagePlus as IMAGE_PLUS,
    LuLink as LINK, LuMailCheck as MAIL_CHECK, LuType as FONT,
};
use thunderlook_model::{ComposeKind, Draft, Importance, Signature};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};

use crate::{
    actions,
    address_book::{AddressBook, AddressField, append},
    icons::*,
    ipc,
    menu::{MenuButton, MenuEntry},
    state::{Ui, use_ui},
};

/// Délai de l'enregistrement automatique (ms).
const AUTOSAVE_MS: i32 = 60_000;

/// Polices du menu « Police ».
const FONTS: &[&str] = &["Aptos", "Arial", "Calibri", "Cambria", "Courier New", "Georgia", "Segoe UI", "Tahoma", "Times New Roman", "Verdana"];

/// Tailles de `execCommand("fontSize")` (1 à 7, balise `<font size>`) et leur équivalent en points.
const SIZES: &[(&str, &str)] = &[("1", "8"), ("2", "10"), ("3", "12"), ("4", "14"), ("5", "18"), ("6", "24"), ("7", "36")];

/// Couleurs standard de la palette d'Office.
const COLORS: &[(&str, &str)] = &[
    ("Rouge foncé", "#C00000"),
    ("Rouge", "#FF0000"),
    ("Orange", "#FFC000"),
    ("Jaune", "#FFFF00"),
    ("Vert clair", "#92D050"),
    ("Vert", "#00B050"),
    ("Bleu clair", "#00B0F0"),
    ("Bleu", "#0070C0"),
    ("Bleu foncé", "#002060"),
    ("Violet", "#7030A0"),
];

/// Couleurs de surlignage.
const HIGHLIGHTS: &[(&str, &str)] =
    &[("Jaune", "#FFFF00"), ("Vert vif", "#00FF00"), ("Turquoise", "#00FFFF"), ("Rose", "#FF00FF"), ("Gris", "#C0C0C0")];

/// Taille maximale d'une image insérée dans le corps (elle part dans le message).
const MAX_IMAGE: f64 = 10.0 * 1024.0 * 1024.0;

fn editor_doc(html: &str, styles: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:\">\
         <style>html{{background:#fff;color:#000;color-scheme:light}}\
         body{{margin:12px 16px;font-family:Calibri,'Segoe UI',Arial,sans-serif;font-size:11pt;line-height:1.35;overflow-wrap:break-word;min-height:200px}}\
         p.MsoNormal{{margin:0}}img{{max-width:100%}}</style><style>{styles}</style></head><body>{html}</body></html>"
    )
}

fn frame_doc(frame: NodeRef<html::Iframe>) -> Option<web_sys::HtmlDocument> {
    // Pas de `dyn_into` : le document de l'iframe appartient à une autre fenêtre, et le test `instanceof`
    // contre le `HTMLDocument` de la fenêtre principale échouerait. C'est bien un document HTML (srcdoc).
    Some(frame.get_untracked()?.content_document()?.unchecked_into())
}

/// Corps courant de l'éditeur.
fn editor_html(frame: NodeRef<html::Iframe>) -> Option<String> {
    frame_doc(frame)?.body().map(|b| b.inner_html())
}

/// Brouillon courant, corps de l'éditeur compris.
fn current(ui: Ui, frame: NodeRef<html::Iframe>) -> Option<Draft> {
    let mut d = ui.compose.get_untracked()?;
    if let Some(h) = editor_html(frame) {
        d.html = h;
    }
    Some(d)
}

/// Enregistre le brouillon ; `then` est appelé après succès.
fn save(ui: Ui, frame: NodeRef<html::Iframe>, then: impl FnOnce() + 'static) {
    let Some(d) = current(ui, frame) else { return };
    spawn_local(async move {
        match ipc::save_draft(&d).await {
            Ok(saved) => {
                // Les champs ont pu changer pendant l'aller-retour : on ne reprend que l'identifiant et les pièces jointes.
                ui.compose.update(|c| {
                    if let Some(c) = c {
                        c.id = saved.id.clone();
                        c.attachments = saved.attachments.clone();
                        c.html = d.html.clone();
                    }
                });
                ui.compose_dirty.set(false);
                ui.status.set(Some("Brouillon enregistré dans « Brouillons ».".into()));
                // Le dossier Brouillons et ses compteurs ont changé.
                actions::refresh_tree(ui);
                actions::reopen_view(ui);
                then();
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

/// « Envoyer » (J7) : le message part dans la Boîte d'envoi du compte ; l'envoi suit (événement « sync »).
fn send(ui: Ui, frame: NodeRef<html::Iframe>) {
    let Some(d) = current(ui, frame) else { return };
    spawn_local(async move {
        match ipc::send_draft(&d).await {
            Ok(_) => {
                ui.compose_dirty.set(false);
                ui.compose.set(None);
                ui.status.set(Some("Envoi en cours…".into()));
                actions::refresh_tree(ui);
                actions::reopen_view(ui);
            }
            Err(e) => ui.error.set(Some(e)),
        }
    });
}

fn exec(frame: NodeRef<html::Iframe>, command: &str, value: Option<&str>) {
    if let Some(doc) = frame_doc(frame) {
        let _ = doc.exec_command_with_show_ui_and_value(command, false, value.unwrap_or(""));
        if let Some(w) = frame.get_untracked().and_then(|f| f.content_window()) {
            let _ = w.focus();
        }
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Cible d'un lien saisi : schéma conservé s'il est sûr, `mailto:` pour une adresse, `https://` sinon. `None` :
/// schéma refusé (`javascript:`, `data:`…).
fn link_target(input: &str) -> Option<String> {
    let t = input.trim();
    let scheme = t.split_once(':').map(|(s, _)| s.to_ascii_lowercase()).filter(|s| {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) && !s.contains('.')
    });
    match scheme.as_deref() {
        Some("http" | "https" | "mailto" | "ftp" | "file" | "tel") => Some(t.to_string()),
        Some(_) => None,
        None if t.contains('@') && !t.contains('/') => Some(format!("mailto:{t}")),
        None => Some(format!("https://{t}")),
    }
}

/// Texte sélectionné dans l'éditeur.
fn selected_text(doc: &web_sys::HtmlDocument) -> String {
    js_sys::Reflect::get(doc, &JsValue::from_str("getSelection"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call0(doc).ok())
        .and_then(|s| s.dyn_into::<js_sys::Object>().ok())
        .map(|s| String::from(s.to_string()))
        .unwrap_or_default()
}

/// « Lien » (Ctrl+K) : adresse demandée ; sur une sélection, elle devient le lien ; sans sélection, l'adresse est
/// insérée comme lien. Une adresse vide retire le lien de la sélection.
fn insert_link(ui: Ui, frame: NodeRef<html::Iframe>) {
    let Some(doc) = frame_doc(frame) else { return };
    let Ok(Some(input)) = window().prompt_with_message_and_default("Adresse du lien (vide : retirer le lien) :", "https://") else {
        return;
    };
    let input = input.trim();
    if input.is_empty() || input == "https://" {
        exec(frame, "unlink", None);
    } else {
        let Some(url) = link_target(input) else {
            ui.error.set(Some("Adresse refusée : seuls http, https, mailto, ftp, file et tel sont acceptés.".into()));
            return;
        };
        if selected_text(&doc).trim().is_empty() {
            exec(frame, "insertHTML", Some(&format!("<a href=\"{0}\">{0}</a>", escape(&url))));
        } else {
            exec(frame, "createLink", Some(&url));
        }
    }
    ui.compose_dirty.set(true);
}

fn get(o: &JsValue, key: &str) -> Option<JsValue> {
    js_sys::Reflect::get(o, &JsValue::from_str(key)).ok().filter(|v| !v.is_undefined() && !v.is_null())
}

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            out.push(if i <= c.len() { A[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// Image choisie dans le sélecteur de fichiers, en URI `data:`. `None` : rien de choisi.
async fn picked_image(input: &web_sys::HtmlInputElement) -> Option<Result<String, String>> {
    let file = get(input.as_ref(), "files").and_then(|f| get(&f, "0"))?;
    let mime = get(&file, "type").and_then(|t| t.as_string()).unwrap_or_default();
    if !matches!(mime.as_str(), "image/png" | "image/jpeg" | "image/gif" | "image/bmp" | "image/webp") {
        return Some(Err("Format d'image non pris en charge (PNG, JPEG, GIF, BMP ou WebP).".into()));
    }
    if get(&file, "size").and_then(|s| s.as_f64()).unwrap_or(0.0) > MAX_IMAGE {
        return Some(Err("Image trop lourde pour le corps du message (10 Mo au plus) : joignez-la plutôt.".into()));
    }
    let read = get(&file, "arrayBuffer")?.dyn_into::<js_sys::Function>().ok()?;
    let promise = read.call0(&file).ok()?.dyn_into::<js_sys::Promise>().ok()?;
    let buffer = match wasm_bindgen_futures::JsFuture::from(promise).await {
        Ok(b) => b,
        Err(_) => return Some(Err("Lecture de l'image impossible.".into())),
    };
    Some(Ok(format!("data:{mime};base64,{}", base64(&js_sys::Uint8Array::new(&buffer).to_vec()))))
}

/// Menu de couleurs : « Automatique » (`auto`) puis la palette, chaque entrée appliquant `command`.
fn color_entries(
    ui: Ui,
    frame: NodeRef<html::Iframe>,
    command: &'static str,
    auto: (&'static str, &'static str),
    list: &'static [(&'static str, &'static str)],
) -> Vec<MenuEntry> {
    std::iter::once(auto)
        .chain(list.iter().copied())
        .enumerate()
        .map(|(i, (name, color))| {
            let entry = MenuEntry::action(name, move || {
                exec(frame, command, Some(color));
                ui.compose_dirty.set(true);
            })
            .with_swatch((i > 0).then(|| color.to_string()));
            if i == 1 { entry.separated() } else { entry }
        })
        .collect()
}

#[component]
pub fn ComposePane() -> impl IntoView {
    let ui = use_ui();
    let frame = NodeRef::<html::Iframe>::new();
    let show_bcc = RwSignal::new(false);
    let signatures = RwSignal::new(Vec::<Signature>::new());
    spawn_local(async move {
        if let Ok(s) = ipc::signatures().await {
            signatures.set(s);
        }
    });
    show_bcc.set(ui.compose.with_untracked(|d| d.as_ref().is_some_and(|d| !d.bcc.is_empty())));

    // Document de l'éditeur, figé à l'ouverture (les modifications vivent dans l'iframe).
    let srcdoc = ui.compose.with_untracked(|d| d.as_ref().map(|d| editor_doc(&d.html, &d.styles)).unwrap_or_default());
    let kind = ui.compose.with_untracked(|d| d.as_ref().map(|d| d.kind).unwrap_or_default());

    // Ctrl+S dans l'éditeur ; toute frappe le marque modifié.
    let on_key = StoredValue::new_local(Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
        if ev.ctrl_key() && ev.key().eq_ignore_ascii_case("s") {
            ev.prevent_default();
            save(ui, frame, || {});
        }
        if ev.ctrl_key() && ev.key() == "Enter" {
            ev.prevent_default();
            send(ui, frame);
        }
        if ev.ctrl_key() && ev.key().eq_ignore_ascii_case("k") {
            ev.prevent_default();
            insert_link(ui, frame);
        }
    }));
    let on_input = StoredValue::new_local(Closure::<dyn Fn(web_sys::Event)>::new(move |_| ui.compose_dirty.set(true)));
    let on_load = move |_| {
        let Some(doc) = frame_doc(frame) else { return };
        doc.set_design_mode("on");
        on_key.with_value(|k| {
            let _ = doc.add_event_listener_with_callback("keydown", k.as_ref().unchecked_ref());
        });
        on_input.with_value(|i| {
            let _ = doc.add_event_listener_with_callback("input", i.as_ref().unchecked_ref());
        });
        // Nouveau message : le curseur dans « À » ; réponse : dans le corps.
        if kind == ComposeKind::New || kind == ComposeKind::Forward {
            if let Some(to) = document().get_element_by_id("compose-to").and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok()) {
                let _ = to.focus();
            }
        } else if let Some(w) = frame.get_untracked().and_then(|f| f.content_window()) {
            let _ = w.focus();
        }
    };

    // Enregistrement automatique.
    let timer = StoredValue::new_local(Closure::<dyn Fn()>::new(move || {
        if ui.compose.with_untracked(Option::is_some) && ui.compose_dirty.get_untracked() {
            save(ui, frame, || {});
        }
    }));
    let handle = timer.with_value(|t| {
        window().set_interval_with_callback_and_timeout_and_arguments_0(t.as_ref().unchecked_ref(), AUTOSAVE_MS).ok()
    });
    on_cleanup(move || {
        if let Some(h) = handle {
            window().clear_interval_with_handle(h);
        }
    });

    let field = move |get: fn(&Draft) -> String, set: fn(&mut Draft, String)| {
        (
            move || ui.compose.with(|d| d.as_ref().map(get).unwrap_or_default()),
            move |ev: leptos::ev::Event| {
                let v = event_target_value(&ev);
                ui.compose.update(|d| {
                    if let Some(d) = d {
                        set(d, v);
                    }
                });
                ui.compose_dirty.set(true);
            },
        )
    };
    let (to, _) = field(|d| d.to.clone(), |d, v| d.to = v);
    let (cc, _) = field(|d| d.cc.clone(), |d, v| d.cc = v);
    let (bcc, _) = field(|d| d.bcc.clone(), |d, v| d.bcc = v);
    // Champs de destinataires (autocomplétion) et Carnet d'adresses : écrivent dans le brouillon comme `field`.
    let set_addr = move |set: fn(&mut Draft, String)| {
        Callback::new(move |v: String| {
            ui.compose.update(|d| {
                if let Some(d) = d {
                    set(d, v);
                }
            });
            ui.compose_dirty.set(true);
        })
    };
    let book_open = RwSignal::new(false);
    let apply_book = Callback::new(move |(t, c, b): (Vec<String>, Vec<String>, Vec<String>)| {
        ui.compose.update(|d| {
            if let Some(d) = d {
                d.to = append(&d.to, &t);
                d.cc = append(&d.cc, &c);
                d.bcc = append(&d.bcc, &b);
            }
        });
        if !b.is_empty() {
            show_bcc.set(true);
        }
        ui.compose_dirty.set(true);
    });
    let (subject, set_subject) = field(|d| d.subject.clone(), |d, v| d.subject = v);
    // Champ « De » : comptes de messagerie. Sans compte choisi, le brouillon reste dans son fichier de données et
    // l'envoi passe par le compte par défaut (premier compte), comme avant le champ.
    let accounts = RwSignal::new(Vec::<thunderlook_model::AccountSummary>::new());
    spawn_local(async move {
        if let Ok(list) = ipc::accounts().await {
            accounts.set(list);
        }
    });
    let from = move || ui.compose.with(|d| d.as_ref().and_then(|d| d.from_account.clone()).unwrap_or_default());
    let set_from = move |ev: leptos::ev::Event| {
        let v = event_target_value(&ev);
        ui.compose.update(|d| {
            if let Some(d) = d {
                d.from_account = (!v.is_empty()).then_some(v);
            }
        });
        ui.compose_dirty.set(true);
    };

    let attach = move |_| {
        spawn_local(async move {
            match ipc::pick_attachments().await {
                Ok(files) if !files.is_empty() => {
                    ui.compose.update(|d| {
                        if let Some(d) = d {
                            d.attachments.extend(files);
                        }
                    });
                    ui.compose_dirty.set(true);
                }
                Ok(_) => {}
                Err(e) => ui.error.set(Some(e)),
            }
        })
    };
    let discard = move |_| {
        // Un brouillon déjà enregistré reste dans « Brouillons » ; seules les modifications non enregistrées sont perdues.
        ui.compose.set(None);
        ui.compose_dirty.set(false);
        ui.status.set(Some("Rédaction fermée.".into()));
    };
    let signature_entries = signature_entries(ui, signatures, frame);

    // Image dans le corps : sélecteur de fichiers du système (champ caché), à la position du curseur.
    let image_input = NodeRef::<html::Input>::new();
    let on_image = move |_| {
        let Some(input) = image_input.get_untracked() else { return };
        spawn_local(async move {
            let picked = picked_image(&input).await;
            input.set_value("");
            match picked {
                Some(Ok(uri)) => {
                    exec(frame, "insertHTML", Some(&format!("<img src=\"{uri}\">")));
                    ui.compose_dirty.set(true);
                }
                Some(Err(e)) => ui.error.set(Some(e)),
                None => {}
            }
        });
    };
    let font_entries: Vec<MenuEntry> = FONTS
        .iter()
        .map(|&f| {
            MenuEntry::action(f, move || {
                exec(frame, "fontName", Some(f));
                ui.compose_dirty.set(true);
            })
        })
        .collect();
    let size_entries: Vec<MenuEntry> = SIZES
        .iter()
        .map(|&(v, pt)| {
            MenuEntry::action(format!("{pt} pt"), move || {
                exec(frame, "fontSize", Some(v));
                ui.compose_dirty.set(true);
            })
        })
        .collect();
    let color_menu = color_entries(ui, frame, "foreColor", ("Automatique", "#000000"), COLORS);
    let highlight_menu = color_entries(ui, frame, "hiliteColor", ("Aucune couleur", "transparent"), HIGHLIGHTS);
    let align = move |command: &'static str| move |_| exec(frame, command, None);

    // Importance (bascule comme le ruban d'Outlook) et confirmation de lecture.
    let importance = move || ui.compose.with(|d| d.as_ref().map(|d| d.importance).unwrap_or_default());
    let toggle_importance = move |level: Importance| {
        move |_| {
            ui.compose.update(|d| {
                if let Some(d) = d {
                    d.importance = if d.importance == level { Importance::Normal } else { level };
                }
            });
            ui.compose_dirty.set(true);
        }
    };
    let receipt = move || ui.compose.with(|d| d.as_ref().is_some_and(|d| d.read_receipt));
    let toggle_receipt = move |_| {
        ui.compose.update(|d| {
            if let Some(d) = d {
                d.read_receipt = !d.read_receipt;
            }
        });
        ui.compose_dirty.set(true);
    };

    let title = match kind {
        ComposeKind::New => "Nouveau message",
        ComposeKind::Reply => "Répondre",
        ComposeKind::ReplyAll => "Répondre à tous",
        ComposeKind::Forward => "Transférer",
    };

    view! {
        <section
            class="pane reading compose"
            aria-label=format!("Rédaction — {title}")
            node_ref=ui.refs.reading
            on:keydown=move |ev| {
                if ev.ctrl_key() && ev.key().eq_ignore_ascii_case("s") {
                    ev.prevent_default();
                    ev.stop_propagation();
                    save(ui, frame, || {});
                }
                if ev.ctrl_key() && ev.key() == "Enter" {
                    ev.prevent_default();
                    ev.stop_propagation();
                    send(ui, frame);
                }
            }
        >
            <div class="compose-bar" role="toolbar" aria-label="Actions de rédaction">
                <button class="cmd primary" title="Envoyer (Ctrl+Entrée)" aria-keyshortcuts="Control+Enter" on:click=move |_| send(ui, frame)>
                    <Svg icon=SEND />
                    <span>"Envoyer"</span>
                </button>
                <button class="cmd" title="Enregistrer le brouillon (Ctrl+S)" aria-keyshortcuts="Control+S" on:click=move |_| save(ui, frame, || {})>
                    <Svg icon=SAVE />
                    <span>"Enregistrer"</span>
                </button>
                <button class="cmd" title="Joindre un fichier" on:click=attach>
                    <Svg icon=PAPERCLIP />
                    <span>"Joindre"</span>
                </button>
                <MenuButton label="Signature" icon=PENCIL entries=signature_entries class="cmd" />
                <button
                    class="cmd icon-only"
                    title="Importance haute"
                    aria-label="Importance haute"
                    aria-pressed=move || (importance() == Importance::High).to_string()
                    on:click=toggle_importance(Importance::High)
                >
                    <Svg icon=IMPORTANCE_HIGH />
                </button>
                <button
                    class="cmd icon-only"
                    title="Importance faible"
                    aria-label="Importance faible"
                    aria-pressed=move || (importance() == Importance::Low).to_string()
                    on:click=toggle_importance(Importance::Low)
                >
                    <Svg icon=IMPORTANCE_LOW />
                </button>
                <button
                    class="cmd icon-only"
                    title="Demander une confirmation de lecture"
                    aria-label="Demander une confirmation de lecture"
                    aria-pressed=move || receipt().to_string()
                    on:click=toggle_receipt
                >
                    <Svg icon=MAIL_CHECK />
                </button>
                <span class="compose-sep" aria-hidden="true"></span>
                <MenuButton label="Police" icon=FONT entries=font_entries show_label=false title="Police" class="cmd icon-only" />
                <MenuButton label="Taille" icon=FONT entries=size_entries title="Taille de police" class="cmd" />
                <MenuButton label="Couleur de police" icon=FONT_COLOR entries=color_menu show_label=false title="Couleur de police" class="cmd icon-only" />
                <MenuButton label="Surligner" icon=HIGHLIGHT entries=highlight_menu show_label=false title="Couleur de surbrillance du texte" class="cmd icon-only" />
                <span class="compose-sep" aria-hidden="true"></span>
                <button class="cmd icon-only" title="Gras (Ctrl+G)" aria-label="Gras" on:click=move |_| exec(frame, "bold", None)>
                    <Svg icon=BOLD />
                </button>
                <button class="cmd icon-only" title="Italique (Ctrl+I)" aria-label="Italique" on:click=move |_| exec(frame, "italic", None)>
                    <Svg icon=ITALIC />
                </button>
                <button class="cmd icon-only" title="Souligné (Ctrl+U)" aria-label="Souligné" on:click=move |_| exec(frame, "underline", None)>
                    <Svg icon=UNDERLINE />
                </button>
                <button class="cmd icon-only" title="Liste à puces" aria-label="Liste à puces" on:click=move |_| exec(frame, "insertUnorderedList", None)>
                    <Svg icon=LIST />
                </button>
                <button class="cmd icon-only" title="Liste numérotée" aria-label="Liste numérotée" on:click=move |_| exec(frame, "insertOrderedList", None)>
                    <Svg icon=LIST_ORDERED />
                </button>
                <button class="cmd icon-only" title="Aligner à gauche" aria-label="Aligner à gauche" on:click=align("justifyLeft")>
                    <Svg icon=ALIGN_LEFT />
                </button>
                <button class="cmd icon-only" title="Centrer" aria-label="Centrer" on:click=align("justifyCenter")>
                    <Svg icon=ALIGN_CENTER />
                </button>
                <button class="cmd icon-only" title="Aligner à droite" aria-label="Aligner à droite" on:click=align("justifyRight")>
                    <Svg icon=ALIGN_RIGHT />
                </button>
                <span class="compose-sep" aria-hidden="true"></span>
                <button class="cmd icon-only" title="Lien (Ctrl+K)" aria-label="Lien" aria-keyshortcuts="Control+K" on:click=move |_| insert_link(ui, frame)>
                    <Svg icon=LINK />
                </button>
                <button
                    class="cmd icon-only"
                    title="Insérer une image dans le corps"
                    aria-label="Image"
                    on:click=move |_| {
                        if let Some(i) = image_input.get_untracked() {
                            i.click();
                        }
                    }
                >
                    <Svg icon=IMAGE_PLUS />
                </button>
                <input
                    type="file"
                    accept="image/png,image/jpeg,image/gif,image/bmp,image/webp"
                    hidden=true
                    tabindex="-1"
                    aria-hidden="true"
                    node_ref=image_input
                    on:change=on_image
                />
                <span class="tabs-spacer"></span>
                <button class="cmd" title="Fermer la rédaction" on:click=discard>
                    <Svg icon=X />
                    <span>"Ignorer"</span>
                </button>
            </div>
            <div class="compose-fields">
                <Show when=move || accounts.with(|a| !a.is_empty())>
                    <label class="compose-field">
                        <span>"De"</span>
                        <select aria-label="De" prop:value=from on:change=set_from>
                            <option value="" selected=move || from().is_empty()>
                                {move || accounts.with(|a| a.first().map(|f| format!("{} (par défaut)", f.address)).unwrap_or_default())}
                            </option>
                            {move || accounts.get().into_iter().map(|a| {
                                let label = if a.sender_name.is_empty() { a.address.clone() } else { format!("{} <{}>", a.sender_name, a.address) };
                                let id = a.id.clone();
                                view! { <option value=a.id.clone() selected=move || from() == id>{label}</option> }
                            }).collect_view()}
                        </select>
                    </label>
                </Show>
                <div class="compose-field">
                    <button class="link-btn" type="button" title="Ouvrir le Carnet d'adresses" on:click=move |_| book_open.set(true)>"À…"</button>
                    <AddressField id="compose-to" label="À" value=Signal::derive(to) on_set=set_addr(|d, v| d.to = v) />
                </div>
                <div class="compose-field">
                    <button class="link-btn" type="button" title="Ouvrir le Carnet d'adresses" on:click=move |_| book_open.set(true)>"Cc…"</button>
                    <AddressField label="Cc" value=Signal::derive(cc) on_set=set_addr(|d, v| d.cc = v) />
                    <button class="link-btn" type="button" title="Afficher la ligne Cci" hidden=move || show_bcc.get() on:click=move |_| show_bcc.set(true)>
                        "Cci"
                    </button>
                </div>
                <Show when=move || show_bcc.get()>
                    <div class="compose-field">
                        <button class="link-btn" type="button" title="Ouvrir le Carnet d'adresses" on:click=move |_| book_open.set(true)>"Cci…"</button>
                        <AddressField label="Cci" value=Signal::derive(bcc) on_set=set_addr(|d, v| d.bcc = v) />
                    </div>
                </Show>
                <label class="compose-field">
                    <span>"Objet"</span>
                    <input type="text" autocomplete="off" prop:value=subject on:input=set_subject />
                </label>
                <Show when=move || ui.compose.with(|d| d.as_ref().is_some_and(|d| !d.attachments.is_empty()))>
                    <div class="compose-attachments" role="list" aria-label="Pièces jointes">
                        {move || {
                            ui.compose
                                .with(|d| d.as_ref().map(|d| d.attachments.clone()).unwrap_or_default())
                                .into_iter()
                                .enumerate()
                                .map(|(i, a)| {
                                    view! {
                                        <span class="att-chip" role="listitem">
                                            <Svg icon=PAPERCLIP size=14 />
                                            <span>{format!("{} ({})", a.name, a.size_label)}</span>
                                            <button
                                                class="icon-btn"
                                                aria-label=format!("Retirer {}", a.name)
                                                on:click=move |_| {
                                                    ui.compose.update(|d| {
                                                        if let Some(d) = d {
                                                            if i < d.attachments.len() {
                                                                d.attachments.remove(i);
                                                            }
                                                        }
                                                    });
                                                    ui.compose_dirty.set(true);
                                                }
                                            >
                                                <Svg icon=X size=12 />
                                            </button>
                                        </span>
                                    }
                                })
                                .collect_view()
                        }}
                    </div>
                </Show>
            </div>
            <AddressBook open=book_open on_apply=apply_book />
            <iframe
                class="compose-body"
                title="Corps du message"
                sandbox="allow-same-origin"
                srcdoc=srcdoc
                node_ref=frame
                on:load=on_load
            ></iframe>
        </section>
    }
}

/// Entrées du menu « Signature » : insertion à la position du curseur.
fn signature_entries(ui: Ui, all: RwSignal<Vec<Signature>>, frame: NodeRef<html::Iframe>) -> Signal<Vec<MenuEntry>> {
    Signal::derive(move || {
        let list = all.get();
        if list.is_empty() {
            return vec![MenuEntry::pending("Aucune signature trouvée")];
        }
        list.into_iter()
            .map(|s| {
                let html = s.html.clone();
                MenuEntry::action(s.name.clone(), move || {
                    exec(frame, "insertHTML", Some(&html));
                    ui.compose_dirty.set(true);
                })
            })
            .collect()
    })
}
