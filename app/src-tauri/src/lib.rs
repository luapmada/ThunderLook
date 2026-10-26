//! Cœur de ThunderLook (Tauri) : lecture du stockage local (J2) et réglages d'affichage.
//!
//! Dossier de données, par ordre de priorité : variable d'environnement `THUNDERLOOK_DATA`,
//! `dataDir` dans `config.json` (dossier de configuration de l'application), sinon `<données locales>\data`.

mod autoconfig_cmds;
mod calendar_cmds;
mod contacts_cmds;
mod dedupe_cmds;
mod exchange_cmds;
mod from_mail_cmds;
mod folders_cmds;
mod items_cmds;
mod mail_cmds;
mod mpe_import_cmds;
mod options_cmds;
mod reminders_cmds;
mod safety_cmds;
mod search_cmds;
mod shared_calendar;
mod tasks_recur_cmds;
mod toast;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindowBuilder, webview::NewWindowResponse};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use thunderlook_mailbox::Mailbox;
use thunderlook_model::{
    AccountSummary, AutoReply, CategoryInfo, JunkLists, JunkMark, ComposeKind, Draft, DraftAttachment, DraftAttachmentSource, FolderOp, FolderTree, IndexStatus, ListQuery, ListView,
    MessageOp, MessageSummary, MessageView, OpResult, OpenItem, SyncEvent, NewRule, RuleDraft, RuleRunReport, RuleRunRequest, RuleSummary, SearchQuery, Selection, Signature,
    SortField, format,
    quicksteps::{QuickStep, default_quick_steps},
    calendar::{CalendarQuery, CalendarView},
    settings::Settings,
};

/// Envoi et réception (J7) : un passage à la fois, dans un fil dédié ; une demande pendant un passage en relance un.
#[derive(Default)]
struct Syncer {
    running: Arc<AtomicBool>,
    again: Arc<AtomicBool>,
    /// « Travailler hors connexion » : ni relève, ni envoi, ni surveillance.
    offline: Arc<AtomicBool>,
    /// Comptes dont la Boîte de réception est surveillée (IDLE), et de quoi arrêter chaque surveillance.
    watched: Arc<Mutex<std::collections::HashMap<i64, Arc<thunderlook_sync::WatchStop>>>>,
    /// Début du dernier passage (millisecondes), pour la relève périodique.
    last_run: Arc<std::sync::atomic::AtomicI64>,
}

/// Minutes entre deux relèves périodiques (la Boîte de réception est en plus surveillée en continu).
const POLL_MINUTES: i64 = 10;

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// Surveille la Boîte de réception de chaque compte qui ne l'est pas encore ; un changement lance un passage.
fn start_watchers(app: &AppHandle) {
    let syncer = app.state::<Syncer>();
    if syncer.offline.load(Ordering::SeqCst) {
        return;
    }
    let Ok(dir) = data_dir(app) else { return };
    let Ok(accounts) = thunderlook_store::open(&dir).map_err(|e| e.to_string()).and_then(|c| thunderlook_sync::account::accounts(&c)) else {
        return;
    };
    for a in accounts {
        let cancel = Arc::new(thunderlook_sync::WatchStop::default());
        let fresh = syncer
            .watched
            .lock()
            .map(|mut w| match w.entry(a.source_id) {
                std::collections::hash_map::Entry::Occupied(_) => false,
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(cancel.clone());
                    true
                }
            })
            .unwrap_or(false);
        if !fresh {
            continue;
        }
        let (app, dir, offline, watched) = (app.clone(), dir.clone(), syncer.offline.clone(), syncer.watched.clone());
        std::thread::spawn(move || {
            let notify = app.clone();
            thunderlook_sync::watch_inbox_blocking(&dir, a.source_id, &offline, &cancel, move || start_sync(&notify), |e| {
                log::warn!("surveillance IDLE : {e}")
            });
            // Seulement la sienne : le compte a pu être retiré puis une autre surveillance lancée.
            if let Ok(mut w) = watched.lock()
                && w.get(&a.source_id).is_some_and(|c| Arc::ptr_eq(c, &cancel))
            {
                w.remove(&a.source_id);
            }
        });
    }
}

/// Arrête tout de suite la surveillance d'un compte (toutes si `None`), sans attendre la fin de l'attente IDLE.
fn stop_watchers(app: &AppHandle, source: Option<i64>) {
    let stopped: Vec<Arc<thunderlook_sync::WatchStop>> = app
        .state::<Syncer>()
        .watched
        .lock()
        .map(|mut w| match source {
            Some(s) => w.remove(&s).into_iter().collect(),
            None => w.drain().map(|(_, c)| c).collect(),
        })
        .unwrap_or_default();
    for c in stopped {
        c.stop();
    }
}

fn has_accounts(app: &AppHandle) -> bool {
    data_dir(app)
        .ok()
        .and_then(|d| thunderlook_store::open(&d).ok())
        .and_then(|c| c.query_row("SELECT EXISTS (SELECT 1 FROM account)", [], |r| r.get::<_, bool>(0)).ok())
        .unwrap_or(false)
}

/// Relève au démarrage, puis toutes les `POLL_MINUTES` minutes (seulement s'il existe un compte : sans compte, la
/// barre d'état reste muette ; F9 dit alors « Aucun compte de messagerie »).
fn start_scheduler(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        if has_accounts(&app) {
            start_sync(&app);
            start_watchers(&app);
        }
        shared_calendar::start(&app);
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
            shared_calendar::tick(&app);
            let last = app.state::<Syncer>().last_run.load(Ordering::SeqCst);
            if now_ms() - last >= POLL_MINUTES * 60_000 && has_accounts(&app) {
                start_sync(&app);
                start_watchers(&app);
            }
        }
    });
}

/// Lance un passage d'envoi et de réception sur tous les comptes ; l'interface suit l'événement « sync ».
fn start_sync(app: &AppHandle) {
    let syncer = app.state::<Syncer>();
    if syncer.offline.load(Ordering::SeqCst) {
        let _ = app.emit("sync", SyncEvent { message: "Hors connexion".into(), ..Default::default() });
        return;
    }
    syncer.last_run.store(now_ms(), Ordering::SeqCst);
    if syncer.running.swap(true, Ordering::SeqCst) {
        syncer.again.store(true, Ordering::SeqCst);
        return;
    }
    let (running, again) = (syncer.running.clone(), syncer.again.clone());
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            again.store(false, Ordering::SeqCst);
            let _ = app.emit("sync", SyncEvent { running: true, message: "Envoi et réception…".into(), ..Default::default() });
            let event = sync_all(&app);
            let _ = app.emit("sync", event);
            if !again.load(Ordering::SeqCst) {
                break;
            }
        }
        running.store(false, Ordering::SeqCst);
    });
}

fn sync_all(app: &AppHandle) -> SyncEvent {
    let dir = match data_dir(app) {
        Ok(d) => d,
        Err(e) => return SyncEvent { message: e.clone(), error: Some(e), ..Default::default() },
    };
    let accounts = thunderlook_store::open(&dir).map_err(|e| e.to_string()).and_then(|c| thunderlook_sync::account::accounts(&c));
    let accounts = match accounts {
        Ok(a) => a,
        Err(e) => return SyncEvent { message: format!("Échec de la relève : {e}"), error: Some(e), ..Default::default() },
    };
    if accounts.is_empty() {
        return SyncEvent { message: "Aucun compte de messagerie".into(), ..Default::default() };
    }
    let (mut sent, mut received, mut errors) = (0, 0, Vec::new());
    // Nouveaux messages de la Boîte de réception, par compte, pour les notifications.
    let mut arrived: Vec<(i64, String, Vec<i64>)> = Vec::new();
    for a in &accounts {
        match thunderlook_sync::sync_account_blocking(&dir, a.source_id) {
            Ok(r) => {
                sent += r.sent;
                received += r.new_messages;
                // Expéditeurs bloqués d'abord : leurs messages vont dans le Courrier indésirable, sans réponse ni règle.
                let new_ids = match junk_on_arrival(app, a.source_id, &r.new_inbox_ids) {
                    Ok(kept) => kept,
                    Err(e) => {
                        errors.push(format!("{} : courrier indésirable : {e}", a.address));
                        r.new_inbox_ids.clone()
                    }
                };
                if !new_ids.is_empty() {
                    arrived.push((a.source_id, a.address.clone(), new_ids.clone()));
                    // Réponses d'abord (sur les messages tels que reçus), puis règles (qui peuvent les déplacer).
                    match auto_replies(app, a.source_id, &new_ids) {
                        Ok(n) if n > 0 => app.state::<Syncer>().again.store(true, Ordering::SeqCst),
                        Ok(_) => {}
                        Err(e) => errors.push(format!("{} : réponse automatique : {e}", a.address)),
                    }
                    if a.rules_on_arrival {
                        if let Err(e) = rules_on_arrival(app, a.source_id, &new_ids) {
                            errors.push(format!("{} : règles : {e}", a.address));
                        }
                    }
                }
                errors.extend(r.send_error.map(|e| format!("{} : envoi : {e}", a.address)));
                errors.extend(r.push_error.map(|e| format!("{} : {e}", a.address)));
            }
            Err(e) => errors.push(format!("{} : {e}", a.address)),
        }
    }
    if received > 0 {
        app.state::<Indexer>().ensure(&dir);
    }
    notify_arrivals(app, &dir, &arrived);
    let mut parts = Vec::new();
    if sent > 0 {
        parts.push(if sent == 1 { "Message envoyé".to_string() } else { format!("{sent} messages envoyés") });
    }
    if received > 0 {
        parts.push(if received == 1 { "1 nouveau message".to_string() } else { format!("{received} nouveaux messages") });
    }
    let message = match (errors.first(), parts.is_empty()) {
        (Some(e), _) => format!("Échec de l'envoi/réception : {e}"),
        (None, true) => "Tous les dossiers sont à jour".into(),
        (None, false) => parts.join(" · "),
    };
    SyncEvent { running: false, message, sent, received, error: errors.into_iter().next() }
}

