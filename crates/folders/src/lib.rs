//! Dossiers de rendez-vous, de contacts, de tâches et de notes : tous viennent de l'import du PST ; ici on les crée, les
//! renomme, les supprime et on colore les calendriers.
//!
//! - Nom unique (sans tenir compte de la casse) par fichier de données.
//! - Nouveaux dossiers : NID ≥ [`FIRST_NID`], jamais un NID déjà pris dans le même fichier de données.
//! - Suppression refusée tant que le dossier contient des éléments ou des sous-dossiers, sauf [`Content::MoveTo`] ou
//!   [`Content::Delete`]. Chaque élément déplacé ou supprimé passe d'abord par `thunderlook_safety` (corbeille interne,
//!   annulation) ; si cet instantané échoue, rien n'est touché.
//! - Affichage seulement (rien n'est supprimé ni fusionné) : un nom technique du PST (`Calendar3098`) devient le nom
//!   générique de la sorte (« Calendrier ») ; deux dossiers de même nom affiché reçoivent le fichier de données en suffixe
//!   (« Calendrier (Pro.pst) »), puis un numéro s'ils sont dans le même fichier ; un dossier technique vide d'un PST est
//!   masqué de [`list`] (il reste dans la base et dans [`all`]).
//! - Couleur d'un calendrier : `calendar-colors.json` du dossier de données ({ "c12": 7 }, 0 à 24 = palette des catégories
//!   d'Outlook).

use std::{collections::BTreeMap, fs, path::Path};

use serde::{Deserialize, Serialize};
use thunderlook_store::rusqlite::{Connection, OptionalExtension, params};

pub type Result<T> = std::result::Result<T, String>;

/// Premier NID attribué à un dossier créé par ThunderLook (les NID de dossier du PST sont très en dessous).
pub const FIRST_NID: i64 = 0x7000_0000;
/// Nombre de couleurs de la palette des catégories d'Outlook (0 à 24).
pub const COLORS: u8 = 25;
/// Fichier des couleurs de calendrier, dans le dossier de données.
pub const COLORS_FILE: &str = "calendar-colors.json";

fn e(x: thunderlook_store::rusqlite::Error) -> String {
    x.to_string()
}

/// Les quatre sortes de dossiers gérées.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Calendar,
    Contact,
    Task,
    Note,
}

impl Kind {
    /// Classe de conteneur Outlook.
    pub fn class(self) -> &'static str {
        match self {
            Kind::Calendar => "IPF.Appointment",
            Kind::Contact => "IPF.Contact",
            Kind::Task => "IPF.Task",
            Kind::Note => "IPF.StickyNote",
        }
    }

    fn like(self) -> String {
        format!("{}%", self.class())
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Calendar => "calendrier",
            Kind::Contact => "dossier de contacts",
            Kind::Task => "dossier de tâches",
            Kind::Note => "dossier de notes",
        }
    }
}

/// Dossier listé (identifiant `f<id>` pour tâches et notes, comme `thunderlook_items`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderInfo {
    pub id: i64,
    pub source_id: i64,
    /// Nom affiché : lisible, jamais technique, distinct des autres dossiers de la même sorte.
    pub name: String,
    /// Nom enregistré dans la base (celui que l'on renomme).
    pub raw_name: String,
    /// Fichier de données.
    pub section: String,
    /// Éléments directement dans le dossier.
    pub count: u32,
    /// Rang du fichier de données (0 à 7, stable : ordre des fichiers), pour une teinte propre à chaque compte.
    pub account: u8,
    /// Dossier technique vide d'un PST : masqué de [`list`].
    pub masked: bool,
}

/// Ce que devient le contenu d'un dossier supprimé.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "mode", content = "folder")]
pub enum Content {
    /// Suppression refusée si le dossier n'est pas vide.
    Refuse,
    /// Déplace les éléments vers ce dossier (même sorte, même fichier de données), puis supprime.
    MoveTo(i64),
    /// Supprime aussi les éléments (corbeille interne).
    Delete,
}

fn clean(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("Le nom ne peut pas être vide.".into());
    }
    if n.chars().count() > 255 {
        return Err("Le nom est trop long (255 caractères au plus).".into());
    }
    if n.chars().any(char::is_control) {
        return Err("Le nom contient un caractère interdit.".into());
    }
    Ok(n.to_string())
}

