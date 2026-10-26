//! Signatures propres à ThunderLook (lot BE) : `signatures.json` dans le dossier de données, une signature par compte
//! pour les nouveaux messages et une pour les réponses/transferts. Le dossier de signatures d'Outlook reste un repli :
//! il sert tant qu'aucune signature enregistrée n'est attribuée au compte (et pour les proposer dans le menu).
//! Rien n'est jamais écrit dans le dossier d'Outlook.

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use thunderlook_model::{ComposeKind, Signature};

use crate::compose::{default_signature, signatures};

const FILE: &str = "signatures.json";

/// Signature enregistrée. `for_new` / `for_reply` : adresses des comptes (minuscules) pour lesquels elle est insérée
/// d'office dans un nouveau message / une réponse ou un transfert.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredSignature {
    pub name: String,
    pub html: String,
    #[serde(default)]
    pub for_new: Vec<String>,
    #[serde(default)]
    pub for_reply: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct FileData {
    signatures: Vec<StoredSignature>,
}

/// Signatures enregistrées. Fichier absent : liste vide. Fichier illisible : erreur (il n'est alors jamais écrasé en silence).
pub fn load(dir: &Path) -> Result<Vec<StoredSignature>, String> {
    let path = dir.join(FILE);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{} : {e}", path.display())),
    };
    serde_json::from_str::<FileData>(&text).map(|d| d.signatures).map_err(|e| format!("{} illisible : {e}", path.display()))
}

/// Enregistre la liste (écriture atomique). Refuse un nom vide ou en double (sans tenir compte de la casse).
pub fn save(dir: &Path, list: &[StoredSignature]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for s in list {
        let name = s.name.trim();
        if name.is_empty() {
            return Err("une signature n'a pas de nom".into());
        }
        if !seen.insert(name.to_lowercase()) {
            return Err(format!("deux signatures s'appellent « {name} »"));
        }
    }
    let clean: Vec<StoredSignature> = list
        .iter()
        .map(|s| StoredSignature {
            name: s.name.trim().to_string(),
            html: s.html.clone(),
            for_new: normalize(&s.for_new),
            for_reply: normalize(&s.for_reply),
        })
        .collect();
    // Un compte n'a qu'une signature par type de message : deux attributions sont refusées, pas départagées en silence.
    for (label, lists) in [("nouveaux messages", clean.iter().map(|s| (&s.name, &s.for_new)).collect::<Vec<_>>()), ("réponses", clean.iter().map(|s| (&s.name, &s.for_reply)).collect())] {
        let mut owners = std::collections::HashMap::new();
        for (name, addrs) in lists {
            for a in addrs {
                if let Some(other) = owners.insert(a, name) {
                    return Err(format!("{a} a deux signatures pour les {label} : « {other} » et « {name} »"));
                }
            }
        }
    }
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&FileData { signatures: clean }).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{FILE}.tmp"));
    fs::write(&tmp, json).map_err(|e| format!("{} : {e}", tmp.display()))?;
    fs::rename(&tmp, dir.join(FILE)).map_err(|e| e.to_string())
}

fn normalize(addrs: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for a in addrs.iter().map(|a| a.trim().to_ascii_lowercase()).filter(|a| !a.is_empty()) {
        if !out.contains(&a) {
            out.push(a);
        }
    }
    out
}

/// Signature enregistrée attribuée à l'un des comptes `mine` pour ce type de message.
pub fn pick<'a>(stored: &'a [StoredSignature], kind: ComposeKind, mine: &[String]) -> Option<&'a StoredSignature> {
    let list = |s: &'a StoredSignature| if kind == ComposeKind::New { &s.for_new } else { &s.for_reply };
    stored.iter().find(|s| list(s).iter().any(|a| mine.iter().any(|m| m.eq_ignore_ascii_case(a))))
}

/// HTML de la signature à insérer d'office : l'enregistrée du compte, sinon celle d'Outlook (nom contenant « (adresse) »).
pub fn for_compose(dir: &Path, kind: ComposeKind, mine: &[String]) -> String {
    let stored = load(dir).unwrap_or_default();
    match pick(&stored, kind, mine) {
        Some(s) => s.html.clone(),
        None => default_signature(&signatures(), mine).map(|s| s.html).unwrap_or_default(),
    }
}

/// Signatures proposées par le menu « Signature » : les enregistrées, puis celles d'Outlook de nom différent.
pub fn menu_list(dir: &Path) -> Vec<Signature> {
    let mut out: Vec<Signature> = load(dir).unwrap_or_default().into_iter().map(|s| Signature { name: s.name, html: s.html }).collect();
    for o in signatures() {
        if !out.iter().any(|s| s.name.eq_ignore_ascii_case(&o.name)) {
            out.push(o);
        }
    }
    out
}
