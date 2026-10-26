//! Échange avec le téléphone : commandes de l'écran « Échange avec le téléphone » et export automatique optionnel.
//! Tout le travail est dans `thunderlook-exchange` ; ici, le choix du dossier d'échange (`exchange.json`, à côté de
//! `settings.json`), les verrous et les événements de rechargement.

use std::{fs, path::PathBuf, sync::Mutex, time::{Duration, Instant}};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use thunderlook_exchange::{ChangeKind, ItemKind};

use crate::{CalendarCache, clear_calendar, data_dir, settings_path};

/// Un seul export, relecture ou application à la fois (l'export automatique tourne dans son propre fil).
pub(crate) static BUSY: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExchangeSettings {
    /// Dossier d'échange choisi ; vide : aucun.
    pub dir: String,
    /// Export automatique toutes les N minutes ; 0 : désactivé (valeur par défaut).
    pub auto_minutes: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeState {
    pub settings: ExchangeSettings,
    /// Horodatage (UTC) du dernier export présent dans le dossier.
    pub last_export: Option<String>,
    /// Un import appliqué peut être annulé.
    pub can_undo: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRow {
    pub key: String,
    pub kind: ChangeKind,
    pub item: ItemKind,
    pub summary: String,
    /// Coché d'office : jamais une suppression.
    pub selected: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub changes: Vec<ChangeRow>,
    pub unchanged: usize,
    pub warnings: Vec<String>,
}

fn settings_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(settings_path(app)?.with_file_name("exchange.json"))
}

fn read_settings(app: &AppHandle) -> ExchangeSettings {
    settings_file(app).ok().and_then(|p| fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn write_settings(app: &AppHandle, s: &ExchangeSettings) -> Result<(), String> {
    let path = settings_file(app)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&path, serde_json::to_string_pretty(s).map_err(|e| e.to_string())?).map_err(|e| format!("{} : {e}", path.display()))
}

fn exchange_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let s = read_settings(app);
    if s.dir.trim().is_empty() {
        return Err("Choisissez d'abord le dossier d'échange.".into());
    }
    Ok(PathBuf::from(s.dir))
}

fn state_of(app: &AppHandle) -> Result<ExchangeState, String> {
    let settings = read_settings(app);
    let last_export = (!settings.dir.is_empty()).then(|| thunderlook_exchange::read_manifest(std::path::Path::new(&settings.dir))).flatten().map(|m| m.exported_at);
    Ok(ExchangeState { settings, last_export, can_undo: thunderlook_exchange::last_undo(&data_dir(app)?).is_some() })
}

fn lock() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    BUSY.lock().map_err(|_| "verrou de l'échange empoisonné".to_string())
}

/// Les éléments ont changé : les listes de l'interface se rechargent.
fn notify_changed(app: &AppHandle) {
    if let Some(cache) = app.try_state::<CalendarCache>() {
        let _ = clear_calendar(&cache);
    }
    let _ = app.emit("contacts-changed", ());
    let _ = app.emit("calendar-changed", ());
}

#[tauri::command(async)]
pub fn phone_exchange_state(app: AppHandle) -> Result<ExchangeState, String> {
    state_of(&app)
}

/// « Choisir le dossier… » ; `None` si l'utilisateur annule.
#[tauri::command(async)]
pub fn phone_exchange_pick(app: AppHandle) -> Result<Option<ExchangeState>, String> {
    let Some(dir) = app.dialog().file().blocking_pick_folder() else { return Ok(None) };
    let dir = dir.into_path().map_err(|e| e.to_string())?;
    let mut s = read_settings(&app);
    s.dir = dir.to_string_lossy().into_owned();
    write_settings(&app, &s)?;
    state_of(&app).map(Some)
}

/// Export automatique : 0 (désactivé) ou de 1 à 1 440 minutes.
#[tauri::command(async)]
pub fn phone_exchange_set_auto(app: AppHandle, minutes: u32) -> Result<ExchangeState, String> {
    let mut s = read_settings(&app);
    s.auto_minutes = minutes.min(1_440);
    write_settings(&app, &s)?;
    state_of(&app)
}

