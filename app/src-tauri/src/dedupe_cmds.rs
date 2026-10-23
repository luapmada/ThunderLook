//! Doublons de contacts (lot S) : analyse d'un dossier, détail d'un groupe, aperçu et fusion, « pas des doublons »
//! (`dedupe-ignore.json` dans le dossier de données). Le calcul est dans `thunderlook_contacts::dedupe`.

use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use thunderlook_contacts::dedupe::{self, Candidate, Choices, Field};
use thunderlook_model::contacts::ContactForm;

use crate::data_dir;

const IGNORE_FILE: &str = "dedupe-ignore.json";

/// Un groupe dans la liste.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSummary {
    /// `p<id>` des membres.
    pub ids: Vec<String>,
    pub titles: Vec<String>,
    pub reason: String,
    pub score: u8,
    pub safe: bool,
    pub chain: Option<String>,
}

/// Une ligne du comparatif : une valeur par fiche ; `key` vide pour une ligne qui n'est pas à choisir.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub key: String,
    pub label: String,
    pub values: Vec<String>,
    /// Fiche dont la valeur est reprise par défaut (`p<id>`).
    pub chosen: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub ids: Vec<String>,
    pub titles: Vec<String>,
    /// Fiche conservée par défaut.
    pub keep: String,
    pub rows: Vec<Row>,
}

/// Retour d'une fusion en série.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bulk {
    pub groups: usize,
    pub deleted: usize,
}

fn parse_id(s: &str) -> Result<i64, String> {
    s.strip_prefix('p').and_then(|f| f.parse().ok()).ok_or_else(|| format!("contact inconnu : {s}"))
}

fn parse_ids(ids: &[String]) -> Result<Vec<i64>, String> {
    ids.iter().map(|s| parse_id(s)).collect()
}

fn parse_choices(c: &BTreeMap<String, String>) -> Result<Choices, String> {
    c.iter().map(|(k, v)| Ok((Field::from_key(k).ok_or_else(|| format!("champ inconnu : {k}"))?, parse_id(v)?))).collect()
}

fn ignore_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join(IGNORE_FILE))
}

fn read_ignored(app: &AppHandle) -> Result<HashSet<(i64, i64)>, String> {
    let Ok(text) = std::fs::read_to_string(ignore_path(app)?) else { return Ok(HashSet::new()) };
    let pairs: Vec<(i64, i64)> = serde_json::from_str(&text).map_err(|e| format!("{IGNORE_FILE} illisible : {e}"))?;
    Ok(pairs.into_iter().map(|(a, b)| (a.min(b), a.max(b))).collect())
}