/// (source, parent, nom, classe) du dossier.
fn row(conn: &Connection, id: i64) -> Result<(i64, Option<i64>, String, Option<String>)> {
    conn.query_row("SELECT source_id, parent_id, name, container_class FROM folder WHERE id = ?1", [id], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    })
    .optional()
    .map_err(e)?
    .ok_or_else(|| "Dossier inconnu.".to_string())
}

fn kind_of(class: Option<&str>) -> Option<Kind> {
    let c = class?.to_ascii_lowercase();
    [Kind::Calendar, Kind::Contact, Kind::Task, Kind::Note].into_iter().find(|k| c.starts_with(&k.class().to_ascii_lowercase()))
}

/// Dossier existant de cette sorte (refuse les dossiers de courrier et ceux d'une autre sorte).
fn checked(conn: &Connection, id: i64, kind: Kind) -> Result<(i64, Option<i64>, String)> {
    let (source, parent, name, class) = row(conn, id)?;
    if kind_of(class.as_deref()) != Some(kind) {
        return Err(format!("Ce dossier n'est pas un {}.", kind.label()));
    }
    Ok((source, parent, name))
}

fn name_taken(conn: &Connection, source: i64, name: &str, except: Option<i64>) -> Result<bool> {
    let mut stmt = conn.prepare("SELECT id, name FROM folder WHERE source_id = ?1").map_err(e)?;
    let rows = stmt.query_map([source], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).map_err(e)?;
    let wanted = name.to_lowercase();
    for r in rows {
        let (id, other) = r.map_err(e)?;
        if Some(id) != except && other.trim().to_lowercase() == wanted {
            return Ok(true);
        }
    }
    Ok(false)
}

fn duplicate(name: &str) -> String {
    format!("Un dossier nommé « {name} » existe déjà dans ce fichier de données.")
}

/// Noms que le PST donne aux dossiers sans nom lisible, suivis d'un numéro (`Calendar3098`).
const TECHNICAL_STEMS: [&str; 14] =
    ["calendar", "calendrier", "contact", "contacts", "task", "tasks", "tache", "taches", "tâche", "tâches", "note", "notes", "appointment", "journal"];

/// Nom technique du PST : mot de dossier collé à un numéro (`Calendar3098`), classe de conteneur (`IPF.Appointment`)
/// ou identifiant entre accolades.
pub fn is_technical(name: &str) -> bool {
    let n = name.trim();
    if n.starts_with("IPF.") || (n.len() > 2 && n.starts_with('{') && n.ends_with('}')) {
        return true;
    }
    let digits = n.chars().rev().take_while(char::is_ascii_digit).count();
    let stem = n[..n.len() - digits].to_lowercase();
    digits >= 2 && TECHNICAL_STEMS.contains(&stem.as_str())
}

/// Nom générique d'une sorte, quand le nom du PST est technique.
fn generic(kind: Kind) -> &'static str {
    match kind {
        Kind::Calendar => "Calendrier",
        Kind::Contact => "Contacts",
        Kind::Task => "Tâches",
        Kind::Note => "Notes",
    }
}

