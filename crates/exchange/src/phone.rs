//! Mode « fichiers seuls » : le dossier d'échange est écrit par MPE (pas de `manifest.json` de ThunderLook).
//!
//! Sans manifeste, ThunderLook ne sait pas ce que le téléphone a changé depuis la dernière lecture. Il tient donc SA base :
//! `data_dir/mpe-baseline.json` retient, par nature d'élément et par `UID` du fichier (le GUID de MPE), l'empreinte de la
//! valeur lue la dernière fois que l'élément a été réglé (appliqué, gardé tel quel, ou identique à la base). La relecture
//! s'en sert exactement comme du manifeste : fichier = empreinte retenue → le téléphone n'a rien touché ; différent et base
//! locale = empreinte retenue → modification du téléphone ; les deux ont bougé → conflit ; élément retenu mais absent du
//! fichier → suppression proposée (jamais appliquée seule).
//!
//! La base n'avance que pour ce qui a été réglé ([`settle`]) : un conflit, une suppression ou un échec restent proposés
//! au passage suivant. Ce module n'écrit jamais dans le dossier d'échange (c'est `writeback` qui le fait).

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use serde::{Deserialize, Serialize};
use thunderlook_store::rusqlite::Connection;

use crate::{ChangeKind, ImportReport, ItemKind, Result, sha256, write_atomic};

/// Ce que la base retient d'un élément réglé : l'empreinte de la valeur lue dans le fichier (`phone`) et celle de
/// l'élément local tel qu'il était juste après (`here`). Le téléphone n'a rien touché quand le fichier a encore `phone` ;
/// ThunderLook n'a rien touché quand l'élément local a encore `here` (les deux peuvent différer si le format de fichier
/// ne reprend pas tout, p. ex. une adresse e-mail que ThunderLook refuse).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Print {
    pub phone: String,
    pub here: String,
}

/// `UID` du fichier → empreintes retenues.
pub(crate) type Prints = HashMap<String, Print>;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Base {
    /// Par nature d'élément (`ItemKind::name`).
    pub items: HashMap<String, Prints>,
    /// Signature des fichiers lus au dernier passage sans reste (voir [`files_signature`]) ; vide : à relire.
    pub signature: String,
    /// Mise en route du report ThunderLook → téléphone (RFC 3339) : seuls les éléments créés ici après cet instant sont
    /// ajoutés aux fichiers (voir `writeback`). Vide : jamais mis en route.
    pub since: String,
}

fn path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("mpe-baseline.json")
}

