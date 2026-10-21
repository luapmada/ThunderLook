//! « Exécuter maintenant » (J6) : rejoue des règles sur les messages d'un dossier, en simulation (rapport : règle →
//! messages concernés → actions) ou pour de bon.
//!
//! Pour chaque message, les règles choisies sont évaluées dans l'ordre de la liste ; « arrêter de traiter plus de
//! règles » interrompt la suite pour ce message. Les effets sont d'abord cumulés message par message, puis appliqués
//! par lots avec les opérations du J3 (annulables), sauf l'importance, la sensibilité et l'indicateur à N jours.
//! Les envois (transférer, répondre…) ne sont pas exécutés avant le J7 ; alertes, sons et impressions sont ignorés.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    time::Instant,
};

use chrono::{Duration, Local, NaiveDate, TimeZone};
use mail_parser::{Address, MessageParser};
use thunderlook_model::{FlagAction, MessageOp, RuleRunEntry, RuleRunReport, RuleRunRequest, RunFilter, Selection};
use thunderlook_rules::{Kind, Rule, Value, catalog, describe_element};
use thunderlook_store::rusqlite::{params, params_from_iter};

use crate::{
    Mailbox, Result,
    conv::bag_value,
    err, folder_db_id,
    index::body_text,
    ops::local_midnight_ms,
    parse_id,
    rules::{FolderMatch, FolderResolver},
};

/// Messages cités par règle dans le rapport.
const SAMPLES: usize = 50;
/// `PidLidInternetAccountName` (PSETID_Common, 0x8580) : compte de réception.
const PSETID_COMMON: &str = "00062008-0000-0000-c000-000000000046";
const ACCOUNT_LID: u32 = 0x8580;

#[derive(Clone)]
struct Who {
    name: Option<String>,
    addr: Option<String>,
}

#[derive(Clone, Copy)]
enum Field {
    Subject,
    Body,
    SubjectOrBody,
    RecipientAddress,
    SenderAddress,
    Header,
}

#[derive(Clone, Copy)]
enum Me {
    InTo,
    OnlyRecipient,
    NotInTo,
    InCc,
    InToOrCc,
}

enum Test {
    Always,
    Me(Me),
    From(Vec<Who>),
    SentTo(Vec<Who>),
    Words(Field, Vec<String>),
    FlagFor(String),
    Importance(u32),
    Sensitivity(u32),
    Category(Vec<String>),
    AnyCategory,
    Attachment,
    /// Bornes en Ko, incluses.
    Size(u32, u32),
    /// Reçu après, avant (millisecondes, heure locale).
    Date(Option<i64>, Option<i64>),
    Account(String),
    AutoReply,
    Meeting,
    Form(String),
    /// Flux RSS, éventuellement avec des mots dans le titre.
    Rss(Option<Vec<String>>),
}

impl Test {
    fn needs_eml(&self) -> bool {
        matches!(
            self,
            Test::Me(_) | Test::SentTo(_) | Test::Words(Field::Body | Field::SubjectOrBody | Field::RecipientAddress | Field::Header, _)
        )
    }
}

struct Cond {
    test: Test,
    negate: bool,
}

#[derive(Clone)]
enum Act {
    Move(i64),
    Copy(i64),
    Delete,
    DeletePermanently,
    Categories(Vec<String>),
    ClearCategories,
    Importance(u32),
    Sensitivity(u32),
    Read,
    Flag(FlagAction),
    /// Indicateur avec libellé, échéance dans N jours.
    FlagIn(String, u32),
    Stop,
}

struct Compiled {
    db_id: String,
    name: String,
    conds: Vec<Cond>,
    acts: Vec<Act>,
    actions: Vec<String>,
    not_run: Vec<String>,
    warnings: Vec<String>,
    skipped: Option<String>,
}

fn lower(s: &str) -> String {
    s.to_lowercase()
}

fn words(e: &thunderlook_rules::Element) -> Vec<String> {
    e.values.iter().filter_map(|v| if let Value::Words { text, .. } = v { Some(lower(text)) } else { None }).collect()
}

fn people(e: &thunderlook_rules::Element) -> Vec<Who> {
    e.values
        .iter()
        .flat_map(|v| if let Value::Recipients { recipients, .. } = v { recipients.as_slice() } else { &[] })
        .map(|r| Who { name: r.name.as_deref().map(lower), addr: r.address.as_deref().map(lower) })
        .collect()
}