/// Tous les dossiers de cette sorte, masqués compris, dans l'ordre d'affichage (fichier de données, parent, position).
pub fn all(conn: &Connection, kind: Kind) -> Result<Vec<FolderInfo>> {
    let ranks: Vec<i64> = {
        let mut stmt = conn.prepare("SELECT id FROM source ORDER BY id").map_err(e)?;
        stmt.query_map([], |r| r.get(0)).map_err(e)?.collect::<std::result::Result<_, _>>().map_err(e)?
    };
    let mut stmt = conn
        .prepare(
            "SELECT f.id, f.source_id, f.name, s.display_name, (SELECT COUNT(*) FROM message m WHERE m.folder_id = f.id),
                    s.kind, f.nid, (SELECT COUNT(*) FROM folder c WHERE c.parent_id = f.id)
             FROM folder f JOIN source s ON s.id = f.source_id
             WHERE f.container_class LIKE ?1 AND f.hidden = 0
             ORDER BY f.source_id, f.parent_id, f.position, f.id",
        )
        .map_err(e)?;
    let mut out: Vec<FolderInfo> = stmt
        .query_map([kind.like()], |r| {
            let (id, source_id, raw, section, count, origin, nid, children): (i64, i64, String, String, u32, String, i64, i64) =
                (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?);
            let technical = is_technical(&raw);
            let account = ranks.iter().position(|&s| s == source_id).unwrap_or(0) % 8;
            Ok(FolderInfo {
                id,
                source_id,
                name: if technical { generic(kind).to_string() } else { raw.trim().to_string() },
                raw_name: raw,
                section,
                count,
                account: account as u8,
                masked: technical && count == 0 && children == 0 && origin == "pst" && nid < FIRST_NID,
            })
        })
        .map_err(e)?
        .collect::<std::result::Result<_, _>>()
        .map_err(e)?;
    // Noms identiques (sans tenir compte de la casse) : fichier de données en suffixe, puis numéro dans un même fichier.
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, f) in out.iter().enumerate().filter(|(_, f)| !f.masked) {
        groups.entry(f.name.to_lowercase()).or_default().push(i);
    }
    for idx in groups.into_values().filter(|g| g.len() > 1) {
        let mut seen: BTreeMap<i64, u32> = BTreeMap::new();
        for i in idx {
            let n = seen.entry(out[i].source_id).or_insert(0);
            *n += 1;
            let f = &mut out[i];
            f.name = if *n > 1 { format!("{} ({}) {}", f.name, f.section, *n) } else { format!("{} ({})", f.name, f.section) };
        }
    }
    Ok(out)
}

/// Dossiers de cette sorte à afficher, y compris les vides (hors dossiers techniques vides masqués).
pub fn list(conn: &Connection, kind: Kind) -> Result<Vec<FolderInfo>> {
    Ok(all(conn, kind)?.into_iter().filter(|f| !f.masked).collect())
}

/// Crée un dossier ; `source` : fichier de données (celui du premier dossier de cette sorte si absent). Rend son `id`.
pub fn create(conn: &Connection, kind: Kind, source: Option<i64>, name: &str) -> Result<i64> {
    let name = clean(name)?;
    let sibling = list(conn, kind)?.into_iter().find(|f| source.is_none_or(|s| s == f.source_id));
    let source = match (source, &sibling) {
        (Some(s), _) => s,
        (None, Some(f)) => f.source_id,
        (None, None) => return Err("Aucun fichier de données pour ce dossier.".into()),
    };
    let known: Option<i64> = conn.query_row("SELECT id FROM source WHERE id = ?1", [source], |r| r.get(0)).optional().map_err(e)?;
    if known.is_none() {
        return Err("Fichier de données inconnu.".into());
    }
    if name_taken(conn, source, &name, None)? {
        return Err(duplicate(&name));
    }
    // Même parent que les dossiers de cette sorte dans ce fichier (en général la racine de l'arborescence).
    let parent: Option<i64> = match sibling.filter(|f| f.source_id == source) {
        Some(f) => row(conn, f.id)?.1,
        None => None,
    };
    let nid: i64 = conn
        .query_row("SELECT MAX(COALESCE(MAX(nid), 0), ?2) + 1 FROM folder WHERE source_id = ?1", params![source, FIRST_NID - 1], |r| r.get(0))
        .map_err(e)?;
    let position: i64 = conn
        .query_row("SELECT COALESCE(MAX(position), -1) + 1 FROM folder WHERE source_id = ?1 AND parent_id IS ?2", params![source, parent], |r| r.get(0))
        .map_err(e)?;
    conn.execute(
        "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position) VALUES (?1, ?2, ?3, ?4, 'folder', ?5, 0, ?6)",
        params![source, parent, nid, name, kind.class(), position],
    )
    .map_err(e)?;
    Ok(conn.last_insert_rowid())
}

/// Renomme un dossier (nom unique dans son fichier de données).
pub fn rename(conn: &Connection, kind: Kind, id: i64, name: &str) -> Result<()> {
    let name = clean(name)?;
    let (source, _, _) = checked(conn, id, kind)?;
    if name_taken(conn, source, &name, Some(id))? {
        return Err(duplicate(&name));
    }
    conn.execute("UPDATE folder SET name = ?2 WHERE id = ?1", params![id, name]).map_err(e)?;
    Ok(())
}

