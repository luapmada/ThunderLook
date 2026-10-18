//! Lecture du stockage pour l'interface (J2) : arborescence avec compteurs, vues triées et groupées
//! d'un dossier (lignes servies par tranches), message complet pour le volet de lecture.
//!
//! Rien n'est écrit ici, à part la migration de schéma appliquée à l'ouverture.

mod message;

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Local, TimeZone};
use thunderlook_model::{
    CategoryChip, FlagState, Folder, FolderRole, FolderSection, FolderTree, Importance, ListGroup, ListQuery,
    ListView, MessageSummary, SortField, format,
};
use thunderlook_store::rusqlite::{Connection, params_from_iter};

pub use message::{AttachmentData, BLOCKED_EXTENSIONS, is_blocked};

pub type Result<T> = std::result::Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Nombre de vues gardées en mémoire (une par dossier ouvert récemment).
const VIEW_CACHE: usize = 8;
/// Lignes servies au plus par appel à [`Mailbox::rows`].
pub const MAX_ROWS: u32 = 500;

pub struct Mailbox {
    data_dir: PathBuf,
    conn: Connection,
    /// (source, nom) → couleur CSS.
    categories: HashMap<(i64, String), Option<String>>,
    views: BTreeMap<u64, Vec<i64>>,
    next_token: u64,
}

/// Couleurs des 25 catégories prédéfinies ([MS-OXOCFG] : `preset0` à `preset24`). Teintes propres
/// au projet, choisies pour rester lisibles en thème clair et sombre.
const CATEGORY_COLORS: [&str; 25] = [
    "#d13438", "#e8730c", "#e3a21a", "#d6b70c", "#3f9b2f", "#1f9b8f", "#7f8c2a", "#2f6fc0", "#7a4cc2",
    "#b0306a", "#6f8299", "#4c5d73", "#8a8886", "#5c5b5a", "#2b2b2b", "#a4262c", "#b85b0a", "#a8771a",
    "#9a840a", "#2d6e22", "#14706a", "#5b641d", "#1f4e8c", "#55348a", "#7d2350",
];

pub fn category_color(index: i64) -> Option<String> {
    usize::try_from(index).ok().and_then(|i| CATEGORY_COLORS.get(i)).map(|c| (*c).to_string())
}

/// Classes de conteneur affichées dans le module Courrier.
fn is_mail_container(class: Option<&str>) -> bool {
    match class {
        None | Some("") => true,
        Some(c) => c == "IPF.Note" || c.starts_with("IPF.Note.") || c.starts_with("IPF.Imap"),
    }
}

