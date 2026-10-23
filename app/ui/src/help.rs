//! Aide intégrée : `docs/utilisation.md` converti à l'affichage par un convertisseur Markdown minimal (titres, listes,
//! tableaux simples, gras, code), avec table des matières, recherche dans la page et rubrique « À propos ».

use std::sync::OnceLock;

use leptos::{html, prelude::*, task::spawn_local};

use crate::{
    ipc,
    state::{Ui, use_ui},
};

const SOURCE: &str = include_str!("../../../docs/utilisation.md");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const ABOUT_ID: &str = "a-propos";

struct Span {
    text: String,
    bold: bool,
    code: bool,
}

enum Block {
    Heading(Vec<Span>),
    Para(Vec<Span>),
    Bullets(Vec<Vec<Span>>),
    Numbered(Vec<Vec<Span>>),
    Table { head: Vec<Vec<Span>>, rows: Vec<Vec<Vec<Span>>> },
}

struct Section {
    id: String,
    title: String,
    blocks: Vec<Block>,
    /// Texte brut replié (minuscules, sans accents) où porte la recherche.
    haystack: String,
}

/// Minuscules sans accents : « Échange » se trouve en tapant « echange ».
fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in fold(title).chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

fn flush(out: &mut Vec<Span>, buf: &mut String, bold: bool, code: bool) {
    if !buf.is_empty() {
        out.push(Span { text: std::mem::take(buf), bold, code });
    }
}

/// `**gras**` et `` `code` ``.
fn spans(s: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let (mut bold, mut code) = (false, false);
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '`' => {
                flush(&mut out, &mut buf, bold, code);
                code = !code;
            }
            '*' if !code && chars.peek() == Some(&'*') => {
                chars.next();
                flush(&mut out, &mut buf, bold, code);
                bold = !bold;
            }
            _ => buf.push(c),
        }
    }
    flush(&mut out, &mut buf, bold, code);
    out
}

fn plain(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

fn cells(line: &str) -> Vec<String> {
    line.trim().trim_start_matches('|').trim_end_matches('|').split('|').map(|c| c.trim().to_string()).collect()
}

fn is_separator(line: &str) -> bool {
    cells(line).iter().all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':' | ' ')))
}

fn numbered(line: &str) -> Option<&str> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    (digits > 0).then(|| line[digits..].strip_prefix(". ")).flatten()
}

/// Bloc en cours de lecture.
enum Pending {
    None,
    Para(String),
    Bullets(Vec<String>),
    Numbered(Vec<String>),
    Table(Vec<String>),
}

fn close(pending: &mut Pending, blocks: &mut Vec<Block>) {
    match std::mem::replace(pending, Pending::None) {
        Pending::None => {}
        Pending::Para(t) => blocks.push(Block::Para(spans(&t))),
        Pending::Bullets(items) => blocks.push(Block::Bullets(items.iter().map(|i| spans(i)).collect())),
        Pending::Numbered(items) => blocks.push(Block::Numbered(items.iter().map(|i| spans(i)).collect())),
        Pending::Table(lines) => {
            let mut body = lines.iter().filter(|l| !is_separator(l));
            let head = body.next().map(|l| cells(l).iter().map(|c| spans(c)).collect()).unwrap_or_default();
            let rows = body.map(|l| cells(l).iter().map(|c| spans(c)).collect()).collect();
            blocks.push(Block::Table { head, rows });
        }
    }
}