fn text_value(e: &thunderlook_rules::Element) -> String {
    e.values.iter().find_map(|v| if let Value::Text { text, .. } = v { Some(text.clone()) } else { None }).unwrap_or_default()
}

fn level(e: &thunderlook_rules::Element) -> u32 {
    e.values.iter().find_map(|v| if let Value::Level { level, .. } = v { Some(*level) } else { None }).unwrap_or(1)
}

/// PT_APPTIME (jours depuis le 30/12/1899, heure locale) en millisecondes.
fn apptime_ms(days: f64) -> i64 {
    let base = NaiveDate::from_ymd_opt(1899, 12, 30).expect("date").and_hms_opt(0, 0, 0).expect("minuit");
    let t = base + Duration::milliseconds((days * 86_400_000.0).round() as i64);
    Local.from_local_datetime(&t).earliest().map_or(0, |d| d.timestamp_millis())
}

/// Condition ou exception (élément `id`, de nature `kind`) ; `None` si non prise en charge.
fn test_of(e: &thunderlook_rules::Element) -> Option<Test> {
    // Chaque exception reprend une condition (même valeur) ; correspondance établie sur les libellés du catalogue.
    let id = match (catalog(e.id).map(|c| c.1), e.id) {
        (Some(Kind::Exception), 0x1F4..=0x212) => e.id - 0x12C,
        (Some(Kind::Exception), 0x213) => 0xE8,
        (Some(Kind::Exception), 0x214) => 0xEE,
        (Some(Kind::Exception), 0x215..=0x216) => e.id - 0x125,
        (Some(Kind::Exception), 0x218..=0x21B) => e.id - 0x124,
        _ => e.id,
    };
    Some(match id {
        0xC8 => Test::Me(Me::InTo),
        0xC9 => Test::Me(Me::OnlyRecipient),
        0xCA => Test::Me(Me::NotInTo),
        0xE2 => Test::Me(Me::InCc),
        0xE3 => Test::Me(Me::InToOrCc),
        0xCB => Test::From(people(e)),
        0xCC => Test::SentTo(people(e)),
        0xCD => Test::Words(Field::Subject, words(e)),
        0xCE => Test::Words(Field::Body, words(e)),
        0xCF => Test::Words(Field::SubjectOrBody, words(e)),
        0xE5 => Test::Words(Field::RecipientAddress, words(e)),
        0xE6 => Test::Words(Field::SenderAddress, words(e)),
        0xE8 => Test::Words(Field::Header, words(e)),
        0xD0 => Test::FlagFor(e.values.iter().find_map(|v| if let Value::FlagFor { text, .. } = v { Some(lower(text)) } else { None })?),
        0xD2 => Test::Importance(level(e)),
        0xD3 => Test::Sensitivity(level(e)),
        0xD7 => Test::Category(text_value(e).split(';').map(|s| lower(s.trim())).filter(|s| !s.is_empty()).collect()),
        0xF6 => Test::AnyCategory,
        0xDE => Test::Attachment,
        0xE0 => match e.values.first()? {
            Value::Size { min_kb, max_kb, .. } => Test::Size(*min_kb, *max_kb),
            _ => return None,
        },
        0xE1 => match e.values.first()? {
            Value::Date { test_after, after, test_before, before, .. } => {
                Test::Date((*test_after != 0).then(|| apptime_ms(*after)), (*test_before != 0).then(|| apptime_ms(*before)))
            }
            _ => return None,
        },
        0xEE => Test::Account(e.values.iter().find_map(|v| if let Value::Account { name, .. } = v { Some(lower(name)) } else { None })?),
        // « Sur cet ordinateur uniquement » : toujours vrai, ThunderLook est local.
        0xEF => Test::Always,
        0xDC => Test::AutoReply,
        0xF1 => Test::Meeting,
        0xE4 => Test::Form(e.values.iter().find_map(|v| if let Value::Form { class, .. } = v { Some(lower(class)) } else { None })?),
        0xF7 => Test::Rss(None),
        0xF5 => Test::Rss(Some(e.values.iter().filter_map(|v| if let Value::Text { text, .. } = v { Some(lower(text)) } else { None }).collect())),
        _ => return None,
    })
}

