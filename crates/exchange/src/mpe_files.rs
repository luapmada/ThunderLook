//! Lot AI : lecture, en LECTURE SEULE, des fichiers de données que MyPhoneExplorer garde pour chaque appareil dans
//! `%APPDATA%\MyPhoneExplorer\<appareil>\` : `Phonebook.dat` (vCard 2.1), `Organizer.dat` (vCalendar 1.0) et `Notes.dat`
//! (VNOTE). Ils reflètent le téléphone à la dernière synchro de MPE ; sens téléphone → ThunderLook seulement.
//!
//! Garanties :
//! - le dossier de MPE n'est jamais écrit, renommé ni supprimé ; les fichiers sont ouverts en lecture seule, copiés en
//!   mémoire, puis analysés sur des copies placées dans un dossier temporaire à nous ; MPE n'est ni lancé ni piloté ;
//! - un fichier absent est signalé, pas une erreur ; un fichier tronqué, vide, ou modifié pendant la lecture (MPE est en
//!   train de l'écrire) fait refuser TOUTE la lecture ;
//! - rien n'est appliqué : le résultat est un [`ImportReport`] ordinaire, que `apply` n'applique que sur choix.
//!
//! Même circuit que l'échange par fichiers : les copies sont présentées à [`crate::import_changes_with`] sous les noms de
//! l'échange (`contacts.vcf`, `notes.vnt`, `calendrier.ics`), avec un manifeste synthétique. Correspondance : identifiant
//! `X-IRMC-LUID` du téléphone (retenu par `apply` dans `exchange-map.json`, sous la forme `mpe-<appareil>-<LUID>` pour ne
//! jamais se mêler aux identifiants de l'échange par fichiers), puis nom + e-mail / objet + début.
//!
//! - Suppressions : proposées seulement pour un élément déjà relié à un LUID de CET appareil et absent du fichier ; jamais
//!   cochées. Notes : le téléphone est la source (lot AG). Calendrier : seule la plage −90 / +90 jours est regardée, ni
//!   modification ni suppression hors plage (les séries ne sont jamais proposées à la suppression).
//! - Sans valeur exportée connue, tout écart d'un contact ou d'un rendez-vous déjà relié est un conflit « origine
//!   inconnue » : rien n'est appliqué sans choix.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde::Serialize;

use crate::{
    CALENDAR_FILE, CONTACTS_FILE, Change, ChangeKind, FileInfo, ImportOptions, ImportReport, ItemKind, MANIFEST_FILE, Manifest, NOTES_FILE, Payload, Result,
    import_changes_with, now_stamp, read_map, write_atomic,
};

pub const PHONEBOOK_DAT: &str = "Phonebook.dat";
pub const ORGANIZER_DAT: &str = "Organizer.dat";
pub const NOTES_DAT: &str = "Notes.dat";

const DAT_FILES: [&str; 3] = [PHONEBOOK_DAT, ORGANIZER_DAT, NOTES_DAT];

/// Un appareil connu de MPE : un dossier qui contient au moins un des trois fichiers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub name: String,
    pub path: String,
    /// Fichiers présents parmi `Phonebook.dat`, `Organizer.dat`, `Notes.dat`.
    pub files: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct MpeOptions {
    /// Un fichier modifié depuis moins longtemps que cela est refusé (MPE est peut-être en train de l'écrire).
    pub settle: Duration,
    /// Plage du calendrier regardée, en jours de part et d'autre d'aujourd'hui (MPE : 90).
    pub range_days: i64,
    /// « Maintenant » en millisecondes UTC ; `None` : l'heure réelle (les tests la fixent).
    pub now_ms: Option<i64>,
    /// Voir [`ImportOptions::notes_from_phone`] (vrai : le téléphone est la source des notes).
    pub notes_from_phone: bool,
}

impl Default for MpeOptions {
    fn default() -> Self {
        MpeOptions { settle: Duration::from_secs(3), range_days: 90, now_ms: None, notes_from_phone: true }
    }
}

/// `%APPDATA%\MyPhoneExplorer`, s'il existe.
pub fn default_root() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os("APPDATA")?).join("MyPhoneExplorer");
    p.is_dir().then_some(p)
}

fn files_in(dir: &Path) -> Vec<String> {
    DAT_FILES.iter().filter(|f| dir.join(f).is_file()).map(|f| f.to_string()).collect()
}