/// Notification Windows des nouveaux messages, une par compte : expéditeur et objet s'il n'y en a qu'un, sinon leur
/// nombre. Le clic (lien `thunderlook:`, voir [`toast`] et [`open_from_notification`]) ouvre la Boîte de réception du
/// compte, ou ce message, même depuis le Centre de notifications.
fn notify_arrivals(_app: &AppHandle, dir: &Path, arrived: &[(i64, String, Vec<i64>)]) {
    let conn = thunderlook_store::open(dir).ok();
    for (source, address, ids) in arrived {
        let single = match ids.as_slice() {
            [id] => conn.as_ref().and_then(|c| {
                c.query_row("SELECT from_name, from_addr, subject FROM message WHERE id = ?1", [id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
                })
                .ok()
            }),
            _ => None,
        };
        let lines: Vec<String> = match &single {
            Some((name, addr, subject)) => {
                let who = if name.trim().is_empty() { addr } else { name };
                let subject = if subject.trim().is_empty() { "(aucun objet)" } else { subject.as_str() };
                vec![who.clone(), subject.to_string()]
            }
            None => vec![format!("{} nouveaux messages pour {address}", ids.len())],
        };
        let message = match ids.as_slice() {
            [id] => Some(*id),
            _ => None,
        };
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        if let Err(e) = toast::show("ThunderLook", &lines, &toast::open_uri(*source, message)) {
            log::warn!("{e}");
        }
    }
}

/// Clic sur une notification : ThunderLook passe au premier plan et affiche la Boîte de réception du compte, ou le
/// message s'il était seul (dans son dossier actuel : une règle a pu le déplacer).
fn open_from_notification(app: &AppHandle, source: i64, message: Option<i64>) {
    bring_to_front(app);
    let Ok(conn) = data_dir(app).and_then(|d| thunderlook_store::open(&d).map_err(|e| e.to_string())) else { return };
    let current = message.and_then(|m| conn.query_row("SELECT folder_id FROM message WHERE id = ?1", [m], |r| r.get::<_, i64>(0)).ok());
    let item = match current {
        Some(folder) => OpenItem { folder_id: format!("f{folder}"), message_id: message.map(|m| format!("m{m}")) },
        None => {
            let Ok(inbox) = conn.query_row("SELECT id FROM folder WHERE source_id = ?1 AND role = 'inbox'", [source], |r| r.get::<_, i64>(0)) else { return };
            OpenItem { folder_id: format!("f{inbox}"), message_id: None }
        }
    };
    let _ = app.emit("open-item", item);
}

/// Fenêtre de ThunderLook restaurée et au premier plan (clic sur une notification, second lancement).
fn bring_to_front(app: &AppHandle) {
    if let Some(w) = app.webview_windows().into_values().next() {
        let _ = w.unminimize();
        let _ = w.show();
        // Windows ne laisse passer au premier plan qu'une fenêtre « au-dessus » : on l'y met le temps du focus.
        let _ = w.set_always_on_top(true);
        let _ = w.set_focus();
        let _ = w.set_always_on_top(false);
    }
}

fn junk_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(settings_path(app)?.with_file_name("junk.json"))
}