fn flag_action(when: u32) -> Option<FlagAction> {
    Some(match when {
        1 => FlagAction::Today,
        2 => FlagAction::Tomorrow,
        3 => FlagAction::ThisWeek,
        4 => FlagAction::NextWeek,
        7 => FlagAction::NoDate,
        10 => FlagAction::Complete,
        _ => return None,
    })
}

fn compile(db_id: i64, rule: &Rule, folders: &FolderResolver) -> Compiled {
    let mut c = Compiled {
        db_id: format!("r{db_id}"),
        name: rule.name.clone(),
        conds: Vec::new(),
        acts: Vec::new(),
        actions: Vec::new(),
        not_run: Vec::new(),
        warnings: folders.warnings(rule),
        skipped: None,
    };
    if let Some(e) = &rule.error {
        c.skipped = Some(format!("règle lue incomplètement : {e}"));
        return c;
    }
    if !rule.on_arrival() {
        c.skipped = Some("règle appliquée à l'envoi des messages".into());
        return c;
    }
    for e in &rule.elements {
        let label = describe_element(e).unwrap_or_default();
        match catalog(e.id).map(|c| c.1) {
            Some(Kind::Header) | None => {}
            Some(kind @ (Kind::Condition | Kind::Exception)) => match test_of(e) {
                Some(test) => c.conds.push(Cond { test, negate: kind == Kind::Exception }),
                None => {
                    c.skipped = Some(format!("condition non prise en charge : {label}"));
                    return c;
                }
            },
            Some(Kind::Action) => {
                let act = match (e.id, e.values.first()) {
                    (0x12C | 0x139, Some(Value::Folder { folder_entry_id, name, .. })) => match folders.resolve(folder_entry_id, name) {
                        FolderMatch::Missing => {
                            c.not_run.push(format!("{label} (dossier introuvable)"));
                            continue;
                        }
                        m => {
                            let f = m.folder().expect("dossier");
                            if e.id == 0x12C { Act::Move(f) } else { Act::Copy(f) }
                        }
                    },
                    (0x12D, _) => Act::Delete,
                    (0x14A, _) => Act::DeletePermanently,
                    (0x133, _) => Act::Categories(text_value(e).split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()),
                    (0x152, _) => Act::ClearCategories,
                    (0x137, _) => Act::Importance(level(e)),
                    (0x138, _) => Act::Sensitivity(level(e)),
                    (0x14C, _) => Act::Read,
                    (0x132, _) => Act::Flag(FlagAction::Clear),
                    (0x151, Some(Value::FollowUp { when, .. })) => match flag_action(*when) {
                        Some(a) => Act::Flag(a),
                        None => {
                            c.not_run.push(format!("{label} (échéance {when} inconnue)"));
                            continue;
                        }
                    },
                    (0x131, Some(Value::FlagFor { n, text, .. })) => Act::FlagIn(text.clone(), *n),
                    (0x142, _) => Act::Stop,
                    (0x12E | 0x12F | 0x13C | 0x144 | 0x146 | 0x147, _) => {
                        c.not_run.push(format!("{label} (envoi : pas avant le J7)"));
                        continue;
                    }
                    _ => {
                        c.not_run.push(format!("{label} (sans effet pour « Exécuter maintenant »)"));
                        continue;
                    }
                };
                c.actions.push(label);
                c.acts.push(act);
            }
        }
    }
    // Les tests qui lisent le `.eml` en dernier : ceux de la base écartent la plupart des messages.
    c.conds.sort_by_key(|cond| cond.test.needs_eml());
    c
}

/// Ce que le `.eml` apporte aux conditions (minuscules).
#[derive(Default)]
struct EmlData {
    headers: String,
    to: Vec<(String, String)>,
    cc: Vec<(String, String)>,
    bcc: Vec<(String, String)>,
    body: String,
}

fn people_of(a: Option<&Address>) -> Vec<(String, String)> {
    a.map(|a| a.iter().map(|x| (x.name().map(lower).unwrap_or_default(), x.address().map(lower).unwrap_or_default())).collect())
        .unwrap_or_default()
}