/// Appareils trouvés : `root` lui-même s'il contient déjà les fichiers (dossier d'appareil choisi directement), sinon ses
/// sous-dossiers qui en contiennent. Lecture de dossiers seulement.
pub fn list_devices(root: &Path) -> Result<Vec<Device>> {
    let name_of = |p: &Path| p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned());
    let own = files_in(root);
    if !own.is_empty() {
        return Ok(vec![Device { name: name_of(root), path: root.to_string_lossy().into_owned(), files: own }]);
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| format!("{} : {e}", root.display()))?.flatten() {
        let p = entry.path();
        let files = if p.is_dir() { files_in(&p) } else { Vec::new() };
        if !files.is_empty() {
            out.push(Device { name: name_of(&p), path: p.to_string_lossy().into_owned(), files });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

// --- Lecture sûre ---------------------------------------------------------------------------------------------------

fn text_of(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap_or_else(|e| e.into_bytes().iter().map(|&b| b as char).collect())
}

/// Contenu du fichier (copie en mémoire), `None` s'il est absent. Refus : récemment modifié, vide, ou modifié pendant la lecture.
fn read_stable(path: &Path, settle: Duration) -> Result<Option<String>> {
    let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let before = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{name} : {e}")),
    };
    let busy = |why: &str| format!("{name} refusé : {why} (MPE est peut-être en train de synchroniser) ; rien n'a été lu, réessayez dans quelques secondes.");
    let age = |m: &fs::Metadata| m.modified().ok().and_then(|t| SystemTime::now().duration_since(t).ok());
    if !settle.is_zero() && age(&before).is_some_and(|a| a < settle) {
        return Err(busy("fichier modifié à l'instant"));
    }
    // Ouverture en lecture seule uniquement (aucun droit d'écriture, donc aucun verrou exclusif).
    let mut bytes = Vec::new();
    fs::OpenOptions::new().read(true).open(path).and_then(|mut f| f.read_to_end(&mut bytes)).map_err(|e| format!("{name} : {e}"))?;
    let after = fs::metadata(path).map_err(|e| format!("{name} : {e}"))?;
    if after.len() != bytes.len() as u64 || after.len() != before.len() || after.modified().ok() != before.modified().ok() {
        return Err(busy("fichier modifié pendant la lecture"));
    }
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Err(busy("fichier vide"));
    }
    Ok(Some(text_of(bytes)))
}

/// Le fichier est-il complet ? Chaque `BEGIN:` d'une sorte de composant doit avoir son `END:`, et la dernière ligne
/// utile doit être un `END:` (un fichier coupé au milieu d'une fiche ne passe pas).
fn check_complete(name: &str, text: &str) -> Result<()> {
    let bad = |why: String| format!("{name} refusé : {why} (fichier tronqué ou en cours d'écriture) ; rien n'a été lu.");
    let mut open: Vec<String> = Vec::new();
    let mut last = "";
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        last = line;
        let up = line.to_ascii_uppercase();
        if let Some(tag) = up.strip_prefix("BEGIN:") {
            open.push(tag.trim().to_string());
        } else if let Some(tag) = up.strip_prefix("END:") {
            match open.pop() {
                Some(t) if t == tag.trim() => {}
                _ => return Err(bad(format!("« END:{} » sans « BEGIN » correspondant", tag.trim()))),
            }
        }
    }
    if let Some(tag) = open.last() {
        return Err(bad(format!("« BEGIN:{tag} » jamais refermé")));
    }
    if !last.to_ascii_uppercase().starts_with("END:") {
        return Err(bad("le fichier ne finit pas sur un « END: »".to_string()));
    }
    Ok(())
}

// --- Identifiants ---------------------------------------------------------------------------------------------------

fn prop_name(line: &str) -> String {
    line.split([':', ';']).next().unwrap_or("").trim().to_ascii_uppercase()
}

