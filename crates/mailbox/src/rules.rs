//! Règles côté client (J6) : import du flux `PR_RW_RULES_STREAM` des PST et des exports `.rwz` dans la table
//! `rule` (schéma v7), liste pour la boîte « Règles et alertes ».

use std::{collections::HashMap, path::Path};

use thunderlook_model::{NewRule, RuleDraft, RuleSummary};
use thunderlook_rules::{Element, Recipient, Rule, RuleStream, Value};
use thunderlook_store::rusqlite::{Connection, OptionalExtension, params};

use crate::{Mailbox, Result, conv::bag_value, err, fold, parse_id};

/// Classe du contenu associé de la Boîte de réception qui porte le flux.
const RULE_ORGANIZER: &str = "IPM.RuleOrganizer";
/// `PR_RW_RULES_STREAM`.
const RULES_STREAM: u16 = 0x6802;
/// `PidTagEntryId`.
const ENTRY_ID: u16 = 0x0FFF;

/// Dossier désigné par une action « déplacer » ou « copier ».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderMatch {
    /// EntryID identique à celui du dossier importé (flux du PST importé).
    EntryId(i64),
    /// Même nom, sans tenir compte de la casse ni des accents : `.rwz` exporté d'un autre PST. Parmi des
    /// homonymes, celui qui a le plus de messages.
    Name { folder: i64, homonyms: usize },
    Missing,
}

impl FolderMatch {
    pub fn folder(self) -> Option<i64> {
        match self {
            Self::EntryId(f) | Self::Name { folder: f, .. } => Some(f),
            Self::Missing => None,
        }
    }
}

/// Correspondance entre les dossiers cités par les règles et ceux du dossier de données.
///
/// L'EntryID d'un dossier de PST fait 24 octets : 4 octets de drapeaux, l'identifiant du magasin sur 16 octets
/// (le même pour tous les dossiers d'un PST), puis le NID. L'EntryID du magasin (126 octets et plus, avec le
/// chemin du PST) n'est pas utile ici : celui du dossier suffit à reconnaître le PST.
pub struct FolderResolver {
    by_entry: HashMap<Vec<u8>, i64>,
    /// Nom replié → (dossier, messages), du plus rempli au moins rempli.
    by_name: HashMap<String, Vec<(i64, i64)>>,
    paths: HashMap<i64, String>,
}