/// Clé de tri insensible à la casse et aux accents courants.
pub fn fold(s: &str) -> String {
    s.trim()
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
            'ç' => 'c',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' | 'ì' => 'i',
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ÿ' | 'ý' => 'y',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// Objet sans préfixes de réponse et de transfert (« RE: », « TR : », « Fwd: »…), pour regrouper.
pub fn normalized_subject(subject: &str) -> &str {
    const PREFIXES: [&str; 9] = ["re", "tr", "fw", "fwd", "aw", "wg", "réf", "rif", "sv"];
    let mut s = subject.trim();
    loop {
        let Some(colon) = s.find(':') else { return s };
        let head = s[..colon].trim_end();
        // « RE[2]: » et « RE: » ; on ne retire que des préfixes connus.
        let word = head.split('[').next().unwrap_or(head);
        if colon <= 8 && PREFIXES.iter().any(|p| word.eq_ignore_ascii_case(p)) {
            s = s[colon + 1..].trim_start();
        } else {
            return s;
        }
    }
}

fn local(ms: Option<i64>) -> Option<DateTime<Local>> {
    ms.and_then(|ms| Local.timestamp_millis_opt(ms).single())
}

fn parse_id(id: &str, prefix: char) -> Result<i64> {
    id.strip_prefix(prefix).and_then(|n| n.parse().ok()).ok_or_else(|| format!("identifiant invalide : {id}"))
}

/// Dossier réel désigné par un identifiant de l'arborescence (favoris compris).
fn folder_db_id(id: &str) -> Result<i64> {
    parse_id(id.strip_prefix("fav-").unwrap_or(id), 'f')
}

pub fn message_db_id(id: &str) -> Result<i64> {
    parse_id(id, 'm')
}

fn role_of(role: &str) -> FolderRole {
    match role {
        "inbox" => FolderRole::Inbox,
        "drafts" => FolderRole::Drafts,
        "sent" => FolderRole::Sent,
        "deleted" => FolderRole::Deleted,
        "junk" => FolderRole::Junk,
        "outbox" => FolderRole::Outbox,
        "archive" => FolderRole::Archive,
        _ => FolderRole::Folder,
    }
}

/// Ordre du volet : réception, brouillons, envoyés, supprimés, puis le reste par ordre alphabétique.
fn rank(role: FolderRole) -> u8 {
    match role {
        FolderRole::Inbox => 0,
        FolderRole::Drafts => 1,
        FolderRole::Sent => 2,
        FolderRole::Deleted => 3,
        _ => 4,
    }
}

struct FolderRow {
    id: i64,
    source: i64,
    parent: Option<i64>,
    name: String,
    role: String,
    class: Option<String>,
    hidden: bool,
}

impl Mailbox {
    /// Ouvre le dossier de données (et applique les migrations de schéma).
    pub fn open(data_dir: &Path) -> Result<Self> {
        if !data_dir.join(thunderlook_store::DB_FILE).exists() {
            return Err(format!(
                "Aucune donnée dans {}. Importez une copie de PST : tl-import <copie.pst> \"{}\"",
                data_dir.display(),
                data_dir.display()
            ));
        }
        let conn = thunderlook_store::open(data_dir).map_err(err)?;
        let mut categories = HashMap::new();
        {
            let mut stmt = conn.prepare("SELECT source_id, name, color FROM category").map_err(err)?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))
                .map_err(err)?;
            for row in rows {
                let (source, name, color) = row.map_err(err)?;
                categories.insert((source, fold(&name)), category_color(color));
            }
        }
        Ok(Self { data_dir: data_dir.to_path_buf(), conn, categories, views: BTreeMap::new(), next_token: 1 })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Arborescence du module Courrier : favoris, puis une section par source importée.
    pub fn folder_tree(&self) -> Result<FolderTree> {
        let mut counts: HashMap<i64, (u32, u32)> = HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT folder_id, COUNT(*), SUM(is_read = 0) FROM message INDEXED BY message_folder_read GROUP BY folder_id")
                .map_err(err)?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, u32>(1)?, r.get::<_, u32>(2)?))).map_err(err)?;
            for row in rows {
                let (f, total, unread) = row.map_err(err)?;
                counts.insert(f, (total, unread));
            }
        }
        let mut sources: Vec<(i64, String, String)> = Vec::new();
        {
            let mut stmt = self.conn.prepare("SELECT id, display_name, origin_path FROM source ORDER BY id").map_err(err)?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(err)?;
            for row in rows {
                sources.push(row.map_err(err)?);
            }
        }
        let mut folders: Vec<FolderRow> = Vec::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT id, source_id, parent_id, name, role, container_class, hidden FROM folder ORDER BY position")
                .map_err(err)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(FolderRow {
                        id: r.get(0)?,
                        source: r.get(1)?,
                        parent: r.get(2)?,
                        name: r.get(3)?,
                        role: r.get(4)?,
                        class: r.get(5)?,
                        hidden: r.get::<_, i64>(6)? != 0,
                    })
                })
                .map_err(err)?;
            for row in rows {
                folders.push(row.map_err(err)?);
            }
        }
        let mut children: HashMap<i64, Vec<&FolderRow>> = HashMap::new();
        for f in &folders {
            if let Some(p) = f.parent {
                children.entry(p).or_default().push(f);
            }
        }

        fn build(f: &FolderRow, children: &HashMap<i64, Vec<&FolderRow>>, counts: &HashMap<i64, (u32, u32)>) -> Folder {
            let (total, unread) = counts.get(&f.id).copied().unwrap_or_default();
            let mut kids: Vec<Folder> = children
                .get(&f.id)
                .into_iter()
                .flatten()
                .filter(|c| !c.hidden && is_mail_container(c.class.as_deref()))
                .map(|c| build(c, children, counts))
                .collect();
            sort_folders(&mut kids);
            Folder { id: format!("f{}", f.id), name: f.name.clone(), role: role_of(&f.role), unread, total, children: kids }
        }
        fn sort_folders(v: &mut [Folder]) {
            v.sort_by(|a, b| rank(a.role).cmp(&rank(b.role)).then_with(|| fold(&a.name).cmp(&fold(&b.name))));
        }

        let mut sections = Vec::new();
        let mut favorites = Vec::new();
        for (source, display, origin) in &sources {
            let Some(ipm) = folders.iter().find(|f| f.source == *source && f.role == "ipm") else { continue };
            let root = build(ipm, &children, &counts);
            if favorites.is_empty() {
                for role in [FolderRole::Inbox, FolderRole::Sent] {
                    if let Some(f) = root.children.iter().find(|f| f.role == role) {
                        favorites.push(Folder { id: format!("fav-{}", f.id), children: vec![], ..f.clone() });
                    }
                }
            }
            // Deux sources au même nom affiché : on précise le fichier d'origine.
            let same_name = sources.iter().filter(|(_, d, _)| d == display).count() > 1;
            let name = if same_name {
                let file = Path::new(origin).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                format!("{display} ({file})")
            } else {
                display.clone()
            };
            sections.push(FolderSection { id: format!("s{source}"), name, folders: root.children });
        }
        if !favorites.is_empty() {
            sections.insert(0, FolderSection { id: "favorites".into(), name: "Favoris".into(), folders: favorites });
        }
        Ok(FolderTree { sections, data_dir: self.data_dir.display().to_string(), error: None })
    }

    /// Ouvre une vue triée et groupée d'un dossier ; les lignes se lisent ensuite avec [`Self::rows`].
    pub fn open_list(&mut self, q: &ListQuery) -> Result<ListView> {
        self.open_list_at(q, Local::now())
    }

    pub fn open_list_at(&mut self, q: &ListQuery, now: DateTime<Local>) -> Result<ListView> {
        let folder = folder_db_id(&q.folder_id)?;
        let (folder_total, folder_unread): (u32, u32) = self
            .conn
            .query_row(
                "SELECT COUNT(*), IFNULL(SUM(is_read = 0), 0) FROM message INDEXED BY message_folder_read WHERE folder_id = ?1",
                [folder],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(err)?;
        let filter = if q.unread_only { " AND is_read = 0" } else { "" };

        // (id, date, clé de tri secondaire, libellé de groupe)
        let mut items: Vec<(i64, i64, String, String)> = Vec::new();
        match q.sort {
            SortField::Date => {
                let sql = format!(
                    "SELECT id, IFNULL(received_at, 0) FROM message INDEXED BY message_folder_read WHERE folder_id = ?1{filter}"
                );
                let mut stmt = self.conn.prepare_cached(&sql).map_err(err)?;
                let rows = stmt.query_map([folder], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).map_err(err)?;
                for row in rows {
                    let (id, date) = row.map_err(err)?;
                    items.push((id, date, String::new(), String::new()));
                }
            }
            SortField::From | SortField::Subject | SortField::Size => {
                let sql = format!(
                    "SELECT id, IFNULL(received_at, 0), from_name, from_addr, subject, size FROM message WHERE folder_id = ?1{filter}"
                );
                let mut stmt = self.conn.prepare_cached(&sql).map_err(err)?;
                let rows = stmt
                    .query_map([folder], |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, i64>(5)?,
                        ))
                    })
                    .map_err(err)?;
                for row in rows {
                    let (id, date, from_name, from_addr, subject, size) = row.map_err(err)?;
                    let (key, label) = match q.sort {
                        SortField::From => {
                            let label = if from_name.trim().is_empty() { from_addr } else { from_name };
                            let label = if label.trim().is_empty() { "(expéditeur inconnu)".to_string() } else { label };
                            (fold(&label), label)
                        }
                        SortField::Subject => {
                            let label = normalized_subject(&subject).to_string();
                            let label = if label.is_empty() { "(aucun objet)".to_string() } else { label };
                            (fold(&label), label)
                        }
                        _ => {
                            // Clé numérique de largeur fixe : l'ordre des chaînes suit celui des tailles.
                            let size = size.max(0) as u64;
                            (format!("{size:020}"), format::size_group(size).to_string())
                        }
                    };
                    items.push((id, date, key, label));
                }
            }
        }

        // Tri principal dans le sens demandé ; à l'intérieur d'un groupe, toujours du plus récent au plus ancien.
        match q.sort {
            SortField::Date => items.sort_by(|a, b| {
                let o = a.1.cmp(&b.1).then(a.0.cmp(&b.0));
                if q.descending { o.reverse() } else { o }
            }),
            SortField::Size => items.sort_by(|a, b| {
                let o = a.2.cmp(&b.2);
                (if q.descending { o.reverse() } else { o }).then(b.1.cmp(&a.1)).then(b.0.cmp(&a.0))
            }),
            _ => items.sort_by(|a, b| {
                let o = a.2.cmp(&b.2);
                (if q.descending { o.reverse() } else { o }).then(b.1.cmp(&a.1)).then(b.0.cmp(&a.0))
            }),
        }

        let mut groups: Vec<ListGroup> = Vec::new();
        for (i, (_, date, _, label)) in items.iter().enumerate() {
            let label = match q.sort {
                SortField::Date => match local(Some(*date).filter(|d| *d > 0)) {
                    Some(d) => format::date_group(&d, &now),
                    None => "Sans date".to_string(),
                },
                _ => label.clone(),
            };
            match groups.last_mut() {
                Some(g) if g.label == label => g.count += 1,
                _ => groups.push(ListGroup { label, start: i as u32, count: 1 }),
            }
        }

        let token = self.next_token;
        self.next_token += 1;
        self.views.insert(token, items.into_iter().map(|(id, ..)| id).collect());
        while self.views.len() > VIEW_CACHE {
            self.views.pop_first();
        }
        let count = self.views[&token].len() as u32;
        Ok(ListView { token, count, folder_total, folder_unread, groups })
    }

    /// Lignes `start..start+count` d'une vue ouverte.
    pub fn rows(&self, token: u64, start: u32, count: u32) -> Result<Vec<MessageSummary>> {
        self.rows_at(token, start, count, Local::now())
    }

    pub fn rows_at(&self, token: u64, start: u32, count: u32, now: DateTime<Local>) -> Result<Vec<MessageSummary>> {
        let ids = self.views.get(&token).ok_or("vue expirée : rouvrir le dossier")?;
        let start = (start as usize).min(ids.len());
        let end = (start + count.min(MAX_ROWS) as usize).min(ids.len());
        let slice = &ids[start..end];
        if slice.is_empty() {
            return Ok(Vec::new());
        }
        let marks = vec!["?"; slice.len()].join(",");
        let mut cats: HashMap<i64, Vec<String>> = HashMap::new();
        {
            let sql = format!("SELECT message_id, name FROM message_category WHERE message_id IN ({marks})");
            let mut stmt = self.conn.prepare(&sql).map_err(err)?;
            let rows = stmt.query_map(params_from_iter(slice), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).map_err(err)?;
            for row in rows {
                let (m, name) = row.map_err(err)?;
                cats.entry(m).or_default().push(name);
            }
        }
        let sql = format!(
            "SELECT m.id, f.source_id, m.from_name, m.from_addr, m.subject, m.preview, m.received_at, m.size, m.is_read,
                    m.flag_status, m.importance,
                    EXISTS (SELECT 1 FROM attachment a WHERE a.message_id = m.id AND a.inline = 0 AND a.hidden = 0)
             FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id IN ({marks})"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(err)?;
        let mut by_id: HashMap<i64, MessageSummary> = HashMap::with_capacity(slice.len());
        let rows = stmt
            .query_map(params_from_iter(slice), |r| {
                let id: i64 = r.get(0)?;
                let source: i64 = r.get(1)?;
                let from_name: String = r.get(2)?;
                let from_addr: String = r.get(3)?;
                let received = local(r.get(6)?);
                let size: i64 = r.get(7)?;
                Ok((
                    id,
                    source,
                    MessageSummary {
                        id: format!("m{id}"),
                        pos: 0,
                        from: if from_name.trim().is_empty() { from_addr } else { from_name },
                        subject: r.get(4)?,
                        preview: thunderlook_mime::clean_preview(&r.get::<_, String>(5)?),
                        time_label: received.map(|d| format::list_time(&d, &now)).unwrap_or_default(),
                        size_label: format::size(size.max(0) as u64),
                        unread: r.get::<_, i64>(8)? == 0,
                        flag: flag_state(r.get(9)?),
                        importance: importance(r.get(10)?),
                        has_attachments: r.get::<_, i64>(11)? != 0,
                        categories: Vec::new(),
                    },
                ))
            })
            .map_err(err)?;
        for row in rows {
            let (id, source, mut m) = row.map_err(err)?;
            m.categories = self.chips(source, cats.remove(&id).unwrap_or_default());
            by_id.insert(id, m);
        }
        Ok(slice
            .iter()
            .enumerate()
            .filter_map(|(i, id)| {
                by_id.remove(id).map(|mut m| {
                    m.pos = (start + i) as u32;
                    m
                })
            })
            .collect())
    }

    fn chips(&self, source: i64, names: Vec<String>) -> Vec<CategoryChip> {
        names
            .into_iter()
            .map(|name| {
                let color = self.categories.get(&(source, fold(&name))).cloned().flatten();
                CategoryChip { name, color }
            })
            .collect()
    }

    /// Identifiants des lignes d'une vue (pour la navigation sans charger les lignes).
    pub fn view_ids(&self, token: u64) -> Option<&[i64]> {
        self.views.get(&token).map(Vec::as_slice)
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}

pub(crate) fn flag_state(v: i64) -> FlagState {
    match v {
        2 => FlagState::Flagged,
        1 => FlagState::Done,
        _ => FlagState::None,
    }
}

pub(crate) fn importance(v: i64) -> Importance {
    match v {
        0 => Importance::Low,
        2 => Importance::High,
        _ => Importance::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subjects() {
        assert_eq!(normalized_subject("RE: TR : Fwd: Devis"), "Devis");
        assert_eq!(normalized_subject("Re[2]: point"), "point");
        assert_eq!(normalized_subject("Réunion : ordre du jour"), "Réunion : ordre du jour");
        assert_eq!(normalized_subject("Facture: 2026-104"), "Facture: 2026-104");
        assert_eq!(fold("  Éléonore ÇA "), "eleonore ca");
    }

    #[test]
    fn colors() {
        assert_eq!(category_color(0).as_deref(), Some("#d13438"));
        assert_eq!(category_color(-1), None);
        assert_eq!(category_color(25), None);
    }
}
