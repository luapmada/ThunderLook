//! Cœur de ThunderLook (Tauri) : lecture du stockage local (J2) et réglages d'affichage.
//!
//! Dossier de données, par ordre de priorité : variable d'environnement `THUNDERLOOK_DATA`,
//! `dataDir` dans `config.json` (dossier de configuration de l'application), sinon `<données locales>\data`.

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
    AccountSummary, AutoReply, CategoryInfo, ComposeKind, Draft, DraftAttachment, DraftAttachmentSource, FolderOp, FolderTree, IndexStatus, ListQuery, ListView,
    MessageOp, MessageSummary, MessageView, OpResult, SyncEvent, RuleRunReport, RuleRunRequest, RuleSummary, SearchQuery, Selection, Signature,
    SortField, format,
    quicksteps::{QuickStep, default_quick_steps},
    settings::Settings,
};

/// Envoi et réception (J7) : un passage à la fois, dans un fil dédié ; une demande pendant un passage en relance un.
#[derive(Default)]
struct Syncer {
    running: Arc<AtomicBool>,
    again: Arc<AtomicBool>,
    /// « Travailler hors connexion » : ni relève, ni envoi, ni surveillance.
    offline: Arc<AtomicBool>,
    /// Comptes dont la Boîte de réception est surveillée (IDLE).
    watched: Arc<Mutex<std::collections::HashSet<i64>>>,
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
        let fresh = syncer.watched.lock().map(|mut w| w.insert(a.source_id)).unwrap_or(false);
        if !fresh {
            continue;
        }
        let (app, dir, offline, watched) = (app.clone(), dir.clone(), syncer.offline.clone(), syncer.watched.clone());
        std::thread::spawn(move || {
            let notify = app.clone();
            thunderlook_sync::watch_inbox_blocking(&dir, a.source_id, &offline, move || start_sync(&notify), |e| log::warn!("surveillance IDLE : {e}"));
            if let Ok(mut w) = watched.lock() {
                w.remove(&a.source_id);
            }
        });
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
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
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
    for a in &accounts {
        match thunderlook_sync::sync_account_blocking(&dir, a.source_id) {
            Ok(r) => {
                sent += r.sent;
                received += r.new_messages;
                if !r.new_inbox_ids.is_empty() {
                    // Réponses d'abord (sur les messages tels que reçus), puis règles (qui peuvent les déplacer).
                    match auto_replies(app, a.source_id, &r.new_inbox_ids) {
                        Ok(n) if n > 0 => app.state::<Syncer>().again.store(true, Ordering::SeqCst),
                        Ok(_) => {}
                        Err(e) => errors.push(format!("{} : réponse automatique : {e}", a.address)),
                    }
                    if let Err(e) = rules_on_arrival(app, a.source_id, &r.new_inbox_ids) {
                        errors.push(format!("{} : règles : {e}", a.address));
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
        use tauri_plugin_notification::NotificationExt;
        let body = if received == 1 { "1 nouveau message".to_string() } else { format!("{received} nouveaux messages") };
        let _ = app.notification().builder().title("ThunderLook").body(body).show();
    }
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
        let rule_ids: Vec<String> = mb.rules()?.into_iter().filter(|r| r.active && r.error.is_none()).map(|r| r.id).collect();
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
        };
        let report = mb.run_rules(&req)?;
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

/// Ajoute un compte après avoir vérifié l'identification IMAP et SMTP (TLS, ports 993 et 465), puis lance la
/// première relève. Le mot de passe va dans le gestionnaire d'identifiants de Windows.
#[tauri::command(async)]
fn add_account(
    app: AppHandle,
    address: String,
    name: String,
    password: String,
    imap_host: String,
    smtp_host: String,
) -> Result<Vec<AccountSummary>, String> {
    let address = address.trim().to_string();
    if !address.contains('@') {
        return Err("Adresse de messagerie invalide.".into());
    }
    let mut ep = thunderlook_sync::Endpoints::hostinger(&address);
    if !imap_host.trim().is_empty() {
        ep.imap.host = imap_host.trim().to_string();
    }
    if !smtp_host.trim().is_empty() {
        ep.smtp.host = smtp_host.trim().to_string();
    }
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
    thunderlook_sync::account::remove(&conn, &dir, source)?;
    thunderlook_sync::account::summaries(&conn)
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
        let _ = app.emit("sync", SyncEvent { message: "Hors connexion".into(), ..Default::default() });
    } else {
        start_sync(&app);
        start_watchers(&app);
    }
    offline
}

/// Prépare un nouveau message, une réponse, une réponse à tous ou un transfert (J5).
#[tauri::command(async)]
fn compose(app: AppHandle, store: State<Store>, kind: ComposeKind, source_id: Option<String>) -> Result<Draft, String> {
    with_mailbox(&app, &store, |mb| mb.compose(kind, source_id.as_deref()))
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
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(Store(Mutex::new(None)))
        .manage(Indexer::default())
        .manage(Syncer::default())
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
            start_scheduler(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            folder_tree,
            open_list,
            list_rows,
            toggle_conversation,
            message_view,
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
            remove_account,
            send_draft,
            sync_now,
            set_offline,
            get_auto_reply,
            set_auto_reply,
            set_rule_active,
            import_rwz,
            run_rules,
            open_attachment,
            save_attachment,
            load_settings,
            save_settings,
            load_quick_steps,
            save_quick_steps,
            perf_mark
        ])
        .run(tauri::generate_context!())
        .expect("échec du démarrage de ThunderLook");
}
