//! État des rappels : `reminders.json` dans le dossier de données, `{"dismissed": {id: until_ms}}`. Ignorer = jusqu'à
//! `i64::MAX` ; reporter = jusqu'à l'heure donnée. Écriture atomique (fichier temporaire, puis renommage).

use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

const FILE: &str = "reminders.json";
const DAY_MS: i64 = 86_400_000;

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct State {
    #[serde(default)]
    pub dismissed: BTreeMap<String, i64>,
}

/// Fichier absent ou illisible : état vide (un rappel de trop vaut mieux qu'un rappel perdu).
pub(crate) fn load(dir: &Path) -> State {
    std::fs::read(dir.join(FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(dir: &Path, st: &State) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|x| x.to_string())?;
    let tmp = dir.join(format!("{FILE}.tmp"));
    std::fs::write(&tmp, serde_json::to_vec_pretty(st).map_err(|x| x.to_string())?).map_err(|x| x.to_string())?;
    std::fs::rename(&tmp, dir.join(FILE)).map_err(|x| x.to_string())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// Retire les entrées devenues inutiles : reports échus depuis plus de 30 jours, rendez-vous ignorés depuis plus de 60
/// jours (jour lu dans l'identifiant).
fn prune(st: &mut State, now: i64) {
    st.dismissed.retain(|id, until| {
        let day = id.split_once('@').and_then(|(_, d)| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
        match day {
            Some(d) => d.and_hms_opt(0, 0, 0).is_none_or(|t| t.and_utc().timestamp_millis() > now - 60 * DAY_MS),
            None => *until == i64::MAX || *until > now - 30 * DAY_MS,
        }
    });
}

fn set(dir: &Path, id: &str, until_ms: i64) -> Result<(), String> {
    let mut st = load(dir);
    st.dismissed.insert(id.to_string(), until_ms);
    prune(&mut st, now_ms());
    save(dir, &st)
}

/// Ignore le rappel `id` définitivement.
pub fn dismiss(dir: &Path, id: &str) -> Result<(), String> {
    set(dir, id, i64::MAX)
}

/// Reporte le rappel `id` : il revient à `until_ms`.
pub fn snooze(dir: &Path, id: &str, until_ms: i64) -> Result<(), String> {
    set(dir, id, until_ms)
}
