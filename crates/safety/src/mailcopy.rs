//! Copie du courrier local : les messages (`eml/<source>/<lot>/<id>.eml`) ne sont ni dans les sauvegardes JSON Lines (éléments
//! seulement) ni dans la copie de la base (qui ne garde que les en-têtes et l'index). Ici, un MIROIR ajouté sous
//! `backups/courrier/eml/`, même arborescence que `eml/` :
//!
//! - lecture seule côté `eml/` (jamais écrit, jamais supprimé) ; chaque fichier est écrit sous un nom provisoire, contrôlé
//!   (taille), puis mis en place ;
//! - incrémental : un fichier déjà copié et inchangé n'est pas recopié ;
//! - additif : un message supprimé de `eml/` reste dans le miroir (rien n'est jamais retiré, c'est le but) ;
//! - [`verify_mail`] compare octet pour octet ; [`restore_mail`] ne remet que les fichiers ABSENTS de `eml/`, sans rien écraser.
//!
//! Pour retrouver aussi les lignes de la base (dossiers, en-têtes), la copie hebdomadaire `backups/base` reste nécessaire.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use chrono::{Local, NaiveDate};
use serde_json::{Value, json};

use crate::{Result, backups_dir, io};

const STATE_FILE: &str = "etat.json";

/// Dossier du miroir : `backups/courrier`.
pub fn mail_dir(data_dir: &Path) -> PathBuf {
    backups_dir(data_dir).join("courrier")
}

fn mirror(data_dir: &Path) -> PathBuf {
    mail_dir(data_dir).join("eml")
}

/// Fichiers de `root` (chemin relatif en `/`), sous-dossiers compris ; liens symboliques et fichiers provisoires ignorés.
fn walk(root: &Path) -> Vec<String> {
    fn rec(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for f in rd.flatten() {
            let Ok(t) = f.file_type() else { continue };
            let p = f.path();
            if t.is_dir() {
                rec(base, &p, out);
            } else if t.is_file() && p.extension().is_none_or(|x| x != "tmp") {
                if let Ok(rel) = p.strip_prefix(base) {
                    out.push(rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"));
                }
            }
        }
    }
    let mut out = Vec::new();
    rec(root, root, &mut out);
    out.sort();
    out
}

fn at(root: &Path, rel: &str) -> PathBuf {
    thunderlook_store::resolve(root, rel)
}

/// Copie `src` vers `dest` : nom provisoire, taille contrôlée, puis mise en place. `src` n'est que lu.
fn copy_checked(src: &Path, dest: &Path) -> Result<u64> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|x| io(parent, x))?;
    }
    let mut tmp = dest.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let n = fs::copy(src, &tmp).map_err(|x| io(src, x))?;
    let want = fs::metadata(src).map_err(|x| io(src, x))?.len();
    if n != want || fs::metadata(&tmp).map_err(|x| io(&tmp, x))?.len() != want {
        let _ = fs::remove_file(&tmp);
        return Err(format!("{} : copie incomplète", src.display()));
    }
    fs::rename(&tmp, dest).map_err(|x| io(dest, x))?;
    Ok(n)
}