impl FolderResolver {
    pub fn load(conn: &Connection) -> Result<Self> {
        let mut counts: HashMap<i64, i64> = HashMap::new();
        {
            let mut stmt = conn.prepare("SELECT folder_id, COUNT(*) FROM message INDEXED BY message_folder_read GROUP BY folder_id").map_err(err)?;
            for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).map_err(err)? {
                let (f, n) = row.map_err(err)?;
                counts.insert(f, n);
            }
        }
        let mut stmt = conn
            .prepare(
                "SELECT f.id, f.parent_id, f.name, f.role, f.hidden, f.props, s.display_name, f.imap_name, f.source_id
                 FROM folder f JOIN source s ON s.id = f.source_id",
            )
            .map_err(err)?;
        type Row = (i64, Option<i64>, String, String, bool, Option<Vec<u8>>, String, Option<String>, i64);
        let rows: Vec<Row> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)))
            .map_err(err)?
            .collect::<std::result::Result<_, _>>()
            .map_err(err)?;
        let parents: HashMap<i64, (Option<i64>, &str, &str)> = rows.iter().map(|r| (r.0, (r.1, r.2.as_str(), r.3.as_str()))).collect();
        let (mut by_entry, mut by_name, mut paths) = (HashMap::new(), HashMap::<String, Vec<(i64, i64)>>::new(), HashMap::new());
        for (id, _, name, _, hidden, props, source, imap_name, source_id) in &rows {
            if let Some(entry) = props.as_deref().and_then(|p| bag_value(p, ENTRY_ID, 0x0102)) {
                by_entry.insert(entry.to_vec(), *id);
            }
            // Règles copiées vers un compte : la cible est désignée par son nom IMAP (« imap:INBOX.Accusés »).
            if let Some(n) = imap_name {
                by_entry.insert(imap_target(*source_id, n), *id);
            }
            if !hidden {
                by_name.entry(fold(name)).or_default().push((*id, counts.get(id).copied().unwrap_or(0)));
            }
            // Chemin affiché : fichier de données, puis les dossiers sous la racine du courrier.
            let mut names = Vec::new();
            let mut at = Some(*id);
            while let Some((parent, n, role)) = at.and_then(|f| parents.get(&f)) {
                if *role != "root" && *role != "ipm" {
                    names.push(*n);
                }
                at = *parent;
            }
            names.push(source);
            names.reverse();
            paths.insert(*id, names.join(" › "));
        }
        for list in by_name.values_mut() {
            list.sort_by_key(|(id, n)| (std::cmp::Reverse(*n), *id));
        }
        Ok(Self { by_entry, by_name, paths })
    }

    pub fn resolve(&self, entry_id: &[u8], name: &str) -> FolderMatch {
        if let Some(f) = self.by_entry.get(entry_id) {
            return FolderMatch::EntryId(*f);
        }
        match self.by_name.get(&fold(name)).and_then(|l| l.first().map(|f| (f.0, l.len()))) {
            Some((folder, homonyms)) => FolderMatch::Name { folder, homonyms },
            None => FolderMatch::Missing,
        }
    }

    pub fn path(&self, folder: i64) -> &str {
        self.paths.get(&folder).map_or("", String::as_str)
    }

    /// Avertissements d'une règle sur ses dossiers cibles.
    pub fn warnings(&self, rule: &Rule) -> Vec<String> {
        let mut out = Vec::new();
        for value in rule.elements.iter().flat_map(|e| &e.values) {
            let Value::Folder { folder_entry_id, name, .. } = value else { continue };
            match self.resolve(folder_entry_id, name) {
                FolderMatch::Missing => out.push(format!("dossier « {name} » introuvable")),
                FolderMatch::Name { folder, homonyms } if homonyms > 1 => {
                    out.push(format!("dossier « {name} » : {homonyms} dossiers de ce nom, le plus rempli est retenu ({})", self.path(folder)))
                }
                _ => {}
            }
        }
        out
    }
}

