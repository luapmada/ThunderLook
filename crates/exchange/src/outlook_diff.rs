//! Écarts entre ThunderLook et Outlook (lecture seule). Trois états du même PST, retrouvés élément par élément par
//! leur NID : la **base** (copie du PST importée dans ThunderLook), **Outlook** (copie fraîche du même PST, importée
//! dans un dossier de données à part) et **ici** (la base de ThunderLook telle qu'elle est maintenant). Une différence
//! base → Outlook est un changement d'Outlook, base → ici un changement de ThunderLook (saisie, téléphone S20).
//! Les empreintes sont celles de la synchro avec le téléphone (`import::canon_*`) : mêmes faux écarts écartés.

use std::{collections::BTreeMap, path::Path};

use serde::Serialize;
use serde_json::Value;
use thunderlook_store::rusqlite::{Connection, params};

use crate::{Change, ChangeKind, ItemKind, Payload, Result, conflict, import};

/// Un état : connexion, dossier de données, source PST à lire.
pub struct Side<'a> {
    pub conn: &'a Connection,
    pub dir: &'a Path,
    pub source: i64,
}

/// Élément d'un état : (type, NID du dossier, NID de l'élément) → empreinte, titre, identifiant local.
#[derive(Clone, Debug)]
pub struct Entry {
    pub print: String,
    pub title: String,
    pub id: i64,
    pub folder: String,
    /// Forme comparée, pour nommer les champs qui diffèrent.
    pub value: Value,
}

/// (type, NID du dossier, NID de l'élément) ; le type est son nom (`ItemKind` n'est pas ordonné).
pub type Key = (&'static str, i64, i64);
pub type Snapshot = BTreeMap<Key, Entry>;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Counts {
    /// Identique partout.
    pub same: usize,
    /// Modifié dans Outlook seulement : à reporter ici.
    pub outlook_modified: usize,
    /// Nouveau dans Outlook : à créer ici.
    pub outlook_created: usize,
    /// Supprimé dans Outlook, inchangé ici : à supprimer ici.
    pub outlook_deleted: usize,
    /// Modifié des deux côtés, de la même façon.
    pub both_same: usize,
    /// Modifié des deux côtés différemment, ou supprimé d'un côté et modifié de l'autre.
    pub conflicts: usize,
    /// Modifié ici seulement (Outlook ne l'a pas).
    pub here_modified: usize,
    /// Supprimé ici, encore dans Outlook.
    pub here_deleted: usize,
    /// Créé ici (aucun NID du PST).
    pub here_created: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Line {
    pub kind: &'static str,
    pub what: &'static str,
    pub folder: String,
    pub title: String,
    pub nid: i64,
    /// Champs changés dans Outlook, puis ici, depuis la base.
    pub outlook: Vec<String>,
    pub here: Vec<String>,
    /// Valeurs avant → après dans Outlook, pour les champs sans contenu privé (rappel, dates, catégories…).
    pub detail: String,
}

/// Champs dont la valeur peut figurer dans le rapport (jamais les notes ni le texte).
const SHOWN: [&str; 7] = ["reminder", "start", "end", "allDay", "busy", "categories", "subject"];

fn detail(base: &Value, outlook: &Value, fields: &[String]) -> String {
    let v = |x: &Value, f: &str| x.get(f).map(Value::to_string).unwrap_or_default();
    fields.iter().filter(|f| SHOWN.contains(&f.as_str())).map(|f| format!("{f} : {} → {}", v(base, f), v(outlook, f))).collect::<Vec<_>>().join(" ; ")
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Diff {
    pub by_kind: BTreeMap<&'static str, Counts>,
    pub lines: Vec<Line>,
}

fn kind_name(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Contact => "contacts",
        ItemKind::Task => "tâches",
        ItemKind::Note => "notes",
        ItemKind::Event => "rendez-vous",
    }
}

/// Message local → (NID du dossier, nom du dossier, NID) pour la source donnée.
fn nids(conn: &Connection, source: i64) -> Result<BTreeMap<i64, (i64, String, i64)>> {
    let sql = |e: thunderlook_store::rusqlite::Error| e.to_string();
    let mut st = conn.prepare("SELECT m.id, f.nid, f.name, m.nid FROM message m JOIN folder f ON f.id = m.folder_id WHERE f.source_id = ?1").map_err(sql)?;
    let rows = st
        .query_map(params![source], |r| Ok((r.get::<_, i64>(0)?, (r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?))))
        .map_err(sql)?;
    rows.collect::<std::result::Result<_, _>>().map_err(sql)
}

/// Série : son VCALENDAR exporté, sans les horodatages de l'export ; les occurrences modifiées (VEVENT après le
/// premier) sont triées, leur ordre dans le PST n'ayant pas de sens.
fn series_print(a: &thunderlook_calendar::Appointment, dir: &Path) -> String {
    let text = thunderlook_calendar::ics::export(&[a], "x", &|a| thunderlook_calendar::description(a, dir));
    let kept: Vec<&str> = text.lines().filter(|l| !l.starts_with("DTSTAMP") && !l.starts_with("PRODID")).collect();
    let (mut events, mut other, mut inside): (Vec<Vec<&str>>, Vec<&str>, bool) = (vec![], vec![], false);
    for l in kept {
        if l == "BEGIN:VEVENT" {
            events.push(vec![]);
            inside = true;
        }
        match events.last_mut() {
            Some(e) if inside => e.push(l),
            _ => other.push(l),
        }
        if l == "END:VEVENT" {
            inside = false;
        }
    }
    if events.len() > 2 {
        events[1..].sort();
    }
    [other, events.concat()].concat().join("
")
}

fn to_value<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or_default()
}

/// Champs de premier niveau qui diffèrent (lignes, pour une série).
fn changed(a: &Value, b: &Value) -> Vec<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            keys.into_iter().filter(|k| x.get(*k) != y.get(*k)).cloned().collect()
        }
        (Value::String(x), Value::String(y)) if x != y => {
            let name = |l: &str| l.split([':', ';']).next().unwrap_or_default().to_string();
            let (xs, ys): (Vec<&str>, Vec<&str>) = (x.lines().collect(), y.lines().collect());
            let mut v: Vec<String> = xs.iter().filter(|l| !ys.contains(l)).chain(ys.iter().filter(|l| !xs.contains(l))).map(|l| name(l)).collect();
            v.sort();
            v.dedup();
            v
        }
        _ if a != b => vec!["(tout)".into()],
        _ => vec![],
    }
}

