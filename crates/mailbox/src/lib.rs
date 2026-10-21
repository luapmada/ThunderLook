//! Accès au stockage pour l'interface : arborescence avec compteurs, vues triées et groupées d'un dossier
//! ou d'une recherche (lignes servies par tranches), message complet pour le volet de lecture (J2),
//! opérations d'organisation (J3), recherche et indexation plein texte (J4), conversations (J3b), règles (J6).

mod compose;
mod conv;
mod index;
mod message;
mod oft;
mod ops;
mod remote;
mod rule_run;
mod rules;
mod search;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Local, TimeZone};
use thunderlook_model::{
    CategoryChip, ConversationInfo, FlagState, Folder, FolderRole, FolderSection, FolderTree, Importance, ListGroup,
    ListQuery, ListView, MessageSummary, SearchQuery, SearchScope, SortField, format,
};
use thunderlook_store::rusqlite::{Connection, params_from_iter, types::Value};

pub use compose::{MailAddress, default_signature, format_address, parse_addresses, signatures};
pub use index::{body_text, index_pending};
pub use oft::draft_from_oft;
pub use remote::file_message;
pub use rules::{FolderMatch, FolderResolver};
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
    views: BTreeMap<u64, View>,
    next_token: u64,
    undo: Vec<ops::UndoEntry>,
    /// Conversations dépliées, gardées d'une ouverture de vue à l'autre (après une opération par exemple).
    expanded: HashSet<[u8; 16]>,
    /// Pendant `send_draft` : nom et adresse du compte d'envoi, qui priment sur ceux de la source du brouillon.
    send_as: Option<(String, String)>,
}

/// Vue ouverte : lignes dans l'ordre, et recherche ou non (les lignes indiquent alors leur dossier).
struct View {
    rows: Vec<Row>,
    search: bool,
    /// Vue en conversations : dossier affiché, et dossiers où chercher les autres éléments.
    conv: Option<ConvView>,
}

struct ConvView {
    folder: i64,
    /// Dossiers de la portée, en liste SQL ([`conv::scope_folders`]).
    scope: String,
    now: DateTime<Local>,
}

/// Ligne d'une vue, et date qui décide de son groupe.
struct Row {
    item: RowItem,
    date: i64,
}

enum RowItem {
    Message(i64),
    /// Conversation : éléments du dossier affiché, du plus récent au plus ancien.
    Conversation { key: [u8; 16], members: Vec<i64>, expanded: bool },
    /// Élément d'une conversation dépliée (éventuellement d'un autre dossier).
    Child(i64),
}

impl RowItem {
    /// Message affiché par la ligne (le plus récent du dossier pour une conversation).
    fn shown(&self) -> i64 {
        match self {
            Self::Message(id) | Self::Child(id) => *id,
            Self::Conversation { members, .. } => members[0],
        }
    }

    /// Messages visés quand la ligne est sélectionnée.
    fn targets(&self) -> &[i64] {
        match self {
            Self::Message(id) | Self::Child(id) => std::slice::from_ref(id),
            Self::Conversation { members, .. } => members,
        }
    }
}

/// (id, date, clé de tri, libellé de groupe) d'un élément à trier.
type Item = (i64, i64, String, String);