/// Désignation d'un dossier de compte dans une règle copiée (à la place de l'EntryID d'un dossier de PST) : la source
/// du compte et le nom IMAP. La source est indispensable : deux comptes ont souvent des dossiers de même nom.
pub(crate) fn imap_target(source: i64, imap_name: &str) -> Vec<u8> {
    format!("imap:{source}:{imap_name}").into_bytes()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// Remplace le jeu de règles `origin` par celles du flux.
fn store_stream(conn: &Connection, kind: &str, source_id: Option<i64>, origin: &str, stream: &RuleStream) -> Result<usize> {
    let tx = conn.unchecked_transaction().map_err(err)?;
    tx.execute("DELETE FROM rule WHERE origin = ?1", params![origin]).map_err(err)?;
    {
        let mut insert = tx
            .prepare(
                "INSERT INTO rule (kind, source_id, origin, position, name, active, version, rule_json, raw, imported_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )
            .map_err(err)?;
        let at = now_ms();
        for (k, rule) in stream.rules.iter().enumerate() {
            let json = serde_json::to_string(rule).map_err(err)?;
            insert
                .execute(params![kind, source_id, origin, k as i64, rule.name, rule.active, stream.version, json, rule.raw, at])
                .map_err(err)?;
        }
    }
    tx.commit().map_err(err)?;
    Ok(stream.rules.len())
}

/// Importe les flux de règles des PST qui ne l'ont pas encore été (ouverture du dossier de données).
/// Un flux déjà importé n'est pas relu : l'état des cases « active » reste celui choisi dans ThunderLook.
/// Renvoie le nombre de règles importées ; un flux illisible est ignoré (il reste dans `fai.props`).
pub(crate) fn import_pst_rules(conn: &Connection) -> Result<usize> {
    let pending: Vec<(i64, i64, Vec<u8>)> = conn
        .prepare(
            "SELECT x.id, f.source_id, x.props FROM fai x JOIN folder f ON f.id = x.folder_id
             WHERE x.message_class = ?1 AND NOT EXISTS (SELECT 1 FROM rule r WHERE r.origin = 'pst:' || x.id)",
        )
        .map_err(err)?
        .query_map(params![RULE_ORGANIZER], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(err)?
        .collect::<std::result::Result<_, _>>()
        .map_err(err)?;
    let mut n = 0;
    for (fai, source, props) in pending {
        let Some(bytes) = bag_value(&props, RULES_STREAM, 0x0102) else { continue };
        if let Ok(stream) = thunderlook_rules::parse(bytes) {
            n += store_stream(conn, "pst", Some(source), &format!("pst:{fai}"), &stream)?;
        }
    }
    Ok(n)
}

/// Libellé d'un jeu de règles : nom du fichier de données pour un PST, nom du fichier pour un `.rwz`.
fn origin_label(conn: &Connection, origin: &str, source_id: Option<i64>) -> String {
    let source_name = || {
        let id = source_id?;
        conn.query_row("SELECT display_name FROM source WHERE id = ?1", params![id], |r| r.get::<_, String>(0)).optional().ok().flatten()
    };
    match (origin.starts_with("pst:") || origin.starts_with("account:")).then(source_name).flatten() {
        Some(name) => name,
        None => Path::new(origin).file_name().map_or_else(|| origin.to_string(), |n| n.to_string_lossy().into_owned()),
    }
}

/// Premier u32 des règles importées des PST et `.rwz` d'Outlook 2021 : version OL2002 (24 bits bas), « locator » 6.
const NEW_RULE_XR: u32 = 0x060F_4240;

/// Jeu de règles qui reçoit une règle créée depuis un message de la source `source` : (`kind`, `origin`).
///
/// Compte : `account:<source>`, le jeu exécuté à l'arrivée. Fichier de données sans compte : son flux importé
/// (`pst:<fai>`, un seul par PST, celui de la Boîte de réception), pour qu'il n'apparaisse qu'un jeu par fichier dans
/// « Règles et alertes » et que « Copier ce jeu vers » emporte aussi la nouvelle règle ; `pst:s<source>` si le PST
/// n'avait pas de règles (jamais pris pour l'identifiant d'un contenu associé, toujours numérique).
fn rule_set(conn: &Connection, source: i64) -> Result<(&'static str, String)> {
    let kind: String = conn.query_row("SELECT kind FROM source WHERE id = ?1", [source], |r| r.get(0)).map_err(err)?;
    if kind == "imap" {
        return Ok(("account", format!("account:{source}")));
    }
    let existing: Option<String> = conn
        .query_row("SELECT origin FROM rule WHERE kind = 'pst' AND source_id = ?1 ORDER BY origin LIMIT 1", [source], |r| r.get(0))
        .optional()
        .map_err(err)?;
    Ok(("pst", existing.unwrap_or_else(|| format!("pst:s{source}"))))
}

/// Message d'origine d'une règle : expéditeur, objet, premier destinataire de la zone À, dossier et source.
struct RuleSourceMessage {
    from_name: String,
    from_addr: String,
    subject: String,
    to: (String, String),
    folder: i64,
    folder_name: String,
    source: i64,
}

fn rule_source_message(conn: &Connection, data_dir: &Path, message_id: &str) -> Result<RuleSourceMessage> {
    let id = parse_id(message_id, 'm')?;
    let (from_name, from_addr, subject, to_line, eml, folder, folder_name, source): (String, String, String, String, String, i64, String, i64) = conn
        .query_row(
            "SELECT m.from_name, m.from_addr, m.subject, m.to_line, m.eml_path, f.id, f.name, f.source_id
             FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
        )
        .optional()
        .map_err(err)?
        .ok_or_else(|| format!("message introuvable : {message_id}"))?;
    // Destinataire : l'en-tête To du `.eml` (adresse), sinon la ligne À de la base.
    let from_eml = std::fs::read(thunderlook_store::resolve(data_dir, &eml)).ok().and_then(|bytes| {
        let m = mail_parser::MessageParser::default().parse(&bytes)?;
        let a = m.to()?.first()?;
        Some((a.name().unwrap_or_default().to_string(), a.address().unwrap_or_default().to_string()))
    });
    let to = from_eml.filter(|(_, a)| !a.is_empty()).unwrap_or_else(|| {
        match crate::parse_addresses(&to_line).into_iter().next() {
            Some(a) => (a.name.unwrap_or_default(), a.addr),
            None => (to_line.split(';').next().unwrap_or_default().trim().to_string(), String::new()),
        }
    });
    Ok(RuleSourceMessage { from_name, from_addr, subject, to, folder, folder_name, source })
}

/// Valeur « dossier » d'une action : `imap:<source>:<nom IMAP>` pour un dossier de compte, l'EntryID du dossier pour
/// un dossier de PST (comme les règles importées) ; sans l'un ni l'autre, le dossier sera retrouvé par son nom.
fn folder_value(conn: &Connection, folder: i64) -> Result<Value> {
    let (name, imap_name, source, props): (String, Option<String>, i64, Option<Vec<u8>>) = conn
        .query_row("SELECT name, imap_name, source_id, props FROM folder WHERE id = ?1", [folder], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .optional()
        .map_err(err)?
        .ok_or_else(|| format!("dossier introuvable : f{folder}"))?;
    let folder_entry_id = match imap_name {
        Some(i) => imap_target(source, &i),
        None => props.as_deref().and_then(|p| bag_value(p, ENTRY_ID, 0x0102)).map(<[u8]>::to_vec).unwrap_or_default(),
    };
    Ok(Value::Folder { a: 0, folder_entry_id, store_entry_id: Vec::new(), name, tail: 0 })
}

fn person(name: &str, addr: &str) -> Value {
    let non_empty = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_string());
    Value::Recipients { a: 0, recipients: vec![Recipient { name: non_empty(name), address: non_empty(addr) }], tail: (1, 0) }
}

impl Mailbox {
    /// « Créer une règle » : expéditeur, objet et destinataire du message, et le jeu de règles qui recevra la règle.
    pub fn rule_draft(&self, message_id: &str) -> Result<RuleDraft> {
        let m = rule_source_message(&self.conn, &self.data_dir, message_id)?;
        let (kind, origin) = rule_set(&self.conn, m.source)?;
        let note = if kind == "account" {
            let on: bool = self
                .conn
                .query_row("SELECT rules_on_arrival FROM account WHERE source_id = ?1", [m.source], |r| r.get(0))
                .optional()
                .map_err(err)?
                .unwrap_or(false);
            (!on).then(|| "Les règles à l'arrivée sont désactivées pour ce compte (Paramètres du compte).".to_string())
        } else {
            Some("Fichier de données sans compte : la règle s'applique avec « Exécuter les règles maintenant ».".to_string())
        };
        Ok(RuleDraft {
            message_id: message_id.to_string(),
            from_name: m.from_name,
            from_addr: m.from_addr,
            subject: m.subject,
            to_name: m.to.0,
            to_addr: m.to.1,
            folder_id: format!("f{}", m.folder),
            folder_name: m.folder_name,
            rule_set: origin_label(&self.conn, &origin, Some(m.source)),
            note,
        })
    }

    /// Crée une règle « à l'arrivée » depuis un message, à la fin du jeu de règles de sa source ([`rule_set`]).
    /// Renvoie son identifiant (`r<id>`).
    pub fn create_rule(&mut self, req: &NewRule) -> Result<String> {
        let name = req.name.trim();
        if name.is_empty() {
            return Err("Nommez la règle.".into());
        }
        let m = rule_source_message(&self.conn, &self.data_dir, &req.message_id)?;
        // En-têtes vus dans toutes les règles d'Outlook 2021 : « appliquer à l'arrivée » (0x190), puis 0x64.
        let mut elements =
            vec![Element { id: 0x190, values: vec![Value::Flags { a: 0, flags: 1 }] }, Element { id: 0x64, values: vec![Value::Flags { a: 0, flags: 1 }] }];
        if req.from {
            if m.from_addr.trim().is_empty() && m.from_name.trim().is_empty() {
                return Err("Ce message n'a pas d'expéditeur.".into());
            }
            elements.push(Element { id: 0xCB, values: vec![person(&m.from_name, &m.from_addr)] });
        }
        if req.sent_to {
            if m.to.0.trim().is_empty() && m.to.1.trim().is_empty() {
                return Err("Ce message n'a pas de destinataire.".into());
            }
            elements.push(Element { id: 0xCC, values: vec![person(&m.to.0, &m.to.1)] });
        }
        if let Some(text) = &req.subject {
            if text.trim().is_empty() {
                return Err("Indiquez le texte que l'objet doit contenir.".into());
            }
            elements.push(Element { id: 0xCD, values: vec![Value::Words { flags: 0, text: text.trim().to_string() }] });
        }
        if elements.len() == 2 {
            return Err("Cochez au moins une condition.".into());
        }
        let folder = crate::folder_db_id(&req.folder_id)?;
        elements.push(Element { id: 0x12C, values: vec![folder_value(&self.conn, folder)?] });
        let rule = Rule {
            name: name.to_string(),
            active: true,
            xr: NEW_RULE_XR,
            unknown: 0,
            magic: [0; 3],
            elements,
            error: None,
            // Pas de flux d'origine : rien n'exige ces octets (exécution et description lisent les éléments).
            raw: Vec::new(),
        };
        let (kind, origin) = rule_set(&self.conn, m.source)?;
        let (position, version): (i64, Option<i64>) = self
            .conn
            .query_row("SELECT COALESCE(MAX(position) + 1, 0), MAX(version) FROM rule WHERE origin = ?1", [&origin], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(err)?;
        self.conn
            .execute(
                "INSERT INTO rule (kind, source_id, origin, position, name, active, version, rule_json, raw, imported_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, ?8, ?9)",
                params![
                    kind,
                    m.source,
                    origin,
                    position,
                    rule.name,
                    version.unwrap_or(i64::from(thunderlook_rules::OL2016)),
                    serde_json::to_string(&rule).map_err(err)?,
                    rule.raw,
                    now_ms()
                ],
            )
            .map_err(err)?;
        Ok(format!("r{}", self.conn.last_insert_rowid()))
    }

    /// Règles de tous les jeux : ceux des PST d'abord, puis les `.rwz`, chacun dans son ordre.
    pub fn rules(&self) -> Result<Vec<RuleSummary>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, origin, source_id, position, name, active, rule_json FROM rule ORDER BY kind, origin, position")
            .map_err(err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, bool>(5)?,
                    r.get::<_, String>(6)?,
                ))
            })
            .map_err(err)?;
        let folders = FolderResolver::load(&self.conn)?;
        let mut out = Vec::new();
        for row in rows {
            let (id, origin, source, position, name, active, json) = row.map_err(err)?;
            let (description, error, warnings) = match serde_json::from_str::<Rule>(&json) {
                Ok(rule) => (thunderlook_rules::describe(&rule), rule.error.clone(), folders.warnings(&rule)),
                Err(e) => (Vec::new(), Some(format!("règle illisible : {e}")), Vec::new()),
            };
            out.push(RuleSummary {
                id: format!("r{id}"),
                origin_label: origin_label(&self.conn, &origin, source),
                origin,
                position: position as u32,
                name,
                active,
                description,
                error,
                warnings,
            });
        }
        Ok(out)
    }

    /// Importe un export `.rwz` ; un nouvel import du même fichier remplace ses règles.
    pub fn import_rwz(&mut self, path: &Path) -> Result<usize> {
        let bytes = std::fs::read(path).map_err(|e| format!("{} : {e}", path.display()))?;
        let stream = thunderlook_rules::parse(&bytes).map_err(|e| format!("{} : {e}", path.display()))?;
        let origin = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()).to_string_lossy().into_owned();
        store_stream(&self.conn, "rwz", None, &origin, &stream)
    }

    /// Copie les règles actives du jeu `origin` vers le compte de la source `account_source` (jeu `account:<source>`,
    /// remplacé s'il existe). Chaque dossier cible est cherché dans le compte par son nom ; s'il manque, il est créé
    /// sous la Boîte de réception (et sur le serveur). Renvoie le nombre de règles copiées et les dossiers créés.
    pub fn copy_rules_to_account(&mut self, origin: &str, account_source: i64) -> Result<(usize, Vec<String>)> {
        let rows: Vec<(String, Vec<u8>, i64)> = self
            .conn
            .prepare("SELECT rule_json, raw, version FROM rule WHERE origin = ?1 AND active = 1 ORDER BY position")
            .map_err(err)?
            .query_map([origin], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(err)?
            .collect::<std::result::Result<_, _>>()
            .map_err(err)?;
        if rows.is_empty() {
            return Err(format!("aucune règle active dans « {origin} »"));
        }
        let inbox: (i64, String) = self
            .conn
            .query_row(
                "SELECT id, imap_name FROM folder WHERE source_id = ?1 AND role = 'inbox' AND imap_name IS NOT NULL",
                [account_source],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(err)?
            .ok_or("ce compte n'a pas encore été relevé (pas de Boîte de réception)")?;
        let tx = self.conn.unchecked_transaction().map_err(err)?;
        let mut created = Vec::new();
        let mut rules = Vec::new();
        for (json, raw, version) in rows {
            let mut rule: Rule = serde_json::from_str(&json).map_err(err)?;
            for value in rule.elements.iter_mut().flat_map(|e| e.values.iter_mut()) {
                let Value::Folder { folder_entry_id, store_entry_id, name, .. } = value else { continue };
                let wanted = name.trim().to_string();
                // Un dossier du compte de ce nom, ou de ce nom IMAP directement sous la Boîte de réception.
                let existing: Option<String> = {
                    let mut stmt = tx
                        .prepare("SELECT name, imap_name FROM folder WHERE source_id = ?1 AND imap_name IS NOT NULL")
                        .map_err(err)?;
                    let list: Vec<(String, String)> =
                        stmt.query_map([account_source], |r| Ok((r.get(0)?, r.get(1)?))).map_err(err)?.collect::<std::result::Result<_, _>>().map_err(err)?;
                    list.into_iter()
                        .find(|(n, i)| {
                            fold(n) == fold(&wanted)
                                || i.strip_prefix(inbox.1.as_str()).is_some_and(|rest| rest.len() > 1 && fold(&crate::imap_utf7::decode(&rest[1..])) == fold(&wanted))
                        })
                        .map(|(_, i)| i)
                };
                let imap_name = match existing {
                    Some(i) => i,
                    None => {
                        let id = crate::ops::create_folder(&tx, inbox.0, &wanted, "folder")?;
                        created.push(wanted.clone());
                        tx.query_row("SELECT imap_name FROM folder WHERE id = ?1", [id], |r| r.get::<_, String>(0)).map_err(err)?
                    }
                };
                *folder_entry_id = imap_target(account_source, &imap_name);
                *store_entry_id = Vec::new();
                *name = wanted;
            }
            rules.push((rule, raw, version));
        }
        let origin_new = format!("account:{account_source}");
        tx.execute("DELETE FROM rule WHERE origin = ?1", [&origin_new]).map_err(err)?;
        let at = now_ms();
        for (k, (rule, raw, version)) in rules.iter().enumerate() {
            tx.execute(
                "INSERT INTO rule (kind, source_id, origin, position, name, active, version, rule_json, raw, imported_at)
                 VALUES ('account', ?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8)",
                params![account_source, origin_new, k as i64, rule.name, version, serde_json::to_string(rule).map_err(err)?, raw, at],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
        Ok((rules.len(), created))
    }

    pub fn set_rule_active(&mut self, id: &str, active: bool) -> Result<()> {
        let n = self.conn.execute("UPDATE rule SET active = ?2 WHERE id = ?1", params![parse_id(id, 'r')?, active]).map_err(err)?;
        if n == 0 { Err(format!("règle introuvable : {id}")) } else { Ok(()) }
    }
}
