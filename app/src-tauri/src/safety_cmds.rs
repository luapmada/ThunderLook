//! Annulation (Ctrl+Z) et sauvegardes : commandes de l'écran « Sauvegardes » et tâche de lancement (purge de la corbeille
//! interne, sauvegarde du jour). Tout le travail est dans `thunderlook-safety`.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
use std::time::Duration;

use thunderlook_safety::{KEEP_DAILY, KEEP_DB, Mode, TRASH_DAYS};

use crate::{CalendarCache, clear_calendar, data_dir};

/// Au lancement, hors du fil principal : purge de la corbeille (60 jours) puis sauvegardes (éléments du jour, copie de la base
/// une fois par semaine). Elles attendent que la base soit au repos plutôt qu'un délai fixe : au moins 20 s après le
/// lancement, puis 10 s sans écriture d'une autre connexion (indexation, import), au plus 15 min d'attente.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Ok(dir) = data_dir(&app) else { return };
        thunderlook_safety::wait_idle(&dir, Duration::from_secs(20), Duration::from_secs(10), Duration::from_secs(15 * 60));
        let purged = thunderlook_safety::purge(&dir, TRASH_DAYS);
        if purged > 0 {
            log::info!("corbeille interne : {purged} instantané(s) de plus de {TRASH_DAYS} jours supprimé(s)");
        }
        match run(&dir) {
            Ok(done) => done.iter().for_each(|f| log::info!("sauvegarde : {}", f.display())),
            Err(e) => log::warn!("sauvegarde impossible : {e}"),
        }
    });
}

/// Sauvegarde des éléments du jour puis, si la dernière a plus de 7 jours, copie contrôlée de la base. Rend les fichiers écrits ;
/// la première erreur est consignée (écran « Sauvegardes ») jusqu'à la prochaine réussite.
fn run(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut done = Vec::new();
    let items = thunderlook_safety::backup(dir, KEEP_DAILY);
    let base = thunderlook_safety::backup_db(dir, KEEP_DB);
    // Copie du courrier local (`eml/`), incrémentale et au plus une fois par jour ; un fichier en échec est signalé sans bloquer le reste.
    let mail = thunderlook_safety::backup_mail_daily(dir).and_then(|r| match r {
        Some(r) if !r.failed.is_empty() => Err(format!("copie du courrier : {} fichier(s) non copié(s), dont {}", r.failed.len(), r.failed[0])),
        r => Ok(r),
    });
    done.extend(items.as_ref().ok().and_then(Option::as_ref).cloned());
    done.extend(base.as_ref().ok().and_then(Option::as_ref).cloned());
    if let Ok(Some(r)) = &mail {
        if r.copied > 0 {
            done.push(thunderlook_safety::mail_dir(dir));
        }
    }
    match (items, base, mail) {
        (Ok(_), Ok(_), Ok(_)) => Ok(done),
        (Err(e), _, _) => Err(e),
        (_, Err(e), _) | (_, _, Err(e)) => {
            thunderlook_safety::record_failure(dir, &e);
            Err(e)
        }
    }
}

/// Les listes ouvertes se rechargent (calendrier, contacts ; les tâches et notes sont rechargées par l'interface).
fn changed(app: &AppHandle) {
    if let Ok(()) = clear_calendar(&app.state::<CalendarCache>()) {
        let _ = app.emit("calendar-changed", ());
    }
    let _ = app.emit("contacts-changed", ());
}