/// Premier NID que ThunderLook attribue aux éléments qu'il crée dans une source PST (au-delà de ceux du fichier).
const LOCAL_NID: i64 = 0x7000_0000;

/// Empreintes des contacts, tâches, notes et rendez-vous de la source `side.source` (les autres sources sont ignorées).
pub fn snapshot(side: &Side) -> Result<Snapshot> {
    let map = nids(side.conn, side.source)?;
    let mut out = Snapshot::new();
    let mut put = |kind: ItemKind, id: i64, value: Value, title: String| {
        if let Some((f, name, n)) = map.get(&id) {
            let print = conflict::fingerprint(&value);
            out.insert((kind_name(kind), *f, *n), Entry { print, title, id, folder: name.clone(), value });
        }
    };    let (conn, dir) = (side.conn, side.dir);
    for c in thunderlook_contacts::load(conn, None, None)?.contacts {
        let f = thunderlook_contacts::write::form_from(&c, dir);
        put(ItemKind::Contact, c.id, to_value(&import::canon_contact(&f)), c.title());
    }
    for r in thunderlook_items::tasks(conn)?.rows {
        let Some(n) = r.id.strip_prefix('t').and_then(|n| n.parse::<i64>().ok()) else { continue };
        let f = thunderlook_items::task_form(conn, dir, n)?;
        put(ItemKind::Task, n, to_value(&import::canon_task(&f)), f.subject.clone());
    }
    for r in thunderlook_items::notes(conn, dir)?.rows {
        let Some(n) = r.id.strip_prefix('n').and_then(|n| n.parse::<i64>().ok()) else { continue };
        let f = thunderlook_items::note_form(conn, dir, n)?;
        let title: String = f.body.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("(note vide)").chars().take(40).collect();
        put(ItemKind::Note, n, Value::String(import::canon_note(&f)), title);
    }
    for a in &thunderlook_calendar::load(conn)?.appointments {
        let value = if a.recurrence.is_some() { Value::String(series_print(a, dir)) } else { to_value(&import::canon_event(a, dir)) };
        let day = chrono::DateTime::from_timestamp_millis(a.start_ms).map(|t| t.format("%Y-%m-%d").to_string()).unwrap_or_default();
        let title = format!("{} ({day}{})", a.subject, if a.recurrence.is_some() { ", série" } else { "" });
        put(ItemKind::Event, a.id, value, title);
    }
    Ok(out)
}

