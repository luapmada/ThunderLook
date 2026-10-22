//! Calendrier commun avec Outlook (J9, étape 4) : Outlook garde les rendez-vous (son PST, que MPE synchronise avec le
//! téléphone) ; ThunderLook les lit en direct et y écrit par le modèle objet d'Outlook.
//!
//! Lecture : pour chaque calendrier du PST ouvert dans Outlook qui correspond à une source de la base (mêmes NID de
//! dossiers), la table d'Outlook (EntryID, date de modification) désigne les rendez-vous nouveaux ou modifiés ; leurs
//! propriétés brutes (celles que lit [`thunderlook_calendar`]) sont lues par `PropertyAccessor` et fusionnées dans le
//! sac MAPI de la ligne, le reste du sac est gardé. Les rendez-vous disparus d'Outlook quittent la base. L'état
//! (EntryID et date de modification par NID) est gardé dans `outlook-link.json` du dossier de données.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    time::Instant,
};

use serde::{Deserialize, Serialize};
use thunderlook_calendar::{
    bag::{Bag, Value},
    named::{Named, PS_PUBLIC_STRINGS, PSETID_APPOINTMENT, PSETID_COMMON, PSETID_MEETING},
};
use thunderlook_store::rusqlite::{Connection, OptionalExtension, params};
use windows::Win32::System::Variant::VARIANT;

use crate::com::{self, Disp, Result};

const STATE_FILE: &str = "outlook-link.json";
/// `olAppointmentItem` (DefaultItemType d'un dossier de calendrier, et `Items.Add`).
const OL_APPOINTMENT: i32 = 1;

/// Propriété lue dans Outlook : (jeu de propriétés nommées ou `None` pour une propriété fixe, numéro, type MAPI).
#[derive(Clone, Copy)]
enum Key {
    Fixed(u16, u16),
    Named(&'static str, u32, u16),
    Keywords,
}

const PT_LONG: u16 = 0x0003;
const PT_BOOLEAN: u16 = 0x000B;
const PT_UNICODE: u16 = 0x001F;
const PT_SYSTIME: u16 = 0x0040;
const PT_BINARY: u16 = 0x0102;

/// Les propriétés que lit `thunderlook_calendar::read`, plus la date de modification.
const KEYS: &[Key] = &[
    Key::Fixed(0x0037, PT_UNICODE),
    Key::Fixed(0x0060, PT_SYSTIME),
    Key::Fixed(0x0061, PT_SYSTIME),
    Key::Fixed(0x0036, PT_LONG),
    Key::Fixed(0x0017, PT_LONG),
    Key::Fixed(0x3008, PT_SYSTIME),
    Key::Named(PSETID_APPOINTMENT, 0x8205, PT_LONG),
    Key::Named(PSETID_APPOINTMENT, 0x8208, PT_UNICODE),
    Key::Named(PSETID_APPOINTMENT, 0x820D, PT_SYSTIME),
    Key::Named(PSETID_APPOINTMENT, 0x820E, PT_SYSTIME),
    Key::Named(PSETID_APPOINTMENT, 0x8215, PT_BOOLEAN),
    Key::Named(PSETID_APPOINTMENT, 0x8216, PT_BINARY),
    Key::Named(PSETID_APPOINTMENT, 0x8217, PT_LONG),
    Key::Named(PSETID_APPOINTMENT, 0x8218, PT_LONG),
    Key::Named(PSETID_APPOINTMENT, 0x8223, PT_BOOLEAN),
    Key::Named(PSETID_APPOINTMENT, 0x8233, PT_BINARY),
    Key::Named(PSETID_APPOINTMENT, 0x8234, PT_UNICODE),
    Key::Named(PSETID_COMMON, 0x8501, PT_LONG),
    Key::Named(PSETID_COMMON, 0x8503, PT_BOOLEAN),
    Key::Named(PSETID_MEETING, 0x0023, PT_BINARY),
    Key::Keywords,
];

impl Key {
    fn dasl(self) -> String {
        match self {
            Key::Fixed(tag, ty) => format!("http://schemas.microsoft.com/mapi/proptag/0x{tag:04X}{ty:04X}"),
            Key::Named(set, lid, ty) => format!("http://schemas.microsoft.com/mapi/id/{{{set}}}/{lid:04X}{ty:04X}"),
            Key::Keywords => format!("http://schemas.microsoft.com/mapi/string/{{{PS_PUBLIC_STRINGS}}}/Keywords"),
        }
    }