/// Supprime un dossier. Rend le nombre d'éléments déplacés ou supprimés avec lui.
pub fn delete(conn: &mut Connection, data_dir: &Path, kind: Kind, id: i64, content: Content) -> Result<u32> {
    let (source, _, name) = checked(conn, id, kind)?;
    let children: i64 = conn.query_row("SELECT COUNT(*) FROM folder WHERE parent_id = ?1", [id], |r| r.get(0)).map_err(e)?;
    if children > 0 {
        return Err(format!("« {name} » contient des sous-dossiers : supprimez-les d'abord."));
    }
    let ids: Vec<i64> = {
        let mut stmt = conn.prepare("SELECT id FROM message WHERE folder_id = ?1 ORDER BY id").map_err(e)?;
        stmt.query_map([id], |r| r.get(0)).map_err(e)?.collect::<std::result::Result<_, _>>().map_err(e)?
    };
    if !ids.is_empty() {
        match content {
            Content::Refuse => {
                return Err(format!("« {name} » contient {} élément(s) : déplacez-les ou supprimez-les avec le dossier.", ids.len()));
            }
            Content::MoveTo(to) => {
                if to == id {
                    return Err("Le dossier de destination est le dossier supprimé.".into());
                }
                let (dest_source, _, _) = checked(conn, to, kind)?;
                if dest_source != source {
                    return Err("La destination doit être dans le même fichier de données.".into());
                }
                for &m in &ids {
                    thunderlook_safety::snapshot_edit(conn, data_dir, m)?;
                }
                let tx = conn.transaction().map_err(e)?;
                for &m in &ids {
                    // Même NID déjà pris dans la destination : nouveau NID au-dessus de tous.
                    let nid: i64 = tx.query_row("SELECT nid FROM message WHERE id = ?1", [m], |r| r.get(0)).map_err(e)?;
                    let taken: bool =
                        tx.query_row("SELECT EXISTS(SELECT 1 FROM message WHERE folder_id = ?1 AND nid = ?2)", params![to, nid], |r| r.get(0)).map_err(e)?;
                    let nid = if taken {
                        tx.query_row("SELECT MAX(COALESCE(MAX(nid), 0), ?2) + 1 FROM message WHERE folder_id = ?1", params![to, FIRST_NID - 1], |r| r.get(0))
                            .map_err(e)?
                    } else {
                        nid
                    };
                    tx.execute("UPDATE message SET folder_id = ?2, nid = ?3 WHERE id = ?1", params![m, to, nid]).map_err(e)?;
                }
                tx.execute("DELETE FROM folder WHERE id = ?1", [id]).map_err(e)?;
                tx.commit().map_err(e)?;
            }
            Content::Delete => {
                // Tous les instantanés d'abord : si l'un échoue, rien n'est supprimé.
                for &m in &ids {
                    thunderlook_safety::snapshot(conn, data_dir, m)?;
                }
                let tx = conn.transaction().map_err(e)?;
                for &m in &ids {
                    tx.execute("DELETE FROM message_fts WHERE rowid = ?1", [m]).map_err(e)?;
                    tx.execute("DELETE FROM message WHERE id = ?1", [m]).map_err(e)?;
                }
                tx.execute("DELETE FROM folder WHERE id = ?1", [id]).map_err(e)?;
                tx.commit().map_err(e)?;
            }
        }
    } else {
        conn.execute("DELETE FROM folder WHERE id = ?1", [id]).map_err(e)?;
    }
    if kind == Kind::Calendar {
        let mut colors = colors(data_dir);
        if colors.remove(&format!("c{id}")).is_some() {
            let _ = write_colors(data_dir, &colors);
        }
    }
    Ok(ids.len() as u32)
}

/// Couleurs des calendriers (`c<dossier>` → 0 à 24). Fichier absent ou illisible : aucune couleur.
pub fn colors(data_dir: &Path) -> BTreeMap<String, u8> {
    fs::read_to_string(data_dir.join(COLORS_FILE))
        .ok()
        .and_then(|s| serde_json::from_str::<BTreeMap<String, u8>>(&s).ok())
        .map(|m| m.into_iter().filter(|(_, c)| *c < COLORS).collect())
        .unwrap_or_default()
}