fn parse(src: &str) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    let mut title = "Introduction".to_string();
    let mut blocks: Vec<Block> = Vec::new();
    let mut pending = Pending::None;
    let mut push_section = |title: &str, blocks: Vec<Block>| {
        let mut text = title.to_string();
        for b in &blocks {
            text.push(' ');
            text.push_str(&block_text(b));
        }
        sections.push(Section { id: slug(title), title: title.to_string(), blocks, haystack: fold(&text) });
    };
    for line in src.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            close(&mut pending, &mut blocks);
        } else if trimmed.starts_with("# ") {
            // Titre du document : l'introduction porte son nom dans la table des matières, pas dans le corps.
            close(&mut pending, &mut blocks);
        } else if let Some(h) = trimmed.strip_prefix("## ") {
            close(&mut pending, &mut blocks);
            push_section(&title, std::mem::take(&mut blocks));
            title = h.trim().to_string();
        } else if let Some(h) = trimmed.strip_prefix("### ") {
            close(&mut pending, &mut blocks);
            blocks.push(Block::Heading(spans(h.trim())));
        } else if trimmed.trim_start().starts_with('|') {
            match &mut pending {
                Pending::Table(lines) => lines.push(trimmed.to_string()),
                _ => {
                    close(&mut pending, &mut blocks);
                    pending = Pending::Table(vec![trimmed.to_string()]);
                }
            }
        } else if let Some(item) = trimmed.strip_prefix("- ") {
            match &mut pending {
                Pending::Bullets(items) => items.push(item.to_string()),
                _ => {
                    close(&mut pending, &mut blocks);
                    pending = Pending::Bullets(vec![item.to_string()]);
                }
            }
        } else if let Some(item) = numbered(trimmed) {
            match &mut pending {
                Pending::Numbered(items) => items.push(item.to_string()),
                _ => {
                    close(&mut pending, &mut blocks);
                    pending = Pending::Numbered(vec![item.to_string()]);
                }
            }
        } else {
            // Ligne de continuation d'un élément de liste ou d'un paragraphe.
            let text = trimmed.trim();
            match &mut pending {
                Pending::Bullets(items) | Pending::Numbered(items) => {
                    if let Some(last) = items.last_mut() {
                        last.push(' ');
                        last.push_str(text);
                    }
                }
                Pending::Para(p) => {
                    p.push(' ');
                    p.push_str(text);
                }
                _ => {
                    close(&mut pending, &mut blocks);
                    pending = Pending::Para(text.to_string());
                }
            }
        }
    }
    close(&mut pending, &mut blocks);
    push_section(&title, blocks);
    sections.retain(|s| !s.blocks.is_empty());
    sections
}

fn block_text(b: &Block) -> String {
    let join = |rows: &[Vec<Span>]| rows.iter().map(|r| plain(r)).collect::<Vec<_>>().join(" ");
    match b {
        Block::Heading(s) | Block::Para(s) => plain(s),
        Block::Bullets(items) | Block::Numbered(items) => join(items),
        Block::Table { head, rows } => {
            let mut t = join(head);
            for r in rows {
                t.push(' ');
                t.push_str(&join(r));
            }
            t
        }
    }
}

fn sections() -> &'static [Section] {
    static SECTIONS: OnceLock<Vec<Section>> = OnceLock::new();
    SECTIONS.get_or_init(|| parse(SOURCE))
}

fn spans_view(spans: &[Span]) -> impl IntoView + use<> {
    spans
        .iter()
        .map(|s| {
            let text = s.text.clone();
            match (s.bold, s.code) {
                (_, true) => view! { <code>{text}</code> }.into_any(),
                (true, false) => view! { <strong>{text}</strong> }.into_any(),
                _ => text.into_any(),
            }
        })
        .collect_view()
}

const CELL: &str = "padding: 3px 12px 3px 0; text-align: left; vertical-align: top; border-bottom: 1px solid var(--border);";

fn block_view(b: &Block) -> AnyView {
    match b {
        Block::Heading(s) => view! { <h4 style="margin: 12px 0 4px">{spans_view(s)}</h4> }.into_any(),
        Block::Para(s) => view! { <p style="margin: 6px 0">{spans_view(s)}</p> }.into_any(),
        Block::Bullets(items) => view! {
            <ul style="margin: 6px 0; padding-left: 20px">{items.iter().map(|i| view! { <li style="margin: 2px 0">{spans_view(i)}</li> }).collect_view()}</ul>
        }
        .into_any(),
        Block::Numbered(items) => view! {
            <ol style="margin: 6px 0; padding-left: 20px">{items.iter().map(|i| view! { <li style="margin: 2px 0">{spans_view(i)}</li> }).collect_view()}</ol>
        }
        .into_any(),
        Block::Table { head, rows } => view! {
            <table style="border-collapse: collapse; margin: 6px 0; width: 100%">
                <thead>
                    <tr>{head.iter().map(|c| view! { <th scope="col" style=CELL>{spans_view(c)}</th> }).collect_view()}</tr>
                </thead>
                <tbody>
                    {rows
                        .iter()
                        .map(|r| view! { <tr>{r.iter().map(|c| view! { <td style=CELL>{spans_view(c)}</td> }).collect_view()}</tr> })
                        .collect_view()}
                </tbody>
            </table>
        }
        .into_any(),
    }
}