pub(crate) fn read_base(data_dir: &Path) -> Base {
    fs::read_to_string(path(data_dir)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub(crate) fn write_base(data_dir: &Path, base: &Base) -> Result<()> {
    write_atomic(&path(data_dir), serde_json::to_string(base).map_err(|e| e.to_string())?.as_bytes())
}

/// Empreinte des fichiers d'échange présents (noms et SHA-256 de leur contenu) : identique d'un passage à l'autre tant
/// que MPE n'a rien réécrit.
pub fn files_signature(dir: &Path) -> String {
    let mut parts = Vec::new();
    for kind in ItemKind::all() {
        if let Ok(bytes) = fs::read(dir.join(kind.file())) {
            parts.push(format!("{}={}", kind.file(), sha256::hex(&bytes)));
        }
    }
    sha256::hex(parts.join("\n").as_bytes())
}

/// Dossier écrit par MPE : des fichiers du téléphone, mais aucun `manifest.json` de ThunderLook. Un export y écraserait
/// les fichiers de MPE (et leurs identifiants) : il ne doit pas s'y faire. Rend les fichiers présents.
pub fn foreign_files(dir: &Path) -> Vec<String> {
    if crate::read_manifest(dir).is_some() {
        return Vec::new();
    }
    ItemKind::all().iter().map(|k| k.file()).filter(|n| dir.join(n).is_file()).map(str::to_string).collect()
}

/// Signature retenue (`None` si aucune).
pub fn last_signature(data_dir: &Path) -> Option<String> {
    Some(read_base(data_dir).signature).filter(|s| !s.is_empty())
}

/// Retient (ou, vide, oublie) la signature des fichiers lus.
pub fn set_signature(data_dir: &Path, signature: &str) -> Result<()> {
    let mut base = read_base(data_dir);
    base.signature = signature.to_string();
    write_base(data_dir, &base)
}

/// Éléments dont la base retient l'empreinte, pour une nature d'élément.
pub(crate) fn prints_of(data_dir: &Path, kind: ItemKind) -> Prints {
    read_base(data_dir).items.remove(kind.name()).unwrap_or_default()
}

/// Règle ce qui vient d'être traité : à appeler APRÈS [`crate::apply`], avec les clés des changements réglés
/// ([`crate::ApplyReport::done`]). Sans effet hors mode « fichiers seuls » (`rep.read` vide).
///
/// - un élément lu sans changement proposé, ou dont le changement est réglé, prend l'empreinte du fichier ;
/// - son identifiant dans le fichier est retenu pour l'élément local (`exchange-map.json`), y compris pour un élément
///   retrouvé sans modification : supprimé ensuite dans ThunderLook, il ne sera pas recréé ;
/// - un changement non réglé (conflit, suppression, échec) laisse la base telle quelle : il sera reproposé ;
/// - les éléments retenus qui ne sont plus dans le fichier ni en attente de suppression sont oubliés.
pub fn settle(conn: &Connection, data_dir: &Path, rep: &ImportReport, done: &[String]) -> Result<()> {
    if rep.read.is_empty() {
        return Ok(());
    }
    let done: HashSet<&str> = done.iter().map(String::as_str).collect();
    let mut base = read_base(data_dir);
    let mut map = crate::read_map(data_dir);
    let mut present: HashMap<ItemKind, HashSet<&str>> = HashMap::new();
    for s in &rep.seen {
        present.entry(s.item).or_default().insert(s.uid.as_str());
        let ids = map.entry(s.item.name().to_string()).or_default();
        // L'identité est connue dès qu'on a retrouvé l'élément, réglé ou non : un conflit qui reste ouvert ne doit pas
        // faire recréer l'élément si le téléphone le renomme.
        let local = s.local.clone().or_else(|| ids.get(&s.uid).cloned());
        if let Some(local) = &local {
            ids.insert(s.uid.clone(), local.clone());
        }
        if s.key.as_deref().is_some_and(|k| !done.contains(k)) {
            continue;
        }
        let prints = base.items.entry(s.item.name().to_string()).or_default();
        let known = prints.get(&s.uid).filter(|p| p.phone == s.fp);
        let print = match (known, s.key.is_some()) {
            // Rien n'a bougé pour cet élément depuis le dernier règlement.
            (Some(p), false) => p.clone(),
            // Appliqué, gardé ou vu pour la première fois : l'élément local d'après (sinon la valeur lue).
            _ => Print { phone: s.fp.clone(), here: local.as_deref().and_then(|l| crate::import::here_print(conn, data_dir, s.item, l)).unwrap_or_else(|| s.fp.clone()) },
        };
        prints.insert(s.uid.clone(), print);
    }
    for &kind in &rep.read {
        let pending: HashSet<&str> = rep.changes.iter().filter(|c| c.item == kind && c.kind == ChangeKind::Deleted && !done.contains(c.key.as_str())).map(|c| c.id.as_str()).collect();
        let here = present.get(&kind);
        let ids = map.get(kind.name());
        if let Some(prints) = base.items.get_mut(kind.name()) {
            prints.retain(|uid, _| here.is_some_and(|h| h.contains(uid.as_str())) || ids.and_then(|m| m.get(uid)).is_some_and(|l| pending.contains(l.as_str())));
        }
    }
    crate::write_map(data_dir, &map)?;
    write_base(data_dir, &base)
}
