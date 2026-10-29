//! Synchronisation AUTOMATIQUE avec le dossier d'échange (MPE y écrit toutes les heures).
//!
//! Un passage ([`run_pass`]) relit le dossier SANS intervention, avec des garde-fous :
//!
//! 1. **Fichiers stables seulement** ([`check_stable`]) : taille et date identiques sur deux lectures espacées de
//!    `stable_gap`, et dernière écriture plus ancienne que `min_age`. Sinon, rien n'est lu (MPE écrit peut-être).
//! 2. **Appliqué automatiquement** : uniquement les créations et modifications d'un seul côté (téléphone).
//! 3. **Mis de côté, comptés** : suppressions, conflits, séries refusées, autres avertissements (jamais appliqués ici).
//! 4. **Seuil de sécurité** : si, pour un module, le passage toucherait plus de `threshold_pct` % de ses éléments (ou
//!    plus de `threshold_max`), rien n'est appliqué du tout et le passage est signalé « bloqué ». Sous `threshold_floor`
//!    changements, seul le plafond absolu joue (sinon un module de 10 éléments ne pourrait jamais recevoir 1 changement).
//! 5. **Instantané annulable** : [`crate::apply`] consigne de quoi défaire dans `exchange-undo/` AVANT d'écrire ; le
//!    passage rend son horodatage (`undo`), défait par [`crate::undo`].
//! 6. **Export après import** seulement si le téléphone n'avait rien modifié depuis le dernier export (sinon l'export
//!    écraserait ce qui est mis de côté).
//!
//! **Dossier écrit par MPE** (aucun `manifest.json`, `contacts.vcf` / `calendrier.ics` / `notes.vnt` seuls) : mode
//! « fichiers seuls ». ThunderLook n'y écrit jamais d'export complet ni de manifeste ; il compare chaque élément à la base
//! qu'il tient lui-même (`phone`, `mpe-baseline.json`) : créé ou modifié sur le téléphone → appliqué ; supprimé → mis de
//! côté. Avec `auto_export`, il reporte ensuite SES modifications fiche par fiche dans ces fichiers ([`crate::write_back`]),
//! que MPE renvoie au téléphone à la synchro suivante.
//! Tant que les fichiers n'ont pas changé depuis un passage sans reste, le passage suivant ne relit rien.
//!
//! Le verrou anti-chevauchement avec le passage manuel est posé par l'appelant (commandes Tauri). Ce module n'écrit
//! jamais dans Outlook et ne pilote pas MPE : il lit des fichiers et écrit dans la base de ThunderLook.

use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use thunderlook_store::rusqlite::Connection;

use crate::{Change, ChangeKind, ItemKind, Result, read_manifest, write_atomic};

pub const MIN_MINUTES: u32 = 5;
pub const MAX_MINUTES: u32 = 120;
/// MPE écrit toutes les heures : relire plus souvent qu'elle ne synchronise, pour ne jamais la suivre de loin.
pub const DEFAULT_MINUTES: u32 = 15;
const JOURNAL_MAX: usize = 200;

/// Réglages (`data_dir/auto-sync.json`, clé `settings`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub enabled: bool,
    /// Intervalle entre deux passages, de 5 à 120 min.
    pub minutes: u32,
    /// Exporter après un import quand le téléphone n'avait rien modifié depuis le dernier export.
    pub auto_export: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { enabled: true, minutes: DEFAULT_MINUTES, auto_export: true }
    }
}

impl Settings {
    pub fn clamped(mut self) -> Self {
        self.minutes = self.minutes.clamp(MIN_MINUTES, MAX_MINUTES);
        self
    }
}

/// Paramètres d'un passage (les durées sont réglables pour les tests).
#[derive(Clone, Debug)]
pub struct Params {
    pub auto_export: bool,
    /// Délai entre les deux lectures de stabilité.
    pub stable_gap: Duration,
    /// Âge minimal de la dernière écriture d'un fichier.
    pub min_age: Duration,
    pub threshold_pct: f64,
    pub threshold_max: usize,
    pub threshold_floor: usize,
    /// « Appliquer malgré le seuil » : le seuil de sécurité n'est pas opposé à ce passage (demandé à la main).
    pub ignore_threshold: bool,
    /// « Relire maintenant » : relit même si les fichiers n'ont pas changé depuis le dernier passage sans reste.
    pub recheck: bool,
}