/// Éléments par module, pour « À propos » (commandes existantes ; le calendrier et la taille de la base n'ont pas de commande).
#[derive(Clone, Default)]
struct Counts {
    contacts: Option<u32>,
    tasks: Option<usize>,
    notes: Option<usize>,
}

async fn load_counts() -> Counts {
    Counts {
        contacts: ipc::contact_folders().await.ok().map(|f| f.iter().map(|f| f.count).sum()),
        tasks: ipc::tasks_list().await.ok().map(|l| l.rows.len()),
        notes: ipc::notes_list().await.ok().map(|l| l.rows.len()),
    }
}

/// Ouvre l'aide ; `anchor` : identifiant de rubrique à atteindre (vide : en haut, champ de recherche actif).
pub fn open(ui: Ui, anchor: &str) {
    ui.help.set(Some(anchor.to_string()));
}

fn scroll_to(id: &str) {
    if let Some(el) = document().get_element_by_id(&format!("help-{id}")) {
        el.scroll_into_view_with_bool(true);
    }
}

#[component]
pub fn HelpDialog() -> impl IntoView {
    let ui = use_ui();
    let query = RwSignal::new(String::new());
    let counts = RwSignal::new(Counts::default());
    let search = NodeRef::<html::Input>::new();

    // À l'ouverture : recherche vidée, compteurs relus, puis rubrique demandée ou champ de recherche.
    Effect::new(move |_| {
        let Some(anchor) = ui.help.get() else { return };
        query.set(String::new());
        spawn_local(async move { counts.set(load_counts().await) });
        request_animation_frame(move || {
            if anchor.is_empty() {
                if let Some(s) = search.get_untracked() {
                    let _ = s.focus();
                }
            } else {
                scroll_to(&anchor);
            }
        });
    });

    let close = move || ui.help.set(None);
    let about_text = fold("a propos version dossier de donnees contacts taches notes");
    // Rubriques retenues par la recherche : tous les mots saisis doivent s'y trouver.
    let visible = Memo::new(move |_| {
        let terms: Vec<String> = fold(&query.get()).split_whitespace().map(str::to_string).collect();
        let hit = |hay: &str| terms.iter().all(|t| hay.contains(t.as_str()));
        let found: Vec<usize> = sections().iter().enumerate().filter(|(_, s)| hit(&s.haystack)).map(|(i, _)| i).collect();
        (found, hit(&about_text))
    });

    let about = move || {
        let c = counts.get();
        let n = |v: Option<String>| v.unwrap_or_else(|| "non disponible".to_string());
        let data_dir = ui.tree.with(|t| t.as_ref().map(|t| t.data_dir.clone()).filter(|d| !d.is_empty()));
        view! {
            <section id=format!("help-{ABOUT_ID}" ) aria-labelledby="help-about-title" style="margin-bottom: 20px">
                <h3 id="help-about-title" style="margin: 0 0 6px">"À propos"</h3>
                <ul style="margin: 6px 0; padding-left: 20px">
                    <li>"Version : "<strong>{VERSION}</strong></li>
                    <li>"Dossier de données : "<code>{data_dir.unwrap_or_else(|| "non disponible".to_string())}</code></li>
                    <li>"Contacts : "{n(c.contacts.map(|v| v.to_string()))}</li>
                    <li>"Tâches : "{n(c.tasks.map(|v| v.to_string()))}</li>
                    <li>"Notes : "{n(c.notes.map(|v| v.to_string()))}</li>
                </ul>
                <p class="dialog-note">"Le nombre de rendez-vous et la taille de la base ne sont pas affichés : aucune commande ne les fournit."</p>
            </section>
        }
    };

    view! {
        <Show when=move || ui.help.with(Option::is_some)>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div
                    class="dialog wide"
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="help-title"
                    style="width: min(960px, calc(100vw - 32px)); height: calc(100vh - 64px)"
                    on:keydown=move |ev| {
                        if ev.key() == "Escape" {
                            ev.prevent_default();
                            close();
                        }
                    }
                >
                    <h2 id="help-title">"Aide"</h2>
                    <input
                        type="search"
                        node_ref=search
                        aria-label="Rechercher dans l'aide"
                        placeholder="Rechercher dans l'aide"
                        prop:value=move || query.get()
                        on:input=move |ev| query.set(event_target_value(&ev))
                    />
                    <div style="display: flex; gap: 16px; flex: 1; min-height: 0">
                        <nav aria-label="Table des matières" style="flex: 0 0 190px; overflow: auto">
                            <ul style="list-style: none; margin: 0; padding: 0">
                                {move || {
                                    let (found, about_hit) = visible.get();
                                    let mut items = found
                                        .into_iter()
                                        .map(|i| {
                                            let s = &sections()[i];
                                            let id = s.id.clone();
                                            view! {
                                                <li>
                                                    <button class="btn" style="width: 100%; text-align: left; border: 0" on:click=move |_| scroll_to(&id)>
                                                        {s.title.clone()}
                                                    </button>
                                                </li>
                                            }
                                            .into_any()
                                        })
                                        .collect::<Vec<_>>();
                                    if about_hit {
                                        items.push(
                                            view! {
                                                <li>
                                                    <button class="btn" style="width: 100%; text-align: left; border: 0" on:click=move |_| scroll_to(ABOUT_ID)>"À propos"</button>
                                                </li>
                                            }
                                            .into_any(),
                                        );
                                    }
                                    items
                                }}
                            </ul>
                        </nav>
                        <div style="flex: 1; overflow: auto; padding-right: 8px" tabindex="0" aria-label="Contenu de l'aide">
                            {move || {
                                let (found, about_hit) = visible.get();
                                let none = found.is_empty() && !about_hit;
                                view! {
                                    <p class="dialog-note" aria-live="polite">
                                        {if query.with(|q| q.trim().is_empty()) {
                                            String::new()
                                        } else {
                                            format!("{} rubrique(s) trouvée(s).", found.len() + usize::from(about_hit))
                                        }}
                                    </p>
                                    {none.then(|| view! { <p>"Aucun résultat."</p> })}
                                    {found
                                        .into_iter()
                                        .map(|i| {
                                            let s = &sections()[i];
                                            view! {
                                                <section id=format!("help-{}", s.id) style="margin-bottom: 20px">
                                                    <h3 style="margin: 0 0 6px">{s.title.clone()}</h3>
                                                    {s.blocks.iter().map(block_view).collect_view()}
                                                </section>
                                            }
                                        })
                                        .collect_view()}
                                    {about_hit.then(about)}
                                }
                            }}
                        </div>
                    </div>
                    <div class="dialog-buttons">
                        <button class="btn" on:click=move |_| close()>"Fermer"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guide_is_parsed_into_sections() {
        let titles: Vec<&str> = sections().iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles.first(), Some(&"Introduction"));
        for t in ["Courrier", "Calendrier", "Contacts", "Tâches", "Notes", "Raccourcis clavier", "Échange avec le téléphone", "Sauvegardes et annulation", "Où sont les données"] {
            assert!(titles.contains(&t), "rubrique absente : {t}");
        }
        let shortcuts = sections().iter().find(|s| s.id == "raccourcis-clavier").unwrap();
        assert!(matches!(&shortcuts.blocks[0], Block::Table { head, rows } if head.len() == 2 && rows.len() > 10));
        assert!(shortcuts.haystack.contains("ctrl+maj+t"));
    }

    #[test]
    fn inline_and_search_folding() {
        let s = spans("un **gras** et `code`");
        assert_eq!(s.len(), 4);
        assert!(s[1].bold && s[3].code);
        assert!(fold("Échange avec le Téléphone").contains("echange avec le telephone"));
        assert_eq!(slug("À propos"), "a-propos");
    }
}
