//! Calendriers importés (`.ics`, ou CSV converti en `.ics`) : fichiers de `calendars/` dans le dossier de données,
//! en lecture seule et superposables, comme « Ouvrir un calendrier » dans Outlook. Ils ne touchent pas aux données
//! d'Outlook (option A du J9). Identifiants négatifs : dossier `-(n + 1)`, rendez-vous à partir de `dossier × 10⁶`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use thunderlook_model::calendar::CalendarInfo;

use crate::{Appointment, csv, ics};

pub const SECTION: &str = "Calendriers importés";

fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("calendars")
}

/// Fichiers importés, dans l'ordre d'import (date de création, puis nom) : un nouvel import arrive en dernier et ne
/// décale pas les identifiants des autres.
fn files(data_dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(dir(data_dir))
        .map(|r| {
            r.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("ics")))
                .map(|p| (fs::metadata(&p).and_then(|m| m.created()).unwrap_or(std::time::UNIX_EPOCH), p))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v.into_iter().map(|(_, p)| p).collect()
}

/// Calendriers importés et leurs rendez-vous ; un fichier illisible est signalé, les autres sont gardés.
pub fn load(data_dir: &Path) -> (Vec<(CalendarInfo, Vec<Appointment>)>, Vec<String>) {
    let (mut out, mut errors) = (Vec::new(), Vec::new());
    for (i, path) in files(data_dir).into_iter().enumerate() {
        let folder = -(i as i64 + 1);
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        match fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| ics::parse(&t, folder, folder * 1_000_000)) {
            Ok((name, items)) => out.push((
                CalendarInfo { id: format!("c{folder}"), name: name.unwrap_or(stem), section: SECTION.into(), color: 0, count: items.len() as u32, shown: false },
                items,
            )),
            Err(e) => errors.push(format!("{} : {e}", path.display())),
        }
    }
    (out, errors)
}

/// Importe un `.ics` ou un CSV d'Outlook : vérifié, puis copié (le CSV converti) dans `calendars/`.
/// Renvoie le nom du calendrier, son nombre d'éléments et le fichier écrit.
pub fn import(data_dir: &Path, src: &Path) -> Result<(String, usize, PathBuf), String> {
    let text = fs::read_to_string(src).map_err(|e| format!("{} : {e}", src.display()))?;
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "calendrier".into());
    let is_csv = src.extension().is_some_and(|x| x.eq_ignore_ascii_case("csv"));
    let (name, body, n) = if is_csv {
        let items = csv::parse(&text, -1, -1)?;
        let refs: Vec<&Appointment> = items.iter().collect();
        (stem.clone(), ics::export(&refs, &stem, &|a| a.description.clone()), items.len())
    } else {
        let (name, items) = ics::parse(&text, -1, -1)?;
        if items.is_empty() {
            return Err(format!("{} : aucun événement (VEVENT)", src.display()));
        }
        (name.unwrap_or_else(|| stem.clone()), text.clone(), items.len())
    };
    fs::create_dir_all(dir(data_dir)).map_err(|e| e.to_string())?;
    // Nom de fichier sans caractère interdit sous Windows, et sans écraser un import précédent.
    let safe: String = stem.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    let mut target = dir(data_dir).join(format!("{safe}.ics"));
    let mut k = 2;
    while target.exists() {
        target = dir(data_dir).join(format!("{safe} ({k}).ics"));
        k += 1;
    }
    fs::write(&target, body).map_err(|e| e.to_string())?;
    Ok((name, n, target))
}

/// Retire un calendrier importé (son fichier) ; `id` = `c-<n>`.
pub fn remove(data_dir: &Path, id: &str) -> Result<(), String> {
    let n: i64 = id.strip_prefix("c-").and_then(|s| s.parse().ok()).ok_or_else(|| format!("pas un calendrier importé : {id}"))?;
    let path = files(data_dir).into_iter().nth((n - 1) as usize).ok_or_else(|| format!("calendrier importé introuvable : {id}"))?;
    fs::remove_file(&path).map_err(|e| format!("{} : {e}", path.display()))
}

/// Identifiant (`c-<n>`) du calendrier importé enregistré dans ce fichier.
pub fn id_of(data_dir: &Path, path: &Path) -> Option<String> {
    files(data_dir).iter().position(|p| p == path).map(|i| format!("c-{}", i + 1))
}