fn write_ignored(app: &AppHandle, set: &HashSet<(i64, i64)>) -> Result<(), String> {
    let mut v: Vec<_> = set.iter().copied().collect();
    v.sort_unstable();
    let path = ignore_path(app)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(&v).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Groupes de doublons d'un dossier de contacts (`k<id>`) ; sans dossier, de toute la base. Les groupes écartés sont retirés.
fn scan(app: &AppHandle, folder: Option<&str>) -> Result<(thunderlook_contacts::Contacts, Vec<dedupe::Group>), String> {
    let folder = match folder.filter(|f| !f.is_empty()) {
        Some(f) => Some(f.strip_prefix('k').and_then(|f| f.parse::<i64>().ok()).ok_or("dossier de contacts inconnu")?),
        None => None,
    };
    let conn = thunderlook_store::open(&data_dir(app)?).map_err(|e| e.to_string())?;
    let all = thunderlook_contacts::load(&conn, folder, None)?;
    let groups = dedupe::without_ignored(dedupe::find_duplicates(&all.contacts), &read_ignored(app)?);
    Ok((all, groups))
}

/// Liste des groupes de doublons (les plus sûrs d'abord).
#[tauri::command(async)]
pub fn dedupe_scan(app: AppHandle, folder: Option<String>) -> Result<Vec<GroupSummary>, String> {
    let (all, groups) = scan(&app, folder.as_deref())?;
    let title = |id: i64| all.contacts.iter().find(|c| c.id == id).map(|c| c.title()).unwrap_or_default();
    Ok(groups
        .into_iter()
        .map(|g| GroupSummary {
            ids: g.ids.iter().map(|i| format!("p{i}")).collect(),
            titles: g.ids.iter().map(|&i| title(i)).collect(),
            reason: g.reason.label(),
            score: g.score,
            safe: g.safe,
            chain: g.chain,
        })
        .collect())
}

fn rows(cands: &[Candidate], keep: i64) -> Vec<Row> {
    let mut rows: Vec<Row> = dedupe::FIELDS
        .iter()
        .map(|&f| Row {
            key: f.key().into(),
            label: f.label().into(),
            // La photo ne s'affiche pas en texte : présence seulement.
            values: cands.iter().map(|c| if f == Field::Photo { if f.value(&c.form).is_empty() { String::new() } else { "(photo)".into() } } else { f.value(&c.form) }).collect(),
            chosen: dedupe::default_source(f, cands, keep).map(|i| format!("p{i}")).unwrap_or_default(),
        })
        .collect();
    let plain = |label: &str, value: &dyn Fn(&Candidate) -> String| Row { key: String::new(), label: label.into(), values: cands.iter().map(value).collect(), chosen: String::new() };
    rows.push(plain("Courriels", &|c| [&c.form.email1, &c.form.email2, &c.form.email3].iter().filter(|s| !s.trim().is_empty()).map(|s| s.trim()).collect::<Vec<_>>().join(", ")));
    rows.push(plain("Téléphones", &|c| {
        let own = [("Mobile", &c.form.mobile), ("Bureau", &c.form.business), ("Domicile", &c.form.home_phone)];
        own.iter().filter(|(_, v)| !v.trim().is_empty()).map(|(l, v)| format!("{l} {}", v.trim())).chain(c.other_phones.iter().map(|(l, v)| format!("{l} {v}"))).collect::<Vec<_>>().join(", ")
    }));
    rows.push(plain("Catégories", &|c| c.form.categories.clone()));
    rows.push(plain("Notes", &|c| c.form.notes.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect()));
    rows
}

/// Fiches côte à côte d'un groupe, avec le contact conservé et les valeurs retenues par défaut.
#[tauri::command(async)]
pub fn dedupe_detail(app: AppHandle, ids: Vec<String>) -> Result<Detail, String> {
    let ids = parse_ids(&ids)?;
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let cands = dedupe::candidates(&conn, &dir, &ids)?;
    let keep = dedupe::default_keep(&cands).ok_or("groupe vide")?;
    Ok(Detail {
        ids: ids.iter().map(|i| format!("p{i}")).collect(),
        titles: cands.iter().map(|c| [c.form.given_name.trim(), c.form.surname.trim()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ")).collect(),
        keep: format!("p{keep}"),
        rows: rows(&cands, keep),
    })
}

/// Aperçu de la fiche fusionnée, sans rien écrire.
#[tauri::command(async)]
pub fn dedupe_preview(app: AppHandle, ids: Vec<String>, keep: String, choices: BTreeMap<String, String>) -> Result<ContactForm, String> {
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let cands = dedupe::candidates(&conn, &dir, &parse_ids(&ids)?)?;
    dedupe::merge(&cands, parse_id(&keep)?, &parse_choices(&choices)?)
}

/// Fusionne un groupe dans le contact `keep` ; rend le nombre de fiches supprimées.
#[tauri::command(async)]
pub fn dedupe_merge(app: AppHandle, ids: Vec<String>, keep: String, choices: BTreeMap<String, String>) -> Result<usize, String> {
    let dir = data_dir(&app)?;
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let done = dedupe::apply_merge(&mut conn, &dir, &parse_ids(&ids)?, parse_id(&keep)?, &parse_choices(&choices)?);
    let _ = app.emit("contacts-changed", ());
    Ok(done?.deleted.len())
}

/// « Ce ne sont pas des doublons » : mémorise toutes les paires du groupe.
#[tauri::command(async)]
pub fn dedupe_ignore(app: AppHandle, ids: Vec<String>) -> Result<(), String> {
    let mut set = read_ignored(&app)?;
    set.extend(dedupe::all_pairs(&parse_ids(&ids)?));
    write_ignored(&app, &set)
}

/// Fusionne tous les groupes sûrs (même courriel ET même nom) du dossier, en gardant la fiche la plus riche.
/// Un groupe qui échoue arrête la série : ceux déjà fusionnés restent fusionnés.
#[tauri::command(async)]
pub fn dedupe_merge_safe(app: AppHandle, folder: Option<String>) -> Result<Bulk, String> {
    let (_, groups) = scan(&app, folder.as_deref())?;
    let dir = data_dir(&app)?;
    let mut conn = thunderlook_store::open(&dir).map_err(|e| e.to_string())?;
    let mut bulk = Bulk { groups: 0, deleted: 0 };
    let mut failure = None;
    for g in groups.iter().filter(|g| g.safe) {
        let step = dedupe::candidates(&conn, &dir, &g.ids).and_then(|cands| {
            let keep = dedupe::default_keep(&cands).ok_or("groupe vide")?;
            dedupe::apply_merge(&mut conn, &dir, &g.ids, keep, &Choices::new())
        });
        match step {
            Ok(m) => {
                bulk.groups += 1;
                bulk.deleted += m.deleted.len();
            }
            Err(e) => {
                failure = Some(format!("{e} (après {} groupe(s) fusionné(s))", bulk.groups));
                break;
            }
        }
    }
    let _ = app.emit("contacts-changed", ());
    failure.map_or(Ok(bulk), Err)
}