impl Default for Params {
    fn default() -> Self {
        Params { auto_export: true, stable_gap: Duration::from_secs(5), min_age: Duration::from_secs(10), threshold_pct: 5.0, threshold_max: 50, threshold_floor: 3, ignore_threshold: false, recheck: false }
    }
}

// --- Stabilité ------------------------------------------------------------------------------------------------------

/// (nom, taille, date de modification en ms) de chaque fichier d'échange ; `None` : absent.
type Sig = Vec<(String, Option<(u64, i64)>)>;

fn names() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = ItemKind::all().iter().map(|k| k.file()).collect();
    v.push(crate::MANIFEST_FILE);
    v
}

fn signature(dir: &Path) -> Sig {
    names()
        .into_iter()
        .map(|n| {
            let meta = fs::metadata(dir.join(n)).ok().filter(|m| m.is_file());
            let sig = meta.map(|m| {
                let ms = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as i64);
                (m.len(), ms)
            });
            (n.to_string(), sig)
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "detail")]
pub enum Stability {
    Stable,
    /// Un fichier a changé entre les deux lectures, ou vient d'être écrit : MPE écrit peut-être.
    Unstable(String),
    /// Aucun fichier du téléphone, ou fichier du manifeste absent : rien à relire.
    Missing(String),
}

/// Deux lectures espacées de `p.stable_gap` ; stable si identiques et plus vieilles que `p.min_age`.
pub fn check_stable(dir: &Path, p: &Params) -> Stability {
    let first = signature(dir);
    thread::sleep(p.stable_gap);
    let second = signature(dir);
    if let Some(m) = read_manifest(dir) {
        for f in &m.files {
            if second.iter().any(|(n, s)| *n == f.name && s.is_none()) {
                return Stability::Missing(format!("{} absent du dossier d'échange", f.name));
            }
        }
    } else if !ItemKind::all().iter().any(|k| second.iter().any(|(n, s)| n == k.file() && s.is_some())) {
        // Dossier écrit par MPE : pas de manifeste, mais au moins un des fichiers du téléphone.
        return Stability::Missing("aucun fichier du téléphone dans le dossier d'échange".into());
    }
    for ((name, a), (_, b)) in first.iter().zip(&second) {
        if a != b {
            return Stability::Unstable(format!("{name} a changé pendant la lecture"));
        }
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    let min_ms = p.min_age.as_millis() as i64;
    for (name, s) in &second {
        if let Some((_, ms)) = s
            && now - ms < min_ms
        {
            return Stability::Unstable(format!("{name} vient d'être écrit"));
        }
    }
    Stability::Stable
}

// --- Passage --------------------------------------------------------------------------------------------------------

/// Éléments mis de côté (jamais appliqués automatiquement), par catégorie.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Aside {
    pub deletions: usize,
    pub conflicts: usize,
    /// Séries refusées par la relecture (non représentables, voir `docs/mpe-fichiers.md` § 6).
    pub series: usize,
    /// Autres avertissements de la relecture.
    pub warnings: usize,
}

impl Aside {
    pub fn total(&self) -> usize {
        self.deletions + self.conflicts + self.series + self.warnings
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "detail")]
pub enum Outcome {
    /// Rien à relire / ignoré (fichiers instables, absents).
    Skipped(String),
    /// Seuil de sécurité dépassé : RIEN n'a été appliqué.
    Blocked(String),
    /// Relu et appliqué (éventuellement 0 changement).
    Done,
    /// Erreur technique.
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PassReport {
    /// `AAAA-MM-JJTHH:MM:SSZ` (UTC).
    pub at: String,
    pub outcome: Outcome,
    pub created: usize,
    pub modified: usize,
    pub aside: Aside,
    /// Horodatage de l'instantané (`exchange-undo`), s'il y en a un.
    pub undo: Option<String>,
    /// `Some(true)` : exporté ; `Some(false)` : export non fait (voir `export_note`) ; `None` : désactivé.
    pub exported: Option<bool>,
    pub export_note: String,
    pub errors: Vec<String>,
    /// Dossier écrit par MPE (sans manifeste) : relu, puis seules les fiches modifiées ici y sont réécrites (`write_back`).
    pub files_only: bool,
    /// Champs d'une fiche que le format de ThunderLook ne reprend pas (télécopie, adresse en trop…) : informatif, ni
    /// mis de côté ni à traiter.
    pub losses: usize,
}

impl Default for PassReport {
    fn default() -> Self {
        PassReport { at: crate::now_stamp(), outcome: Outcome::Done, created: 0, modified: 0, aside: Aside::default(), undo: None, exported: None, export_note: String::new(), errors: Vec::new(), files_only: false, losses: 0 }
    }
}

/// Début du motif d'un passage ignoré parce que rien n'a changé : ni journalisé, ni compté comme dernier passage.
pub const UNCHANGED: &str = "fichiers inchangés";

fn skipped(why: String) -> PassReport {
    PassReport { outcome: Outcome::Skipped(why), ..Default::default() }
}

fn module_label(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Contact => "contacts",
        ItemKind::Task => "tâches",
        ItemKind::Note => "notes",
        ItemKind::Event => "rendez-vous",
    }
}