/// Dans chaque fiche `block` (`BEGIN:<block>` … `END:<block>`) qui porte un `X-IRMC-LUID`, remplace les identifiants du
/// fichier (`UID`, `X-OUTLOOK-UID`, `OUTLOOK-UID`) par `<id_prop>:<prefixe><LUID>`. Une fiche sans LUID est laissée telle quelle.
fn with_luid_ids(text: &str, block: &str, id_prop: &str, prefix: &str) -> String {
    let (begin, end) = (format!("BEGIN:{block}"), format!("END:{block}"));
    let mut out = String::with_capacity(text.len() + 64);
    let mut card: Vec<&str> = Vec::new();
    let mut inside = false;
    let flush = |card: &mut Vec<&str>, out: &mut String| {
        let nl = if card.first().is_some_and(|l| l.ends_with("\r\n")) { "\r\n" } else { "\n" };
        let luid = card.iter().filter(|l| prop_name(l) == "X-IRMC-LUID").find_map(|l| l.split_once(':').map(|(_, v)| v.trim().to_string())).filter(|v| !v.is_empty());
        let mut skipping = false;
        for (i, line) in card.iter().enumerate() {
            let folded = line.starts_with([' ', '\t']);
            if folded && skipping {
                continue;
            }
            skipping = luid.is_some() && !folded && matches!(prop_name(line).as_str(), "UID" | "X-OUTLOOK-UID" | "OUTLOOK-UID");
            if skipping {
                continue;
            }
            out.push_str(line);
            if i == 0 {
                if let Some(l) = &luid {
                    out.push_str(&format!("{id_prop}:{prefix}{l}{nl}"));
                }
            }
        }
        card.clear();
    };
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if !inside && t.eq_ignore_ascii_case(&begin) {
            inside = true;
            card.push(line);
        } else if inside {
            card.push(line);
            if t.eq_ignore_ascii_case(&end) {
                inside = false;
                flush(&mut card, &mut out);
            }
        } else {
            out.push_str(line);
        }
    }
    for line in card {
        out.push_str(line); // fiche jamais refermée : recopiée telle quelle (check_complete la refuse avant).
    }
    out
}

/// Nom d'appareil réduit à des caractères sans risque dans un identifiant (`UID:`).
fn slug(device: &str) -> String {
    let s: String = device.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '_' { c } else { '_' }).collect();
    if s.is_empty() { "appareil".into() } else { s }
}

// --- Plage du calendrier --------------------------------------------------------------------------------------------

/// Bornes `AAAA-MM-JJTHH:MM` (heure locale approchée par l'UTC : la plage est de ±90 jours, une heure n'y change rien).
fn range_bounds(opts: &MpeOptions) -> (String, String) {
    let now = opts.now_ms.and_then(chrono::DateTime::from_timestamp_millis).unwrap_or_else(chrono::Utc::now);
    let fmt = |d: chrono::DateTime<chrono::Utc>| d.format("%Y-%m-%dT%H:%M").to_string();
    (fmt(now - chrono::Duration::days(opts.range_days)), fmt(now + chrono::Duration::days(opts.range_days)))
}

// --- Lecture --------------------------------------------------------------------------------------------------------

