//! Copie cohérente de la base SQLite entière (`VACUUM INTO`, jamais une copie du fichier ouvert), contrôlée par
//! `PRAGMA integrity_check`, gardée en `N` exemplaires, restaurable par l'API de sauvegarde en ligne de SQLite.
//! Complète les sauvegardes JSON Lines (éléments seulement) : elle contient aussi le courrier indexé, d'où une fréquence
//! hebdomadaire et peu d'exemplaires.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use chrono::{Local, NaiveDate};
use thunderlook_store::rusqlite::{Connection, OpenFlags, backup::Backup};

use crate::{Result, backups_dir, e, io};

/// Copies de la base gardées.
pub const KEEP_DB: usize = 2;
/// Jours minimum entre deux copies automatiques.
const DB_EVERY_DAYS: i64 = 7;
const ERROR_FILE: &str = "last_error.txt";

pub fn db_dir(data_dir: &Path) -> PathBuf {
    backups_dir(data_dir).join("base")
}

/// (date, nom) d'après `base-AAAA-MM-JJ.sqlite`.
fn parse(name: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(name.strip_prefix("base-")?.strip_suffix(".sqlite")?, "%Y-%m-%d").ok()
}

#[derive(Clone, Debug)]
pub struct DbCopy {
    pub path: PathBuf,
    pub date: NaiveDate,
    pub size: u64,
}

/// Copies présentes, la plus récente d'abord.
pub fn list_db_copies(data_dir: &Path) -> Vec<DbCopy> {
    let Ok(rd) = fs::read_dir(db_dir(data_dir)) else { return Vec::new() };
    let mut out: Vec<DbCopy> = rd
        .flatten()
        .filter_map(|f| Some(DbCopy { date: parse(&f.file_name().to_string_lossy())?, size: f.metadata().ok()?.len(), path: f.path() }))
        .collect();
    out.sort_by(|a, b| b.date.cmp(&a.date));
    out
}

/// `PRAGMA integrity_check` : `Ok` seulement si SQLite répond « ok ».
pub fn verify_db(file: &Path) -> Result<()> {
    let conn = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(|x| format!("{} : {x}", file.display()))?;
    let mut stmt = conn.prepare("PRAGMA integrity_check").map_err(|x| format!("{} illisible : {x}", file.display()))?;
    let lines: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .and_then(|rows| rows.collect())
        .map_err(|x| format!("{} illisible : {x}", file.display()))?;
    if lines.len() == 1 && lines[0] == "ok" {
        Ok(())
    } else {
        Err(format!("{} : contrôle d'intégrité échoué ({})", file.display(), lines.into_iter().take(3).collect::<Vec<_>>().join(" ; ")))
    }
}

/// Copie `src` (base ouverte, même en écriture par ailleurs) dans `dest`, d'un seul tenant cohérent.
fn vacuum_into(data_dir: &Path, dest: &Path) -> Result<()> {
    let db = data_dir.join(thunderlook_store::DB_FILE);
    let conn = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(e)?;
    conn.execute_batch("PRAGMA busy_timeout = 8000;").map_err(e)?;
    let _ = fs::remove_file(dest);
    conn.execute("VACUUM INTO ?1", [dest.to_string_lossy().as_ref()]).map_err(e)?;
    Ok(())
}

/// Copie de la base, au plus une par semaine : écrite sous un nom provisoire, contrôlée, puis mise en place ; les plus
/// anciennes au-delà de `keep` sont supprimées après coup. `None` : une copie récente existe déjà.
pub fn backup_db_on(data_dir: &Path, keep: usize, today: NaiveDate) -> Result<Option<PathBuf>> {
    let dir = db_dir(data_dir);
    if list_db_copies(data_dir).first().is_some_and(|c| (today - c.date).num_days() < DB_EVERY_DAYS) {
        return Ok(None);
    }
    fs::create_dir_all(&dir).map_err(|x| io(&dir, x))?;
    let path = dir.join(format!("base-{}.sqlite", today.format("%Y-%m-%d")));
    let tmp = path.with_extension("tmp");
    let made = vacuum_into(data_dir, &tmp).and_then(|()| verify_db(&tmp));
    if let Err(x) = made {
        let _ = fs::remove_file(&tmp);
        return Err(format!("copie de la base : {x}"));
    }
    fs::rename(&tmp, &path).map_err(|x| io(&path, x))?;
    for old in list_db_copies(data_dir).into_iter().skip(keep.max(1)) {
        let _ = fs::remove_file(old.path);
    }
    Ok(Some(path))
}

pub fn backup_db(data_dir: &Path, keep: usize) -> Result<Option<PathBuf>> {
    backup_db_on(data_dir, keep, Local::now().date_naive())
}

/// Remet la base entière dans `conn` depuis une copie contrôlée. L'état actuel est d'abord copié dans
/// `backups/base/avant-restauration.sqlite` (écrasé à chaque fois). Les autres connexions voient la base changer sous elles :
/// à n'appeler qu'avec leur accord (listes à recharger).
pub fn restore_db(conn: &mut Connection, data_dir: &Path, file: &Path) -> Result<()> {
    verify_db(file)?;
    let dir = db_dir(data_dir);
    fs::create_dir_all(&dir).map_err(|x| io(&dir, x))?;
    let before = dir.join("avant-restauration.sqlite");
    vacuum_into(data_dir, &before)?;
    let src = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(e)?;
    let b = Backup::new(&src, conn).map_err(e)?;
    b.run_to_completion(256, Duration::from_millis(5), None).map_err(e)
}

/// Dernier échec de sauvegarde (message), tant qu'une sauvegarde n'a pas réussi depuis.
pub fn last_failure(data_dir: &Path) -> Option<String> {
    fs::read_to_string(backups_dir(data_dir).join(ERROR_FILE)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Consigne un échec de sauvegarde pour l'écran « Sauvegardes ».
pub fn record_failure(data_dir: &Path, message: &str) {
    set_failure(data_dir, Some(message));
}

pub(crate) fn set_failure(data_dir: &Path, failure: Option<&str>) {
    let dir = backups_dir(data_dir);
    let file = dir.join(ERROR_FILE);
    match failure {
        Some(msg) => {
            let _ = fs::create_dir_all(&dir);
            let _ = fs::write(&file, format!("{} — {msg}", Local::now().format("%Y-%m-%d %H:%M")));
        }
        None => {
            let _ = fs::remove_file(&file);
        }
    }
}

/// Attend que la base soit au repos : au moins `min_wait` après l'appel, puis `quiet` sans qu'une autre connexion n'ait
/// validé d'écriture (`PRAGMA data_version`). Au bout de `max_wait`, rend la main quand même : mieux vaut sauvegarder
/// (lecture seule, par paquets) que ne jamais le faire.
pub fn wait_idle(data_dir: &Path, min_wait: Duration, quiet: Duration, max_wait: Duration) {
    let start = Instant::now();
    std::thread::sleep(min_wait.min(max_wait));
    let Ok(conn) = thunderlook_store::open_readonly(data_dir) else { return };
    let version = || conn.query_row("PRAGMA data_version", [], |r| r.get::<_, i64>(0)).unwrap_or(0);
    let (mut last, mut since) = (version(), Instant::now());
    while start.elapsed() < max_wait && since.elapsed() < quiet {
        std::thread::sleep(Duration::from_secs(1));
        let v = version();
        if v != last {
            last = v;
            since = Instant::now();
        }
    }
}