/// Modules dont le passage dépasserait le seuil : (module, changements, éléments de référence).
fn over_threshold(touched: &[(ItemKind, usize)], sizes: impl Fn(ItemKind) -> usize, p: &Params) -> Vec<String> {
    let mut out = Vec::new();
    for kind in ItemKind::all() {
        let n = touched.iter().find(|(k, _)| *k == kind).map_or(0, |(_, n)| *n);
        if n == 0 {
            continue;
        }
        let total = sizes(kind);
        let pct_hit = n > p.threshold_floor && (n as f64) * 100.0 > p.threshold_pct * (total.max(1) as f64);
        if pct_hit || n > p.threshold_max {
            out.push(format!("{} : {n} changement(s) sur {total} élément(s)", module_label(kind)));
        }
    }
    out
}

/// Un passage complet. `dir` : dossier d'échange. L'appelant tient le verrou contre le passage manuel.
pub fn run_pass(conn: &mut Connection, data_dir: &Path, dir: &Path, p: &Params) -> PassReport {
    match pass(conn, data_dir, dir, p) {
        Ok(r) => r,
        Err(e) => PassReport { outcome: Outcome::Failed(e), ..Default::default() },
    }
}

/// Fichier complet : sa dernière ligne non vide ferme l'enregistrement. MPE qui s'arrête au milieu d'une écriture laisse un
/// fichier tronqué, stable mais faux : le lire ferait passer la dernière fiche pour une modification.
fn incomplete(dir: &Path) -> Option<String> {
    for kind in ItemKind::all() {
        let Ok(bytes) = fs::read(dir.join(kind.file())) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let last = text.lines().map(str::trim).rev().find(|l| !l.is_empty());
        let end = match kind {
            ItemKind::Contact => "END:VCARD",
            ItemKind::Note => "END:VNOTE",
            ItemKind::Task | ItemKind::Event => "END:VCALENDAR",
        };
        match last {
            None => return Some(format!("{} est vide", kind.file())),
            Some(l) if !l.eq_ignore_ascii_case(end) => return Some(format!("{} semble tronqué (dernière ligne : « {} »)", kind.file(), l.chars().take(40).collect::<String>())),
            _ => {}
        }
    }
    None
}