    /// Identifiant dans le sac de la source (propriétés nommées : table de la source).
    fn local_id(self, named: &Named) -> Option<u16> {
        match self {
            Key::Fixed(tag, _) => Some(tag),
            Key::Named(set, lid, _) => named.id(set, lid),
            Key::Keywords => named.name(PS_PUBLIC_STRINGS, "Keywords"),
        }
    }
}

/// Date OLE (jours depuis le 30/12/1899) → millisecondes Unix, sans changer de fuseau.
pub fn ole_to_ms(d: f64) -> i64 {
    ((d - 25_569.0) * 86_400_000.0).round() as i64
}

pub fn ms_to_ole(ms: i64) -> f64 {
    ms as f64 / 86_400_000.0 + 25_569.0
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemState {
    entry_id: String,
    folder: u32,
    /// Date de modification d'Outlook en millisecondes (heure locale) : un réel ne survit pas toujours exactement à
    /// l'aller-retour par JSON.
    modified: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct State {
    source_id: i64,
    store_path: String,
    /// NID → EntryID des calendriers liés.
    folders: BTreeMap<u32, String>,
    items: BTreeMap<u32, ItemState>,
    /// Fin de la dernière synchronisation réussie (millisecondes Unix).
    #[serde(default)]
    last_sync_ms: i64,
    /// NID du calendrier par défaut d'Outlook (celui qu'il propose pour un nouveau rendez-vous).
    #[serde(default)]
    default_folder: u32,
}

/// Calendriers communs avec Outlook, d'après la dernière synchronisation (sans ouvrir Outlook).
#[derive(Clone, Debug, Default)]
pub struct Linked {
    pub source_id: i64,
    /// NID des calendriers liés.
    pub folders: Vec<u32>,
    pub last_sync_ms: i64,
    pub default_folder: u32,
}

pub fn linked(data_dir: &Path) -> Option<Linked> {
    let s = load_state(data_dir)?;
    Some(Linked { source_id: s.source_id, folders: s.folders.keys().copied().collect(), last_sync_ms: s.last_sync_ms, default_folder: s.default_folder })
}

fn state_path(data_dir: &Path) -> PathBuf {
    data_dir.join(STATE_FILE)
}

fn load_state(data_dir: &Path) -> Option<State> {
    serde_json::from_str(&std::fs::read_to_string(state_path(data_dir)).ok()?).ok()
}

fn save_state(data_dir: &Path, s: &State) -> Result<()> {
    std::fs::write(state_path(data_dir), serde_json::to_string(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Bilan d'une synchronisation.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub store_path: String,
    pub source_id: i64,
    pub folders: Vec<String>,
    pub checked: usize,
    pub fetched: usize,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    /// Descriptions changées dans Outlook (nouveau `.eml`).
    pub bodies: usize,
    /// Essai à blanc : écarts entre Outlook et la base, par propriété (« 0x820D » …), sur les éléments relus.
    pub differences: BTreeMap<String, usize>,
    pub errors: Vec<String>,
    pub ms: u64,
}

/// NID d'un EntryID de PST (ses 4 derniers octets, petit-boutiste).
pub fn nid(entry_id: &str) -> Option<u32> {
    let h = entry_id.get(entry_id.len().checked_sub(8)?..)?;
    let b = u32::from_str_radix(h, 16).ok()?;
    Some(b.swap_bytes())
}

fn file_stem(p: &str) -> String {
    Path::new(p).file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Session Outlook : espace MAPI et fichier de données lié.
struct Session {
    ns: Disp,
    store: Disp,
    store_id: String,
}

fn calendars(store: &Disp) -> Result<Vec<Disp>> {
    let mut out = Vec::new();
    let mut stack = vec![store.obj("GetRootFolder")?];
    while let Some(f) = stack.pop() {
        let subs = f.obj("Folders")?;
        for i in 1..=subs.int("Count")? {
            stack.push(subs.obj_with("Item", &[VARIANT::from(i as i32)])?);
        }
        if f.int("DefaultItemType")? == i64::from(OL_APPOINTMENT) {
            out.push(f);
        }
    }
    Ok(out)
}

/// Fichier de données d'Outlook lié à une source PST de la base : nom du fichier d'origine de la source commençant par
/// celui du PST d'Outlook (`Outlook_300923_BACKUP_….pst` ↔ `Outlook_300923.pst`), et calendrier par défaut connu de
/// la source sous le même NID.
fn link(conn: &Connection, ns: &Disp, previous: Option<&State>) -> Result<(Session, i64, u32)> {
    let stores = ns.obj("Stores")?;
    let sources: Vec<(i64, String)> = conn
        .prepare("SELECT id, origin_path FROM source WHERE kind = 'pst'")
        .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
        .map_err(|e| e.to_string())?;
    for i in 1..=stores.int("Count")? {
        let store = stores.obj_with("Item", &[VARIANT::from(i as i32)])?;
        let path = store.string("FilePath").unwrap_or_default();
        if !path.to_lowercase().ends_with(".pst") {
            continue;
        }
        let stem = file_stem(&path);
        for (source_id, origin) in &sources {
            if previous.is_some_and(|p| p.source_id != *source_id) || !file_stem(origin).starts_with(&stem) {
                continue;
            }
            let Ok(cal) = store.obj_with("GetDefaultFolder", &[VARIANT::from(9i32)]) else { continue };
            let Some(cal_nid) = nid(&cal.string("EntryID")?) else { continue };
            let known: Option<i64> = conn
                .query_row("SELECT id FROM folder WHERE source_id = ?1 AND nid = ?2", params![source_id, cal_nid], |r| r.get(0))
                .optional()
                .map_err(|e| e.to_string())?;
            if known.is_some() {
                let store_id = store.string("StoreID")?;
                return Ok((Session { ns: ns.clone(), store, store_id }, *source_id, cal_nid));
            }
        }
    }
    Err("aucun fichier de données ouvert dans Outlook ne correspond à un PST importé dans ThunderLook".into())
}

/// Ce qu'Outlook dit d'un rendez-vous.
struct Fetched {
    folder_id: i64,
    nid: u32,
    entry_id: String,
    modified: i64,
    class: String,
    subject: String,
    organizer: String,
    attendees: String,
    sensitivity: i64,
    importance: i64,
    body: String,
    /// (identifiant dans le sac, valeur ; `None` : absente d'Outlook).
    props: Vec<(u16, Option<Value>)>,
    modified_ms: Option<i64>,
}

fn fetch(s: &Session, named: &Named, folder_id: i64, nid: u32, entry_id: &str, modified: i64) -> Result<Fetched> {
    let item = s.ns.obj_with("GetItemFromID", &[VARIANT::from(entry_id), VARIANT::from(s.store_id.as_str())])?;
    // Toutes les propriétés brutes, par un `.msg` temporaire (`olMSGUnicode`) : l'élément n'est pas modifié.
    let tmp = std::env::temp_dir().join("thunderlook-outlook");
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let path = tmp.join(format!("{nid:08x}.msg"));
    item.call("SaveAs", &[VARIANT::from(path.to_string_lossy().as_ref()), VARIANT::from(9i32)])?;
    let read = crate::msg::read(&path, named);
    let _ = std::fs::remove_file(&path);
    let (mut found, _unknown) = read?;
    // L'export `.msg` date l'élément de l'export (0x3007, 0x3008) et omet les identifiants globaux (0x0003, 0x0023) :
    // ceux-là viennent de `PropertyAccessor`, qui les rend (dates en UTC).
    found.retain(|(id, _)| *id != 0x3007 && *id != 0x3008);
    let pa = item.obj("PropertyAccessor")?;
    let mut modified_ms = None;
    for k in [Key::Fixed(0x3007, PT_SYSTIME), Key::Fixed(0x3008, PT_SYSTIME), Key::Named(PSETID_MEETING, 0x0003, PT_BINARY), Key::Named(PSETID_MEETING, 0x0023, PT_BINARY)] {
        let Some(id) = k.local_id(named) else { continue };
        let Ok(v) = pa.call("GetProperty", &[VARIANT::from(k.dasl().as_str())]) else { continue };
        let value = match k {
            Key::Fixed(tag, _) => com::as_date(&v).map(|d| {
                let ms = ole_to_ms(d);
                if tag == 0x3008 {
                    modified_ms = Some(ms);
                }
                Value::time_ms(ms)
            }),
            _ => com::as_bytes(&v).map(Value::Binary),
        };
        if let Some(value) = value {
            found.push((id, value));
        }
    }
    // Propriétés du calendrier absentes de l'élément : retirées du sac (périodicité supprimée, rappel retiré…).
    let mut props: Vec<(u16, Option<Value>)> =
        KEYS.iter().filter_map(|k| k.local_id(named)).filter(|id| !found.iter().any(|(f, _)| f == id)).map(|id| (id, None)).collect();
    props.extend(found.into_iter().map(|(id, v)| (id, Some(v))));
    let attendees = [item.string("RequiredAttendees").unwrap_or_default(), item.string("OptionalAttendees").unwrap_or_default()]
        .into_iter()
        .filter(|a| !a.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    Ok(Fetched {
        folder_id,
        nid,
        entry_id: entry_id.to_string(),
        modified,
        class: item.string("MessageClass")?,
        subject: item.string("Subject")?,
        organizer: item.string("Organizer").unwrap_or_default(),
        attendees,
        sensitivity: item.int("Sensitivity").unwrap_or(0),
        importance: item.int("Importance").unwrap_or(1),
        body: item.string("Body").unwrap_or_default(),
        props,
        modified_ms,
    })
}

fn eml_text(data_dir: &Path, rel: &str) -> String {
    std::fs::read(thunderlook_store::resolve(data_dir, rel))
        .ok()
        .and_then(|b| mail_parser::MessageParser::default().parse(&b).and_then(|m| m.body_text(0).map(|t| t.into_owned())))
        .unwrap_or_default()
}

fn same_text(a: &str, b: &str) -> bool {
    let n = |s: &str| s.replace("\r\n", "\n").trim().to_string();
    n(a) == n(b)
}

/// `.eml` minimal d'un rendez-vous relu dans Outlook : sa description en texte.
fn write_eml(data_dir: &Path, source_id: i64, nid: u32, subject: &str, body: &str) -> Result<String> {
    let rel = format!("eml/{source_id}/outlook/{nid}.eml");
    let path = thunderlook_store::resolve(data_dir, &rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let subject = subject.replace(['\r', '\n'], " ");
    let text = format!("MIME-Version: 1.0\r\nSubject: {subject}\r\nContent-Type: text/plain; charset=\"utf-8\"\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{body}");
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(rel)
}

/// Synchronise les calendriers liés (tous, ou le seul dossier `only`, NID d'Outlook). `dry_run` : rien n'est écrit,
/// le bilan donne les écarts par propriété.
fn sync_in(s: &Session, conn: &mut Connection, data_dir: &Path, source_id: i64, state: &mut State, only: Option<u32>, dry_run: bool) -> Result<SyncReport> {
    let t0 = Instant::now();
    let e = |x: thunderlook_store::rusqlite::Error| x.to_string();
    let raw_named: Vec<u8> = conn.query_row("SELECT props FROM source WHERE id = ?1", [source_id], |r| r.get::<_, Option<Vec<u8>>>(0)).map_err(e)?.unwrap_or_default();
    let named = Named::parse(&raw_named)?;
    let mut report = SyncReport { store_path: state.store_path.clone(), source_id, ..Default::default() };
    let mut fetched = Vec::new();
    let mut seen: HashMap<i64, Vec<u32>> = HashMap::new();
    for cal in calendars(&s.store)? {
        let entry = cal.string("EntryID")?;
        let Some(fnid) = nid(&entry) else { continue };
        if only.is_some_and(|o| o != fnid) {
            continue;
        }
        let name = cal.string("Name")?;
        let Some(folder_id) = conn
            .query_row("SELECT id FROM folder WHERE source_id = ?1 AND nid = ?2", params![source_id, fnid], |r| r.get::<_, i64>(0))
            .optional()
            .map_err(e)?
        else {
            report.errors.push(format!("calendrier « {name} » créé dans Outlook après l'import : pas encore repris"));
            continue;
        };
        state.folders.insert(fnid, entry);
        report.folders.push(name);
        let in_db: HashMap<u32, i64> = conn
            .prepare("SELECT nid, id FROM message WHERE folder_id = ?1")
            .and_then(|mut st| st.query_map([folder_id], |r| Ok((r.get::<_, u32>(0)?, r.get::<_, i64>(1)?)))?.collect())
            .map_err(e)?;
        let table = cal.obj("GetTable")?;
        let ids = seen.entry(folder_id).or_default();
        while !com::as_bool(&table.get("EndOfTable")?).unwrap_or(true) {
            let row = table.obj("GetNextRow")?;
            let entry_id = com::as_string(&row.get_with("Item", &[VARIANT::from("EntryID")])?);
            let modified = com::as_date(&row.get_with("Item", &[VARIANT::from("LastModificationTime")])?).map(ole_to_ms).unwrap_or(0);
            let Some(item_nid) = nid(&entry_id) else { continue };
            report.checked += 1;
            ids.push(item_nid);
            let known = state.items.get(&item_nid).is_some_and(|i| i.modified == modified && i.entry_id == entry_id);
            if known && in_db.contains_key(&item_nid) && !dry_run {
                continue;
            }
            if std::env::var_os("TL_OUTLOOK_DEBUG").is_some() {
                let st = state.items.get(&item_nid);
                eprintln!(
                    "relu {item_nid:#x} : état {:?} / table {modified} ; EntryID identique {} ; en base {}",
                    st.map(|s| s.modified),
                    st.is_some_and(|s| s.entry_id == entry_id),
                    in_db.contains_key(&item_nid)
                );
            }
            match fetch(s, &named, folder_id, item_nid, &entry_id, modified) {
                Ok(f) => fetched.push(f),
                Err(x) => report.errors.push(format!("rendez-vous {item_nid:#x} : {x}")),
            }
        }
    }
    report.fetched = fetched.len();

    let tx = conn.transaction().map_err(e)?;
    let mut old_emls = Vec::new();
    // Rendez-vous déjà dans la base : seules les propriétés lues par le calendrier sont remplacées, le reste du sac
    // importé est gardé.
    let calendar_ids: Vec<u16> = KEYS.iter().filter_map(|k| k.local_id(&named)).collect();
    let only_calendar = |f: &Fetched| -> Vec<(u16, Option<Value>)> { f.props.iter().filter(|(id, _)| calendar_ids.contains(id)).cloned().collect() };
    for f in &fetched {
        let row: Option<(i64, Vec<u8>, String)> = tx
            .query_row("SELECT id, props, eml_path FROM message WHERE folder_id = ?1 AND nid = ?2", params![f.folder_id, f.nid], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()
            .map_err(e)?;
        if dry_run {
            if let Some((_, old, _)) = &row {
                if !same_text(&eml_text(data_dir, &row.as_ref().map(|r| r.2.clone()).unwrap_or_default()), &f.body) {
                    report.bodies += 1;
                }
                // Propriétés du calendrier, valeurs décodées : un texte 8 bits du PST égale le même texte Unicode.
                let bag = Bag::parse(old)?;
                let merged = thunderlook_calendar::bag::merge(old, &only_calendar(f))?;
                let now = Bag::parse(&merged)?;
                for k in KEYS {
                    let Some(id) = k.local_id(&named) else { continue };
                    let is_text = |t: u16| t == 0x001E || t == 0x001F || t == 0x0001;
                    // Texte vide ou nul, marqueur de préfixe de l'objet (« \x01\x01 ») : même valeur.
                    let text = |b: &Bag| b.text(id).map(|s| thunderlook_store::strip_subject_marker(&s).to_string()).unwrap_or_default();
                    let same = match (bag.raw(id), now.raw(id)) {
                        (None, None) => true,
                        (Some((ta, a)), Some((tb, b))) if is_text(ta) && is_text(tb) => text(&bag) == text(&now) || a == b,
                        (a, b) => a == b,
                    };
                    if !same {
                        let name = k.dasl().rsplit('/').next().unwrap_or_default().to_string();
                        *report.differences.entry(name.clone()).or_default() += 1;
                        if report.errors.len() < 12 {
                            let show = |x: Option<(u16, &[u8])>| x.map(|(t, b)| format!("{t:#06x} {}", b.iter().take(24).map(|y| format!("{y:02x}")).collect::<String>()));
                            report.errors.push(format!("écart {name} sur « {} » : base {:?} / Outlook {:?}", f.subject, show(bag.raw(id)), show(now.raw(id))));
                        }
                    }
                }
            }
            continue;
        }
        match row {
            Some((id, old, eml)) => {
                let props = thunderlook_calendar::bag::merge(&old, &only_calendar(f))?;
                // Description changée dans Outlook : nouveau `.eml` ; celui de l'import reste sur le disque (il peut
                // porter des pièces jointes).
                let eml = if same_text(&eml_text(data_dir, &eml), &f.body) {
                    eml
                } else {
                    report.bodies += 1;
                    write_eml(data_dir, source_id, f.nid, &f.subject, &f.body)?
                };
                tx.execute(
                    "UPDATE message SET message_class = ?2, subject = ?3, from_name = ?4, to_line = ?5, sensitivity = ?6, importance = ?7,
                        modified_at = COALESCE(?8, modified_at), preview = ?9, eml_path = ?10, props = ?11, indexed = 0 WHERE id = ?1",
                    params![id, f.class, f.subject, f.organizer, f.attendees, f.sensitivity, f.importance, f.modified_ms, preview(&f.body), eml, props],
                )
                .map_err(e)?;
                tx.execute("DELETE FROM message_fts WHERE rowid = ?1", [id]).map_err(e)?;
                report.updated += 1;
            }
            None => {
                let props = thunderlook_calendar::bag::merge(&[], &f.props)?;
                let eml = write_eml(data_dir, source_id, f.nid, &f.subject, &f.body)?;
                tx.execute(
                    "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, created_at, modified_at,
                        received_at, message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview,
                        eml_path, props)
                     VALUES (?1, ?2, ?3, ?4, ?5, '', ?6, '', ?7, ?7, ?7, 0, 1, 0, ?8, ?9, 0, 0, 'text', ?10, ?11, ?12)",
                    params![f.folder_id, f.nid, f.class, f.subject, f.organizer, f.attendees, f.modified_ms, f.importance, f.sensitivity, preview(&f.body), eml, props],
                )
                .map_err(e)?;
                report.added += 1;
            }
        }
        state.items.insert(f.nid, ItemState { entry_id: f.entry_id.clone(), folder: 0, modified: f.modified });
    }
    if !dry_run {
        // Disparus d'Outlook (supprimés ou déplacés hors des calendriers liés).
        for (folder_id, ids) in &seen {
            let gone: Vec<(i64, u32, String)> = tx
                .prepare("SELECT id, nid, eml_path FROM message WHERE folder_id = ?1")
                .and_then(|mut st| st.query_map([folder_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<std::result::Result<Vec<_>, _>>())
                .map_err(e)?
                .into_iter()
                .filter(|(_, n, _)| !ids.contains(n))
                .collect();
            for (id, n, eml) in gone {
                tx.execute("DELETE FROM message_fts WHERE rowid = ?1", [id]).map_err(e)?;
                tx.execute("DELETE FROM message WHERE id = ?1", [id]).map_err(e)?;
                state.items.remove(&n);
                old_emls.push(eml);
                report.removed += 1;
            }
        }
        tx.commit().map_err(e)?;
        for rel in old_emls {
            // Les `.eml` d'origine (import) et ceux d'Outlook ont des chemins distincts ; un chemin réécrit sur
            // place (`eml/<source>/outlook/<nid>.eml`) n'est pas effacé.
            if !rel.contains("/outlook/") {
                let _ = std::fs::remove_file(thunderlook_store::resolve(data_dir, &rel));
            }
        }
        state.last_sync_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
        save_state(data_dir, state)?;
    }
    report.ms = t0.elapsed().as_millis() as u64;
    Ok(report)
}

fn preview(body: &str) -> String {
    body.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect()
}

/// Ouvre la session Outlook liée au dossier de données, et l'état gardé.
fn open(conn: &Connection, data_dir: &Path) -> Result<(Session, State)> {
    let previous = load_state(data_dir);
    let ol = com::outlook()?;
    let ns = ol.obj_with("GetNamespace", &[VARIANT::from("MAPI")])?;
    let (session, source_id, default_folder) = link(conn, &ns, previous.as_ref())?;
    let store_path = session.store.string("FilePath")?;
    let mut state = match previous {
        Some(p) if p.source_id == source_id && p.store_path == store_path => p,
        _ => State { source_id, store_path, ..Default::default() },
    };
    state.default_folder = default_folder;
    Ok((session, state))
}

/// Synchronisation des calendriers liés (Outlook doit être ouvert).
pub fn sync(data_dir: &Path, dry_run: bool) -> Result<SyncReport> {
    let dir = data_dir.to_path_buf();
    com::with_com(move || {
        let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
        let (s, mut state) = open(&conn, &dir)?;
        let source_id = state.source_id;
        sync_in(&s, &mut conn, &dir, source_id, &mut state, None, dry_run)
    })
}

/// Rendez-vous saisi dans ThunderLook. Dates locales (heure de Paris) `AAAA-MM-JJTHH:MM`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edit {
    pub subject: String,
    pub location: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
    /// Minutes avant le début ; `None` : pas de rappel.
    pub reminder: Option<i32>,
    /// `olFree` 0, `olTentative` 1, `olBusy` 2, `olOutOfOffice` 3, `olWorkingElsewhere` 4.
    pub busy: i32,
    pub body: String,
    /// Catégories séparées par des virgules, comme `Categories` d'Outlook.
    pub categories: String,
    pub private: bool,
}

/// `AAAA-MM-JJTHH:MM` (heure locale) → date OLE locale, comme l'attend le modèle objet d'Outlook.
fn local_ole(s: &str) -> Result<f64> {
    let bad = || format!("date invalide : {s}");
    let (d, t) = s.split_once('T').unwrap_or((s, "00:00"));
    let mut p = d.split('-').map(|x| x.parse::<i64>());
    let (y, m, day) = (p.next().ok_or_else(bad)?.map_err(|_| bad())?, p.next().ok_or_else(bad)?.map_err(|_| bad())?, p.next().ok_or_else(bad)?.map_err(|_| bad())?);
    let (h, mi) = t.split_once(':').ok_or_else(bad)?;
    let (h, mi): (i64, i64) = (h.parse().map_err(|_| bad())?, mi.get(..2).unwrap_or(mi).parse().map_err(|_| bad())?);
    // Jours depuis 1970-01-01 (H. Hinnant), puis décalage de l'époque OLE.
    let yy = if m <= 2 { y - 1 } else { y };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Ok(days as f64 + 25_569.0 + (h * 60 + mi) as f64 / 1440.0)
}

fn apply(item: &Disp, edit: &Edit, times: bool) -> Result<()> {
    item.put("Subject", VARIANT::from(edit.subject.as_str()))?;
    item.put("Location", VARIANT::from(edit.location.as_str()))?;
    if times {
        item.put("AllDayEvent", VARIANT::from(edit.all_day))?;
        item.put("Start", com::date(local_ole(&edit.start)?))?;
        item.put("End", com::date(local_ole(&edit.end)?))?;
    }
    item.put("ReminderSet", VARIANT::from(edit.reminder.is_some()))?;
    if let Some(m) = edit.reminder {
        item.put("ReminderMinutesBeforeStart", VARIANT::from(m))?;
    }
    item.put("BusyStatus", VARIANT::from(edit.busy))?;
    item.put("Body", VARIANT::from(edit.body.as_str()))?;
    item.put("Categories", VARIANT::from(edit.categories.as_str()))?;
    // `olPrivate` 2, `olNormal` 0.
    item.put("Sensitivity", VARIANT::from(if edit.private { 2i32 } else { 0 }))?;
    Ok(())
}

/// Écrit dans Outlook puis resynchronise le calendrier concerné.
fn write(data_dir: &Path, f: impl FnOnce(&Session, &Connection, &State) -> Result<u32> + Send + 'static) -> Result<SyncReport> {
    let dir = data_dir.to_path_buf();
    com::with_com(move || {
        let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
        let (s, mut state) = open(&conn, &dir)?;
        // L'état doit connaître les EntryID : première synchronisation au besoin.
        if state.items.is_empty() {
            let source_id = state.source_id;
            sync_in(&s, &mut conn, &dir, source_id, &mut state, None, false)?;
        }
        let folder = f(&s, &conn, &state)?;
        let source_id = state.source_id;
        sync_in(&s, &mut conn, &dir, source_id, &mut state, Some(folder), false)
    })
}

fn folder_nid(conn: &Connection, folder_id: i64) -> Result<u32> {
    conn.query_row("SELECT nid FROM folder WHERE id = ?1", [folder_id], |r| r.get(0)).map_err(|e| e.to_string())
}

/// (NID du dossier, NID et EntryID) d'un rendez-vous de la base.
fn item_of(conn: &Connection, state: &State, message_id: i64) -> Result<(u32, String)> {
    let (folder, n): (u32, u32) = conn
        .query_row("SELECT f.nid, m.nid FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1 AND f.source_id = ?2", params![message_id, state.source_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(|_| "ce rendez-vous n'est pas dans un calendrier partagé avec Outlook".to_string())?;
    let entry = state.items.get(&n).map(|i| i.entry_id.clone()).ok_or("rendez-vous inconnu d'Outlook : synchroniser d'abord")?;
    Ok((folder, entry))
}

/// Nouveau rendez-vous dans le calendrier `folder_id` (base).
pub fn create(data_dir: &Path, folder_id: i64, edit: Edit) -> Result<SyncReport> {
    write(data_dir, move |s, conn, state| {
        let fnid = folder_nid(conn, folder_id)?;
        let entry = state.folders.get(&fnid).ok_or("ce calendrier n'est pas partagé avec Outlook")?;
        let folder = s.ns.obj_with("GetFolderFromID", &[VARIANT::from(entry.as_str()), VARIANT::from(s.store_id.as_str())])?;
        let item = folder.obj("Items")?.obj_with("Add", &[VARIANT::from(OL_APPOINTMENT)])?;
        apply(&item, &edit, true)?;
        item.call("Save", &[])?;
        Ok(fnid)
    })
}

/// Modification d'un rendez-vous. Pour une série, les heures et la périodicité restent celles d'Outlook.
pub fn update(data_dir: &Path, message_id: i64, edit: Edit) -> Result<SyncReport> {
    write(data_dir, move |s, conn, state| {
        let (fnid, entry) = item_of(conn, state, message_id)?;
        let item = s.ns.obj_with("GetItemFromID", &[VARIANT::from(entry.as_str()), VARIANT::from(s.store_id.as_str())])?;
        let recurring = com::as_bool(&item.get("IsRecurring")?).unwrap_or(false);
        apply(&item, &edit, !recurring)?;
        item.call("Save", &[])?;
        Ok(fnid)
    })
}

/// Suppression d'un rendez-vous, ou d'une seule occurrence d'une série (`occurrence` : sa date d'origine,
/// `AAAA-MM-JJ` ; l'heure est celle du motif de la série). Outlook range l'élément supprimé dans ses Éléments
/// supprimés.
pub fn delete(data_dir: &Path, message_id: i64, occurrence: Option<String>) -> Result<SyncReport> {
    write(data_dir, move |s, conn, state| {
        let (fnid, entry) = item_of(conn, state, message_id)?;
        let item = s.ns.obj_with("GetItemFromID", &[VARIANT::from(entry.as_str()), VARIANT::from(s.store_id.as_str())])?;
        match occurrence {
            Some(day) if com::as_bool(&item.get("IsRecurring")?).unwrap_or(false) => {
                let pattern = item.obj("GetRecurrencePattern")?;
                let time = com::as_date(&pattern.get("StartTime")?).unwrap_or(0.0).fract();
                let occ = pattern.obj_with("GetOccurrence", &[com::date(local_ole(&format!("{day}T00:00"))? + time)])?;
                occ.call("Delete", &[])?;
            }
            _ => {
                item.call("Delete", &[])?;
            }
        }
        Ok(fnid)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nid_from_entry_id() {
        assert_eq!(nid("00000000ABCDEF0102030405060708090A0B0C0D0E0F1002870000"), Some(0x8702));
    }

    #[test]
    fn local_dates() {
        assert_eq!(local_ole("1899-12-30T00:00").unwrap(), 0.0);
        assert_eq!(local_ole("2026-10-22T12:00").unwrap(), 46_317.5);
        assert_eq!(ole_to_ms(25_569.0), 0);
    }
}