fn read_eml(data_dir: &std::path::Path, rel: &str) -> EmlData {
    let Ok(bytes) = fs::read(thunderlook_store::resolve(data_dir, rel)) else { return EmlData::default() };
    let end = bytes.windows(4).position(|w| w == b"\r\n\r\n").or_else(|| bytes.windows(2).position(|w| w == b"\n\n")).unwrap_or(bytes.len());
    let headers = lower(&String::from_utf8_lossy(&bytes[..end]));
    let Some(m) = MessageParser::default().parse(&bytes) else { return EmlData { headers, ..Default::default() } };
    EmlData { headers, to: people_of(m.to()), cc: people_of(m.cc()), bcc: people_of(m.bcc()), body: lower(&body_text(&m)) }
}

struct Msg {
    id: i64,
    folder: i64,
    is_read: bool,
    subject: String,
    from_name: String,
    from_addr: String,
    to_line: String,
    cc_line: String,
    received_at: Option<i64>,
    size: i64,
    attachments: i64,
    importance: u32,
    sensitivity: u32,
    flag_status: i64,
    flag_request: String,
    class: String,
    account: Option<String>,
    categories: Vec<String>,
    eml: String,
    data: Option<EmlData>,
}

impl Msg {
    fn eml<'a>(&'a mut self, data_dir: &std::path::Path) -> &'a EmlData {
        if self.data.is_none() {
            self.data = Some(read_eml(data_dir, &self.eml));
        }
        self.data.as_ref().expect("lu")
    }

    fn matches(&mut self, t: &Test, data_dir: &std::path::Path) -> bool {
        let who_matches = |w: &Who, name: &str, addr: &str| match &w.addr {
            Some(a) if !a.is_empty() => a == addr,
            _ => w.name.as_deref().is_some_and(|n| n == name),
        };
        match t {
            Test::Always => true,
            Test::Me(kind) => {
                let Some(me) = self.account.clone() else { return false };
                let d = self.eml(data_dir);
                let in_to = d.to.iter().any(|p| p.1 == me);
                let in_cc = d.cc.iter().any(|p| p.1 == me);
                match kind {
                    Me::InTo => in_to,
                    Me::NotInTo => !in_to,
                    Me::InCc => in_cc,
                    Me::InToOrCc => in_to || in_cc,
                    Me::OnlyRecipient => d.to.len() + d.cc.len() == 1 && (in_to || in_cc),
                }
            }
            Test::From(list) => {
                let (name, addr) = (lower(&self.from_name), lower(&self.from_addr));
                list.iter().any(|w| who_matches(w, &name, &addr))
            }
            Test::SentTo(list) => {
                let d = self.eml(data_dir);
                let all: Vec<&(String, String)> = d.to.iter().chain(&d.cc).chain(&d.bcc).collect();
                if all.is_empty() {
                    // `.eml` sans destinataires lisibles : les lignes À et Cc de la base (noms, parfois adresses).
                    let lines = lower(&format!("{} {}", self.to_line, self.cc_line));
                    return list.iter().any(|w| w.addr.as_deref().or(w.name.as_deref()).is_some_and(|x| lines.contains(x)));
                }
                list.iter().any(|w| all.iter().any(|(n, a)| who_matches(w, n, a)))
            }
            Test::Words(field, list) => {
                let hay = match field {
                    Field::Subject => lower(&self.subject),
                    Field::SenderAddress => lower(&self.from_addr),
                    Field::Body => self.eml(data_dir).body.clone(),
                    Field::SubjectOrBody => format!("{}\n{}", lower(&self.subject), self.eml(data_dir).body),
                    Field::Header => self.eml(data_dir).headers.clone(),
                    Field::RecipientAddress => {
                        let d = self.eml(data_dir);
                        d.to.iter().chain(&d.cc).chain(&d.bcc).map(|p| p.1.as_str()).collect::<Vec<_>>().join(" ")
                    }
                };
                list.iter().any(|w| hay.contains(w.as_str()))
            }
            Test::FlagFor(text) => self.flag_status == 2 && lower(&self.flag_request) == *text,
            Test::Importance(l) => self.importance == *l,
            Test::Sensitivity(l) => self.sensitivity == *l,
            Test::Category(names) => self.categories.iter().any(|c| names.contains(&lower(c))),
            Test::AnyCategory => !self.categories.is_empty(),
            Test::Attachment => self.attachments > 0,
            Test::Size(min, max) => {
                let kb = self.size / 1024;
                kb >= i64::from(*min) && kb <= i64::from(*max)
            }
            Test::Date(after, before) => {
                let Some(t) = self.received_at else { return false };
                after.is_none_or(|a| t > a) && before.is_none_or(|b| t < b)
            }
            Test::Account(name) => self.account.as_deref() == Some(name.as_str()),
            Test::AutoReply => lower(&self.class).starts_with("ipm.note.rules."),
            Test::Meeting => lower(&self.class).starts_with("ipm.schedule.meeting"),
            Test::Form(class) => lower(&self.class) == *class,
            Test::Rss(title) => {
                lower(&self.class).starts_with("ipm.post.rss")
                    && title.as_ref().is_none_or(|w| w.iter().any(|x| lower(&self.subject).contains(x.as_str())))
            }
        }
    }
}