fn pass(conn: &mut Connection, data_dir: &Path, dir: &Path, p: &Params) -> Result<PassReport> {
    match check_stable(dir, p) {
        Stability::Stable => {}
        Stability::Unstable(w) | Stability::Missing(w) => return Ok(skipped(w)),
    }
    let manifest = read_manifest(dir);
    // Sans manifeste : dossier écrit par MPE, relu seul (voir l'en-tête du module).
    let files_only = manifest.is_none();
    let signature = files_only.then(|| crate::files_signature(dir));
    if let Some(sig) = &signature {
        if !p.recheck && crate::last_signature(data_dir).as_ref() == Some(sig) {
            // Rien à relire, mais ce qui a été modifié ici depuis part quand même vers le téléphone.
            if p.auto_export {
                match crate::write_back(conn, data_dir, dir, false) {
                    Ok(w) if !w.is_empty() => {
                        let mut note = format!("envoyé vers le téléphone : {} modifié(s), {} créé(s), {} supprimé(s)", w.modified, w.created, w.deleted);
                        if !w.skipped.is_empty() {
                            note.push_str(&format!(" ; non envoyé : {}", w.skipped.join(" ; ")));
                        }
                        return Ok(PassReport { files_only: true, exported: Some(true), export_note: note, ..Default::default() });
                    }
                    Ok(_) => {}
                    Err(e) => return Ok(PassReport { files_only: true, exported: Some(false), export_note: format!("envoi vers le téléphone en échec : {e}"), ..Default::default() }),
                }
            }
            return Ok(skipped(format!("{UNCHANGED} depuis le dernier passage")));
        }
        if let Some(why) = incomplete(dir) {
            return Ok(skipped(why));
        }
    }
    // Décidé AVANT d'appliquer : le téléphone a-t-il touché un fichier depuis le dernier export ?
    let phone_untouched = files_only || crate::modified_files(dir).is_empty();
    let manifest = manifest.unwrap_or_default();
    let rep = crate::import_changes(conn, data_dir, dir)?;

    let mut aside = Aside { deletions: 0, conflicts: rep.conflicts.len(), series: 0, warnings: 0 };
    let mut losses = 0;
    for w in &rep.warnings {
        if w.contains(" : fiche ") {
            losses += 1;
        } else if w.contains("série") {
            aside.series += 1;
        } else {
            aside.warnings += 1;
        }
    }
    // Seuil : seulement ce qui serait appliqué. Suppressions et conflits sont toujours mis de côté, donc sans risque ; les compter
    // bloquerait chaque passage tant que des conflits anciens restent en attente (le nouvel élément du téléphone n'entrerait jamais).
    let mut touched: Vec<(ItemKind, usize)> = Vec::new();
    let mut count = |k: ItemKind| match touched.iter_mut().find(|(i, _)| *i == k) {
        Some((_, n)) => *n += 1,
        None => touched.push((k, 1)),
    };
    let mut auto: Vec<Change> = Vec::new();
    for c in &rep.changes {
        match c.kind {
            ChangeKind::Deleted => aside.deletions += 1,
            _ if c.conflict.is_some() => aside.conflicts += 1,
            _ => {
                count(c.item);
                auto.push(c.clone());
            }
        }
    }
    let sizes = |k: ItemKind| if files_only { rep.seen.iter().filter(|s| s.item == k).count() } else { manifest.file(k.file()).map_or(0, |f| f.ids.len()) };
    let over = if p.ignore_threshold { Vec::new() } else { over_threshold(&touched, sizes, p) };
    if !over.is_empty() {
        if let Some(sig) = &signature {
            // Même fichier, même verdict : inutile de le redire (et de le journaliser) à chaque passage.
            let _ = crate::set_signature(data_dir, sig);
        }
        return Ok(PassReport { outcome: Outcome::Blocked(over.join(" ; ")), aside, files_only, losses, ..Default::default() });
    }

    let mut report = PassReport { aside, files_only, losses, ..Default::default() };
    let mut done_keys: Vec<String> = Vec::new();
    if !auto.is_empty() {
        let done = crate::apply(conn, data_dir, &auto)?;
        report.undo = done.undo;
        for c in &auto {
            if done.errors.iter().any(|e| e.starts_with(&format!("{} : ", c.summary))) {
                continue;
            }
            if c.kind == ChangeKind::New { report.created += 1 } else { report.modified += 1 }
        }
        report.errors = done.errors;
        done_keys = done.done;
    }
    if files_only {
        crate::settle(conn, data_dir, &rep, &done_keys)?;
        // Rien ne reste à réessayer (échec d'application) : les mêmes fichiers n'ont plus à être relus.
        let _ = crate::set_signature(data_dir, if report.errors.is_empty() { signature.as_deref().unwrap_or("") } else { "" });
        // Sens ThunderLook → téléphone : les modifications faites ici, reportées fiche par fiche dans les fichiers de MPE.
        if p.auto_export && report.errors.is_empty() {
            match crate::write_back(conn, data_dir, dir, false) {
                Ok(w) if w.is_empty() => {}
                Ok(w) => {
                    report.exported = Some(true);
                    report.export_note = format!("envoyé vers le téléphone : {} modifié(s), {} créé(s), {} supprimé(s)", w.modified, w.created, w.deleted);
                    if !w.skipped.is_empty() {
                        report.export_note.push_str(&format!(" ; non envoyé : {}", w.skipped.join(" ; ")));
                    }
                }
                Err(e) => {
                    report.exported = Some(false);
                    report.export_note = format!("envoi vers le téléphone en échec : {e}");
                }
            }
        }
    }

    if p.auto_export && !files_only {
        if !phone_untouched {
            report.exported = Some(false);
            report.export_note = "export non fait : le téléphone avait modifié des fichiers depuis le dernier export".into();
        } else {
            match crate::export_all(conn, data_dir, dir) {
                Ok(_) => report.exported = Some(true),
                Err(e) => {
                    report.exported = Some(false);
                    report.export_note = format!("export en échec : {e}");
                }
            }
        }
    }
    Ok(report)
}