struct Staging(PathBuf);

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn staging_dir() -> Result<Staging> {
    let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("thunderlook-mpe-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    Ok(Staging(dir))
}

/// Éléments déjà reliés à ce téléphone (identifiants locaux), par nature : ceux dont l'identifiant mémorisé porte `prefix`.
fn linked(data_dir: &Path, kind: ItemKind, prefix: &str) -> Vec<String> {
    read_map(data_dir).get(kind.name()).map(|m| m.iter().filter(|(uid, _)| uid.starts_with(prefix)).map(|(_, local)| local.clone()).collect()).unwrap_or_default()
}

fn synthetic(kind: ItemKind, ids: Vec<String>) -> FileInfo {
    // `sha256` vide : jamais égal à celui du fichier, donc le fichier est toujours relu en entier.
    FileInfo { name: kind.file().to_string(), count: 0, sha256: String::new(), ids, ..Default::default() }
}

/// Changements entre les fichiers de données de MPE de `device_dir` et la base, SANS RIEN ÉCRIRE nulle part (ni dans
/// `device_dir`, ni dans la base). Les changements se passent ensuite à [`crate::apply`] sur choix de l'utilisateur.
pub fn import_changes_mpe(conn: &thunderlook_store::rusqlite::Connection, data_dir: &Path, device_dir: &Path, opts: &MpeOptions) -> Result<ImportReport> {
    if !device_dir.is_dir() {
        return Err(format!("{} n'est pas un dossier.", device_dir.display()));
    }
    let device = device_dir.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let prefix = format!("mpe-{}-", slug(&device));
    let mut warnings = Vec::new();

    // 1. Lecture et contrôle de tout, avant d'analyser quoi que ce soit : un seul fichier douteux refuse l'ensemble.
    let mut read: Vec<(&str, ItemKind, String)> = Vec::new();
    for (dat, kind) in [(PHONEBOOK_DAT, ItemKind::Contact), (ORGANIZER_DAT, ItemKind::Event), (NOTES_DAT, ItemKind::Note)] {
        match read_stable(&device_dir.join(dat), opts.settle)? {
            Some(text) => {
                check_complete(dat, &text)?;
                read.push((dat, kind, text));
            }
            None => warnings.push(format!("{dat} absent de {} : {} non lus.", device_dir.display(), item_plural(kind))),
        }
    }
    if read.is_empty() {
        return Err(format!("Aucun fichier de MPE ({}) dans {}.", DAT_FILES.join(", "), device_dir.display()));
    }

    // 2. Copies de travail sous les noms de l'échange, identifiants ramenés au LUID, et manifeste synthétique.
    let stage = staging_dir()?;
    let (lo, hi) = range_bounds(opts);
    let mut files = Vec::new();
    for (dat, kind, text) in &read {
        let (name, text, ids) = match kind {
            ItemKind::Contact => (CONTACTS_FILE, with_luid_ids(text, "VCARD", "UID", &prefix), linked(data_dir, *kind, &prefix)),
            ItemKind::Note => (NOTES_FILE, with_luid_ids(text, "VNOTE", "X-OUTLOOK-UID", &prefix), linked(data_dir, *kind, &prefix)),
            _ => {
                // Suppressions : seulement les rendez-vous simples déjà reliés, dans la plage.
                let cal = thunderlook_calendar::load(conn)?;
                let ids = linked(data_dir, *kind, &prefix)
                    .into_iter()
                    .filter(|id| {
                        cal.appointments.iter().any(|a| {
                            format!("a{}", a.id) == *id && a.recurrence.is_none() && {
                                let s = local_start(a.start_ms);
                                s >= lo && s <= hi
                            }
                        })
                    })
                    .collect();
                (CALENDAR_FILE, with_luid_ids(text, "VEVENT", "UID", &prefix), ids)
            }
        };
        write_atomic(&stage.0.join(name), text.as_bytes()).map_err(|e| format!("{dat} : {e}"))?;
        files.push(synthetic(*kind, ids));
    }
    let manifest = Manifest { exported_at: now_stamp(), files };
    write_atomic(&stage.0.join(MANIFEST_FILE), serde_json::to_string(&manifest).map_err(|e| e.to_string())?.as_bytes())?;

    // 3. Le circuit ordinaire de comparaison.
    let mut rep = import_changes_with(conn, data_dir, &stage.0, &ImportOptions { notes_from_phone: opts.notes_from_phone })?;
    drop(stage);

    // 4. Calendrier : rien hors de la plage regardée par MPE.
    let outside = |c: &Change| matches!(&c.payload, Payload::Event(e) if c.kind != ChangeKind::Deleted && e.series.is_none() && !(e.start.as_str() >= lo.as_str() && e.start.as_str() <= hi.as_str()));
    let (before, before_c) = (rep.changes.len(), rep.conflicts.len());
    rep.changes.retain(|c| !outside(c));
    rep.conflicts.retain(|c| !outside(c));
    let skipped = before + before_c - rep.changes.len() - rep.conflicts.len();
    if skipped > 0 {
        warnings.push(format!("{skipped} rendez-vous hors de la plage de synchro de MPE (−{0} / +{0} jours) ignoré(s).", opts.range_days));
    }
    warnings.extend(std::mem::take(&mut rep.warnings));
    rep.warnings = warnings;
    Ok(rep)
}

fn item_plural(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Contact => "contacts",
        ItemKind::Task => "tâches",
        ItemKind::Note => "notes",
        ItemKind::Event => "rendez-vous",
    }
}

fn local_start(ms: i64) -> String {
    let utc = chrono::DateTime::from_timestamp_millis(ms).map(|t| t.naive_utc()).unwrap_or_default();
    thunderlook_calendar::tz::TimeZone::local().to_local(utc).format("%Y-%m-%dT%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luid_replaces_ids() {
        let t = "BEGIN:VCARD\r\nVERSION:2.1\r\nUID:zzz\r\nX-IRMC-LUID:42\r\nOUTLOOK-UID:00AB\r\n  CD\r\nFN:A\r\nEND:VCARD\r\nBEGIN:VCARD\r\nFN:B\r\nUID:keep\r\nEND:VCARD\r\n";
        let o = with_luid_ids(t, "VCARD", "UID", "mpe-d-");
        assert!(o.contains("BEGIN:VCARD\r\nUID:mpe-d-42\r\nVERSION:2.1"));
        assert!(!o.contains("zzz") && !o.contains("00AB") && !o.contains("CD"));
        assert!(o.contains("UID:keep"));
    }

    #[test]
    fn truncated_is_refused() {
        assert!(check_complete("x", "BEGIN:VCARD\nFN:A\nEND:VCARD\nBEGIN:VCARD\nFN:B\n").is_err());
        assert!(check_complete("x", "BEGIN:VCARD\nFN:A\nEND:VCARD\n").is_ok());
        assert!(check_complete("x", "BEGIN:VCARD\nFN:A\nEND:VCARD\nFN:B").is_err());
    }
}