/// Compare les trois états. `base` : par type, l'état importé dans ThunderLook (les calendriers peuvent venir d'une
/// copie plus récente que le reste).
pub fn compare<'a>(base: &dyn Fn(ItemKind) -> &'a Snapshot, outlook: &Snapshot, here: &Snapshot) -> Diff {
    let mut d = Diff::default();
    let note = |k: &'static str, what: &'static str, e: &Entry, nid: i64, fields: [Vec<String>; 2], detail: String, d: &mut Diff| {
        let c = d.by_kind.entry(k).or_default();
        let slot = match what {
            "identique" => &mut c.same,
            "modifié dans Outlook" => &mut c.outlook_modified,
            "créé dans Outlook" => &mut c.outlook_created,
            "supprimé dans Outlook" => &mut c.outlook_deleted,
            "modifié des deux côtés à l'identique" => &mut c.both_same,
            "modifié ici seulement" => &mut c.here_modified,
            "supprimé ici seulement" => &mut c.here_deleted,
            "créé ici" => &mut c.here_created,
            _ => &mut c.conflicts,
        };
        *slot += 1;
        if !matches!(what, "identique" | "modifié des deux côtés à l'identique") {
            let [outlook, here] = fields;
            d.lines.push(Line { kind: k, what, folder: e.folder.clone(), title: e.title.clone(), nid, outlook, here, detail });
        }
    };
    let kinds = [ItemKind::Contact, ItemKind::Task, ItemKind::Note, ItemKind::Event];
    for kind in kinds {
        let (b, k) = (base(kind), kind_name(kind));
        for (key, be) in b.iter().filter(|(key, _)| key.0 == k) {
            let (o, h) = (outlook.get(key), here.get(key));
            let nid = key.2;
            match (o, h) {
                (Some(o), Some(h)) => {
                    let (oc, hc) = (o.print != be.print, h.print != be.print);
                    let what = match (oc, hc) {
                        (false, false) => "identique",
                        (true, false) => "modifié dans Outlook",
                        (false, true) => "modifié ici seulement",
                        (true, true) if o.print == h.print => "modifié des deux côtés à l'identique",
                        (true, true) => "conflit : modifié des deux côtés",
                    };
                    let oc = changed(&be.value, &o.value);
                    let shown = detail(&be.value, &o.value, &oc);
                    note(k, what, h, nid, [oc, changed(&be.value, &h.value)], shown, &mut d);
                }
                (None, Some(h)) => {
                    let what = if h.print == be.print { "supprimé dans Outlook" } else { "conflit : supprimé dans Outlook, modifié ici" };
                    note(k, what, h, nid, [vec![], changed(&be.value, &h.value)], String::new(), &mut d);
                }
                (Some(o), None) => {
                    let what = if o.print == be.print { "supprimé ici seulement" } else { "conflit : supprimé ici, modifié dans Outlook" };
                    note(k, what, o, nid, [changed(&be.value, &o.value), vec![]], String::new(), &mut d);
                }
                (None, None) => {}
            }
        }
        for (key, o) in outlook.iter().filter(|(key, _)| key.0 == k && !b.contains_key(key)) {
            // Déjà reporté ici (même NID) : identique, ou conflit si le contenu diffère.
            let (what, fields) = match here.get(key) {
                None => ("créé dans Outlook", vec![]),
                Some(h) if h.print == o.print => ("modifié des deux côtés à l'identique", vec![]),
                Some(h) => ("conflit : créé des deux côtés", changed(&o.value, &h.value)),
            };
            note(k, what, o, key.2, [vec![], fields], String::new(), &mut d);
        }
        for (key, h) in here.iter().filter(|(key, _)| key.0 == k && key.2 >= LOCAL_NID && !b.contains_key(key)) {
            note(k, "créé ici", h, key.2, [vec![], vec![]], String::new(), &mut d);
        }
    }
    d
}