/// Dossiers de recherche prédéfinis : identifiant, nom, requête.
const BUILTIN_SEARCH_FOLDERS: [(&str, &str, &str); 2] = [("s-unread", "Courrier non lu", "lu:non"), ("s-flagged", "Pour le suivi", "suivi:oui")];

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
        conv::fill_conversation_ids(&conn)?;
        rules::import_pst_rules(&conn)?;
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            conn,
            categories,
            views: BTreeMap::new(),
            next_token: 1,
            undo: Vec::new(),
            expanded: HashSet::new(),
            send_as: None,
        })
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
        let favorite_ids = self.favorite_ids()?;
        let mut favorites: Vec<Folder> = Vec::new();
        let mut built: HashMap<String, Folder> = HashMap::new();
        for (source, display, origin) in &sources {
            let Some(ipm) = folders.iter().find(|f| f.source == *source && f.role == "ipm") else { continue };
            let root = build(ipm, &children, &counts);
            fn index(folders: &[Folder], out: &mut HashMap<String, Folder>) {
                for f in folders {
                    out.insert(f.id.clone(), f.clone());
                    index(&f.children, out);
                }
            }
            index(&root.children, &mut built);
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
        for id in favorite_ids {
            if let Some(f) = built.get(&format!("f{id}")) {
                favorites.push(Folder { id: format!("fav-{}", f.id), children: vec![], ..f.clone() });
            }
        }
        if !favorites.is_empty() {
            sections.insert(0, FolderSection { id: "favorites".into(), name: "Favoris".into(), folders: favorites });
        }
        if !sources.is_empty() {
            sections.push(FolderSection { id: "searches".into(), name: "Dossiers de recherche".into(), folders: self.search_folders()? });
        }
        Ok(FolderTree { sections, data_dir: self.data_dir.display().to_string(), error: None })
    }

    /// Ouvre une vue triée et groupée d'un dossier (ou d'un dossier de recherche, `s-…`) ;
    /// les lignes se lisent ensuite avec [`Self::rows`].
    pub fn open_list(&mut self, q: &ListQuery) -> Result<ListView> {
        self.open_list_at(q, Local::now())
    }

    pub fn open_list_at(&mut self, q: &ListQuery, now: DateTime<Local>) -> Result<ListView> {
        if q.folder_id.starts_with("s-") {
            let (text, scope_sql, scope_params) = self.search_folder_def(&q.folder_id)?;
            return self.search_view(&text, &scope_sql, scope_params, q.unread_only, q.sort, q.descending, now);
        }
        let folder = folder_db_id(&q.folder_id)?;
        let (folder_total, folder_unread): (u32, u32) = self
            .conn
            .query_row(
                "SELECT COUNT(*), IFNULL(SUM(is_read = 0), 0) FROM message INDEXED BY message_folder_read WHERE folder_id = ?1",
                [folder],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(err)?;
        let filter = if q.unread_only { " AND m.is_read = 0" } else { "" };
        if q.conversations && q.sort == SortField::Date {
            return self.conversation_view(folder, filter, q.descending, now, (folder_total, folder_unread));
        }
        // Tri par date : l'index couvrant suffit, sans lire les lignes.
        let from = if q.sort == SortField::Date { "message m INDEXED BY message_folder_read" } else { "message m" };
        let items = self.collect(&format!("{from} WHERE m.folder_id = ?{filter}"), &[Value::Integer(folder)], q.sort)?;
        Ok(self.finish_view(items, q.sort, q.descending, now, (folder_total, folder_unread), false))
    }

    /// Vue en conversations d'un dossier, triée par date du dernier élément du dossier.
    fn conversation_view(
        &mut self,
        folder: i64,
        filter: &str,
        descending: bool,
        now: DateTime<Local>,
        counts: (u32, u32),
    ) -> Result<ListView> {
        let mut convs: Vec<([u8; 16], i64, Vec<(i64, i64)>)> = Vec::new();
        let mut singles: Vec<(i64, i64)> = Vec::new();
        {
            let mut by_key: HashMap<[u8; 16], usize> = HashMap::new();
            let mut stmt = self
                .conn
                .prepare_cached(&format!(
                    "SELECT m.id, IFNULL(m.received_at, 0), m.conversation_id FROM message m INDEXED BY message_folder_conv
                     WHERE m.folder_id = ?1{filter}"
                ))
                .map_err(err)?;
            let mut rows = stmt.query([folder]).map_err(err)?;
            while let Some(r) = rows.next().map_err(err)? {
                let (id, date): (i64, i64) = (r.get(0).map_err(err)?, r.get(1).map_err(err)?);
                let key = r.get_ref(2).map_err(err)?.as_blob_or_null().ok().flatten().and_then(|b| <[u8; 16]>::try_from(b).ok());
                match key {
                    Some(key) => {
                        let i = *by_key.entry(key).or_insert_with(|| {
                            convs.push((key, i64::MIN, Vec::new()));
                            convs.len() - 1
                        });
                        let c = &mut convs[i];
                        c.1 = c.1.max(date);
                        c.2.push((date, id));
                    }
                    None => singles.push((date, id)),
                }
            }
        }
        // (date, message affiché, ligne) : la date et l'identifiant du dernier élément décident de l'ordre.
        let mut rows: Vec<(i64, i64, RowItem)> = Vec::with_capacity(convs.len() + singles.len());
        for (key, date, mut members) in convs {
            members.sort_unstable_by(|a, b| b.cmp(a));
            let head = members[0].1;
            let expanded = self.expanded.contains(&key);
            rows.push((date, head, RowItem::Conversation { key, members: members.into_iter().map(|(_, id)| id).collect(), expanded }));
        }
        rows.extend(singles.into_iter().map(|(date, id)| (date, id, RowItem::Message(id))));
        rows.sort_unstable_by(|a, b| {
            let o = (a.0, a.1).cmp(&(b.0, b.1));
            if descending { o.reverse() } else { o }
        });
        let scope = conv::scope_folders(&self.conn, folder)?;
        let mut out: Vec<Row> = Vec::with_capacity(rows.len());
        for (date, _, item) in rows {
            // Dépliée lors d'une vue précédente : ses éléments suivent l'en-tête.
            let reopen = match &item {
                RowItem::Conversation { key, expanded: true, .. } => Some(*key),
                _ => None,
            };
            out.push(Row { item, date });
            if let Some(key) = reopen {
                for id in conv::members(&self.conn, &key, &scope)? {
                    out.push(Row { item: RowItem::Child(id), date });
                }
            }
        }
        let conv = ConvView { folder, scope, now };
        Ok(self.store_view(out, SortField::Date, now, counts, false, Some(conv), &[]))
    }

    /// Déplie ou replie (`expand` absent : inverse) la conversation à la position `pos` : ses éléments, tous
    /// dossiers confondus, se placent sous l'en-tête. La vue garde son jeton ; les lignes à partir de `pos`
    /// sont à recharger.
    pub fn toggle_conversation(&mut self, token: u64, pos: u32, expand: Option<bool>) -> Result<ListView> {
        let view = self.views.get_mut(&token).ok_or("vue expirée : rouvrir le dossier")?;
        let conv = view.conv.as_ref().ok_or("cette vue n'est pas en conversations")?;
        let pos = pos as usize;
        let Some(Row { item: RowItem::Conversation { key, expanded, .. }, date }) = view.rows.get_mut(pos) else {
            return Err("aucune conversation à cette position".into());
        };
        let (key, date) = (*key, *date);
        let want = expand.unwrap_or(!*expanded);
        if want != *expanded {
            *expanded = want;
            if want {
                let children: Vec<Row> =
                    conv::members(&self.conn, &key, &conv.scope)?.into_iter().map(|id| Row { item: RowItem::Child(id), date }).collect();
                view.rows.splice(pos + 1..pos + 1, children);
                self.expanded.insert(key);
            } else {
                let end = view.rows[pos + 1..]
                    .iter()
                    .position(|r| !matches!(r.item, RowItem::Child(_)))
                    .map_or(view.rows.len(), |n| pos + 1 + n);
                view.rows.drain(pos + 1..end);
                self.expanded.remove(&key);
            }
        }
        let view = &self.views[&token];
        let conv = view.conv.as_ref().expect("vue en conversations");
        let groups = groups(&view.rows, SortField::Date, conv.now, &[]);
        let (folder_total, folder_unread): (u32, u32) = self
            .conn
            .query_row(
                "SELECT COUNT(*), IFNULL(SUM(is_read = 0), 0) FROM message INDEXED BY message_folder_read WHERE folder_id = ?1",
                [conv.folder],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(err)?;
        Ok(ListView { token, count: view.rows.len() as u32, folder_total, folder_unread, groups })
    }

    /// Éléments à trier, avec les colonnes nécessaires au tri demandé.
    fn collect(&self, from_where: &str, params: &[Value], sort: SortField) -> Result<Vec<Item>> {
        let mut items: Vec<Item> = Vec::new();
        if sort == SortField::Date {
            let mut stmt = self.conn.prepare_cached(&format!("SELECT m.id, IFNULL(m.received_at, 0) FROM {from_where}")).map_err(err)?;
            let rows = stmt.query_map(params_from_iter(params), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).map_err(err)?;
            for row in rows {
                let (id, date) = row.map_err(err)?;
                items.push((id, date, String::new(), String::new()));
            }
            return Ok(items);
        }
        if sort == SortField::Categories {
            return self.collect_categories(from_where, params);
        }
        let mut stmt = self
            .conn
            .prepare_cached(&format!(
                "SELECT m.id, IFNULL(m.received_at, 0), m.from_name, m.from_addr, m.subject, m.size, m.flag_status,
                        EXISTS (SELECT 1 FROM attachment a WHERE a.message_id = m.id AND a.inline = 0 AND a.hidden = 0)
                 FROM {from_where}"
            ))
            .map_err(err)?;
        let rows = stmt
            .query_map(params_from_iter(params), |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                    r.get::<_, bool>(7)?,
                ))
            })
            .map_err(err)?;
        for row in rows {
            let (id, date, from_name, from_addr, subject, size, flag, attached) = row.map_err(err)?;
            let (key, label) = match sort {
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
                // Clés d'un caractère : l'ordre croissant donne l'ordre de la référence.
                SortField::Flag => match flag {
                    2 => ("0".into(), "Avec suivi".into()),
                    1 => ("1".into(), "Terminé".into()),
                    _ => ("2".into(), "Sans indicateur".into()),
                },
                SortField::Attachments if attached => ("0".into(), "Avec pièce jointe".into()),
                SortField::Attachments => ("1".into(), "Sans pièce jointe".into()),
                _ => {
                    // Clé numérique de largeur fixe : l'ordre des chaînes suit celui des tailles.
                    let size = size.max(0) as u64;
                    (format!("{size:020}"), format::size_group(size).to_string())
                }
            };
            items.push((id, date, key, label));
        }
        Ok(items)
    }

    /// Classement par catégories : un élément par catégorie qu'il porte, et « (aucune catégorie) » sinon,
    /// en dernier. Un message peut donc occuper plusieurs lignes ; une sélection n'en garde qu'un exemplaire.
    fn collect_categories(&self, from_where: &str, params: &[Value]) -> Result<Vec<Item>> {
        let mut stmt = self
            .conn
            .prepare_cached(&format!(
                "SELECT m.id, IFNULL(m.received_at, 0), c.name FROM {from_where_join}",
                from_where_join = from_where.replacen("message m", "message m LEFT JOIN message_category c ON c.message_id = m.id", 1)
            ))
            .map_err(err)?;
        let rows = stmt
            .query_map(params_from_iter(params), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<String>>(2)?)))
            .map_err(err)?;
        let mut items = Vec::new();
        // Un seul libellé par catégorie, quelle que soit la casse des attributions (le premier rencontré).
        let mut labels: HashMap<String, String> = HashMap::new();
        for row in rows {
            let (id, date, name) = row.map_err(err)?;
            items.push(match name.filter(|n| !n.trim().is_empty()) {
                Some(n) => {
                    let key = fold(&n);
                    let label = labels.entry(key.clone()).or_insert(n).clone();
                    (id, date, format!("0{key}"), label)
                }
                None => (id, date, "1".into(), "(aucune catégorie)".into()),
            });
        }
        Ok(items)
    }

    /// Tri, groupes et enregistrement de la vue.
    fn finish_view(
        &mut self,
        mut items: Vec<Item>,
        sort: SortField,
        descending: bool,
        now: DateTime<Local>,
        counts: (u32, u32),
        search: bool,
    ) -> ListView {
        // Tri principal dans le sens demandé ; à l'intérieur d'un groupe, toujours du plus récent au plus ancien.
        match sort {
            SortField::Date => items.sort_by(|a, b| {
                let o = a.1.cmp(&b.1).then(a.0.cmp(&b.0));
                if descending { o.reverse() } else { o }
            }),
            _ => items.sort_by(|a, b| {
                let o = a.2.cmp(&b.2);
                (if descending { o.reverse() } else { o }).then(b.1.cmp(&a.1)).then(b.0.cmp(&a.0))
            }),
        }
        let labels: Vec<String> = if sort == SortField::Date { Vec::new() } else { items.iter().map(|i| i.3.clone()).collect() };
        let rows = items.into_iter().map(|(id, date, ..)| Row { item: RowItem::Message(id), date }).collect();
        self.store_view(rows, sort, now, counts, search, None, &labels)
    }

    /// Groupes et enregistrement d'une vue dont les lignes sont dans l'ordre.
    #[allow(clippy::too_many_arguments)]
    fn store_view(
        &mut self,
        rows: Vec<Row>,
        sort: SortField,
        now: DateTime<Local>,
        (folder_total, folder_unread): (u32, u32),
        search: bool,
        conv: Option<ConvView>,
        labels: &[String],
    ) -> ListView {
        let groups = groups(&rows, sort, now, labels);
        let token = self.next_token;
        self.next_token += 1;
        let count = rows.len() as u32;
        self.views.insert(token, View { rows, search, conv });
        while self.views.len() > VIEW_CACHE {
            self.views.pop_first();
        }
        ListView { token, count, folder_total, folder_unread, groups }
    }

    /// Condition SQL (sur `m`) d'une étendue de recherche, à partir du dossier courant.
    fn scope_sql(&self, scope: SearchScope, folder_id: &str) -> Result<(String, Vec<Value>)> {
        // Dossiers de courrier visibles : ni éléments masqués (vues, rappels…), ni calendrier, contacts, tâches.
        const VISIBLE: &str = "m.folder_id IN (SELECT id FROM folder WHERE hidden = 0 AND (container_class IS NULL
            OR container_class = '' OR container_class LIKE 'IPF.Note%' OR container_class LIKE 'IPF.Imap%'))";
        let folder = if folder_id.starts_with("s-") || folder_id.is_empty() { None } else { Some(folder_db_id(folder_id)?) };
        Ok(match (scope, folder) {
            (SearchScope::Folder, Some(f)) => ("m.folder_id = ?".into(), vec![Value::Integer(f)]),
            (SearchScope::Subfolders, Some(f)) => (
                "m.folder_id IN (WITH RECURSIVE t(id) AS (SELECT ? UNION ALL SELECT x.id FROM folder x JOIN t ON x.parent_id = t.id) SELECT id FROM t)".into(),
                vec![Value::Integer(f)],
            ),
            (SearchScope::Source, Some(f)) => (
                format!("{VISIBLE} AND m.folder_id IN (SELECT id FROM folder WHERE source_id = (SELECT source_id FROM folder WHERE id = ?))"),
                vec![Value::Integer(f)],
            ),
            _ => (VISIBLE.into(), Vec::new()),
        })
    }

    /// Requête et étendue d'un dossier de recherche. Les prédéfinis excluent « Éléments supprimés »
    /// et « Courrier indésirable », comme la référence.
    fn search_folder_def(&self, id: &str) -> Result<(String, String, Vec<Value>)> {
        const ELIGIBLE: &str = "m.folder_id IN (WITH RECURSIVE excl(id) AS (
                SELECT id FROM folder WHERE role IN ('deleted', 'junk')
                UNION ALL SELECT x.id FROM folder x JOIN excl ON x.parent_id = excl.id)
            SELECT id FROM folder WHERE hidden = 0 AND id NOT IN (SELECT id FROM excl) AND (container_class IS NULL
                OR container_class = '' OR container_class LIKE 'IPF.Note%' OR container_class LIKE 'IPF.Imap%'))";
        if let Some((_, _, q)) = BUILTIN_SEARCH_FOLDERS.iter().find(|(i, ..)| *i == id) {
            return Ok((q.to_string(), ELIGIBLE.into(), Vec::new()));
        }
        let n: i64 = id.strip_prefix("s-").and_then(|n| n.parse().ok()).ok_or_else(|| format!("dossier de recherche inconnu : {id}"))?;
        let (query, scope): (String, String) = self
            .conn
            .query_row("SELECT query, scope FROM search_folder WHERE id = ?1", [n], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|_| format!("dossier de recherche introuvable : {id}"))?;
        match scope.strip_prefix("source:").and_then(|s| s.parse::<i64>().ok()) {
            Some(src) => Ok((query, format!("{ELIGIBLE} AND m.folder_id IN (SELECT id FROM folder WHERE source_id = ?)"), vec![Value::Integer(src)])),
            None => Ok((query, ELIGIBLE.into(), Vec::new())),
        }
    }

    /// Résultats d'une recherche, triés et groupés comme un dossier ; les lignes indiquent leur dossier.
    pub fn search(&mut self, q: &SearchQuery, sort: SortField, descending: bool) -> Result<ListView> {
        let (scope_sql, scope_params) = self.scope_sql(q.scope, &q.folder_id)?;
        self.search_view(&q.text, &scope_sql, scope_params, false, sort, descending, Local::now())
    }

    #[allow(clippy::too_many_arguments)]
    fn search_view(
        &mut self,
        text: &str,
        scope_sql: &str,
        scope_params: Vec<Value>,
        unread_only: bool,
        sort: SortField,
        descending: bool,
        now: DateTime<Local>,
    ) -> Result<ListView> {
        let parsed = search::parse(text, now.date_naive());
        if parsed.fts.is_empty() && parsed.preds.is_empty() {
            return Ok(self.finish_view(Vec::new(), sort, descending, now, (0, 0), true));
        }
        let mut sql = format!("message m WHERE {scope_sql}");
        let mut params = scope_params;
        if !parsed.fts.is_empty() {
            sql.push_str(" AND m.id IN (SELECT rowid FROM message_fts WHERE message_fts MATCH ?)");
            params.push(Value::Text(parsed.fts.clone()));
        }
        let (pred_sql, pred_params) = search::predicates_sql(&parsed.preds);
        sql.push_str(&pred_sql);
        params.extend(pred_params);
        if unread_only {
            sql.push_str(" AND m.is_read = 0");
        }
        let items = self.collect(&sql, &params, sort).map_err(|e| {
            if e.contains("fts5") { format!("Recherche invalide : {text}") } else { e }
        })?;
        let total = items.len() as u32;
        let unread = if total == 0 {
            0
        } else {
            let ids: Vec<i64> = items.iter().map(|i| i.0).collect();
            let mut n = 0u32;
            for chunk in ids.chunks(900) {
                let marks = vec!["?"; chunk.len()].join(",");
                let c: u32 = self
                    .conn
                    .query_row(&format!("SELECT COUNT(*) FROM message WHERE is_read = 0 AND id IN ({marks})"), params_from_iter(chunk), |r| r.get(0))
                    .map_err(err)?;
                n += c;
            }
            n
        };
        Ok(self.finish_view(items, sort, descending, now, (total, unread), true))
    }

    /// Dossiers de recherche du volet : prédéfinis (avec compteurs), puis enregistrés.
    fn search_folders(&self) -> Result<Vec<Folder>> {
        let mut out = Vec::new();
        for (id, name, _) in BUILTIN_SEARCH_FOLDERS {
            let (scope, _) = (self.search_folder_def(id)?.1, ());
            let cond = if id == "s-unread" { "m.is_read = 0" } else { "m.flag_status = 2" };
            let hint = if id == "s-unread" { "INDEXED BY message_folder_read" } else { "INDEXED BY message_flagged" };
            let total: u32 = self
                .conn
                .query_row(&format!("SELECT COUNT(*) FROM message m {hint} WHERE {cond} AND {scope}"), [], |r| r.get(0))
                .map_err(err)?;
            let unread = if id == "s-unread" { total } else { 0 };
            out.push(Folder { id: id.into(), name: name.into(), role: FolderRole::Virtual, unread, total, children: vec![] });
        }
        let mut stmt = self.conn.prepare("SELECT id, name FROM search_folder ORDER BY position, id").map_err(err)?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).map_err(err)?;
        for row in rows {
            let (id, name) = row.map_err(err)?;
            out.push(Folder { id: format!("s-{id}"), name, role: FolderRole::Virtual, unread: 0, total: 0, children: vec![] });
        }
        Ok(out)
    }

    /// Lignes `start..start+count` d'une vue ouverte.
    pub fn rows(&self, token: u64, start: u32, count: u32) -> Result<Vec<MessageSummary>> {
        self.rows_at(token, start, count, Local::now())
    }

    pub fn rows_at(&self, token: u64, start: u32, count: u32, now: DateTime<Local>) -> Result<Vec<MessageSummary>> {
        let view = self.views.get(&token).ok_or("vue expirée : rouvrir le dossier")?;
        let start = (start as usize).min(view.rows.len());
        let end = (start + count.min(MAX_ROWS) as usize).min(view.rows.len());
        let items = &view.rows[start..end];
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let shown: Vec<i64> = items.iter().map(|r| r.item.shown()).collect();
        let slice = shown.as_slice();
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
                    EXISTS (SELECT 1 FROM attachment a WHERE a.message_id = m.id AND a.inline = 0 AND a.hidden = 0), f.name, f.id
             FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id IN ({marks})"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(err)?;
        let mut by_id: HashMap<i64, MessageSummary> = HashMap::with_capacity(slice.len());
        let search = view.search;
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
                        folder: if search { Some(r.get(12)?) } else { None },
                        conversation: None,
                        child: false,
                    },
                    (r.get::<_, i64>(13)?, r.get::<_, String>(12)?),
                ))
            })
            .map_err(err)?;
        let mut folder_of: HashMap<i64, (i64, String)> = HashMap::with_capacity(slice.len());
        for row in rows {
            let (id, source, mut m, folder) = row.map_err(err)?;
            m.categories = self.chips(source, cats.remove(&id).unwrap_or_default());
            folder_of.insert(id, folder);
            by_id.insert(id, m);
        }
        let mut out = Vec::with_capacity(items.len());
        for (i, row) in items.iter().enumerate() {
            // Un même message peut figurer deux fois (en-tête et élément déplié) : on clone.
            let Some(mut m) = by_id.get(&row.item.shown()).cloned() else { continue };
            m.pos = (start + i) as u32;
            if let Some(conv) = &view.conv {
                match &row.item {
                    RowItem::Conversation { key, expanded, .. } => self.conversation_head(&mut m, key, *expanded, conv)?,
                    RowItem::Child(id) => {
                        m.child = true;
                        m.folder = folder_of.get(id).filter(|(f, _)| *f != conv.folder).map(|(_, name)| name.clone());
                    }
                    RowItem::Message(_) => {}
                }
            }
            out.push(m);
        }
        Ok(out)
    }

    /// Fait de la ligne du dernier élément l'en-tête de sa conversation (si elle en compte plusieurs) :
    /// participants, compteurs, indicateurs, pièces jointes et catégories de tous ses éléments.
    fn conversation_head(&self, m: &mut MessageSummary, key: &[u8; 16], expanded: bool, conv: &ConvView) -> Result<()> {
        let sum = conv::summary(&self.conn, key, &conv.scope, conv.folder)?;
        if sum.count < 2 {
            return Ok(());
        }
        let source: i64 = self.conn.query_row("SELECT source_id FROM folder WHERE id = ?1", [conv.folder], |r| r.get(0)).map_err(err)?;
        if !sum.senders.is_empty() {
            m.from = sum.senders.join("; ");
        }
        m.subject = normalized_subject(&m.subject).to_string();
        m.flag = if sum.flagged {
            FlagState::Flagged
        } else if sum.done {
            FlagState::Done
        } else {
            FlagState::None
        };
        if sum.high {
            m.importance = Importance::High;
        }
        m.has_attachments = sum.attachments;
        m.categories = self.chips(source, sum.categories);
        // En gras si un élément du dossier affiché est non lu.
        m.unread = sum.unread_here > 0;
        m.conversation = Some(ConversationInfo { count: sum.count, unread: sum.unread, expanded });
        Ok(())
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

    /// Messages visés par les lignes `[a, b[` d'une vue (ceux du dossier pour un en-tête de conversation).
    pub fn view_targets(&self, token: u64, a: u32, b: u32) -> Option<Vec<i64>> {
        let rows = &self.views.get(&token)?.rows;
        let (a, b) = ((a as usize).min(rows.len()), (b as usize).min(rows.len()));
        Some(rows[a.min(b)..b].iter().flat_map(|r| r.item.targets().iter().copied()).collect())
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}

/// Groupes contigus des lignes : par date pour le tri par date, sinon d'après `labels` (un par ligne).
fn groups(rows: &[Row], sort: SortField, now: DateTime<Local>, labels: &[String]) -> Vec<ListGroup> {
    let mut groups: Vec<ListGroup> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let label = match sort {
            SortField::Date => match local(Some(row.date).filter(|d| *d > 0)) {
                Some(d) => format::date_group(&d, &now),
                None => "Sans date".to_string(),
            },
            _ => labels.get(i).cloned().unwrap_or_default(),
        };
        match groups.last_mut() {
            Some(g) if g.label == label => g.count += 1,
            _ => groups.push(ListGroup { label, start: i as u32, count: 1 }),
        }
    }
    groups
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