/// Destination finale d'un message.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Dest {
    Folder(i64),
    Trash,
    Purge,
}

#[derive(Default)]
struct Plan {
    dest: Option<Dest>,
    copies: Vec<i64>,
    read: bool,
    clear_categories: bool,
    add_categories: Vec<String>,
    importance: Option<u32>,
    sensitivity: Option<u32>,
    flag: Option<Act>,
}

impl Plan {
    fn add(&mut self, act: &Act) {
        match act {
            Act::Move(f) => self.dest = Some(Dest::Folder(*f)),
            Act::Copy(f) => self.copies.push(*f),
            Act::Delete => self.dest = Some(Dest::Trash),
            Act::DeletePermanently => self.dest = Some(Dest::Purge),
            Act::Categories(names) => {
                for n in names {
                    if !self.add_categories.iter().any(|c| c.eq_ignore_ascii_case(n)) {
                        self.add_categories.push(n.clone());
                    }
                }
            }
            Act::ClearCategories => {
                self.clear_categories = true;
                self.add_categories.clear();
            }
            Act::Importance(l) => self.importance = Some(*l),
            Act::Sensitivity(l) => self.sensitivity = Some(*l),
            Act::Read => self.read = true,
            Act::Flag(_) | Act::FlagIn(..) => self.flag = Some(act.clone()),
            Act::Stop => {}
        }
    }

    /// Retire ce qui ne changerait rien : le rapport ne compte que les messages réellement modifiés.
    fn drop_no_ops(&mut self, msg: &Msg) {
        if self.dest == Some(Dest::Folder(msg.folder)) {
            self.dest = None;
        }
        self.read &= !msg.is_read;
        self.clear_categories &= !msg.categories.is_empty();
        if !self.clear_categories {
            self.add_categories.retain(|n| !msg.categories.iter().any(|c| c.eq_ignore_ascii_case(n)));
        }
        if self.importance == Some(msg.importance) {
            self.importance = None;
        }
        if self.sensitivity == Some(msg.sensitivity) {
            self.sensitivity = None;
        }
    }

    fn is_empty(&self) -> bool {
        self.dest.is_none()
            && self.copies.is_empty()
            && !self.read
            && !self.clear_categories
            && self.add_categories.is_empty()
            && self.importance.is_none()
            && self.sensitivity.is_none()
            && self.flag.is_none()
    }
}

/// Règles remplies par un message (indices) et effets cumulés, dans l'ordre de la liste, jusqu'à « arrêter ».
fn evaluate(msg: &mut Msg, rules: &[Compiled], data_dir: &std::path::Path) -> (Vec<usize>, Plan) {
    let mut hits = Vec::new();
    let mut plan = Plan::default();
    for (k, rule) in rules.iter().enumerate() {
        if rule.skipped.is_some() || !rule.conds.iter().all(|c| msg.matches(&c.test, data_dir) != c.negate) {
            continue;
        }
        hits.push(k);
        for a in &rule.acts {
            plan.add(a);
        }
        if rule.acts.iter().any(|a| matches!(a, Act::Stop)) {
            break;
        }
    }
    // Le `.eml` lu ne sert plus : on libère la mémoire tout de suite.
    msg.data = None;
    plan.drop_no_ops(msg);
    (hits, plan)
}

