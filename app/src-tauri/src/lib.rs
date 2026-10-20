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
use tauri::{AppHandle, Manager, State, WebviewWindowBuilder, webview::NewWindowResponse};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use thunderlook_mailbox::Mailbox;
use thunderlook_model::{
    CategoryInfo, ComposeKind, Draft, DraftAttachment, DraftAttachmentSource, FolderOp, FolderTree, IndexStatus, ListQuery, ListView,
    MessageOp, MessageSummary, MessageView, OpResult, RuleRunReport, RuleRunRequest, RuleSummary, SearchQuery, Selection, Signature,
    SortField, format, settings::Settings,
};

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
        .manage(Store(Mutex::new(None)))
        .manage(Indexer::default())
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
            set_rule_active,
            import_rwz,
            run_rules,
            open_attachment,
            save_attachment,
            load_settings,
            save_settings,
            perf_mark
        ])
        .run(tauri::generate_context!())
        .expect("échec du démarrage de ThunderLook");
}