/// Ctrl+Z / bouton « Annuler » des modules Calendrier, Contacts, Tâches et Notes : défait la dernière suppression ou
/// modification. Rend le message de la barre d'état.
#[tauri::command(async)]
pub fn safety_undo(app: AppHandle) -> Result<String, String> {
    let dir = data_dir(&app)?;
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let msg = thunderlook_safety::undo(&mut conn, &dir)?;
    changed(&app);
    Ok(msg)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRow {
    pub name: String,
    pub weekly: bool,
    pub size: u64,
    pub count: usize,
    pub created: String,
    /// Dernier échec de sauvegarde, tant qu'aucune n'a réussi depuis.
    pub error: Option<String>,
}

#[tauri::command(async)]
pub fn backups_list(app: AppHandle) -> Result<Vec<BackupRow>, String> {
    let dir = data_dir(&app)?;
    let error = thunderlook_safety::last_failure(&dir);
    let mut rows: Vec<BackupRow> = thunderlook_safety::list_backups(&dir)
        .into_iter()
        .map(|b| BackupRow { name: b.name, weekly: b.weekly, size: b.size, count: b.count, created: b.created, error: error.clone() })
        .collect();
    // Sans aucune sauvegarde, l'échec doit quand même se voir : ligne sans nom, que l'écran n'affiche pas comme sauvegarde.
    if rows.is_empty() && error.is_some() {
        rows.push(BackupRow { name: String::new(), weekly: false, size: 0, count: 0, created: String::new(), error });
    }
    Ok(rows)
}

/// Sauvegarde à la demande ; sans effet pour ce qui existe déjà (éléments du jour, copie de la base de moins de 7 jours).
#[tauri::command(async)]
pub fn backup_now(app: AppHandle) -> Result<String, String> {
    let dir = data_dir(&app)?;
    // Dans un fil à part (lecture seule, basse priorité) : la commande attend sa fin pour répondre, l'interface non.
    let done = std::thread::spawn(move || run(&dir)).join().map_err(|_| "la sauvegarde s'est arrêtée brutalement".to_string())??;
    Ok(if done.is_empty() { "Rien de nouveau : la sauvegarde d'aujourd'hui existe déjà".to_string() } else { format!("Sauvegarde faite ({} fichier(s))", done.len()) })
}

/// `mode` : « merge » (éléments manquants seulement) ou « all » (tout remettre ; l'état actuel va en corbeille interne).
#[tauri::command(async)]
pub fn backup_restore(app: AppHandle, name: String, mode: String) -> Result<String, String> {
    let dir = data_dir(&app)?;
    // Le nom vient de la liste : jamais un chemin.
    let info = thunderlook_safety::list_backups(&dir).into_iter().find(|b| b.name == name).ok_or("sauvegarde introuvable")?;
    let mode = match mode.as_str() {
        "merge" => Mode::Merge,
        "all" => Mode::All,
        _ => return Err("mode inconnu".into()),
    };
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let r = thunderlook_safety::restore_backup(&mut conn, &dir, &info.path, mode)?;
    changed(&app);
    let mut msg = format!("Restauration : {} remis, {} remplacé(s), {} déjà présent(s)", r.added, r.replaced, r.skipped);
    if !r.failed.is_empty() {
        msg.push_str(&format!(" — {} non remis : {}", r.failed.len(), r.failed.iter().take(3).cloned().collect::<Vec<_>>().join(" ; ")));
    }
    Ok(msg)
}

#[tauri::command(async)]
pub fn backups_open_folder(app: AppHandle) -> Result<(), String> {
    let dir = thunderlook_safety::backups_dir(&data_dir(&app)?);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailBackupRow {
    pub created: String,
    pub files: usize,
    pub size: u64,
    pub failed: usize,
}

/// Copie du courrier local. `action` : « status » (dernière copie), « check » (test, lecture seule) ou « restore » (remet les
/// messages absents de `eml/`). Une seule commande pour les trois : l'écran « Sauvegardes » n'en a pas besoin de plus.
#[tauri::command(async)]
pub fn mail_backup(app: AppHandle, action: String) -> Result<MailBackupAnswer, String> {
    let dir = data_dir(&app)?;
    let message = match action.as_str() {
        "status" => String::new(),
        "check" => mail_check(&dir)?,
        "restore" => mail_restore(&dir)?,
        _ => return Err("action inconnue".into()),
    };
    let status = thunderlook_safety::mail_status(&dir).map(|s| MailBackupRow { created: s.created, files: s.files, size: s.bytes, failed: s.failed });
    Ok(MailBackupAnswer { status, message })
}

#[derive(Serialize)]
pub struct MailBackupAnswer {
    pub status: Option<MailBackupRow>,
    pub message: String,
}

/// Chaque message de `eml/` comparé octet pour octet à sa copie, plus contrôle d'intégrité de la dernière copie de la base.
fn mail_check(dir: &std::path::Path) -> Result<String, String> {
    let dir = dir.to_path_buf();
    let c = std::thread::spawn(move || {
        let mail = thunderlook_safety::verify_mail(&dir);
        let db = thunderlook_safety::list_db_copies(&dir).first().map(|c| thunderlook_safety::verify_db(&c.path).map(|()| c.date));
        (mail, db)
    })
    .join()
    .map_err(|_| "le test s'est arrêté brutalement".to_string())?;
    let (mail, db) = c;
    let mut msg = if mail.is_ok() {
        format!("Courrier : {} message(s) identiques à leur copie", mail.checked)
    } else {
        format!("Courrier : {} message(s) sans copie, {} différent(s) de leur copie (sur {}) — « Sauvegarder maintenant » recopie", mail.missing.len(), mail.different.len(), mail.checked)
    };
    if mail.only_in_copy > 0 {
        msg.push_str(&format!(" ; {} message(s) supprimé(s) depuis, gardés dans la copie", mail.only_in_copy));
    }
    match db {
        Some(Ok(d)) => msg.push_str(&format!(" · Base du {d} : intègre")),
        Some(Err(e)) => msg.push_str(&format!(" · Base : {e}")),
        None => msg.push_str(" · Pas encore de copie de la base"),
    }
    Ok(msg)
}

/// Remet dans `eml/` les messages de la copie qui n'y sont plus. Rien n'est écrasé. Les lignes de la base ne sont pas touchées.
fn mail_restore(dir: &std::path::Path) -> Result<String, String> {
    let r = thunderlook_safety::restore_mail(dir)?;
    let mut msg = format!("Courrier : {} fichier(s) remis, {} déjà présent(s)", r.restored, r.present);
    if !r.failed.is_empty() {
        msg.push_str(&format!(" — {} non remis : {}", r.failed.len(), r.failed.iter().take(3).cloned().collect::<Vec<_>>().join(" ; ")));
    }
    Ok(msg)
}