/// « Exporter maintenant ». Refusé si le téléphone a modifié des fichiers depuis le dernier export (ils seraient
/// écrasés) tant que `force` n'est pas vrai.
#[tauri::command(async)]
pub fn phone_exchange_export(app: AppHandle, force: bool) -> Result<String, String> {
    let out = exchange_dir(&app)?;
    let _guard = lock()?;
    let foreign = thunderlook_exchange::foreign_files(&out);
    if !foreign.is_empty() {
        return Err(format!("Ce dossier est écrit par MPE ({}) : un export de ThunderLook les écraserait. ThunderLook les relit tout seul (synchronisation automatique) et n'y écrit rien.", foreign.join(", ")));
    }
    let touched = thunderlook_exchange::modified_files(&out);
    if !touched.is_empty() && !force {
        return Err(format!("MODIFIED:{}", touched.join(", ")));
    }
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let report = thunderlook_exchange::export_all(&conn, &dir, &out)?;
    let total: usize = report.files.iter().map(|f| f.count).sum();
    Ok(format!("{total} élément(s) exportés dans {}", out.display()))
}

/// « Vérifier les changements » : ce qui diffère entre les fichiers du dossier et la base. N'écrit rien.
#[tauri::command(async)]
pub fn phone_exchange_check(app: AppHandle) -> Result<CheckResult, String> {
    let inp = exchange_dir(&app)?;
    let _guard = lock()?;
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let rep = thunderlook_exchange::import_changes(&conn, &dir, &inp)?;
    let changes = rep.changes.iter().map(|c| ChangeRow { key: c.key.clone(), kind: c.kind, item: c.item, summary: c.summary.clone(), selected: c.default_selected() }).collect();
    Ok(CheckResult { changes, unchanged: rep.unchanged, warnings: rep.warnings })
}

/// « Appliquer » les changements cochés (`keys`). La liste est relue : seuls les changements qui existent encore sont appliqués.
#[tauri::command(async)]
pub fn phone_exchange_apply(app: AppHandle, keys: Vec<String>) -> Result<String, String> {
    let inp = exchange_dir(&app)?;
    let _guard = lock()?;
    let dir = data_dir(&app)?;
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let rep = thunderlook_exchange::import_changes(&conn, &dir, &inp)?;
    let chosen: Vec<_> = rep.changes.iter().filter(|c| keys.contains(&c.key)).cloned().collect();
    if chosen.is_empty() {
        return Ok("Rien à appliquer.".into());
    }
    let done = thunderlook_exchange::apply(&mut conn, &dir, &chosen)?;
    // Dossier écrit par MPE : ce qui est réglé ici ne sera pas reproposé par la relecture automatique.
    thunderlook_exchange::settle(&conn, &dir, &rep, &done.done)?;
    notify_changed(&app);
    let mut msg = format!("{} changement(s) appliqué(s)", done.applied);
    if !done.errors.is_empty() {
        msg.push_str(&format!(", {} échec(s) : {}", done.errors.len(), done.errors.join(" ; ")));
    }
    Ok(msg)
}

/// « Annuler le dernier import ».
#[tauri::command(async)]
pub fn phone_exchange_undo(app: AppHandle) -> Result<String, String> {
    let _guard = lock()?;
    let dir = data_dir(&app)?;
    let stamp = thunderlook_exchange::last_undo(&dir).ok_or("Aucun import à annuler.")?;
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let result = thunderlook_exchange::undo(&mut conn, &dir, &stamp);
    notify_changed(&app);
    result.map(|n| format!("Import annulé : {n} élément(s) rétabli(s)"))
}

// --- Écran détaillé (lot AF) : ancienne/nouvelle valeur, conflits à part, récapitulatif ------------------------------

