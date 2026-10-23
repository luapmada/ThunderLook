//! Contacts communs avec Outlook (J10) : Outlook garde les contacts (son PST, que MPE synchronise avec le téléphone) ;
//! ThunderLook les lit en direct, comme le calendrier (`crate::calendar`) : la table de chaque dossier de contacts
//! désigne les contacts nouveaux ou modifiés, relus par un `.msg` temporaire ; ceux qui ont disparu d'Outlook quittent
//! la base. Les notes et la photo (`ContactPicture.jpg`) sont reprises dans le `.eml` du contact.
//! L'état (EntryID et date de modification par NID) est gardé dans `outlook-contacts.json`.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    time::Instant,
};

use base64::Engine;
use serde::{Deserialize, Serialize};
use thunderlook_calendar::named::Named;
use thunderlook_store::rusqlite::{OptionalExtension, params};
use windows::Win32::System::Variant::VARIANT;

use crate::{
    calendar::{Fetched, Session, State as CalState, fetch, link, nid, ole_to_ms, preview},
    com::{self, Disp, Result},
};

const STATE_FILE: &str = "outlook-contacts.json";
/// `olContactItem` (DefaultItemType d'un dossier de contacts).
const OL_CONTACT: i32 = 2;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemState {
    entry_id: String,
    modified: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct State {
    source_id: i64,
    store_path: String,
    items: BTreeMap<u32, ItemState>,
    #[serde(default)]
    last_sync_ms: i64,
    /// NID des dossiers de contacts suivis.
    #[serde(default)]
    folders: Vec<u32>,
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

/// Dernière synchronisation réussie : (source, NID des dossiers suivis, millisecondes Unix).
pub fn linked(data_dir: &Path) -> Option<(i64, Vec<u32>, i64)> {
    load_state(data_dir).filter(|s| s.last_sync_ms > 0).map(|s| (s.source_id, s.folders, s.last_sync_ms))
}

/// Bilan d'une synchronisation.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub folders: Vec<String>,
    pub checked: usize,
    pub fetched: usize,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub errors: Vec<String>,
    pub ms: u64,
}

fn contact_folders(store: &Disp) -> Result<Vec<Disp>> {
    let mut out = Vec::new();
    let mut stack = vec![store.obj("GetRootFolder")?];
    while let Some(f) = stack.pop() {
        let subs = f.obj("Folders")?;
        for i in 1..=subs.int("Count")? {
            stack.push(subs.obj_with("Item", &[VARIANT::from(i as i32)])?);
        }
        if f.int("DefaultItemType")? == i64::from(OL_CONTACT) {
            out.push(f);
        }
    }
    Ok(out)
}

/// « Classer sous » (Outlook le calcule quand l'export `.msg` ne le porte pas) et photo du contact (pièce jointe
/// `ContactPicture.jpg`, enregistrée par Outlook dans un fichier temporaire).
fn extras(s: &Session, entry_id: &str, nid: u32) -> (Option<String>, Option<Vec<u8>>) {
    let Ok(item) = s.ns.obj_with("GetItemFromID", &[VARIANT::from(entry_id), VARIANT::from(s.store_id.as_str())]) else { return (None, None) };
    (item.string("FileAs").ok().filter(|f| !f.is_empty()), picture(&item, nid))
}

fn picture(item: &Disp, nid: u32) -> Option<Vec<u8>> {
    let atts = item.obj("Attachments").ok()?;
    for i in 1..=atts.int("Count").ok()? {
        let a = atts.obj_with("Item", &[VARIANT::from(i as i32)]).ok()?;
        if !a.string("FileName").is_ok_and(|n| n.eq_ignore_ascii_case("ContactPicture.jpg")) {
            continue;
        }
        let path = std::env::temp_dir().join("thunderlook-outlook").join(format!("{nid:08x}.jpg"));
        std::fs::create_dir_all(path.parent()?).ok()?;
        a.call("SaveAsFile", &[VARIANT::from(path.to_string_lossy().as_ref())]).ok()?;
        let bytes = std::fs::read(&path).ok();
        let _ = std::fs::remove_file(&path);
        return bytes;
    }
    None
}

/// `.eml` d'un contact : ses notes en texte, et sa photo en pièce jointe.
fn write_eml(data_dir: &Path, source_id: i64, nid: u32, subject: &str, body: &str, photo: Option<&[u8]>) -> Result<String> {
    let rel = format!("eml/{source_id}/outlook/{nid}.eml");
    let path = thunderlook_store::resolve(data_dir, &rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let subject = subject.replace(['\r', '\n'], " ");
    let text = match photo {
        None => format!("MIME-Version: 1.0\r\nSubject: {subject}\r\nContent-Type: text/plain; charset=\"utf-8\"\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{body}"),
        Some(jpg) => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(jpg);
            let lines = b64.as_bytes().chunks(76).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join("\r\n");
            format!(
                "MIME-Version: 1.0\r\nSubject: {subject}\r\nContent-Type: multipart/mixed; boundary=\"tl-contact\"\r\n\r\n--tl-contact\r\nContent-Type: text/plain; charset=\"utf-8\"\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{body}\r\n--tl-contact\r\nContent-Type: image/jpeg; name=\"ContactPicture.jpg\"\r\nContent-Disposition: attachment; filename=\"ContactPicture.jpg\"\r\nContent-Transfer-Encoding: base64\r\n\r\n{lines}\r\n--tl-contact--\r\n"
            )
        }
    };
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(rel)
}

/// Synchronise les dossiers de contacts d'Outlook avec la base (Outlook doit être ouvert).
pub fn sync(data_dir: &Path) -> Result<SyncReport> {
    let dir = data_dir.to_path_buf();
    com::with_com(move || {
        let t0 = Instant::now();
        let e = |x: thunderlook_store::rusqlite::Error| x.to_string();
        let mut conn = thunderlook_store::open(&dir).map_err(|x| x.to_string())?;
        let ns = com::outlook()?.obj_with("GetNamespace", &[VARIANT::from("MAPI")])?;
        let (s, source_id, _) = link(&conn, &ns, None::<&CalState>)?;
        let store_path = s.store.string("FilePath")?;
        let mut state = match load_state(&dir) {
            Some(p) if p.source_id == source_id && p.store_path == store_path => p,
            _ => State { source_id, store_path, ..Default::default() },
        };
        let raw_named: Vec<u8> = conn.query_row("SELECT props FROM source WHERE id = ?1", [source_id], |r| r.get::<_, Option<Vec<u8>>>(0)).map_err(e)?.unwrap_or_default();
        let named = Named::parse(&raw_named)?;
        let tracked = thunderlook_contacts::tracked_ids(&named);
        let mut report = SyncReport::default();
        let mut fetched: Vec<Fetched> = Vec::new();
        let mut seen: HashMap<i64, Vec<u32>> = HashMap::new();
        state.folders.clear();
        for folder in contact_folders(&s.store)? {
            let entry = folder.string("EntryID")?;
            let Some(fnid) = nid(&entry) else { continue };
            let name = folder.string("Name")?;
            let Some(folder_id) = conn
                .query_row("SELECT id FROM folder WHERE source_id = ?1 AND nid = ?2", params![source_id, fnid], |r| r.get::<_, i64>(0))
                .optional()
                .map_err(e)?
            else {
                report.errors.push(format!("dossier « {name} » créé dans Outlook après l'import : pas encore repris"));
                continue;
            };
            report.folders.push(name);
            state.folders.push(fnid);
            // NID → (id en base, date de modification en base).
            let in_db: HashMap<u32, (i64, i64)> = conn
                .prepare("SELECT nid, id, COALESCE(modified_at, 0) FROM message WHERE folder_id = ?1")
                .and_then(|mut st| st.query_map([folder_id], |r| Ok((r.get::<_, u32>(0)?, (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))))?.collect())
                .map_err(e)?;
            let table = folder.obj("GetTable")?;
            let ids = seen.entry(folder_id).or_default();
            while !com::as_bool(&table.get("EndOfTable")?).unwrap_or(true) {
                let row = table.obj("GetNextRow")?;
                let entry_id = com::as_string(&row.get_with("Item", &[VARIANT::from("EntryID")])?);
                let modified = com::as_date(&row.get_with("Item", &[VARIANT::from("LastModificationTime")])?).map(ole_to_ms).unwrap_or(0);
                let Some(item_nid) = nid(&entry_id) else { continue };
                report.checked += 1;
                ids.push(item_nid);
                let known_state = state.items.get(&item_nid).is_some_and(|i| i.modified == modified && i.entry_id == entry_id);
                // Première synchronisation : un contact dont la date de modification en base (UTC) égale celle d'Outlook
                // (heure locale) à l'heure d'été ou d'hiver près n'a pas changé depuis l'import.
                let same_as_import = in_db.get(&item_nid).is_some_and(|(_, at)| {
                    let d = modified - at;
                    [0, 3_600_000, 7_200_000].iter().any(|off| (d - off).abs() <= 2_000)
                });
                let forced = std::env::var("TL_CONTACTS_FORCE").ok().and_then(|v| v.parse::<u32>().ok()) == Some(item_nid);
                if !forced && in_db.contains_key(&item_nid) && (known_state || same_as_import) {
                    state.items.insert(item_nid, ItemState { entry_id, modified });
                    continue;
                }
                match fetch(&s, &named, folder_id, item_nid, &entry_id, modified) {
                    Ok(f) => {
                        if forced {
                            eprintln!("props {item_nid}: {:?}", f.props.iter().map(|(i, v)| format!("{i:#06x}{}", if v.is_some() { "" } else { "-" })).collect::<Vec<_>>());
                        }
                        fetched.push(f)
                    }
                    Err(x) => report.errors.push(format!("contact {item_nid:#x} : {x}")),
                }
            }
        }
        report.fetched = fetched.len();

        let tx = conn.transaction().map_err(e)?;
        for f in &fetched {
            let (file_as, jpg) = extras(&s, &f.entry_id, f.nid);
            let row: Option<(i64, Vec<u8>)> = tx
                .query_row("SELECT id, props FROM message WHERE folder_id = ?1 AND nid = ?2", params![f.folder_id, f.nid], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()
                .map_err(e)?;
            // Propriétés lues par ThunderLook : celles d'Outlook, et l'absence de celles qu'Outlook n'a plus ; le reste du
            // sac importé est gardé.
            let mut set: Vec<(u16, Option<thunderlook_calendar::bag::Value>)> = f.props.iter().filter(|(id, _)| tracked.contains(id)).cloned().collect();
            // « Classer sous » : valeur d'Outlook (jeu d'adresses, propriété 0x8005).
            if let (Some(text), Some(id)) = (file_as, named.id("00062004-0000-0000-C000-000000000046", 0x8005)) {
                set.retain(|(i, _)| *i != id);
                set.push((id, Some(thunderlook_calendar::bag::Value::Unicode(text))));
            }
            for id in &tracked {
                if !set.iter().any(|(i, _)| i == id) {
                    set.push((*id, None));
                }
            }
            let eml = write_eml(&dir, source_id, f.nid, &f.subject, &f.body, jpg.as_deref())?;
            match row {
                Some((id, old)) => {
                    let props = thunderlook_calendar::bag::merge(&old, &set)?;
                    tx.execute(
                        "UPDATE message SET subject = ?2, modified_at = COALESCE(?3, modified_at), preview = ?4, eml_path = ?5, props = ?6, indexed = 0 WHERE id = ?1",
                        params![id, f.subject, f.modified_ms, preview(&f.body), eml, props],
                    )
                    .map_err(e)?;
                    tx.execute("DELETE FROM message_fts WHERE rowid = ?1", [id]).map_err(e)?;
                    report.updated += 1;
                }
                None => {
                    let props = thunderlook_calendar::bag::merge(&[], &f.props)?;
                    tx.execute(
                        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, created_at, modified_at,
                            received_at, message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview,
                            eml_path, props)
                         VALUES (?1, ?2, ?3, ?4, '', '', '', '', ?5, ?5, ?5, 0, 1, 0, ?6, ?7, 0, 0, 'text', ?8, ?9, ?10)",
                        params![f.folder_id, f.nid, f.class, f.subject, f.modified_ms, f.importance, f.sensitivity, preview(&f.body), eml, props],
                    )
                    .map_err(e)?;
                    report.added += 1;
                }
            }
            state.items.insert(f.nid, ItemState { entry_id: f.entry_id.clone(), modified: f.modified });
        }
        // Disparus d'Outlook (supprimés ou déplacés hors des dossiers de contacts).
        for (folder_id, ids) in &seen {
            let gone: Vec<(i64, u32)> = tx
                .prepare("SELECT id, nid FROM message WHERE folder_id = ?1 AND (message_class = 'IPM.Contact' OR message_class LIKE 'IPM.Contact.%')")
                .and_then(|mut st| st.query_map([folder_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<std::result::Result<Vec<_>, _>>())
                .map_err(e)?
                .into_iter()
                .filter(|(_, n)| !ids.contains(n))
                .collect();
            for (id, n) in gone {
                tx.execute("DELETE FROM message_fts WHERE rowid = ?1", [id]).map_err(e)?;
                tx.execute("DELETE FROM message WHERE id = ?1", [id]).map_err(e)?;
                state.items.remove(&n);
                report.removed += 1;
            }
        }
        tx.commit().map_err(e)?;
        state.last_sync_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
        save_state(&dir, &state)?;
        report.ms = t0.elapsed().as_millis() as u64;
        Ok(report)
    })
}