/// Identifiant 0x8000+ d'une propriété nommée numérotée, dans la table d'une source (`source.props`).
/// Format : id u16, GUID texte (36 octets), type (0 numéro u32, 1 nom : longueur u32 + octets).
fn named_id(raw: &[u8], guid: &str, lid: u32) -> Option<u16> {
    let mut i = 0;
    while i + 39 <= raw.len() {
        let id = u16::from_le_bytes([raw[i], raw[i + 1]]);
        let g = std::str::from_utf8(&raw[i + 2..i + 38]).ok()?;
        let kind = raw[i + 38];
        i += 39;
        let v = u32::from_le_bytes(raw.get(i..i + 4)?.try_into().ok()?);
        i += 4;
        if kind == 0 {
            if v == lid && g.eq_ignore_ascii_case(guid) {
                return Some(id);
            }
        } else {
            i += v as usize;
        }
    }
    None
}

fn utf16(b: &[u8]) -> String {
    String::from_utf16_lossy(&b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
}

impl Mailbox {
    /// « Exécuter maintenant » : simulation (`apply` faux) ou application des règles choisies.
    pub fn run_rules(&mut self, req: &RuleRunRequest) -> Result<RuleRunReport> {
        let started = Instant::now();
        let wanted: HashSet<i64> = req.rule_ids.iter().map(|id| parse_id(id, 'r')).collect::<Result<_>>()?;
        let folders = FolderResolver::load(&self.conn)?;
        let mut rules: Vec<Compiled> = Vec::new();
        {
            let mut stmt = self.conn.prepare("SELECT id, rule_json FROM rule ORDER BY kind, origin, position").map_err(err)?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).map_err(err)?;
            for row in rows {
                let (id, json) = row.map_err(err)?;
                if !wanted.contains(&id) {
                    continue;
                }
                let rule: Rule = serde_json::from_str(&json).map_err(|e| format!("règle r{id} illisible : {e}"))?;
                rules.push(compile(id, &rule, &folders));
            }
        }
        if rules.len() != wanted.len() {
            return Err("règle introuvable".into());
        }

        // Dossiers parcourus : le dossier choisi, et ses sous-dossiers si demandé.
        let root = folder_db_id(&req.folder_id)?;
        let scope: Vec<i64> = if req.subfolders {
            self.conn
                .prepare("WITH RECURSIVE t(id) AS (SELECT ?1 UNION ALL SELECT f.id FROM folder f JOIN t ON f.parent_id = t.id) SELECT id FROM t")
                .map_err(err)?
                .query_map([root], |r| r.get(0))
                .map_err(err)?
                .collect::<std::result::Result<_, _>>()
                .map_err(err)?
        } else {
            vec![root]
        };

        // Compte de réception : propriété nommée, dont l'identifiant dépend de la source.
        let mut account_ids: HashMap<i64, Option<u16>> = HashMap::new();
        {
            let mut stmt = self.conn.prepare("SELECT id, props FROM source").map_err(err)?;
            for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<Vec<u8>>>(1)?))).map_err(err)? {
                let (s, raw) = row.map_err(err)?;
                account_ids.insert(s, raw.as_deref().and_then(|r| named_id(r, PSETID_COMMON, ACCOUNT_LID)));
            }
        }

        let marks = vec!["?"; scope.len()].join(",");
        let only: Vec<i64> = req.only.iter().map(|id| parse_id(id, 'm')).collect::<Result<_>>()?;
        let only_filter = if only.is_empty() {
            String::new()
        } else {
            format!(" AND m.id IN ({})", only.iter().map(i64::to_string).collect::<Vec<_>>().join(","))
        };
        let read_filter = match req.filter {
            RunFilter::All => format!("{only_filter}"),
            RunFilter::Read => format!(" AND m.is_read = 1{only_filter}"),
            RunFilter::Unread => format!(" AND m.is_read = 0{only_filter}"),
        };
        let mut categories: HashMap<i64, Vec<String>> = HashMap::new();
        {
            let sql = format!(
                "SELECT c.message_id, c.name FROM message_category c JOIN message m ON m.id = c.message_id WHERE m.folder_id IN ({marks}){read_filter}"
            );
            let mut stmt = self.conn.prepare(&sql).map_err(err)?;
            for row in stmt.query_map(params_from_iter(&scope), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).map_err(err)? {
                let (id, name) = row.map_err(err)?;
                categories.entry(id).or_default().push(name);
            }
        }

        let mut msgs: Vec<Msg> = Vec::new();
        {
            let sql = format!(
                "SELECT m.id, m.subject, m.from_name, m.from_addr, m.to_line, m.cc_line, m.received_at, m.size, m.attachment_count,
                    m.importance, m.sensitivity, m.flag_status, m.flag_request, m.message_class, m.eml_path, m.props, f.source_id,
                    m.folder_id, m.is_read
                 FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.folder_id IN ({marks}){read_filter}
                 ORDER BY m.received_at DESC"
            );
            let mut stmt = self.conn.prepare(&sql).map_err(err)?;
            let mut rows = stmt.query(params_from_iter(&scope)).map_err(err)?;
            while let Some(r) = rows.next().map_err(err)? {
                let id: i64 = r.get(0).map_err(err)?;
                let source: i64 = r.get(16).map_err(err)?;
                let account = account_ids.get(&source).copied().flatten().and_then(|pid| {
                    let props = r.get_ref(15).ok()?.as_blob().ok()?;
                    bag_value(props, pid, 0x001F).map(|b| lower(&utf16(b)))
                });
                msgs.push(Msg {
                    id,
                    folder: r.get(17).map_err(err)?,
                    is_read: r.get(18).map_err(err)?,
                    subject: r.get(1).map_err(err)?,
                    from_name: r.get(2).map_err(err)?,
                    from_addr: r.get(3).map_err(err)?,
                    to_line: r.get(4).map_err(err)?,
                    cc_line: r.get(5).map_err(err)?,
                    received_at: r.get(6).map_err(err)?,
                    size: r.get(7).map_err(err)?,
                    attachments: r.get(8).map_err(err)?,
                    importance: r.get(9).map_err(err)?,
                    sensitivity: r.get(10).map_err(err)?,
                    flag_status: r.get(11).map_err(err)?,
                    flag_request: r.get::<_, Option<String>>(12).map_err(err)?.unwrap_or_default(),
                    class: r.get(13).map_err(err)?,
                    account,
                    categories: categories.remove(&id).unwrap_or_default(),
                    eml: r.get(14).map_err(err)?,
                    data: None,
                });
            }
        }

        // Évaluation en parallèle : la lecture des `.eml` (corps, en-têtes) domine. Chaque fil rend, par message,
        // les règles remplies et les effets cumulés ; la fusion garde l'ordre des messages.
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 16);
        let chunk = msgs.len().div_ceil(threads).max(1);
        let data_dir = self.data_dir.clone();
        let outcomes: Vec<(Vec<usize>, Plan)> = std::thread::scope(|scope| {
            let handles: Vec<_> = msgs
                .chunks_mut(chunk)
                .map(|part| {
                    let (rules, data_dir) = (&rules, &data_dir);
                    scope.spawn(move || part.iter_mut().map(|m| evaluate(m, rules, data_dir)).collect::<Vec<_>>())
                })
                .collect();
            handles.into_iter().flat_map(|h| h.join().expect("évaluation des règles")).collect()
        });
        let scanned = msgs.len() as u32;
        let mut matched: Vec<Vec<String>> = vec![Vec::new(); rules.len()];
        let mut counts = vec![0u32; rules.len()];
        let mut plans: BTreeMap<i64, Plan> = BTreeMap::new();
        for (msg, (hits, plan)) in msgs.iter().zip(outcomes) {
            for k in hits {
                counts[k] += 1;
                if matched[k].len() < SAMPLES {
                    let date = msg.received_at.and_then(|t| Local.timestamp_millis_opt(t).single()).map(|d| d.format("%d/%m/%Y").to_string());
                    matched[k].push(format!("{} · {} · {}", date.unwrap_or_default(), msg.from_name, msg.subject));
                }
            }
            if !plan.is_empty() {
                plans.insert(msg.id, plan);
            }
        }

        // Sécurité : au-delà de la limite, il faut confirmer le nombre exact de déplacements vu dans un aperçu.
        let moves = plans.values().filter(|p| p.dest.is_some()).count() as u32;
        let needs_confirmation = req.apply && moves > thunderlook_model::RULE_MOVES_LIMIT && req.confirm_moves != Some(moves);
        let apply = req.apply && !needs_confirmation;
        let mut errors = Vec::new();
        if apply {
            errors = self.apply_plans(&plans);
        }
        let entries = rules
            .into_iter()
            .zip(matched)
            .zip(counts)
            .map(|((r, samples), n)| RuleRunEntry {
                rule_id: r.db_id,
                name: r.name,
                matched: n,
                samples,
                actions: r.actions,
                not_run: r.not_run,
                warnings: r.warnings,
                skipped: r.skipped,
            })
            .collect();
        Ok(RuleRunReport {
            scanned,
            changed: plans.len() as u32,
            applied: apply,
            moves,
            needs_confirmation,
            elapsed_ms: started.elapsed().as_millis() as u32,
            rules: entries,
            errors,
        })
    }

    /// Applique les effets cumulés, par lots : modifications, puis copies, puis déplacements et suppressions.
    fn apply_plans(&mut self, plans: &BTreeMap<i64, Plan>) -> Vec<String> {
        let mut errors = Vec::new();
        let sel = |ids: Vec<i64>| Selection::ids(ids.into_iter().map(|id| format!("m{id}")).collect());
        let mut ops: Vec<(Vec<i64>, MessageOp)> = Vec::new();
        let mut group = |pick: &dyn Fn(&Plan) -> Option<MessageOp>| {
            let mut by_op: Vec<(MessageOp, Vec<i64>)> = Vec::new();
            for (id, p) in plans {
                if let Some(op) = pick(p) {
                    match by_op.iter_mut().find(|(o, _)| *o == op) {
                        Some((_, ids)) => ids.push(*id),
                        None => by_op.push((op, vec![*id])),
                    }
                }
            }
            ops.extend(by_op.into_iter().map(|(op, ids)| (ids, op)));
        };
        group(&|p| p.read.then_some(MessageOp::Read { read: true }));
        group(&|p| p.clear_categories.then_some(MessageOp::ClearCategories));
        let names: Vec<String> = {
            let mut v: Vec<String> = plans.values().flat_map(|p| p.add_categories.iter().cloned()).collect();
            v.sort();
            v.dedup();
            v
        };
        for name in names {
            group(&|p| p.add_categories.contains(&name).then(|| MessageOp::Category { name: name.clone(), on: true }));
        }
        group(&|p| match &p.flag {
            Some(Act::Flag(a)) => Some(MessageOp::Flag { action: *a }),
            _ => None,
        });
        let copies: Vec<i64> = {
            let mut v: Vec<i64> = plans.values().flat_map(|p| p.copies.iter().copied()).collect();
            v.sort();
            v.dedup();
            v
        };
        for f in copies {
            group(&|p| p.copies.contains(&f).then(|| MessageOp::Copy { folder_id: format!("f{f}") }));
        }
        group(&|p| match p.dest {
            Some(Dest::Folder(f)) => Some(MessageOp::Move { folder_id: format!("f{f}") }),
            Some(Dest::Trash) => Some(MessageOp::Delete),
            Some(Dest::Purge) => Some(MessageOp::DeletePermanently),
            None => None,
        });
        // Hors opérations du J3 : importance, sensibilité, indicateur à N jours.
        let today = Local::now().date_naive();
        let direct = (|| -> Result<()> {
            let tx = self.conn.transaction().map_err(err)?;
            for (id, p) in plans {
                if let Some(l) = p.importance {
                    tx.execute("UPDATE message SET importance = ?2 WHERE id = ?1", params![id, l]).map_err(err)?;
                }
                if let Some(l) = p.sensitivity {
                    tx.execute("UPDATE message SET sensitivity = ?2 WHERE id = ?1", params![id, l]).map_err(err)?;
                }
                if let Some(Act::FlagIn(text, n)) = &p.flag {
                    let due = local_midnight_ms(today + Duration::days(i64::from(*n)));
                    tx.execute(
                        "UPDATE message SET flag_status = 2, flag_request = ?2, flag_due = ?3, flag_completed = NULL WHERE id = ?1",
                        params![id, text, due],
                    )
                    .map_err(err)?;
                    crate::remote::queue(&tx, crate::remote::Change::Flagged { id: *id, on: true })?;
                }
            }
            tx.commit().map_err(err)
        })();
        if let Err(e) = direct {
            errors.push(e);
        }
        for (ids, op) in ops {
            for chunk in ids.chunks(5000) {
                if let Err(e) = self.apply(&sel(chunk.to_vec()), &op) {
                    errors.push(format!("{op:?} : {e}"));
                }
            }
        }
        errors
    }
}