/// Reporte ici les changements faits dans Outlook seul : modifiés, créés, supprimés (avec le fichier d'annulation de
/// [`import::apply`]). Conflits, éléments modifiés ici et éléments créés dans la corbeille d'Outlook ne sont pas
/// touchés, sauf les éléments modifiés des deux côtés dont le NID est dans `outlook_wins` (conflit tranché pour Outlook).
/// Un élément créé reprend le NID d'Outlook, pour être retrouvé à la comparaison suivante.
#[allow(clippy::too_many_arguments)]
pub fn align<'a>(
    conn: &mut Connection,
    dir: &Path,
    source: i64,
    from: &Side,
    base: &dyn Fn(ItemKind) -> &'a Snapshot,
    outlook: &Snapshot,
    here: &Snapshot,
    outlook_wins: &[i64],
) -> Result<import::ApplyReport> {
    let sql = |e: thunderlook_store::rusqlite::Error| e.to_string();
    let folders: BTreeMap<i64, (i64, String)> = conn
        .prepare("SELECT nid, id, role FROM folder WHERE source_id = ?1")
        .map_err(sql)?
        .query_map(params![source], |r| Ok((r.get::<_, i64>(0)?, (r.get::<_, i64>(1)?, r.get::<_, Option<String>>(2)?.unwrap_or_default()))))
        .map_err(sql)?
        .collect::<std::result::Result<_, _>>()
        .map_err(sql)?;
    let mut changes = Vec::new();
    for kind in [ItemKind::Contact, ItemKind::Task, ItemKind::Note, ItemKind::Event] {
        let (b, k, p) = (base(kind), kind_name(kind), import::prefix(kind));
        let change = |key: &Key, what: ChangeKind, id: i64, title: &str, payload: Payload| Change {
            key: format!("{}:{}:{}", key.0, key.1, key.2),
            kind: what,
            item: kind,
            id: format!("{p}{id}"),
            summary: format!("{k} « {title} »"),
            ext_uid: None,
            payload,
            conflict: None,
            resolution: None,
        };
        // Contenu d'Outlook, rangé dans le dossier de même NID d'ici.
        let payload = |o: &Entry, folder: i64| -> Result<Payload> {
            let mut payload = import::snapshot(from.conn, from.dir, kind, &format!("{p}{}", o.id))?;
            match &mut payload {
                Payload::Contact(f) => f.folder_id = format!("k{folder}"),
                Payload::Task(f) => f.folder_id = format!("f{folder}"),
                Payload::Note(f) => f.folder_id = format!("f{folder}"),
                Payload::Event(e) => e.calendar_id = format!("c{folder}"),
                Payload::None => {}
            }
            Ok(payload)
        };
        for (key, be) in b.iter().filter(|(key, _)| key.0 == k) {
            match (outlook.get(key), here.get(key)) {
                (Some(o), Some(h)) if o.print != be.print && (h.print == be.print || outlook_wins.contains(&key.2)) => {
                    changes.push(change(key, ChangeKind::Modified, h.id, &h.title, payload(o, folders.get(&key.1).map_or(0, |f| f.0))?));
                }
                (None, Some(h)) if h.print == be.print => changes.push(change(key, ChangeKind::Deleted, h.id, &h.title, Payload::None)),
                _ => {}
            }
        }
        for (key, o) in outlook.iter().filter(|(key, _)| key.0 == k && !b.contains_key(key) && !here.contains_key(key)) {
            let Some((folder, role)) = folders.get(&key.1) else { continue };
            if role == "deleted" {
                continue;
            }
            changes.push(change(key, ChangeKind::New, o.id, &o.title, payload(o, *folder)?));
        }
    }
    let report = import::apply(conn, dir, &changes)?;
    for (key, local) in &report.created {
        let nid: Option<i64> = key.rsplit(':').next().and_then(|n| n.parse().ok());
        let Some(id) = local.get(1..).and_then(|n| n.parse::<i64>().ok()) else { continue };
        if let Some(nid) = nid {
            conn.execute("UPDATE message SET nid = ?1 WHERE id = ?2", params![nid, id]).map_err(sql)?;
        }
    }
    // La création prête au rendez-vous sans catégorie celle que nomme son objet (règle du téléphone), le remplacement
    // d'une série garde rappel et catégories de l'existant quand le fichier n'en porte pas : celui d'Outlook fait foi.
    for c in changes.iter().filter(|c| report.done.contains(&c.key)) {
        let Payload::Event(e) = &c.payload else { continue };
        let local = match c.kind {
            ChangeKind::New => report.created.iter().find(|(k, _)| k == &c.key).map(|(_, l)| l.clone()),
            ChangeKind::Modified if e.series.is_some() => Some(c.id.clone()),
            _ => None,
        };
        if let Some(id) = local.and_then(|l| l.get(1..).and_then(|n| n.parse::<i64>().ok())) {
            thunderlook_calendar::write::update(conn, dir, id, &import::edit_of(e))?;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(print: &str) -> Entry {
        Entry { print: print.into(), title: print.into(), id: 1, folder: "Contacts".into(), value: Value::String(print.into()) }
    }

    #[test]
    fn classe_les_ecarts_des_trois_etats() {
        let c = "contacts";
        let base: Snapshot = [((c, 1, 1), e("a")), ((c, 1, 2), e("b")), ((c, 1, 3), e("c")), ((c, 1, 4), e("d")), ((c, 1, 5), e("x"))].into();
        let outlook: Snapshot = [((c, 1, 1), e("a")), ((c, 1, 2), e("B")), ((c, 1, 3), e("C")), ((c, 1, 5), e("x")), ((c, 1, 9), e("new"))].into();
        let here: Snapshot = [((c, 1, 1), e("a")), ((c, 1, 2), e("b")), ((c, 1, 3), e("c2")), ((c, 1, 4), e("d")), ((c, 1, LOCAL_NID), e("ici"))].into();
        let d = compare(&|_| &base, &outlook, &here);
        let n = &d.by_kind["contacts"];
        assert_eq!(
            (n.same, n.outlook_modified, n.conflicts, n.outlook_deleted, n.here_deleted, n.outlook_created, n.here_created),
            (1, 1, 1, 1, 1, 1, 1),
            "{n:?}"
        );
    }
}