fn write_colors(data_dir: &Path, colors: &BTreeMap<String, u8>) -> Result<()> {
    let path = data_dir.join(COLORS_FILE);
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(colors).map_err(|x| x.to_string())?;
    fs::write(&tmp, text).and_then(|()| fs::rename(&tmp, &path)).map_err(|x| format!("{} : {x}", path.display()))
}

/// Couleur d'un calendrier (0 à 24) ; `None` revient à la couleur automatique.
pub fn set_color(conn: &Connection, data_dir: &Path, id: i64, color: Option<u8>) -> Result<()> {
    checked(conn, id, Kind::Calendar)?;
    if color.is_some_and(|c| c >= COLORS) {
        return Err(format!("Couleur inconnue (0 à {}).", COLORS - 1));
    }
    let mut all = colors(data_dir);
    match color {
        Some(c) => all.insert(format!("c{id}"), c),
        None => all.remove(&format!("c{id}")),
    };
    write_colors(data_dir, &all)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Base fabriquée : un fichier de données avec un calendrier, un dossier de contacts, un de tâches et un de courrier.
    fn fixture(name: &str) -> (std::path::PathBuf, Connection) {
        let dir = std::env::temp_dir().join(format!("tl-folders-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let conn = thunderlook_store::open(&dir).unwrap();
        conn.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (1, 'pst', 'a.pst', 'a.pst', 'A', 0, 0)", []).unwrap();
        conn.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (2, 'pst', 'b.pst', 'b.pst', 'B', 0, 0)", []).unwrap();
        for (id, source, nid, name, class) in [
            (1, 1, 100, "Calendrier", "IPF.Appointment"),
            (2, 1, 101, "Contacts", "IPF.Contact"),
            (3, 1, 102, "Tâches", "IPF.Task"),
            (4, 1, 103, "Boîte de réception", "IPF.Note"),
            (5, 2, 100, "Calendrier", "IPF.Appointment"),
        ] {
            conn.execute(
                "INSERT INTO folder (id, source_id, nid, name, role, container_class, hidden, position) VALUES (?1, ?2, ?3, ?4, 'folder', ?5, 0, ?1)",
                params![id, source, nid, name, class],
            )
            .unwrap();
        }
        (dir, conn)
    }

    fn add_item(conn: &Connection, folder: i64, nid: i64, class: &str, subject: &str) -> i64 {
        conn.execute(
            "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, created_at, message_flags, is_read, flag_status,
                importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
             VALUES (?1, ?2, ?3, ?4, '', '', '', '', ?5, 0, 1, 0, 1, 0, 0, 0, 'text', '', '', x'0102')",
            params![folder, nid, class, subject, 1_700_000_000_000 + nid],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn count(conn: &Connection, folder: i64) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM message WHERE folder_id = ?1", [folder], |r| r.get(0)).unwrap()
    }

    #[test]
    fn create_gives_high_unique_nid_and_is_listed_empty() {
        let (dir, conn) = fixture("create");
        let a = create(&conn, Kind::Calendar, None, "  Chantiers ").unwrap();
        let b = create(&conn, Kind::Calendar, Some(1), "Famille").unwrap();
        let nid = |id: i64| conn.query_row("SELECT nid FROM folder WHERE id = ?1", [id], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(nid(a), FIRST_NID);
        assert_eq!(nid(b), FIRST_NID + 1);
        let names: Vec<String> = list(&conn, Kind::Calendar).unwrap().into_iter().filter(|f| f.source_id == 1).map(|f| f.raw_name).collect();
        assert_eq!(names, ["Calendrier", "Chantiers", "Famille"]);
        let c = create(&conn, Kind::Note, Some(1), "Idées").unwrap();
        assert_eq!(list(&conn, Kind::Note).unwrap()[0].id, c);
        // Nouveau NID au-dessus d'un NID déjà haut.
        conn.execute("UPDATE folder SET nid = ?2 WHERE id = ?1", params![a, FIRST_NID + 50]).unwrap();
        let d = create(&conn, Kind::Task, Some(1), "Perso").unwrap();
        assert_eq!(nid(d), FIRST_NID + 51);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn duplicate_name_is_refused_per_data_file_only() {
        let (dir, conn) = fixture("dup");
        assert!(create(&conn, Kind::Calendar, Some(1), "calendrier").unwrap_err().contains("existe déjà"));
        assert!(create(&conn, Kind::Contact, Some(1), "Boîte de réception").is_err());
        assert!(create(&conn, Kind::Calendar, Some(1), "   ").is_err());
        // Autre fichier de données : même nom permis.
        create(&conn, Kind::Contact, Some(2), "Contacts").unwrap();
        let k = create(&conn, Kind::Contact, Some(1), "Clients").unwrap();
        assert!(rename(&conn, Kind::Contact, k, "contacts").is_err());
        rename(&conn, Kind::Contact, k, "CLIENTS").unwrap();
        rename(&conn, Kind::Contact, k, "Fournisseurs").unwrap();
        assert!(rename(&conn, Kind::Task, k, "X").is_err(), "mauvaise sorte");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn delete_refused_while_not_empty() {
        let (dir, mut conn) = fixture("refuse");
        add_item(&conn, 1, 1, "IPM.Appointment", "Rdv");
        let err = delete(&mut conn, &dir, Kind::Calendar, 1, Content::Refuse).unwrap_err();
        assert!(err.contains("1 élément"), "{err}");
        assert_eq!(count(&conn, 1), 1);
        let empty = create(&conn, Kind::Calendar, Some(1), "Vide").unwrap();
        assert_eq!(delete(&mut conn, &dir, Kind::Calendar, empty, Content::Refuse).unwrap(), 0);
        assert!(list(&conn, Kind::Calendar).unwrap().iter().all(|f| f.id != empty));
        assert!(delete(&mut conn, &dir, Kind::Calendar, 4, Content::Refuse).is_err(), "dossier de courrier");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn delete_moves_content_to_another_folder() {
        let (dir, mut conn) = fixture("move");
        let src = create(&conn, Kind::Calendar, Some(1), "Ancien").unwrap();
        let a = add_item(&conn, src, 7, "IPM.Appointment", "A");
        let b = add_item(&conn, src, 8, "IPM.Appointment", "B");
        add_item(&conn, 1, 7, "IPM.Appointment", "Déjà là avec le même NID");
        assert!(delete(&mut conn, &dir, Kind::Calendar, src, Content::MoveTo(5)).is_err(), "autre fichier de données");
        assert!(delete(&mut conn, &dir, Kind::Calendar, src, Content::MoveTo(2)).is_err(), "autre sorte");
        assert_eq!(count(&conn, src), 2);
        assert_eq!(delete(&mut conn, &dir, Kind::Calendar, src, Content::MoveTo(1)).unwrap(), 2);
        assert_eq!(count(&conn, 1), 3);
        let nids: Vec<i64> = [a, b].iter().map(|&m| conn.query_row("SELECT nid FROM message WHERE id = ?1", [m], |r| r.get(0)).unwrap()).collect();
        assert_eq!(nids[1], 8);
        assert!(nids[0] >= FIRST_NID, "NID en collision remplacé");
        assert!(list(&conn, Kind::Calendar).unwrap().iter().all(|f| f.id != src));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn delete_with_content_keeps_trash_copy() {
        let (dir, mut conn) = fixture("del");
        let f = create(&conn, Kind::Task, Some(1), "Projet").unwrap();
        add_item(&conn, f, 1, "IPM.Task", "T1");
        add_item(&conn, f, 2, "IPM.Task", "T2");
        assert_eq!(delete(&mut conn, &dir, Kind::Task, f, Content::Delete).unwrap(), 2);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM message WHERE subject LIKE 'T_'", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        let trash = fs::read_dir(dir.join("trash")).unwrap().count();
        assert_eq!(trash, 2);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn subfolders_block_deletion() {
        let (dir, mut conn) = fixture("sub");
        conn.execute("INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position) VALUES (1, 1, 300, 'Sous', 'folder', 'IPF.Appointment', 0, 0)", []).unwrap();
        assert!(delete(&mut conn, &dir, Kind::Calendar, 1, Content::Delete).unwrap_err().contains("sous-dossiers"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn calendar_colors_roundtrip_and_cleanup() {
        let (dir, mut conn) = fixture("color");
        assert!(colors(&dir).is_empty());
        set_color(&conn, &dir, 1, Some(24)).unwrap();
        assert!(set_color(&conn, &dir, 1, Some(25)).is_err());
        assert!(set_color(&conn, &dir, 2, Some(3)).is_err(), "contacts : pas de couleur");
        assert_eq!(colors(&dir).get("c1"), Some(&24));
        let f = create(&conn, Kind::Calendar, Some(1), "Rouge").unwrap();
        set_color(&conn, &dir, f, Some(0)).unwrap();
        delete(&mut conn, &dir, Kind::Calendar, f, Content::Refuse).unwrap();
        assert_eq!(colors(&dir).len(), 1);
        set_color(&conn, &dir, 1, None).unwrap();
        assert!(colors(&dir).is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn technical_names_are_detected() {
        for n in ["Calendar3098", "calendar12", "Calendrier0042", " Tasks77 ", "Notes123", "IPF.Appointment", "{1A2B}"] {
            assert!(is_technical(n), "{n}");
        }
        for n in ["Calendrier", "Calendar 2", "Budget2024", "Calendar1", "Famille", "Contacts"] {
            assert!(!is_technical(n), "{n}");
        }
    }

    #[test]
    fn technical_and_duplicate_names_get_readable_labels() {
        let (dir, conn) = fixture("labels");
        // Fichier A : « Calendar3098 » rempli, « Calendar77 » vide ; fichier B : « Calendrier » existe déjà (id 5).
        for (id, source, nid, name) in [(6, 1, 200, "Calendar3098"), (7, 1, 201, "Calendar77"), (8, 2, 200, "Calendar4001")] {
            conn.execute(
                "INSERT INTO folder (id, source_id, nid, name, role, container_class, hidden, position) VALUES (?1, ?2, ?3, ?4, 'folder', 'IPF.Appointment', 0, ?1)",
                params![id, source, nid, name],
            )
            .unwrap();
        }
        add_item(&conn, 6, 1, "IPM.Appointment", "Rdv");
        add_item(&conn, 8, 1, "IPM.Appointment", "Rdv B");
        let shown = list(&conn, Kind::Calendar).unwrap();
        let names: Vec<(i64, &str)> = shown.iter().map(|f| (f.id, f.name.as_str())).collect();
        // Jamais de nom technique ; « Calendar77 » (vide, PST) masqué ; doublons suffixés par le fichier de données.
        assert_eq!(names, [(1, "Calendrier (A)"), (6, "Calendrier (A) 2"), (5, "Calendrier (B)"), (8, "Calendrier (B) 2")]);
        assert!(shown.iter().all(|f| !is_technical(&f.name)));
        assert_eq!(shown[1].raw_name, "Calendar3098");
        // Masqué mais intact dans la base et dans `all`.
        assert!(all(&conn, Kind::Calendar).unwrap().iter().any(|f| f.id == 7 && f.masked));
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM folder WHERE id = 7", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
        // Comptes distincts = teintes distinctes.
        assert_ne!(shown[0].account, shown[2].account);
        assert_eq!(shown[0].account, shown[1].account);
        // Ordre stable d'un appel à l'autre.
        assert_eq!(list(&conn, Kind::Calendar).unwrap(), shown);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn same_name_in_different_files_is_suffixed_only_when_duplicated() {
        let (dir, conn) = fixture("suffix");
        create(&conn, Kind::Contact, Some(1), "Clients").unwrap();
        create(&conn, Kind::Contact, Some(2), "Contacts").unwrap();
        let names: Vec<String> = list(&conn, Kind::Contact).unwrap().into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["Contacts (A)", "Clients", "Contacts (B)"]);
        // Renommer agit sur le nom enregistré, pas sur le libellé.
        let id = list(&conn, Kind::Contact).unwrap()[0].id;
        rename(&conn, Kind::Contact, id, "Perso").unwrap();
        let names: Vec<String> = list(&conn, Kind::Contact).unwrap().into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["Perso", "Clients", "Contacts"]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn folders_made_by_thunderlook_are_never_masked() {
        let (dir, conn) = fixture("nomask");
        let f = create(&conn, Kind::Calendar, Some(1), "Calendar9999").unwrap();
        assert!(list(&conn, Kind::Calendar).unwrap().iter().any(|x| x.id == f && x.name.starts_with("Calendrier")));
        let _ = fs::remove_dir_all(dir);
    }
}