/// Une valeur qui change : `old` est vide pour un nouvel élément, `new` pour une suppression.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDiff {
    pub label: String,
    pub old: String,
    pub new: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailRow {
    pub key: String,
    pub kind: ChangeKind,
    pub item: ItemKind,
    pub summary: String,
    pub selected: bool,
    /// `Some("both")` / `Some("unknown")` pour un conflit.
    pub conflict: Option<String>,
    pub fields: Vec<FieldDiff>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailCheck {
    pub changes: Vec<DetailRow>,
    pub conflicts: Vec<DetailRow>,
    pub unchanged: usize,
    pub warnings: Vec<String>,
}

/// Choix pour un conflit : `choice` vaut `thunderlook` ou `phone` (un conflit sans choix est ignoré).
#[derive(Clone, Debug, Deserialize)]
pub struct ConflictChoice {
    pub key: String,
    pub choice: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplySummary {
    pub created: usize,
    pub modified: usize,
    pub deleted: usize,
    /// Conflits réglés en gardant ThunderLook (rien d'écrit).
    pub kept: usize,
    /// Changements ou conflits proposés mais non retenus.
    pub ignored: usize,
    /// Avertissements de la relecture et échecs de l'application.
    pub warnings: Vec<String>,
}

fn field_label(key: &str) -> &str {
    match key {
        "givenName" => "Prénom",
        "surname" => "Nom",
        "company" => "Société",
        "jobTitle" => "Fonction",
        "department" => "Service",
        "email1" => "E-mail",
        "email2" => "E-mail 2",
        "email3" => "E-mail 3",
        "mobile" => "Mobile",
        "business" => "Tél. professionnel",
        "homePhone" => "Tél. domicile",
        "workStreet" => "Rue (pro)",
        "workCity" => "Ville (pro)",
        "workPostal" => "Code postal (pro)",
        "workCountry" => "Pays (pro)",
        "homeStreet" => "Rue (domicile)",
        "homeCity" => "Ville (domicile)",
        "homePostal" => "Code postal (domicile)",
        "homeCountry" => "Pays (domicile)",
        "webPage" => "Site web",
        "birthday" => "Anniversaire",
        "notes" => "Notes",
        "subject" => "Objet",
        "location" => "Lieu",
        "start" => "Début",
        "end" => "Fin",
        "due" => "Échéance",
        "status" => "État",
        "importance" => "Importance",
        "body" => "Texte",
        "categories" => "Catégories",
        "reminder" => "Rappel",
        "allDay" => "Journée entière",
        "busy" => "Disponibilité",
        "private" => "Confidentiel",
        "color" => "Couleur",
        "repeat" => "Répétition",
        "repeatEvery" => "Répéter tous les",
        "repeatEnd" => "Fin de répétition",
        "repeatCount" => "Nombre de répétitions",
        "repeatUntil" => "Répéter jusqu'au",
        other => other,
    }
}

/// Valeurs affichables d'un élément : champs simples seulement (pas d'identifiant, de dossier, de photo ni de série).
fn fields_of(payload: &thunderlook_exchange::Payload) -> std::collections::BTreeMap<String, String> {
    use thunderlook_exchange::Payload::*;
    let value = match payload {
        None => return Default::default(),
        Contact(f) => serde_json::to_value(f),
        Task(f) => serde_json::to_value(f),
        Note(f) => serde_json::to_value(f),
        Event(f) => serde_json::to_value(f),
    };
    let mut out = std::collections::BTreeMap::new();
    if let Ok(serde_json::Value::Object(map)) = value {
        for (k, v) in map {
            if matches!(k.as_str(), "id" | "folderId" | "calendarId" | "photo" | "series") {
                continue;
            }
            let text = match v {
                serde_json::Value::Null => String::new(),
                serde_json::Value::String(s) => s,
                serde_json::Value::Bool(b) => (if b { "oui" } else { "non" }).to_string(),
                other => other.to_string(),
            };
            out.insert(k, text);
        }
    }
    out
}

/// Valeur actuelle de la base pour un élément modifié ou supprimé (pour montrer l'ancienne valeur).
fn current_payload(conn: &thunderlook_store::rusqlite::Connection, dir: &std::path::Path, item: ItemKind, id: &str) -> Option<thunderlook_exchange::Payload> {
    use thunderlook_exchange::{EventData, Payload};
    let num = |p: char| id.strip_prefix(p)?.parse::<i64>().ok();
    match item {
        ItemKind::Contact => thunderlook_contacts::write::form_of(conn, dir, num('p')?).ok().map(Payload::Contact),
        ItemKind::Task => thunderlook_items::task_form(conn, dir, num('t')?).ok().map(Payload::Task),
        ItemKind::Note => thunderlook_items::note_form(conn, dir, num('n')?).ok().map(Payload::Note),
        ItemKind::Event => {
            let n = num('a')?;
            let cal = thunderlook_calendar::load(conn).ok()?;
            let a = cal.appointments.iter().find(|a| a.id == n)?;
            let local = |ms: i64| thunderlook_calendar::view::local_now(ms).format("%Y-%m-%dT%H:%M").to_string();
            Some(Payload::Event(EventData {
                subject: a.subject.clone(),
                location: a.location.clone().unwrap_or_default(),
                start: local(a.start_ms),
                end: local(a.end_ms),
                all_day: a.all_day,
                body: thunderlook_calendar::description(a, dir).map(|b| b.trim_end().to_string()).unwrap_or_default(),
                categories: a.categories.join(", "),
                private: a.private,
                ..Default::default()
            }))
        }
    }
}

fn shorten(s: &str) -> String {
    let flat = s.replace(['\r', '\n'], " ");
    if flat.chars().count() > 120 { format!("{}…", flat.chars().take(120).collect::<String>()) } else { flat }
}

fn detail_row(conn: &thunderlook_store::rusqlite::Connection, dir: &std::path::Path, c: &thunderlook_exchange::Change) -> DetailRow {
    use thunderlook_exchange::Payload;
    let fields = match c.kind {
        ChangeKind::New => fields_of(&c.payload).into_iter().filter(|(_, v)| !v.is_empty() && v != "0").map(|(k, v)| FieldDiff { label: field_label(&k).to_string(), old: String::new(), new: shorten(&v) }).collect(),
        ChangeKind::Deleted => current_payload(conn, dir, c.item, &c.id).map(|p| fields_of(&p)).unwrap_or_default().into_iter().filter(|(_, v)| !v.is_empty() && v != "0").map(|(k, v)| FieldDiff { label: field_label(&k).to_string(), old: shorten(&v), new: String::new() }).collect(),
        ChangeKind::Modified => {
            let old = current_payload(conn, dir, c.item, &c.id).unwrap_or(Payload::None);
            let (old, new) = (fields_of(&old), fields_of(&c.payload));
            new.iter()
                // Un champ que l'élément de la base n'expose pas (événement : rappel, disponibilité…) n'est pas comparable.
                .filter(|(k, v)| old.get(*k).is_some_and(|o| o != *v))
                .map(|(k, v)| FieldDiff { label: field_label(k).to_string(), old: shorten(&old[k]), new: shorten(v) })
                .collect()
        }
    };
    DetailRow {
        key: c.key.clone(),
        kind: c.kind,
        item: c.item,
        summary: c.summary.clone(),
        selected: c.default_selected(),
        conflict: c.conflict.map(|o| match o {
            thunderlook_exchange::Origin::Both => "both".to_string(),
            thunderlook_exchange::Origin::Unknown => "unknown".to_string(),
        }),
        fields,
    }
}

/// « Vérifier les changements » avec le détail de chaque valeur et les conflits à part. N'écrit rien.
#[tauri::command(async)]
pub fn phone_exchange_check_detail(app: AppHandle) -> Result<DetailCheck, String> {
    let inp = exchange_dir(&app)?;
    let _guard = lock()?;
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let rep = thunderlook_exchange::import_changes(&conn, &dir, &inp)?;
    Ok(DetailCheck {
        changes: rep.changes.iter().map(|c| detail_row(&conn, &dir, c)).collect(),
        conflicts: rep.conflicts.iter().map(|c| detail_row(&conn, &dir, c)).collect(),
        unchanged: rep.unchanged,
        warnings: rep.warnings,
    })
}

/// « Appliquer » : `keys` = changements cochés, `choices` = conflits tranchés (les autres sont ignorés). La liste est
/// relue : seuls les éléments qui existent encore sont appliqués. Rend le récapitulatif.
#[tauri::command(async)]
pub fn phone_exchange_apply_detail(app: AppHandle, keys: Vec<String>, choices: Vec<ConflictChoice>) -> Result<ApplySummary, String> {
    use thunderlook_exchange::Resolution;
    let inp = exchange_dir(&app)?;
    let _guard = lock()?;
    let dir = data_dir(&app)?;
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let rep = thunderlook_exchange::import_changes(&conn, &dir, &inp)?;
    let proposed = rep.changes.len() + rep.conflicts.len();
    let mut chosen: Vec<_> = rep.changes.iter().filter(|c| keys.contains(&c.key)).cloned().collect();
    for mut c in rep.conflicts.iter().cloned() {
        c.resolution = match choices.iter().find(|x| x.key == c.key).map(|x| x.choice.as_str()) {
            Some("thunderlook") => Some(Resolution::KeepThunderlook),
            Some("phone") => Some(Resolution::KeepPhone),
            _ => None,
        };
        if c.resolution.is_some() {
            chosen.push(c);
        }
    }
    let mut sum = ApplySummary { ignored: proposed - chosen.len(), warnings: rep.warnings.clone(), ..Default::default() };
    if chosen.is_empty() {
        return Ok(sum);
    }
    let done = thunderlook_exchange::apply(&mut conn, &dir, &chosen)?;
    thunderlook_exchange::settle(&conn, &dir, &rep, &done.done)?;
    notify_changed(&app);
    sum.kept = done.kept;
    for c in chosen.iter().filter(|c| c.resolution != Some(Resolution::KeepThunderlook)) {
        // Un échec est rapporté sous la forme « <résumé> : <erreur> ».
        if done.errors.iter().any(|e| e.starts_with(&format!("{} : ", c.summary))) {
            continue;
        }
        match c.kind {
            ChangeKind::New => sum.created += 1,
            ChangeKind::Modified => sum.modified += 1,
            ChangeKind::Deleted => sum.deleted += 1,
        }
    }
    sum.warnings.extend(done.errors);
    Ok(sum)
}

/// Export automatique (désactivé par défaut) : toutes les `auto_minutes` minutes tant que le téléphone n'a pas modifié
/// les fichiers du dossier ; sinon rien n'est écrasé et il faut les relire (« Vérifier les changements »).
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(20));
        let mut last = Instant::now();
        let mut was_on = false;
        loop {
            let s = read_settings(&app);
            let on = s.auto_minutes > 0 && !s.dir.trim().is_empty();
            // Activé à l'instant : premier export après N minutes, pas tout de suite.
            if on && !was_on {
                last = Instant::now();
            }
            was_on = on;
            if on && last.elapsed() >= Duration::from_secs(u64::from(s.auto_minutes) * 60) {
                last = Instant::now();
                let out = PathBuf::from(&s.dir);
                if let Ok(_guard) = lock() {
                    let touched = thunderlook_exchange::modified_files(&out);
                    if !thunderlook_exchange::foreign_files(&out).is_empty() {
                        log::warn!("échange téléphone : export automatique ignoré, dossier écrit par MPE (relu seulement)");
                    } else if touched.is_empty() {
                        let result = data_dir(&app).and_then(|dir| {
                            let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
                            thunderlook_exchange::export_all(&conn, &dir, &out)
                        });
                        if let Err(e) = result {
                            log::warn!("échange téléphone (export automatique) : {e}");
                        }
                    } else {
                        log::warn!("échange téléphone : export automatique suspendu, fichiers modifiés par le téléphone : {}", touched.join(", "));
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(30));
        }
    });
}