// --- État et journal ------------------------------------------------------------------------------------------------

/// Ce que la barre d'état affiche : réglages, dernier passage, éléments mis de côté (valeurs du DERNIER passage).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoState {
    pub settings: Settings,
    pub last: Option<PassReport>,
    /// Dernier passage qui a relu les fichiers (ni ignoré ni en erreur) : les compteurs de côté restent visibles même
    /// quand le passage suivant est ignoré (fichiers instables).
    pub last_read: Option<PassReport>,
    /// Dernière fois que le dossier a été regardé (même sans rien relire parce que les fichiers n'avaient pas changé).
    pub last_checked: Option<String>,
}

fn state_path(data_dir: &Path) -> PathBuf {
    data_dir.join("auto-sync.json")
}

fn journal_path(data_dir: &Path) -> PathBuf {
    data_dir.join("auto-sync-journal.json")
}

pub fn read_state(data_dir: &Path) -> AutoState {
    fs::read_to_string(state_path(data_dir)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn write_state(data_dir: &Path, st: &AutoState) -> Result<()> {
    write_atomic(&state_path(data_dir), serde_json::to_string_pretty(st).map_err(|e| e.to_string())?.as_bytes())
}

pub fn set_settings(data_dir: &Path, settings: Settings) -> Result<AutoState> {
    let mut st = read_state(data_dir);
    st.settings = settings.clamped();
    write_state(data_dir, &st)?;
    Ok(st)
}

/// Journal des passages, du plus récent au plus ancien (200 entrées au plus).
pub fn read_journal(data_dir: &Path) -> Vec<PassReport> {
    fs::read_to_string(journal_path(data_dir)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Consigne un passage dans le journal et dans l'état.
pub fn record(data_dir: &Path, report: &PassReport) -> Result<AutoState> {
    if matches!(&report.outcome, Outcome::Skipped(w) if w.starts_with(UNCHANGED)) {
        let mut st = read_state(data_dir);
        st.last_checked = Some(report.at.clone());
        write_state(data_dir, &st)?;
        return Ok(st);
    }
    let mut journal = read_journal(data_dir);
    journal.insert(0, report.clone());
    journal.truncate(JOURNAL_MAX);
    write_atomic(&journal_path(data_dir), serde_json::to_string_pretty(&journal).map_err(|e| e.to_string())?.as_bytes())?;
    let mut st = read_state(data_dir);
    st.last_checked = Some(report.at.clone());
    st.last = Some(report.clone());
    if matches!(report.outcome, Outcome::Done | Outcome::Blocked(_)) {
        st.last_read = Some(report.clone());
    }
    write_state(data_dir, &st)?;
    Ok(st)
}

/// Un passage entier, consigné : relit, applique, journalise.
pub fn run_and_record(conn: &mut Connection, data_dir: &Path, dir: &Path, p: &Params) -> (PassReport, Result<AutoState>) {
    let report = run_pass(conn, data_dir, dir, p);
    let st = record(data_dir, &report);
    (report, st)
}

/// Après une annulation de `stamp` : l'instantané n'est plus annulable (état) et l'annulation est inscrite au journal.
/// Les compteurs « mis de côté » du dernier passage lu restent affichés.
pub fn mark_undone(data_dir: &Path, stamp: &str, message: &str) -> Result<AutoState> {
    let mut journal = read_journal(data_dir);
    journal.insert(0, PassReport { outcome: Outcome::Skipped(format!("annulation du passage : {message}")), ..Default::default() });
    journal.truncate(JOURNAL_MAX);
    write_atomic(&journal_path(data_dir), serde_json::to_string_pretty(&journal).map_err(|e| e.to_string())?.as_bytes())?;
    let mut st = read_state(data_dir);
    for r in [st.last.as_mut(), st.last_read.as_mut()].into_iter().flatten() {
        if r.undo.as_deref() == Some(stamp) {
            r.undo = None;
        }
    }
    write_state(data_dir, &st)?;
    Ok(st)
}