fn load_junk(app: &AppHandle) -> JunkLists {
    junk_path(app).ok().and_then(|p| fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn store_junk(app: &AppHandle, lists: &JunkLists) -> Result<(), String> {
    let path = junk_path(app)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(lists).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Nouveaux messages d'expéditeurs bloqués : déplacés dans le Courrier indésirable du compte (report sur le serveur
/// au passage suivant). Renvoie les autres, qui passent ensuite à la réponse automatique et aux règles.
fn junk_on_arrival(app: &AppHandle, source: i64, ids: &[i64]) -> Result<Vec<i64>, String> {
    let lists = load_junk(app);
    if lists.blocked.is_empty() || ids.is_empty() {
        return Ok(ids.to_vec());
    }
    let dir = data_dir(app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let junk: Option<i64> = conn
        .query_row("SELECT id FROM folder WHERE source_id = ?1 AND role = 'junk' ORDER BY imap_name IS NULL LIMIT 1", [source], |r| r.get(0))
        .ok();
    let Some(junk) = junk else { return Ok(ids.to_vec()) };
    let (mut blocked, mut kept) = (Vec::new(), Vec::new());
    for &id in ids {
        let from: String = conn.query_row("SELECT from_addr FROM message WHERE id = ?1", [id], |r| r.get(0)).unwrap_or_default();
        if lists.is_blocked(&from) { blocked.push(format!("m{id}")) } else { kept.push(id) }
    }
    if !blocked.is_empty() {
        let store = app.state::<Store>();
        with_mailbox(app, &store, |mb| mb.apply(&Selection::ids(blocked), &MessageOp::Move { folder_id: format!("f{junk}") }).map(|_| ()))?;
    }
    Ok(kept)
}

#[tauri::command]
fn junk_lists(app: AppHandle) -> JunkLists {
    load_junk(&app)
}

#[tauri::command]
fn set_junk_lists(app: AppHandle, lists: JunkLists) -> Result<(), String> {
    store_junk(&app, &lists)
}

/// « Bloquer l'expéditeur », « Ne jamais bloquer l'expéditeur / son domaine » sur les messages sélectionnés ;
/// renvoie le message de résultat.
#[tauri::command(async)]
fn junk_mark(app: AppHandle, store: State<Store>, selection: Selection, mark: JunkMark) -> Result<String, String> {
    let addresses = with_mailbox(&app, &store, |mb| mb.sender_addresses(&selection))?;
    if addresses.is_empty() {
        return Err("expéditeur sans adresse : rien à ajouter".into());
    }
    let mut lists = load_junk(&app);
    let entries: Vec<String> = match mark {
        JunkMark::SafeDomain => addresses.iter().filter_map(|a| a.rsplit_once('@').map(|(_, d)| format!("@{d}"))).collect(),
        _ => addresses,
    };
    for e in &entries {
        lists.add(e, mark == JunkMark::Block);
    }
    store_junk(&app, &lists)?;
    Ok(match mark {
        JunkMark::Block => format!("Expéditeur(s) bloqué(s) : {}", entries.join(", ")),
        _ => format!("Ne seront jamais bloqués : {}", entries.join(", ")),
    })
}

fn auto_reply_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(settings_path(app)?.with_file_name("autoreply.json"))
}

fn load_auto_reply(app: &AppHandle) -> AutoReply {
    auto_reply_path(app).ok().and_then(|p| fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn store_auto_reply(app: &AppHandle, cfg: &AutoReply) -> Result<(), String> {
    let path = auto_reply_path(app)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Réponses automatiques aux nouveaux messages d'un compte ; renvoie le nombre de réponses mises dans la Boîte d'envoi.
fn auto_replies(app: &AppHandle, source: i64, ids: &[i64]) -> Result<usize, String> {
    let mut cfg = load_auto_reply(app);
    if !thunderlook_sync::autoreply::active(&cfg, &chrono::Local::now().format("%Y-%m-%d").to_string()) {
        return Ok(0);
    }
    let dir = data_dir(app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let account = thunderlook_sync::account::account(&conn, source)?;
    let replied = thunderlook_sync::autoreply::queue_replies(&conn, &dir, &account, ids, &cfg)?;
    if !replied.is_empty() {
        cfg.replied.extend(replied.iter().cloned());
        store_auto_reply(app, &cfg)?;
    }
    Ok(replied.len())
}

#[tauri::command]
fn get_auto_reply(app: AppHandle) -> AutoReply {
    load_auto_reply(&app)
}

/// Enregistre la réponse automatique ; une nouvelle activation repart d'une liste d'expéditeurs vide.
#[tauri::command]
fn set_auto_reply(app: AppHandle, config: AutoReply) -> Result<(), String> {
    let previous = load_auto_reply(&app);
    let mut cfg = config;
    cfg.replied = if cfg.enabled && previous.enabled { previous.replied } else { Vec::new() };
    store_auto_reply(&app, &cfg)
}

/// Règles actives appliquées aux nouveaux messages de la Boîte de réception d'un compte (comme Outlook à l'arrivée).
fn rules_on_arrival(app: &AppHandle, source: i64, ids: &[i64]) -> Result<(), String> {
    let dir = data_dir(app)?;
    let inbox: i64 = thunderlook_store::open(&dir)
        .map_err(|e| e.to_string())?
        .query_row("SELECT id FROM folder WHERE source_id = ?1 AND role = 'inbox' AND imap_name IS NOT NULL", [source], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let store = app.state::<Store>();
    with_mailbox(app, &store, |mb| {
        // Seulement le jeu de règles du compte (copié depuis « Règles et alertes »), dont les cibles sont ses dossiers.
        let set = format!("account:{source}");
        let rule_ids: Vec<String> = mb.rules()?.into_iter().filter(|r| r.origin == set && r.active && r.error.is_none()).map(|r| r.id).collect();
        if rule_ids.is_empty() {
            return Ok(());
        }
        let req = thunderlook_model::RuleRunRequest {
            rule_ids,
            folder_id: format!("f{inbox}"),
            subfolders: false,
            filter: thunderlook_model::RunFilter::All,
            apply: true,
            only: ids.iter().map(|id| format!("m{id}")).collect(),
            confirm_moves: None,
        };
        let report = mb.run_rules(&req)?;
        if report.needs_confirmation {
            return Err(format!(
                "{} déplacements prévus par les règles à l'arrivée, au-delà de {} : rien n'est appliqué (Règles et alertes › Exécuter les règles pour un aperçu)",
                report.moves,
                thunderlook_model::RULE_MOVES_LIMIT
            ));
        }
        report.errors.into_iter().next().map_or(Ok(()), Err)
    })
}

/// Boîte aux lettres ouverte à la première demande ; `Err` mémorise l'échec d'ouverture.
struct Store(Mutex<Option<Result<Mailbox, String>>>);

/// Indexation plein texte en tâche de fond (un seul fil à la fois).
#[derive(Default)]
struct Indexer {
    status: Arc<Mutex<IndexStatus>>,
    running: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl Indexer {
    /// Lance l'indexation des messages en attente, si elle ne tourne pas déjà.
    fn ensure(&self, data_dir: &Path) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let (status, running, stop) = (self.status.clone(), self.running.clone(), self.stop.clone());
        let dir = data_dir.to_path_buf();
        std::thread::spawn(move || {
            let mut update = |done, total| {
                if let Ok(mut s) = status.lock() {
                    *s = IndexStatus { done, total, running: true };
                }
            };
            if let Err(e) = thunderlook_mailbox::index_pending(&dir, &stop, &mut update) {
                log::warn!("indexation : {e}");
            }
            if let Ok(mut s) = status.lock() {
                s.running = false;
            }
            running.store(false, Ordering::SeqCst);
        });
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Config {
    data_dir: Option<PathBuf>,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("THUNDERLOOK_DATA").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let config_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let config: Config = fs::read_to_string(config_dir.join("config.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    match config.data_dir {
        Some(d) => Ok(d),
        None => Ok(app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("data")),
    }
}

/// Exécute `f` sur la boîte aux lettres, en l'ouvrant au besoin (l'indexation démarre alors).
fn with_mailbox<T>(app: &AppHandle, store: &Store, f: impl FnOnce(&mut Mailbox) -> Result<T, String>) -> Result<T, String> {
    let mut guard = store.0.lock().map_err(|_| "verrou de la base empoisonné".to_string())?;
    let mailbox = guard.get_or_insert_with(|| {
        let mb = data_dir(app).and_then(|d| Mailbox::open(&d));
        if let Ok(mb) = &mb {
            app.state::<Indexer>().ensure(mb.data_dir());
        }
        mb
    });
    match mailbox {
        Ok(mb) => f(mb),
        Err(e) => Err(e.clone()),
    }
}

#[tauri::command(async)]
fn folder_tree(app: AppHandle, store: State<Store>) -> FolderTree {
    let dir = data_dir(&app).map(|d| d.display().to_string()).unwrap_or_default();
    with_mailbox(&app, &store, |mb| mb.folder_tree())
        .unwrap_or_else(|e| FolderTree { sections: vec![], data_dir: dir, error: Some(e) })
}

#[tauri::command(async)]
fn open_list(app: AppHandle, store: State<Store>, query: ListQuery) -> Result<ListView, String> {
    with_mailbox(&app, &store, |mb| mb.open_list(&query))
}

#[tauri::command(async)]
fn list_rows(app: AppHandle, store: State<Store>, token: u64, start: u32, count: u32) -> Result<Vec<MessageSummary>, String> {
    with_mailbox(&app, &store, |mb| mb.rows(token, start, count))
}

/// Déplie ou replie la conversation à la position `pos` (`expand` absent : inverse).
#[tauri::command(async)]
fn toggle_conversation(app: AppHandle, store: State<Store>, token: u64, pos: u32, expand: Option<bool>) -> Result<ListView, String> {
    with_mailbox(&app, &store, |mb| mb.toggle_conversation(token, pos, expand))
}

#[tauri::command(async)]
fn message_view(app: AppHandle, store: State<Store>, id: String, allow_remote: bool) -> Result<MessageView, String> {
    with_mailbox(&app, &store, |mb| mb.message_view(&id, allow_remote))
}

#[tauri::command(async)]
fn apply_op(app: AppHandle, store: State<Store>, selection: Selection, op: MessageOp) -> Result<OpResult, String> {
    let copy = matches!(op, MessageOp::Copy { .. });
    with_mailbox(&app, &store, |mb| {
        let r = mb.apply(&selection, &op)?;
        // Les copies sont de nouveaux messages, à indexer.
        if copy {
            app.state::<Indexer>().ensure(mb.data_dir());
        }
        Ok(r)
    })
}

#[tauri::command(async)]
fn search(app: AppHandle, store: State<Store>, query: SearchQuery, sort: SortField, descending: bool) -> Result<ListView, String> {
    with_mailbox(&app, &store, |mb| mb.search(&query, sort, descending))
}

#[tauri::command]
fn index_status(indexer: State<Indexer>) -> IndexStatus {
    indexer.status.lock().map(|s| *s).unwrap_or_default()
}

/// Position d'un message dans une vue ouverte (clic sur une notification).
#[tauri::command(async)]
fn message_position(app: AppHandle, store: State<Store>, token: u64, id: String) -> Result<Option<u32>, String> {
    with_mailbox(&app, &store, |mb| mb.position_in_view(token, &id))
}

#[tauri::command(async)]
fn folder_op(app: AppHandle, store: State<Store>, op: FolderOp) -> Result<OpResult, String> {
    with_mailbox(&app, &store, |mb| mb.folder_op(&op))
}

/// Annule la dernière opération ; renvoie son libellé (`None` : rien à annuler).
#[tauri::command(async)]
fn undo(app: AppHandle, store: State<Store>) -> Result<Option<String>, String> {
    with_mailbox(&app, &store, |mb| mb.undo())
}

#[tauri::command(async)]
fn categories(app: AppHandle, store: State<Store>) -> Result<Vec<CategoryInfo>, String> {
    with_mailbox(&app, &store, |mb| mb.categories())
}

/// Règles importées (J6), dans l'ordre d'exécution.
#[tauri::command(async)]
fn rules(app: AppHandle, store: State<Store>) -> Result<Vec<RuleSummary>, String> {
    with_mailbox(&app, &store, |mb| mb.rules())
}

#[tauri::command(async)]
fn set_rule_active(app: AppHandle, store: State<Store>, id: String, active: bool) -> Result<(), String> {
    with_mailbox(&app, &store, |mb| mb.set_rule_active(&id, active))
}

/// « Créer une règle » : valeurs proposées d'après le message `m<id>`.
#[tauri::command(async)]
fn rule_draft(app: AppHandle, store: State<Store>, id: String) -> Result<RuleDraft, String> {
    with_mailbox(&app, &store, |mb| mb.rule_draft(&id))
}

/// Enregistre une règle créée depuis un message, dans le jeu de règles de son compte ; renvoie `r<id>`.
#[tauri::command(async)]
fn create_rule(app: AppHandle, store: State<Store>, rule: NewRule) -> Result<String, String> {
    with_mailbox(&app, &store, |mb| mb.create_rule(&rule))
}

/// Importe un `.rwz` choisi par l'utilisateur ; renvoie le nombre de règles (`None` s'il annule).
#[tauri::command(async)]
fn import_rwz(app: AppHandle, store: State<Store>) -> Result<Option<usize>, String> {
    let Some(file) = app.dialog().file().add_filter("Règles Outlook", &["rwz"]).blocking_pick_file() else { return Ok(None) };
    let path = file.into_path().map_err(|e| e.to_string())?;
    with_mailbox(&app, &store, |mb| mb.import_rwz(&path)).map(Some)
}

/// « Exécuter maintenant » : aperçu ou exécution ; les messages modifiés sont à réindexer s'ils ont été copiés.
#[tauri::command(async)]
fn run_rules(app: AppHandle, store: State<Store>, request: RuleRunRequest) -> Result<RuleRunReport, String> {
    with_mailbox(&app, &store, |mb| {
        let report = mb.run_rules(&request)?;
        if report.applied {
            app.state::<Indexer>().ensure(mb.data_dir());
        }
        Ok(report)
    })
}

/// Comptes de messagerie (J7).
#[tauri::command(async)]
fn accounts(app: AppHandle) -> Result<Vec<AccountSummary>, String> {
    let conn = thunderlook_store::open(&data_dir(&app)?).map_err(|e| e.to_string())?;
    thunderlook_sync::account::summaries(&conn)
}

/// Chiffrement choisi dans « Ajouter un compte » : « tls » (SSL/TLS dès l'ouverture) ou « starttls » ; absent : TLS.
fn security_arg(s: Option<&str>) -> Result<thunderlook_sync::Security, String> {
    match s.map(str::trim) {
        None | Some("" | "tls") => Ok(thunderlook_sync::Security::Tls),
        Some("starttls") => Ok(thunderlook_sync::Security::StartTls),
        Some(other) => Err(format!("chiffrement « {other} » non pris en charge (SSL/TLS ou STARTTLS)")),
    }
}

/// Ajoute un compte après avoir vérifié l'identification IMAP et SMTP (par défaut TLS, ports 993 et 465 ; STARTTLS
/// sur 143 et 587 au choix), puis lance la première relève. Le mot de passe va dans le gestionnaire d'identifiants de
/// Windows.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
fn add_account(
    app: AppHandle,
    address: String,
    name: String,
    password: String,
    imap_host: String,
    smtp_host: String,
    imap_port: Option<u16>,
    imap_security: Option<String>,
    smtp_port: Option<u16>,
    smtp_security: Option<String>,
) -> Result<Vec<AccountSummary>, String> {
    // Domaine en minuscules (l'adresse sert d'expéditeur) ; la partie locale est gardée telle quelle.
    let address = match address.trim().rsplit_once('@') {
        Some((local, domain)) if !local.is_empty() && domain.contains('.') => format!("{local}@{}", domain.to_ascii_lowercase()),
        _ => return Err("Adresse de messagerie invalide.".into()),
    };
    let mut ep = thunderlook_sync::Endpoints::hostinger(&address);
    if !imap_host.trim().is_empty() {
        ep.imap.host = imap_host.trim().to_string();
    }
    if !smtp_host.trim().is_empty() {
        ep.smtp.host = smtp_host.trim().to_string();
    }
    ep.imap.security = security_arg(imap_security.as_deref())?;
    ep.smtp.security = security_arg(smtp_security.as_deref())?;
    ep.imap.port = imap_port.filter(|p| *p > 0).unwrap_or(if ep.imap.security == thunderlook_sync::Security::Tls { 993 } else { 143 });
    ep.smtp.port = smtp_port.filter(|p| *p > 0).unwrap_or(if ep.smtp.security == thunderlook_sync::Security::Tls { 465 } else { 587 });
    // Vérification dans un fil à part : la commande tourne déjà sur le runtime de Tauri.
    let (ep2, pw) = (ep.clone(), password.clone());
    std::thread::spawn(move || -> Result<(), String> {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        rt.block_on(async {
            thunderlook_sync::imap_login(&ep2.imap, &ep2.username, &pw).await?.logout().await.map_err(|e| e.to_string())?;
            thunderlook_sync::smtp_check(&ep2.smtp, &ep2.username, &pw).await
        })
    })
    .join()
    .map_err(|_| "vérification de la connexion interrompue".to_string())??;
    let conn = thunderlook_store::open(&data_dir(&app)?).map_err(|e| e.to_string())?;
    thunderlook_sync::account::create(&conn, &address, &name, &ep, &password)?;
    start_sync(&app);
    start_watchers(&app);
    thunderlook_sync::account::summaries(&conn)
}

/// Retire un compte de ThunderLook (rien n'est effacé sur le serveur).
#[tauri::command(async)]
fn remove_account(app: AppHandle, id: String) -> Result<Vec<AccountSummary>, String> {
    let source: i64 = id.strip_prefix('s').and_then(|n| n.parse().ok()).ok_or_else(|| format!("identifiant invalide : {id}"))?;
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    // Surveillance arrêtée d'abord : plus de connexion au serveur pour un compte retiré.
    stop_watchers(&app, Some(source));
    thunderlook_sync::account::remove(&conn, &dir, source)?;
    thunderlook_sync::account::summaries(&conn)
}

/// Règles à l'arrivée pour un compte (case de « Paramètres du compte »).
#[tauri::command(async)]
fn set_account_rules(app: AppHandle, id: String, on: bool) -> Result<Vec<AccountSummary>, String> {
    let source: i64 = id.strip_prefix('s').and_then(|n| n.parse().ok()).ok_or_else(|| format!("identifiant invalide : {id}"))?;
    let conn = thunderlook_store::open(&data_dir(&app)?).map_err(|e| e.to_string())?;
    thunderlook_sync::account::set_rules_on_arrival(&conn, source, on)?;
    thunderlook_sync::account::summaries(&conn)
}

/// Copie les règles actives d'un jeu vers un compte (dossiers manquants créés, aussi sur le serveur).
#[tauri::command(async)]
fn copy_rules(app: AppHandle, store: State<Store>, origin: String, account: String) -> Result<String, String> {
    let source: i64 = account.strip_prefix('s').and_then(|n| n.parse().ok()).ok_or_else(|| format!("identifiant invalide : {account}"))?;
    let (n, created) = with_mailbox(&app, &store, |mb| mb.copy_rules_to_account(&origin, source))?;
    if !created.is_empty() {
        start_sync(&app);
    }
    Ok(if created.is_empty() {
        format!("{n} règle(s) copiée(s)")
    } else {
        format!("{n} règle(s) copiée(s) ; dossiers créés : {}", created.join(", "))
    })
}

/// « Envoyer » : le message va dans la Boîte d'envoi et un passage d'envoi démarre aussitôt.
#[tauri::command(async)]
fn send_draft(app: AppHandle, store: State<Store>, draft: Draft) -> Result<String, String> {
    let id = with_mailbox(&app, &store, |mb| mb.send_draft(&draft))?;
    start_sync(&app);
    Ok(id)
}

/// F9 : envoyer et recevoir sur tous les comptes.
#[tauri::command]
fn sync_now(app: AppHandle) {
    start_sync(&app);
    start_watchers(&app);
}

/// « Travailler hors connexion » ; renvoie le nouvel état. En repassant en ligne, un passage démarre.
#[tauri::command]
fn set_offline(app: AppHandle, offline: bool) -> bool {
    app.state::<Syncer>().offline.store(offline, Ordering::SeqCst);
    if offline {
        stop_watchers(&app, None);
        let _ = app.emit("sync", SyncEvent { message: "Hors connexion".into(), ..Default::default() });
    } else {
        start_sync(&app);
        start_watchers(&app);
    }
    offline
}

/// Prépare un nouveau message, une réponse, une réponse à tous ou un transfert (J5).
#[tauri::command(async)]
fn compose(app: AppHandle, store: State<Store>, kind: ComposeKind, source_id: Option<String>, folder_id: Option<String>) -> Result<Draft, String> {
    let mut draft = with_mailbox(&app, &store, |mb| mb.compose(kind, source_id.as_deref()))?;
    // Champ « De » : le compte du message d'origine, sinon celui du dossier affiché (comme Outlook), s'il en est un.
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let folder_of = |id: &str| -> Option<i64> {
        let n: i64 = id.trim_start_matches("fav-").strip_prefix('f')?.parse().ok()?;
        conn.query_row("SELECT source_id FROM folder WHERE id = ?1", [n], |r| r.get(0)).ok()
    };
    let message_of = |id: &str| -> Option<i64> {
        let n: i64 = id.strip_prefix('m')?.parse().ok()?;
        conn.query_row("SELECT f.source_id FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1", [n], |r| r.get(0)).ok()
    };
    let source = source_id.as_deref().and_then(message_of).or_else(|| folder_id.as_deref().and_then(folder_of));
    let has_account = |s: i64| conn.query_row("SELECT 1 FROM account WHERE source_id = ?1", [s], |_| Ok(())).is_ok();
    draft.from_account = source.filter(|s| has_account(*s)).map(|s| format!("s{s}"));
    Ok(draft)
}

/// Enregistre un brouillon ; le message enregistré est à indexer.
#[tauri::command(async)]
fn save_draft(app: AppHandle, store: State<Store>, draft: Draft) -> Result<Draft, String> {
    with_mailbox(&app, &store, |mb| {
        let saved = mb.save_draft(&draft)?;
        app.state::<Indexer>().ensure(mb.data_dir());
        Ok(saved)
    })
}

/// Rouvre un brouillon ; `None` si le message n'en est pas un.
#[tauri::command(async)]
fn open_draft(app: AppHandle, store: State<Store>, id: String) -> Result<Option<Draft>, String> {
    with_mailbox(&app, &store, |mb| if mb.is_draft(&id)? { mb.open_draft(&id).map(Some) } else { Ok(None) })
}

/// Nouveau message d'après un modèle `.oft` choisi par l'utilisateur (`None` s'il annule).
#[tauri::command(async)]
fn compose_from_template(app: AppHandle) -> Result<Option<Draft>, String> {
    let Some(file) = app.dialog().file().add_filter("Modèle Outlook", &["oft", "msg"]).blocking_pick_file() else { return Ok(None) };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let scratch = std::env::temp_dir().join("ThunderLook").join(format!("modele-{stamp}"));
    thunderlook_mailbox::draft_from_oft(&path, &scratch).map(Some)
}

#[tauri::command(async)]
fn signatures() -> Vec<Signature> {
    thunderlook_mailbox::signatures()
}

/// Fichiers à joindre, choisis par l'utilisateur (vide s'il annule).
#[tauri::command(async)]
fn pick_attachments(app: AppHandle) -> Vec<DraftAttachment> {
    let Some(files) = app.dialog().file().blocking_pick_files() else { return Vec::new() };
    files
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .map(|path| DraftAttachment {
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            size_label: format::size(fs::metadata(&path).map(|m| m.len()).unwrap_or(0)),
            source: DraftAttachmentSource::File { path: path.to_string_lossy().into_owned() },
        })
        .collect()
}

/// Nom de fichier sûr pour Windows.
fn safe_file_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_control() || r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
        .collect();
    let s = s.trim().trim_end_matches(['.', ' ']).to_string();
    if s.is_empty() { "piece-jointe".into() } else { s }
}

/// Écrit la pièce jointe dans le dossier temporaire et l'ouvre avec l'application associée.
#[tauri::command(async)]
fn open_attachment(app: AppHandle, store: State<Store>, id: String, index: u32) -> Result<(), String> {
    let att = with_mailbox(&app, &store, |mb| mb.attachment(&id, index))?;
    if att.blocked {
        return Err(format!(
            "« {} » n'est pas ouvert : ce type de fichier peut exécuter du code. Utilisez « Enregistrer sous ».",
            att.name
        ));
    }
    let dir = std::env::temp_dir().join("ThunderLook").join(format!("{id}-{index}"));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(safe_file_name(&att.name));
    fs::write(&path, &att.bytes).map_err(|e| format!("{} : {e}", path.display()))?;
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

/// « Enregistrer sous » ; renvoie `false` si l'utilisateur annule.
#[tauri::command(async)]
fn save_attachment(app: AppHandle, store: State<Store>, id: String, index: u32) -> Result<bool, String> {
    let att = with_mailbox(&app, &store, |mb| mb.attachment(&id, index))?;
    let Some(target) = app.dialog().file().set_file_name(safe_file_name(&att.name)).blocking_save_file() else {
        return Ok(false);
    };
    let path = target.into_path().map_err(|e| e.to_string())?;
    fs::write(&path, &att.bytes).map_err(|e| format!("{} : {e}", path.display()))?;
    Ok(true)
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Settings {
    settings_path(&app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .unwrap_or_default()
        .clamped()
}

/// Écriture atomique : fichier temporaire puis renommage.
#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let path = settings_path(&app)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(&settings.clamped()).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Actions rapides (J6) : `quicksteps.json`, à défaut celles d'Outlook.
#[tauri::command]
fn load_quick_steps(app: AppHandle) -> Vec<QuickStep> {
    settings_path(&app)
        .ok()
        .and_then(|p| fs::read_to_string(p.with_file_name("quicksteps.json")).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(default_quick_steps)
}

/// Écriture atomique, comme les réglages.
#[tauri::command]
fn save_quick_steps(app: AppHandle, steps: Vec<QuickStep>) -> Result<(), String> {
    let path = settings_path(&app)?.with_file_name("quicksteps.json");
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(&steps).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Calendrier (J9, option A : lecture seule) : rendez-vous lus une fois puis gardés. Une écriture ou un import
/// (`clear_calendar`) ne vide pas le cache : il est marqué périmé et, à la lecture suivante, seuls les rendez-vous dont
/// l'empreinte a changé (modifiés, ajoutés, supprimés) sont relus.
#[derive(Default)]
struct CalendarCache(Mutex<CalendarState>);

#[derive(Default)]
struct CalendarState {
    calendar: Option<thunderlook_calendar::Calendar>,
    /// Empreintes des rendez-vous de la base au moment de leur lecture.
    stamps: std::collections::HashMap<i64, thunderlook_calendar::Stamp>,
    stale: bool,
}

/// Calendriers affichés : `calendar.json` (identifiants `c<dossier>`) ; absent = calendrier principal seul.
fn shown_calendars(app: &AppHandle) -> Option<Vec<String>> {
    let path = settings_path(app).ok()?.with_file_name("calendar.json");
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// Calendriers de la base, puis calendriers importés (couleurs à la suite).
fn all_calendars(app: &AppHandle, conn: &thunderlook_store::rusqlite::Connection, dir: &Path) -> Result<Vec<thunderlook_model::calendar::CalendarInfo>, String> {
    let mut calendars = thunderlook_calendar::view::calendars(conn)?;
    let n = calendars.len();
    let (imported, _) = thunderlook_calendar::imported::load(dir);
    calendars.extend(imported.into_iter().enumerate().map(|(i, (mut c, _))| {
        c.color = ((n + i) % 8) as u8;
        c
    }));
    let shown = shown_calendars(app).unwrap_or_else(|| thunderlook_calendar::view::default_shown(&calendars));
    calendars.iter_mut().for_each(|c| c.shown = shown.contains(&c.id));
    shared_calendar::mark_shared(app, conn, &mut calendars);
    for (id, c) in thunderlook_folders::colors(dir) {
        if let Some(cal) = calendars.iter_mut().find(|k| k.id == id) {
            cal.color = folders_cmds::CUSTOM_COLOR_BASE + c;
        }
    }
    Ok(calendars)
}

/// Rendez-vous de la base et des calendriers importés, lus une fois puis gardés (fichiers illisibles : `id` 0).
fn with_calendar<T>(
    cache: &CalendarCache,
    conn: &thunderlook_store::rusqlite::Connection,
    dir: &Path,
    f: impl FnOnce(&thunderlook_calendar::Calendar) -> T,
) -> Result<T, String> {
    let mut state = cache.0.lock().map_err(|_| "verrou du calendrier empoisonné".to_string())?;
    if state.calendar.is_none() || state.stale {
        refresh_calendar(&mut state, conn, dir)?;
    }
    Ok(f(state.calendar.as_ref().unwrap()))
}

/// Lecture complète au premier appel ; ensuite, relecture des seuls rendez-vous changés (`thunderlook_calendar::refresh`).
/// Les calendriers importés (fichiers `.ics`) sont relus entiers : un import ou un retrait est la raison d'être du rafraîchissement.
fn refresh_calendar(state: &mut CalendarState, conn: &thunderlook_store::rusqlite::Connection, dir: &Path) -> Result<(), String> {
    let old = state.calendar.take().unwrap_or(thunderlook_calendar::Calendar { appointments: Vec::new(), issues: Vec::new() });
    let (mut cal, stamps) = match thunderlook_calendar::refresh(old, &state.stamps, conn) {
        Ok(r) => r,
        Err(e) => {
            // Cache vidé : la prochaine lecture repart d'une lecture complète.
            state.stamps.clear();
            return Err(e);
        }
    };
    let (imported, errors) = thunderlook_calendar::imported::load(dir);
    cal.appointments.extend(imported.into_iter().flat_map(|(_, items)| items));
    cal.issues.extend(errors.into_iter().map(|detail| thunderlook_calendar::Issue { id: 0, subject: String::new(), detail }));
    state.calendar = Some(cal);
    state.stamps = stamps;
    state.stale = false;
    Ok(())
}

/// Marque le cache périmé (écriture, import, retrait) ; la prochaine lecture ne relira que ce qui a changé.
fn clear_calendar(cache: &CalendarCache) -> Result<(), String> {
    cache.0.lock().map_err(|_| "verrou du calendrier empoisonné".to_string())?.stale = true;
    Ok(())
}

/// Contacts (J10) : dossiers qui en contiennent.
#[tauri::command(async)]
fn contact_folders(app: AppHandle) -> Result<Vec<thunderlook_model::contacts::ContactFolder>, String> {
    let conn = thunderlook_store::open(&data_dir(&app)?).map_err(|e| e.to_string())?;
    thunderlook_contacts::view::folders(&conn)
}

/// Contacts d'un dossier (`k<id>`), triés comme Outlook.
#[tauri::command(async)]
fn contact_list(app: AppHandle, folder: String) -> Result<thunderlook_model::contacts::ContactList, String> {
    let id: i64 = folder.strip_prefix('k').and_then(|f| f.parse().ok()).ok_or("dossier de contacts inconnu")?;
    let conn = thunderlook_store::open(&data_dir(&app)?).map_err(|e| e.to_string())?;
    let all = thunderlook_contacts::load(&conn, Some(id), None)?;
    let mut list = thunderlook_contacts::view::list(&all, id);
    list.source_note = None;
    Ok(list)
}

// --- Tâches et notes -----------------------------------------------------------------------------------------------

fn items_conn(app: &AppHandle) -> Result<(std::path::PathBuf, thunderlook_store::rusqlite::Connection), String> {
    let dir = data_dir(app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    Ok((dir, conn))
}

fn task_num(id: &str) -> Result<i64, String> {
    id.strip_prefix('t').and_then(|f| f.parse().ok()).ok_or_else(|| "tâche inconnue".to_string())
}

fn note_num(id: &str) -> Result<i64, String> {
    id.strip_prefix('n').and_then(|f| f.parse().ok()).ok_or_else(|| "note inconnue".to_string())
}

#[tauri::command(async)]
fn tasks_list(app: AppHandle) -> Result<thunderlook_model::items::TaskList, String> {
    let (_, conn) = items_conn(&app)?;
    thunderlook_items::tasks(&conn)
}

/// Boîte « Tâche » : tâche existante (`id`), ou nouvelle dans le dossier le plus fourni.
#[tauri::command(async)]
fn task_form(app: AppHandle, id: Option<String>, folder: Option<String>) -> Result<thunderlook_model::items::TaskForm, String> {
    let (dir, conn) = items_conn(&app)?;
    match id {
        Some(id) => thunderlook_items::task_form(&conn, &dir, task_num(&id)?),
        None => {
            let folder_id = match folder.filter(|f| !f.is_empty()) {
                Some(f) => f,
                None => thunderlook_items::default_folder(&conn, "IPM.Task%")?.ok_or("aucun dossier de tâches")?,
            };
            Ok(thunderlook_model::items::TaskForm { folder_id, ..Default::default() })
        }
    }
}

#[tauri::command(async)]
fn task_save(app: AppHandle, form: thunderlook_model::items::TaskForm) -> Result<String, String> {
    let (dir, mut conn) = items_conn(&app)?;
    if let Some(n) = form.id.as_deref().and_then(|id| task_num(id).ok()) {
        thunderlook_safety::snapshot_edit(&conn, &dir, n)?;
    }
    thunderlook_items::task_save(&mut conn, &dir, &form)
}

#[tauri::command(async)]
fn task_complete(app: AppHandle, id: String, done: bool) -> Result<(), String> {
    let (dir, mut conn) = items_conn(&app)?;
    thunderlook_safety::snapshot_edit(&conn, &dir, task_num(&id)?)?;
    thunderlook_items::task_complete(&mut conn, &dir, task_num(&id)?, done)
}

#[tauri::command(async)]
fn task_delete(app: AppHandle, id: String) -> Result<(), String> {
    let (_, mut conn) = items_conn(&app)?;
    thunderlook_items::task_delete(&mut conn, task_num(&id)?)
}

#[tauri::command(async)]
fn notes_list(app: AppHandle) -> Result<thunderlook_model::items::NoteList, String> {
    let (dir, conn) = items_conn(&app)?;
    thunderlook_items::notes(&conn, &dir)
}

#[tauri::command(async)]
fn note_form(app: AppHandle, id: Option<String>) -> Result<thunderlook_model::items::NoteForm, String> {
    let (dir, conn) = items_conn(&app)?;
    match id {
        Some(id) => thunderlook_items::note_form(&conn, &dir, note_num(&id)?),
        None => Ok(thunderlook_model::items::NoteForm {
            folder_id: thunderlook_items::default_folder(&conn, "IPM.StickyNote%")?.ok_or("aucun dossier de notes")?,
            color: 3,
            ..Default::default()
        }),
    }
}

#[tauri::command(async)]
fn note_save(app: AppHandle, form: thunderlook_model::items::NoteForm) -> Result<String, String> {
    let (dir, mut conn) = items_conn(&app)?;
    if let Some(n) = form.id.as_deref().and_then(|id| note_num(id).ok()) {
        thunderlook_safety::snapshot_edit(&conn, &dir, n)?;
    }
    thunderlook_items::note_save(&mut conn, &dir, &form)
}

#[tauri::command(async)]
fn note_delete(app: AppHandle, id: String) -> Result<(), String> {
    let (_, mut conn) = items_conn(&app)?;
    thunderlook_items::note_delete(&mut conn, note_num(&id)?)
}

/// Échange avec le téléphone par fichiers : exporte (`.vcf`, `.ics` de tâches, `.vnt`) tous les contacts du dossier affiché,
/// toutes les tâches ou toutes les notes. `None` si l'utilisateur annule.
#[tauri::command(async)]
fn exchange_export(app: AppHandle, kind: String, folder: Option<String>) -> Result<Option<String>, String> {
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let (text, count, ext, filter, name) = match kind.as_str() {
        "contacts" => {
            let folder: i64 = folder.as_deref().and_then(|f| f.strip_prefix('k')).and_then(|f| f.parse().ok()).ok_or("choisissez un dossier de contacts")?;
            let all = thunderlook_contacts::load(&conn, Some(folder), None)?;
            let forms: Vec<_> = all.contacts.iter().map(|c| thunderlook_contacts::write::form_from(c, &dir)).collect();
            (thunderlook_contacts::vcard::export(&forms, thunderlook_contacts::vcard::Version::V30), forms.len(), "vcf", "vCard", "contacts")
        }
        "tasks" => {
            let list = thunderlook_items::tasks(&conn)?;
            let forms: Vec<_> = list.rows.iter().filter_map(|r| r.id.strip_prefix('t').and_then(|n| n.parse().ok())).map(|n| thunderlook_items::task_form(&conn, &dir, n)).collect::<Result<_, _>>()?;
            (thunderlook_items::vtodo::export(&forms), forms.len(), "ics", "Tâches (iCalendar)", "taches")
        }
        "notes" => {
            let list = thunderlook_items::notes(&conn, &dir)?;
            let forms: Vec<_> = list.rows.iter().filter_map(|r| r.id.strip_prefix('n').and_then(|n| n.parse().ok())).map(|n| thunderlook_items::note_form(&conn, &dir, n)).collect::<Result<_, _>>()?;
            (thunderlook_items::vnote::export(&forms), forms.len(), "vnt", "Notes (vNote)", "notes")
        }
        _ => return Err("type d'échange inconnu".into()),
    };
    let Some(target) = app.dialog().file().add_filter(filter, &[ext]).set_file_name(format!("{name}.{ext}")).blocking_save_file() else { return Ok(None) };
    let path = target.into_path().map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("{} : {e}", path.display()))?;
    Ok(Some(format!("{count} élément(s) exporté(s) dans {}", path.display())))
}

/// Importe un fichier du téléphone dans la base : les éléments déjà présents (même nom et même adresse, même objet) sont
/// ignorés. `None` si l'utilisateur annule.
#[tauri::command(async)]
fn exchange_import(app: AppHandle, kind: String, folder: Option<String>) -> Result<Option<String>, String> {
    let (filter, exts): (&str, &[&str]) = match kind.as_str() {
        "contacts" => ("vCard", &["vcf"]),
        "tasks" => ("Tâches (iCalendar)", &["ics"]),
        "notes" => ("Notes (vNote)", &["vnt", "vnote"]),
        _ => return Err("type d'échange inconnu".into()),
    };
    let Some(file) = app.dialog().file().add_filter(filter, exts).blocking_pick_file() else { return Ok(None) };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let bytes = fs::read(&path).map_err(|e| format!("{} : {e}", path.display()))?;
    let text = String::from_utf8(bytes.clone()).unwrap_or_else(|_| bytes.iter().map(|&b| b as char).collect());
    let (dir, mut conn) = items_conn(&app)?;
    let (mut added, mut skipped, mut warnings) = (0usize, 0usize, 0usize);
    match kind.as_str() {
        "contacts" => {
            let folder_id = folder.filter(|f| !f.is_empty()).ok_or("choisissez un dossier de contacts")?;
            let k: i64 = folder_id.strip_prefix('k').and_then(|f| f.parse().ok()).ok_or("dossier inconnu")?;
            let existing = thunderlook_contacts::load(&conn, Some(k), None)?;
            let key = |g: &str, s: &str, m: &str| format!("{}|{}|{}", g.trim().to_lowercase(), s.trim().to_lowercase(), m.trim().to_lowercase());
            let mut seen: std::collections::HashSet<String> = existing.contacts.iter().map(|c| key(&c.given_name, &c.surname, c.emails.first().map_or("", |e| e.address.as_str()))).collect();
            for p in thunderlook_contacts::vcard::parse(&text)? {
                warnings += p.warnings.len();
                let mut f = p.form;
                if f == thunderlook_model::contacts::ContactForm::default() {
                    continue;
                }
                if !seen.insert(key(&f.given_name, &f.surname, &f.email1)) {
                    skipped += 1;
                    continue;
                }
                f.id = None;
                f.folder_id = folder_id.clone();
                thunderlook_contacts::write::create(&mut conn, &dir, k, &f)?;
                added += 1;
            }
        }
        "tasks" => {
            let folder_id = thunderlook_items::default_folder(&conn, "IPM.Task%")?.ok_or("aucun dossier de tâches")?;
            let mut seen: std::collections::HashSet<String> = thunderlook_items::tasks(&conn)?.rows.iter().map(|r| format!("{}|{}", r.title.to_lowercase(), r.due)).collect();
            for p in thunderlook_items::vtodo::parse(&text)? {
                warnings += p.warnings.len();
                let mut f = p.form;
                if f == thunderlook_model::items::TaskForm::default() {
                    continue;
                }
                if !seen.insert(format!("{}|{}", f.subject.to_lowercase(), f.due)) {
                    skipped += 1;
                    continue;
                }
                f.id = None;
                f.folder_id = folder_id.clone();
                thunderlook_items::task_save(&mut conn, &dir, &f)?;
                added += 1;
            }
        }
        _ => {
            let folder_id = thunderlook_items::default_folder(&conn, "IPM.StickyNote%")?.ok_or("aucun dossier de notes")?;
            let mut seen: std::collections::HashSet<String> = thunderlook_items::notes(&conn, &dir)?.rows.iter().map(|r| r.text.trim().to_string()).collect();
            for p in thunderlook_items::vnote::parse(&text)? {
                warnings += p.warnings.len();
                let mut f = p.form;
                if f == thunderlook_model::items::NoteForm::default() {
                    continue;
                }
                if !seen.insert(f.body.trim().to_string()) {
                    skipped += 1;
                    continue;
                }
                f.id = None;
                f.folder_id = folder_id.clone();
                thunderlook_items::note_save(&mut conn, &dir, &f)?;
                added += 1;
            }
        }
    }
    let _ = app.emit("contacts-changed", ());
    Ok(Some(format!("{added} ajouté(s), {skipped} déjà présent(s){}", if warnings > 0 { format!(", {warnings} avertissement(s)") } else { String::new() })))
}

/// Boîte « Contact » : contact existant (`id`), ou nouveau dans le dossier `folder`.
#[tauri::command(async)]
fn contact_form(app: AppHandle, id: Option<String>, folder: Option<String>) -> Result<thunderlook_model::contacts::ContactForm, String> {
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    match id {
        Some(id) => {
            let n: i64 = id.strip_prefix('p').and_then(|f| f.parse().ok()).ok_or("contact inconnu")?;
            thunderlook_contacts::write::form_of(&conn, &dir, n)
        }
        None => Ok(thunderlook_model::contacts::ContactForm { folder_id: folder.unwrap_or_default(), ..Default::default() }),
    }
}

/// Enregistre un contact (création ou modification) ; rend son identifiant.
#[tauri::command(async)]
fn contact_save(app: AppHandle, form: thunderlook_model::contacts::ContactForm) -> Result<String, String> {
    let dir = data_dir(&app)?;
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let id = match &form.id {
        Some(id) => {
            let n: i64 = id.strip_prefix('p').and_then(|f| f.parse().ok()).ok_or("contact inconnu")?;
            thunderlook_safety::snapshot_edit(&conn, &dir, n)?;
            thunderlook_contacts::write::update(&mut conn, &dir, n, &form)?;
            n
        }
        None => {
            let folder: i64 = form.folder_id.strip_prefix('k').and_then(|f| f.parse().ok()).ok_or("choisissez un dossier de contacts")?;
            thunderlook_contacts::write::create(&mut conn, &dir, folder, &form)?
        }
    };
    let _ = app.emit("contacts-changed", ());
    Ok(format!("p{id}"))
}

/// Supprime un contact (`p<id>`).
#[tauri::command(async)]
fn contact_delete(app: AppHandle, id: String) -> Result<(), String> {
    let n: i64 = id.strip_prefix('p').and_then(|f| f.parse().ok()).ok_or("contact inconnu")?;
    let mut conn = thunderlook_store::open(&data_dir(&app)?).map_err(|e| e.to_string())?;
    thunderlook_contacts::write::delete(&mut conn, n)?;
    let _ = app.emit("contacts-changed", ());
    Ok(())
}

/// Fiche d'un contact (`p<id>`), avec ses notes et sa photo.
#[tauri::command(async)]
fn contact_card(app: AppHandle, id: String) -> Result<thunderlook_model::contacts::ContactCard, String> {
    use base64::Engine;
    let id: i64 = id.strip_prefix('p').and_then(|f| f.parse().ok()).ok_or("contact inconnu")?;
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let all = thunderlook_contacts::load(&conn, None, Some(id))?;
    let c = all.contacts.first().ok_or("contact introuvable")?;
    let (notes, photo) = thunderlook_contacts::extras(c, &dir);
    let photo = photo.map(|(mime, bytes)| format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)));
    Ok(thunderlook_contacts::view::card(c, notes, photo))
}

#[tauri::command(async)]
fn calendar_view(app: AppHandle, cache: State<CalendarCache>, query: CalendarQuery) -> Result<CalendarView, String> {
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let calendars = all_calendars(&app, &conn, &dir)?;
    let (mut view, unreadable) = with_calendar(&cache, &conn, &dir, |cal| {
        let unreadable: Vec<String> = cal.issues.iter().filter(|i| i.id == 0).map(|i| i.detail.clone()).collect();
        (thunderlook_calendar::view::build(cal, calendars, &query, thunderlook_calendar::view::local_now(now_ms())), unreadable)
    })?;
    // Calendriers communs avec Outlook : état de la synchronisation ; les autres (copies de PST) : leur source.
    let copies: Vec<thunderlook_model::calendar::CalendarInfo> = view.calendars.iter().filter(|c| !c.shared).cloned().collect();
    let copy_note = thunderlook_calendar::view::source_note(&conn, &copies)?;
    let shared_note = view.calendars.iter().any(|c| c.shown && c.shared).then(|| shared_calendar::note(&app));
    view.source_note = match (shared_note, copy_note) {
        (Some(a), Some(b)) => Some(format!("{a} {b}")),
        (a, b) => a.or(b),
    };
    if !unreadable.is_empty() {
        let note = format!("Calendrier importé illisible : {}", unreadable.join(" ; "));
        view.source_note = Some(match view.source_note {
            Some(n) => format!("{n} {note}"),
            None => note,
        });
    }
    Ok(view)
}

/// « Ouvrir un calendrier » : `.ics` ou CSV d'Outlook, ajouté aux calendriers importés et affiché.
#[tauri::command(async)]
fn calendar_import(app: AppHandle, cache: State<CalendarCache>) -> Result<Option<String>, String> {
    let Some(file) = app.dialog().file().add_filter("Calendrier (iCalendar, CSV d'Outlook)", &["ics", "csv"]).blocking_pick_file() else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let dir = data_dir(&app)?;
    let (name, n, target) = thunderlook_calendar::imported::import(&dir, &path)?;
    clear_calendar(&cache)?;
    if let Some(id) = thunderlook_calendar::imported::id_of(&dir, &target) {
        set_calendar_shown(app.clone(), id, true)?;
    }
    Ok(Some(format!("Calendrier « {name} » importé : {n} élément(s), en lecture seule.")))
}

/// Retire un calendrier importé ; les identifiants suivants (ordre des fichiers) reculent d'un rang.
#[tauri::command]
fn calendar_remove(app: AppHandle, cache: State<CalendarCache>, id: String) -> Result<(), String> {
    let dir = data_dir(&app)?;
    let n: i64 = id.strip_prefix("c-").and_then(|s| s.parse().ok()).ok_or("identifiant de calendrier importé invalide")?;
    thunderlook_calendar::imported::remove(&dir, &id)?;
    let after: Vec<String> = shown_calendars(&app)
        .unwrap_or_default()
        .into_iter()
        .filter(|c| *c != id)
        .map(|c| match c.strip_prefix("c-").and_then(|s| s.parse::<i64>().ok()) {
            Some(k) if k > n => format!("c-{}", k - 1),
            _ => c,
        })
        .collect();
    let path = settings_path(&app)?.with_file_name("calendar.json");
    fs::write(&path, serde_json::to_string(&after).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    clear_calendar(&cache)
}

/// « Enregistrer le calendrier » (`.ics` : tous les éléments des calendriers affichés) ou export CSV d'Outlook
/// (occurrences de la période affichée). Renvoie le message de résultat, `None` si l'utilisateur annule.
#[tauri::command(async)]
fn calendar_export(app: AppHandle, cache: State<CalendarCache>, format: String, query: CalendarQuery) -> Result<Option<String>, String> {
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let calendars = all_calendars(&app, &conn, &dir)?;
    let shown: Vec<&thunderlook_model::calendar::CalendarInfo> = calendars.iter().filter(|c| c.shown).collect();
    if shown.is_empty() {
        return Err("aucun calendrier affiché à exporter".into());
    }
    let name = shown.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ");
    let ids: Vec<i64> = shown.iter().filter_map(|c| c.id[1..].parse().ok()).collect();
    let csv = format == "csv";
    let (text, count, label) = with_calendar(&cache, &conn, &dir, |cal| {
        let items: Vec<&thunderlook_calendar::Appointment> = cal.appointments.iter().filter(|a| ids.contains(&a.folder_id)).collect();
        if csv {
            let anchor = chrono::NaiveDate::parse_from_str(&query.anchor, "%Y-%m-%d").unwrap_or_else(|_| chrono::Local::now().date_naive());
            let (first, last) = thunderlook_calendar::view::period(query.kind, anchor);
            let ms = |d: chrono::NaiveDate| thunderlook_calendar::tz::TimeZone::PARIS.to_utc(d.and_hms_opt(0, 0, 0).unwrap()).and_utc().timestamp_millis();
            let mut rows: Vec<_> = items
                .iter()
                .flat_map(|a| thunderlook_calendar::expand::occurrences(a, ms(first), ms(last + chrono::Duration::days(1))).into_iter().map(move |o| (*a, o)))
                .collect();
            rows.sort_by_key(|(_, o)| o.start_ms);
            let n = rows.len();
            let label = format!("occurrence(s) du {} au {}", first.format("%d/%m/%Y"), last.format("%d/%m/%Y"));
            (thunderlook_calendar::csv::export(&rows, &|a| thunderlook_calendar::description(a, &dir)), n, label)
        } else {
            (thunderlook_calendar::ics::export(&items, &name, &|a| thunderlook_calendar::description(a, &dir)), items.len(), "élément(s)".to_string())
        }
    })?;
    let ext = if csv { "csv" } else { "ics" };
    let Some(target) = app
        .dialog()
        .file()
        .add_filter(if csv { "CSV (Outlook)" } else { "iCalendar" }, &[ext])
        .set_file_name(safe_file_name(&format!("{name}.{ext}")))
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = target.into_path().map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("{} : {e}", path.display()))?;
    Ok(Some(format!("{count} {label} exporté(s) dans {}", path.display())))
}

#[tauri::command]
fn set_calendar_shown(app: AppHandle, id: String, shown: bool) -> Result<(), String> {
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let list = thunderlook_calendar::view::calendars(&conn)?;
    let mut current = shown_calendars(&app).unwrap_or_else(|| thunderlook_calendar::view::default_shown(&list));
    current.retain(|c| c != &id && list.iter().any(|l| &l.id == c));
    if shown {
        current.push(id);
    }
    let path = settings_path(&app)?.with_file_name("calendar.json");
    if let Some(d) = path.parent() {
        fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string(&current).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Mesure envoyée par l'interface, ajoutée à `perf.log` si `THUNDERLOOK_PERF` est défini.
#[tauri::command]
fn perf_mark(app: AppHandle, label: String, ms: f64) {
    if std::env::var_os("THUNDERLOOK_PERF").is_none() {
        return;
    }
    if let Ok(dir) = app.path().app_config_dir() {
        use std::io::Write;
        let _ = fs::create_dir_all(&dir);
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(dir.join("perf.log")) {
            let _ = writeln!(f, "{}\t{label}\t{ms:.1}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Lancé par Windows depuis une notification (lien `thunderlook:`) alors que ThunderLook est déjà ouvert : cette
    // seconde instance s'arrête aussitôt (single-instance) ; elle cède d'abord le premier plan à la première.
    #[cfg(windows)]
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(windows::Win32::UI::WindowsAndMessaging::ASFW_ANY);
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| match argv.iter().find_map(|a| toast::parse_uri(a)) {
            Some((source, message)) => open_from_notification(app, source, message),
            None => bring_to_front(app),
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Store(Mutex::new(None)))
        .manage(Indexer::default())
        .manage(Syncer::default())
        .manage(CalendarCache::default())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            // Fenêtre principale créée ici, et non par la configuration, pour intercepter les ouvertures de
            // fenêtres : un lien cliqué dans un message (iframe isolé, `target=_blank`) part dans le navigateur.
            let config = app.config().app.windows.first().cloned().ok_or("fenêtre principale absente de la configuration")?;
            let handle = app.handle().clone();
            WebviewWindowBuilder::from_config(app.handle(), &config)?
                .on_new_window(move |url, _| {
                    if matches!(url.scheme(), "http" | "https") {
                        let _ = handle.opener().open_url(url.as_str(), None::<&str>);
                    }
                    NewWindowResponse::Deny
                })
                .build()?;
            // Identité des notifications et lien `thunderlook:` (voir `toast`) ; l'icône est copiée dans le dossier de
            // configuration, d'où Windows la lit.
            if let Ok(dir) = app.path().app_config_dir() {
                let icon = dir.join("notification.png");
                let _ = fs::create_dir_all(&dir).and_then(|_| fs::write(&icon, include_bytes!("../icons/128x128.png")));
                if let Err(e) = toast::register(&icon) {
                    log::warn!("{e}");
                }
            }
            // Démarré par un clic sur une notification alors que ThunderLook était fermé : le lien est en argument.
            if let Some((source, message)) = std::env::args().find_map(|a| toast::parse_uri(&a)) {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    // Laisser l'interface s'abonner à « open-item ».
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    let h = handle.clone();
                    let _ = handle.run_on_main_thread(move || open_from_notification(&h, source, message));
                });
            }
            start_scheduler(app.handle());
            reminders_cmds::start(app.handle());
            exchange_cmds::start(app.handle());
            safety_cmds::start(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            folder_tree,
            open_list,
            list_rows,
            toggle_conversation,
            message_view,
            message_position,
            compose,
            save_draft,
            open_draft,
            signatures,
            pick_attachments,
            compose_from_template,
            apply_op,
            search,
            index_status,
            folder_op,
            undo,
            categories,
            rules,
            accounts,
            add_account,
            autoconfig_cmds::detect_account,
            remove_account,
            set_account_rules,
            copy_rules,
            send_draft,
            sync_now,
            set_offline,
            get_auto_reply,
            set_auto_reply,
            set_rule_active,
            rule_draft,
            create_rule,
            import_rwz,
            run_rules,
            open_attachment,
            save_attachment,
            mail_cmds::save_all_attachments,
            from_mail_cmds::from_mail_appointment,
            from_mail_cmds::from_mail_task,
            from_mail_cmds::from_mail_note,
            from_mail_cmds::from_mail_contact,
            load_settings,
            save_settings,
            load_quick_steps,
            save_quick_steps,
            perf_mark,
            calendar_view,
            shared_calendar::appointment_form,
            shared_calendar::appointment_save,
            shared_calendar::appointment_delete,
            calendar_cmds::appointment_shift,
            reminders_cmds::reminders_due,
            reminders_cmds::reminder_dismiss,
            reminders_cmds::reminder_snooze,
            search_cmds::global_search,
            contact_folders,
            contact_list,
            contact_card,
            contacts_cmds::contact_dist_lists,
            dedupe_cmds::dedupe_scan,
            dedupe_cmds::dedupe_detail,
            dedupe_cmds::dedupe_preview,
            dedupe_cmds::dedupe_merge,
            dedupe_cmds::dedupe_ignore,
            dedupe_cmds::dedupe_merge_safe,
            exchange_export,
            exchange_import,
            exchange_cmds::phone_exchange_state,
            exchange_cmds::phone_exchange_pick,
            exchange_cmds::phone_exchange_set_auto,
            exchange_cmds::phone_exchange_export,
            exchange_cmds::phone_exchange_check,
            exchange_cmds::phone_exchange_apply,
            exchange_cmds::phone_exchange_undo,
            exchange_cmds::phone_exchange_check_detail,
            exchange_cmds::phone_exchange_apply_detail,
            mpe_import_cmds::mpe_import_devices,
            mpe_import_cmds::mpe_import_pick,
            mpe_import_cmds::mpe_import_check,
            mpe_import_cmds::mpe_import_apply,
            tasks_list,
            items_cmds::tasks_query,
            items_cmds::notes_query,
            task_form,
            task_save,
            task_complete,
            task_delete,
            tasks_recur_cmds::task_delete_occurrence,
            tasks_recur_cmds::task_delete_series,
            notes_list,
            note_form,
            note_save,
            note_delete,
            contact_form,
            contact_save,
            contact_delete,
            set_calendar_shown,
            calendar_import,
            calendar_export,
            calendar_remove,
            junk_lists,
            set_junk_lists,
            junk_mark,
            safety_cmds::safety_undo,
            safety_cmds::backups_list,
            safety_cmds::backup_now,
            safety_cmds::backup_restore,
            safety_cmds::backups_open_folder,
            folders_cmds::item_folders,
            folders_cmds::item_folder_create,
            folders_cmds::item_folder_rename,
            folders_cmds::item_folder_delete,
            folders_cmds::calendar_folder_color
        ])
        .run(tauri::generate_context!())
        .expect("échec du démarrage de ThunderLook");
}