/// Même contenu, octet pour octet.
fn same_bytes(a: &Path, b: &Path) -> bool {
    let (Ok(mut fa), Ok(mut fb)) = (fs::File::open(a), fs::File::open(b)) else { return false };
    if fa.metadata().map(|m| m.len()).ok() != fb.metadata().map(|m| m.len()).ok() {
        return false;
    }
    let (mut ba, mut bb) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
    loop {
        let Ok(n) = fa.read(&mut ba) else { return false };
        if n == 0 {
            return true;
        }
        if fb.read_exact(&mut bb[..n]).is_err() || ba[..n] != bb[..n] {
            return false;
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MailReport {
    /// Fichiers écrits (nouveaux ou modifiés).
    pub copied: usize,
    /// Fichiers déjà à jour.
    pub unchanged: usize,
    pub bytes: u64,
    /// Fichiers qui n'ont pas pu être copiés (message d'erreur).
    pub failed: Vec<String>,
}

/// Copie de `eml/` vers le miroir. Une erreur sur un fichier n'arrête pas les autres ; elle est rendue dans `failed`.
pub fn backup_mail(data_dir: &Path) -> Result<MailReport> {
    let src_root = data_dir.join("eml");
    let dest_root = mirror(data_dir);
    let mut r = MailReport::default();
    for rel in walk(&src_root) {
        let (src, dest) = (at(&src_root, &rel), at(&dest_root, &rel));
        let up_to_date = match (fs::metadata(&src), fs::metadata(&dest)) {
            (Ok(s), Ok(d)) => s.len() == d.len() && matches!((s.modified(), d.modified()), (Ok(sm), Ok(dm)) if dm >= sm),
            _ => false,
        };
        if up_to_date {
            r.unchanged += 1;
            continue;
        }
        match copy_checked(&src, &dest) {
            Ok(n) => {
                r.copied += 1;
                r.bytes += n;
            }
            Err(x) => r.failed.push(x),
        }
    }
    let (files, bytes) = walk(&dest_root).iter().fold((0usize, 0u64), |(n, b), rel| (n + 1, b + fs::metadata(at(&dest_root, rel)).map(|m| m.len()).unwrap_or(0)));
    let state = json!({ "date": Local::now().date_naive().format("%Y-%m-%d").to_string(), "created": Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        "files": files, "bytes": bytes, "copied": r.copied, "failed": r.failed.len() });
    let dir = mail_dir(data_dir);
    fs::create_dir_all(&dir).map_err(|x| io(&dir, x))?;
    fs::write(dir.join(STATE_FILE), state.to_string()).map_err(|x| io(&dir, x))?;
    Ok(r)
}

/// Une fois par jour au plus (un échec partiel ne compte pas : il sera retenté). `None` : déjà faite aujourd'hui.
pub fn backup_mail_on(data_dir: &Path, today: NaiveDate) -> Result<Option<MailReport>> {
    if mail_status(data_dir).is_some_and(|s| s.date == today && s.failed == 0) {
        return Ok(None);
    }
    backup_mail(data_dir).map(Some)
}

pub fn backup_mail_daily(data_dir: &Path) -> Result<Option<MailReport>> {
    backup_mail_on(data_dir, Local::now().date_naive())
}

#[derive(Clone, Debug, PartialEq)]
pub struct MailStatus {
    pub date: NaiveDate,
    pub created: String,
    /// Fichiers et octets dans le miroir à la fin de la dernière copie.
    pub files: usize,
    pub bytes: u64,
    pub failed: usize,
}

/// Dernière copie du courrier, d'après `etat.json` ; `None` si aucune.
pub fn mail_status(data_dir: &Path) -> Option<MailStatus> {
    let v: Value = serde_json::from_str(&fs::read_to_string(mail_dir(data_dir).join(STATE_FILE)).ok()?).ok()?;
    Some(MailStatus {
        date: NaiveDate::parse_from_str(v["date"].as_str()?, "%Y-%m-%d").ok()?,
        created: v["created"].as_str().unwrap_or_default().to_string(),
        files: v["files"].as_u64()? as usize,
        bytes: v["bytes"].as_u64().unwrap_or(0),
        failed: v["failed"].as_u64().unwrap_or(0) as usize,
    })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MailCheck {
    pub checked: usize,
    /// Dans `eml/` mais pas (encore) dans le miroir.
    pub missing: Vec<String>,
    /// Présents des deux côtés mais différents : le miroir est périmé ou abîmé (rien n'est modifié).
    pub different: Vec<String>,
    /// Seulement dans le miroir (message supprimé depuis) : conservés, c'est voulu.
    pub only_in_copy: usize,
}

impl MailCheck {
    pub fn is_ok(&self) -> bool {
        self.missing.is_empty() && self.different.is_empty()
    }
}

/// Test de la copie : chaque fichier de `eml/` doit exister dans le miroir, identique octet pour octet. Lecture seule.
pub fn verify_mail(data_dir: &Path) -> MailCheck {
    let (src_root, dest_root) = (data_dir.join("eml"), mirror(data_dir));
    let mut c = MailCheck::default();
    for rel in walk(&src_root) {
        c.checked += 1;
        let (src, dest) = (at(&src_root, &rel), at(&dest_root, &rel));
        if !dest.exists() {
            c.missing.push(rel);
        } else if !same_bytes(&src, &dest) {
            c.different.push(rel);
        }
    }
    c.only_in_copy = walk(&dest_root).into_iter().filter(|rel| !at(&src_root, rel).exists()).count();
    c
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MailRestore {
    pub restored: usize,
    /// Déjà présents dans `eml/` (jamais écrasés).
    pub present: usize,
    pub failed: Vec<String>,
}

/// Remet dans `eml/` les messages du miroir qui n'y sont plus. Un fichier existant n'est jamais remplacé, même différent.
pub fn restore_mail(data_dir: &Path) -> Result<MailRestore> {
    let (src_root, dest_root) = (mirror(data_dir), data_dir.join("eml"));
    let mut r = MailRestore::default();
    for rel in walk(&src_root) {
        let dest = at(&dest_root, &rel);
        if dest.exists() {
            r.present += 1;
        } else {
            match copy_checked(&at(&src_root, &rel), &dest) {
                Ok(_) => r.restored += 1,
                Err(x) => r.failed.push(x),
            }
        }
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tl-mailcopy-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("eml/1/0000")).unwrap();
        d
    }

    fn put(d: &Path, rel: &str, text: &str) {
        let p = at(&d.join("eml"), rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    #[test]
    fn backup_is_incremental_and_checked() {
        let d = dir("incr");
        put(&d, "1/0000/1.eml", "Subject: a\r\n\r\ncorps é\r\n");
        put(&d, "1/0000/2.eml", "Subject: b\r\n\r\n");
        let r = backup_mail(&d).unwrap();
        assert_eq!((r.copied, r.unchanged, r.failed.len()), (2, 0, 0));
        assert_eq!(fs::read_to_string(d.join("backups/courrier/eml/1/0000/1.eml")).unwrap(), "Subject: a\r\n\r\ncorps é\r\n");
        let r = backup_mail(&d).unwrap();
        assert_eq!((r.copied, r.unchanged), (0, 2), "rien n'est recopié");
        put(&d, "1/0000/3.eml", "Subject: c\r\n\r\n");
        assert_eq!(backup_mail(&d).unwrap().copied, 1);
        let s = mail_status(&d).unwrap();
        assert_eq!((s.files, s.failed), (3, 0));
        assert!(verify_mail(&d).is_ok());
        assert!(walk(&mail_dir(&d)).iter().all(|f| !f.ends_with(".tmp")));
    }

    #[test]
    fn backup_daily_runs_once_a_day() {
        let d = dir("daily");
        put(&d, "1/0000/1.eml", "x");
        let day = Local::now().date_naive();
        assert!(backup_mail_on(&d, day).unwrap().is_some());
        assert!(backup_mail_on(&d, day).unwrap().is_none());
        assert!(backup_mail_on(&d, day.succ_opt().unwrap()).unwrap().is_some());
    }

    #[test]
    fn deleted_message_stays_in_copy_and_comes_back() {
        let d = dir("restore");
        put(&d, "1/0000/1.eml", "un");
        put(&d, "1/0000/2.eml", "deux");
        backup_mail(&d).unwrap();
        fs::remove_file(d.join("eml/1/0000/2.eml")).unwrap();
        backup_mail(&d).unwrap();
        let c = verify_mail(&d);
        assert!(c.is_ok());
        assert_eq!(c.only_in_copy, 1, "le message supprimé reste dans le miroir");
        let r = restore_mail(&d).unwrap();
        assert_eq!((r.restored, r.present), (1, 1));
        assert_eq!(fs::read_to_string(d.join("eml/1/0000/2.eml")).unwrap(), "deux");
        assert_eq!(verify_mail(&d).only_in_copy, 0);
    }

    #[test]
    fn restore_never_overwrites() {
        let d = dir("nooverwrite");
        put(&d, "1/0000/1.eml", "copie");
        backup_mail(&d).unwrap();
        fs::write(d.join("eml/1/0000/1.eml"), "modifié depuis").unwrap();
        let r = restore_mail(&d).unwrap();
        assert_eq!((r.restored, r.present), (0, 1));
        assert_eq!(fs::read_to_string(d.join("eml/1/0000/1.eml")).unwrap(), "modifié depuis");
    }

    #[test]
    fn verify_reports_missing_and_damaged_copy() {
        let d = dir("verify");
        put(&d, "1/0000/1.eml", "aaaa");
        put(&d, "1/0000/2.eml", "bbbb");
        backup_mail(&d).unwrap();
        put(&d, "1/0000/3.eml", "cccc");
        fs::write(d.join("backups/courrier/eml/1/0000/1.eml"), "aaab").unwrap();
        let c = verify_mail(&d);
        assert_eq!((c.checked, c.missing.clone(), c.different.clone()), (3, vec!["1/0000/3.eml".to_string()], vec!["1/0000/1.eml".to_string()]));
        assert!(!c.is_ok());
    }

    #[test]
    fn source_is_never_written_and_empty_data_is_fine() {
        let d = dir("ro");
        let r = backup_mail(&d).unwrap();
        assert_eq!((r.copied, r.unchanged), (0, 0));
        put(&d, "1/0000/1.eml", "z");
        let before = fs::metadata(d.join("eml/1/0000/1.eml")).unwrap().modified().unwrap();
        backup_mail(&d).unwrap();
        assert_eq!(fs::metadata(d.join("eml/1/0000/1.eml")).unwrap().modified().unwrap(), before);
        assert_eq!(fs::read_dir(d.join("eml/1/0000")).unwrap().count(), 1);
    }
}
