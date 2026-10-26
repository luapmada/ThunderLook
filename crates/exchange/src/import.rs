//! Lecture des fichiers du téléphone : ne propose que ce qui a changé ([`import_changes`], sans rien écrire), applique
//! les changements acceptés ([`apply`]) en consignant de quoi les défaire ([`undo`]).
//!
//! Comparaison : un élément du fichier est comparé à l'élément de la base APRÈS un aller-retour par le même format
//! (`export` puis `parse`), de sorte que ce que le format ne porte pas (photo, périodicité d'une tâche, couleur d'une
//! note…) ne produit jamais de « modification ». Une modification n'écrase que les champs qui diffèrent.
//!
//! Identification d'un élément du fichier, dans l'ordre : son `UID` est l'identifiant local (`p12`) que nous avons
//! exporté ; sinon l'`UID` est dans `exchange-map.json` (identifiants propres à MPE, retenus à l'application) ; sinon
//! le nom + courriel / l'objet + échéance / le texte / l'objet + début.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use thunderlook_calendar::{Appointment, Busy, ics_series, tz::TimeZone};
use thunderlook_contacts::vcard;
use thunderlook_model::{contacts::ContactForm, items::{NoteForm, TaskForm}};
use thunderlook_store::rusqlite::Connection;

use crate::conflict::{self, Origin, Resolution, Verdict};
use crate::{Change, ChangeKind, EventData, IdMap, ImportOptions, ImportReport, ItemKind, Manifest, Payload, Result, Seen, event_uid, read_manifest, sha256, undo_dir};

/// Champs jamais comparés ni écrasés : l'identité de l'élément et sa photo.
const SKIP: [&str; 3] = ["id", "folderId", "photo"];

fn text_of(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap_or_else(|_| bytes.iter().map(|&b| b as char).collect())
}

fn json<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// Noms (camelCase) des champs qui diffèrent entre deux formulaires.
fn diff_fields<T: Serialize>(canon: &T, phone: &T) -> Vec<String> {
    let (a, b) = (json(canon), json(phone));
    let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else { return Vec::new() };
    let mut out: Vec<String> = b.iter().filter(|(k, v)| !SKIP.contains(&k.as_str()) && a.get(*k) != Some(*v)).map(|(k, _)| k.clone()).collect();
    out.sort();
    out
}

/// `existing` dont seuls les `fields` prennent la valeur de `phone`.
fn overlay<T: Serialize + DeserializeOwned + Clone>(existing: &T, phone: &T, fields: &[String]) -> T {
    let (mut base, from) = (json(existing), json(phone));
    if let (Some(base), Some(from)) = (base.as_object_mut(), from.as_object()) {
        for f in fields {
            if let Some(v) = from.get(f) {
                base.insert(f.clone(), v.clone());
            }
        }
    }
    serde_json::from_value(base).unwrap_or_else(|_| existing.clone())
}

fn lower(s: &str) -> String {
    s.trim().to_lowercase()
}

// --- Correspondance -------------------------------------------------------------------------------------------------

/// Un élément de la base, vu par la correspondance.
struct Ex {
    id: String,
    /// Identifiant écrit dans le fichier à l'export.
    uid: String,
    key: String,
    title: String,
}

struct Matcher {
    exs: Vec<Ex>,
    ids: HashSet<String>,
    by_uid: HashMap<String, usize>,
    by_key: HashMap<String, Vec<usize>>,
    map: HashMap<String, String>,
    used: HashSet<usize>,
}

impl Matcher {
    fn new(exs: Vec<Ex>, map: HashMap<String, String>) -> Self {
        let ids = exs.iter().map(|e| e.id.clone()).collect();
        let by_uid = exs.iter().enumerate().map(|(i, e)| (e.uid.clone(), i)).collect();
        let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, e) in exs.iter().enumerate() {
            if !e.key.is_empty() {
                by_key.entry(e.key.clone()).or_default().push(i);
            }
        }
        Matcher { exs, ids, by_uid, by_key, map, used: HashSet::new() }
    }

    fn take(&mut self, i: usize) -> Option<usize> {
        self.used.insert(i).then_some(i)
    }

    fn find(&mut self, uid: Option<&str>, key: &str) -> Option<usize> {
        if let Some(u) = uid.filter(|u| !u.is_empty()) {
            if let Some(&i) = self.by_uid.get(u)
                && let Some(i) = self.take(i)
            {
                return Some(i);
            }
            let local = self.map.get(u).filter(|l| self.ids.contains(*l)).cloned();
            if let Some(i) = local.and_then(|l| self.exs.iter().position(|e| e.id == l))
                && let Some(i) = self.take(i)
            {
                return Some(i);
            }
        }
        let candidates = if key.is_empty() { Vec::new() } else { self.by_key.get(key).cloned().unwrap_or_default() };
        candidates.into_iter().find_map(|i| self.take(i))
    }
}

/// Ce que `import_changes` sait du fichier en cours.
struct Ctx<'a> {
    kind: ItemKind,
    /// Identifiants exportés (manifeste, ou base de comparaison en mode « fichiers seuls ») ; `None` : aucune suppression
    /// n'est proposée.
    exported: Option<Vec<String>>,
    /// Identifiant du fichier → identifiant local, tels qu'exportés (manifeste).
    uids: Option<&'a HashMap<String, String>>,
    /// Identifiant local → empreinte de la valeur exportée (manifeste) ; vide pour un ancien manifeste.
    values: Option<&'a HashMap<String, String>>,
    map: HashMap<String, String>,
    /// Mode « fichiers seuls » (pas de manifeste) : `UID` du fichier → empreinte retenue au dernier règlement (voir `phone`).
    base: Option<crate::phone::Prints>,
}

impl Ctx<'_> {
    /// Empreinte d'une valeur lue, calculée seulement en mode « fichiers seuls ».
    fn fp<T: Serialize>(&self, v: &T) -> String {
        if self.base.is_some() { conflict::fingerprint(v) } else { String::new() }
    }

    /// L'élément du fichier a été exporté, ou déjà importé (identifiant retenu), et n'est plus dans la base : supprimé ici
    /// depuis, pas « nouveau ». Le téléphone qui le garde ne le ressuscite pas.
    fn deleted_here(&self, uid: Option<&str>, m: &Matcher) -> bool {
        uid.is_some_and(|u| {
            let by_manifest = self.uids.and_then(|x| x.get(u)).cloned().or_else(|| self.exported.as_deref().filter(|e| e.iter().any(|x| x == u)).map(|_| u.to_string()));
            by_manifest.or_else(|| self.map.get(u).cloned()).is_some_and(|local| !m.ids.contains(&local))
        })
    }

    fn change(&self, kind: ChangeKind, id: &str, summary: String, ext_uid: Option<&str>, payload: Payload) -> Change {
        let k = match kind {
            ChangeKind::New => "new",
            ChangeKind::Modified => "modified",
            ChangeKind::Deleted => "deleted",
        };
        Change { key: format!("{}:{k}:{id}", self.kind.name()), kind, item: self.kind, id: id.to_string(), summary, ext_uid: ext_uid.filter(|u| !u.is_empty()).map(str::to_string), payload, conflict: None, resolution: None }
    }

    /// Note l'élément lu (mode « fichiers seuls » seulement) : de quoi régler la base une fois les changements appliqués.
    fn see(&self, rep: &mut ImportReport, uid: Option<&str>, local: Option<&str>, fp: String, key: Option<String>) {
        if self.base.is_none() {
            return;
        }
        if let Some(uid) = uid.filter(|u| !u.is_empty()) {
            rep.seen.push(Seen { item: self.kind, uid: uid.to_string(), local: local.map(str::to_string), fp, key });
        }
    }

    /// Classe une modification venue du téléphone (le fichier diffère de la base) : modification à appliquer, élément
    /// intact côté téléphone (ThunderLook fait foi, rien à proposer) ou conflit (proposé à part).
    fn propose(&self, rep: &mut ImportReport, id: &str, summary: String, uid: Option<&str>, payload: Payload, canon: &impl Serialize, phone: &impl Serialize) {
        let phone_fp = conflict::fingerprint(phone);
        let here_fp = conflict::fingerprint(canon);
        let verdict = match &self.base {
            // Base de ThunderLook : le fichier a encore la valeur lue / l'élément local a encore celle d'après le règlement.
            Some(base) => match uid.and_then(|u| base.get(u)) {
                Some(p) if p.phone == phone_fp => Verdict::Intact,
                Some(p) if p.here == here_fp => Verdict::PhoneEdit,
                Some(_) => Verdict::Conflict(Origin::Both),
                None => Verdict::Conflict(Origin::Unknown),
            },
            None => conflict::classify(self.values.and_then(|v| v.get(id)).map(String::as_str), &phone_fp, &here_fp),
        };
        match verdict {
            Verdict::Intact => {
                rep.unchanged += 1;
                self.see(rep, uid, Some(id), phone_fp, None);
            }
            Verdict::PhoneEdit => {
                let c = self.change(ChangeKind::Modified, id, summary, uid, payload);
                self.see(rep, uid, Some(id), phone_fp, Some(c.key.clone()));
                rep.changes.push(c);
            }
            Verdict::Conflict(origin) => {
                let mut c = self.change(ChangeKind::Modified, id, summary, uid, payload);
                c.key = format!("{}:conflict:{id}", self.kind.name());
                c.conflict = Some(origin);
                self.see(rep, uid, Some(id), phone_fp, Some(c.key.clone()));
                rep.conflicts.push(c);
            }
        }
    }

    /// Un nouvel élément du fichier : `Change` à créer, noté pour la base.
    fn created(&self, rep: &mut ImportReport, id: &str, summary: String, uid: Option<&str>, payload: Payload, fp: String) {
        let c = self.change(ChangeKind::New, id, summary, uid, payload);
        self.see(rep, uid, None, fp, Some(c.key.clone()));
        rep.changes.push(c);
    }

    /// Une modification du téléphone qui n'a pas à passer par le classement (série, note forcée par le téléphone).
    fn modified(&self, rep: &mut ImportReport, id: &str, summary: String, uid: Option<&str>, payload: Payload, fp: String) {
        let c = self.change(ChangeKind::Modified, id, summary, uid, payload);
        self.see(rep, uid, Some(id), fp, Some(c.key.clone()));
        rep.changes.push(c);
    }

    /// Suppressions proposées : éléments exportés, toujours dans la base, absents du fichier.
    fn deletions(&self, m: &Matcher, rep: &mut ImportReport) {
        let Some(exported) = &self.exported else { return };
        for id in exported {
            if let Some(i) = m.exs.iter().position(|e| &e.id == id).filter(|i| !m.used.contains(i)) {
                rep.changes.push(self.change(ChangeKind::Deleted, id, m.exs[i].title.clone(), None, Payload::None));
            }
        }
    }
}

// --- Lecture --------------------------------------------------------------------------------------------------------

/// Changements entre les fichiers de `in_dir` et la base. N'écrit rien.
pub fn import_changes(conn: &Connection, data_dir: &Path, in_dir: &Path) -> Result<ImportReport> {
    import_changes_with(conn, data_dir, in_dir, &ImportOptions::default())
}

/// Comme [`import_changes`], avec des réglages (notes « venues du téléphone » ou bidirectionnelles).
pub fn import_changes_with(conn: &Connection, data_dir: &Path, in_dir: &Path, opts: &ImportOptions) -> Result<ImportReport> {
    let manifest: Option<Manifest> = read_manifest(in_dir);
    let maps: IdMap = crate::read_map(data_dir);
    let mut rep = ImportReport::default();
    let mut found = false;
    for kind in ItemKind::all() {
        let path = in_dir.join(kind.file());
        if !path.is_file() {
            continue;
        }
        found = true;
        let bytes = fs::read(&path).map_err(|e| format!("{} : {e}", path.display()))?;
        let info = manifest.as_ref().and_then(|m| m.file(kind.file()));
        // Notes forcées par le téléphone : le fichier intact est quand même relu, une note modifiée ici doit être ramenée à lui.
        let forced = kind == ItemKind::Note && opts.notes_from_phone;
        if !forced && info.is_some_and(|i| i.sha256 == sha256::hex(&bytes)) {
            // Fichier intact depuis l'export : rien n'a pu changer côté téléphone.
            rep.unchanged += info.map_or(0, |i| i.count);
            continue;
        }
        let map: HashMap<String, String> = maps.get(kind.name()).cloned().unwrap_or_default();
        // Sans manifeste (dossier écrit par MPE) : la base de comparaison de ThunderLook (`phone`) en tient lieu.
        let base = manifest.is_none().then(|| crate::phone::prints_of(data_dir, kind));
        let exported = match (&base, info) {
            (Some(b), _) => Some(b.keys().filter_map(|uid| map.get(uid).cloned()).collect()),
            (None, i) => i.map(|i| i.ids.clone()),
        };
        let ctx = Ctx { kind, exported, uids: info.map(|i| &i.uids), values: info.map(|i| &i.values), map, base };
        if ctx.base.is_some() {
            rep.read.push(kind);
        }
        let text = text_of(&bytes);
        match kind {
            ItemKind::Contact => contacts(conn, data_dir, &text, &ctx, &mut rep)?,
            ItemKind::Task => tasks(conn, data_dir, &text, &ctx, &mut rep)?,
            ItemKind::Note => notes(conn, data_dir, &text, &ctx, forced, &mut rep)?,
            ItemKind::Event => events(conn, data_dir, &text, &ctx, &mut rep)?,
        }
    }
    if !found {
        return Err(format!("Aucun fichier d'échange dans {}.", in_dir.display()));
    }
    Ok(rep)
}

fn warn(rep: &mut ImportReport, file: &str, ws: &[String]) {
    rep.warnings.extend(ws.iter().map(|w| format!("{file} : {w}")));
}

fn contact_title(f: &ContactForm) -> String {
    let name = [f.given_name.trim(), f.surname.trim()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ");
    [name, f.company.trim().to_string(), f.email1.trim().to_string()].into_iter().find(|s| !s.is_empty()).unwrap_or_else(|| "(sans nom)".into())
}

fn contact_key(f: &ContactForm) -> String {
    let k = format!("{}|{}|{}|{}", lower(&f.given_name), lower(&f.surname), lower(&f.company), lower(&f.email1));
    if k == "|||" { String::new() } else { k }
}

fn norm_contact(mut f: ContactForm) -> ContactForm {
    f.id = None;
    f.folder_id.clear();
    f.photo = None;
    f
}

pub(crate) fn canon_contact(f: &ContactForm) -> ContactForm {
    vcard::parse(&vcard::export(std::slice::from_ref(f), vcard::Version::V30)).ok().and_then(|mut v| v.pop()).map_or_else(|| norm_contact(f.clone()), |p| norm_contact(p.form))
}

fn contacts(conn: &Connection, dir: &Path, text: &str, ctx: &Ctx, rep: &mut ImportReport) -> Result<()> {
    let all = thunderlook_contacts::load(conn, None, None)?;
    let (mut exs, mut data) = (Vec::new(), HashMap::new());
    for c in &all.contacts {
        let f = thunderlook_contacts::write::form_from(c, dir);
        let id = f.id.clone().unwrap_or_else(|| format!("p{}", c.id));
        exs.push(Ex { id: id.clone(), uid: id.clone(), key: contact_key(&f), title: c.title() });
        data.insert(id, (canon_contact(&f), f));
    }
    let mut m = Matcher::new(exs, ctx.map.clone());
    for (n, p) in vcard::parse(text)?.into_iter().enumerate() {
        warn(rep, crate::CONTACTS_FILE, &p.warnings);
        if p.form == ContactForm::default() {
            continue;
        }
        let phone = norm_contact(p.form);
        if ctx.deleted_here(p.uid.as_deref(), &m) {
            continue;
        }
        match m.find(p.uid.as_deref(), &contact_key(&phone)) {
            Some(i) => {
                let id = m.exs[i].id.clone();
                let (canon, existing) = &data[&id];
                let fields = diff_fields(canon, &phone);
                if fields.is_empty() {
                    rep.unchanged += 1;
                    ctx.see(rep, p.uid.as_deref(), Some(&id), ctx.fp(&phone), None);
                } else {
                    let merged = overlay(existing, &phone, &fields);
                    let summary = format!("{} : {}", m.exs[i].title, fields.join(", "));
                    ctx.propose(rep, &id, summary, p.uid.as_deref(), Payload::Contact(merged), canon, &phone);
                }
            }
            None => {
                let (fp, title) = (ctx.fp(&phone), contact_title(&phone));
                ctx.created(rep, &format!("#{n}"), title, p.uid.as_deref(), Payload::Contact(phone), fp);
            }
        }
    }
    ctx.deletions(&m, rep);
    Ok(())
}

fn task_key(f: &TaskForm) -> String {
    format!("{}|{}", lower(&f.subject), f.due.trim())
}

fn norm_task(mut f: TaskForm) -> TaskForm {
    f.id = None;
    f.folder_id.clear();
    f
}

pub(crate) fn canon_task(f: &TaskForm) -> TaskForm {
    thunderlook_items::vtodo::parse(&thunderlook_items::vtodo::export(std::slice::from_ref(f))).ok().and_then(|mut v| v.pop()).map_or_else(|| norm_task(f.clone()), |p| norm_task(p.form))
}

fn tasks(conn: &Connection, dir: &Path, text: &str, ctx: &Ctx, rep: &mut ImportReport) -> Result<()> {
    let (mut exs, mut data) = (Vec::new(), HashMap::new());
    for r in thunderlook_items::tasks(conn)?.rows {
        let Some(n) = r.id.strip_prefix('t').and_then(|n| n.parse::<i64>().ok()) else { continue };
        let f = thunderlook_items::task_form(conn, dir, n)?;
        exs.push(Ex { id: r.id.clone(), uid: r.id.clone(), key: task_key(&f), title: f.subject.clone() });
        data.insert(r.id.clone(), (canon_task(&f), f));
    }
    let mut m = Matcher::new(exs, ctx.map.clone());
    for (n, p) in thunderlook_items::vtodo::parse(text)?.into_iter().enumerate() {
        warn(rep, crate::TASKS_FILE, &p.warnings);
        if p.form == TaskForm::default() {
            continue;
        }
        let phone = norm_task(p.form);
        if ctx.deleted_here(p.uid.as_deref(), &m) {
            continue;
        }
        match m.find(p.uid.as_deref(), &task_key(&phone)) {
            Some(i) => {
                let id = m.exs[i].id.clone();
                let (canon, existing) = &data[&id];
                let fields = diff_fields(canon, &phone);
                if fields.is_empty() {
                    rep.unchanged += 1;
                    ctx.see(rep, p.uid.as_deref(), Some(&id), ctx.fp(&phone), None);
                } else {
                    let summary = format!("{} : {}", m.exs[i].title, fields.join(", "));
                    ctx.propose(rep, &id, summary, p.uid.as_deref(), Payload::Task(overlay(existing, &phone, &fields)), canon, &phone);
                }
            }
            None => {
                let (fp, subject) = (ctx.fp(&phone), phone.subject.clone());
                ctx.created(rep, &format!("#{n}"), subject, p.uid.as_deref(), Payload::Task(phone), fp);
            }
        }
    }
    ctx.deletions(&m, rep);
    Ok(())
}

fn body_of(s: &str) -> String {
    s.replace("\r\n", "\n").trim_end().to_string()
}

fn note_title(body: &str) -> String {
    body.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("(note vide)").chars().take(80).collect()
}

pub(crate) fn canon_note(f: &NoteForm) -> String {
    thunderlook_items::vnote::parse(&thunderlook_items::vnote::export(std::slice::from_ref(f))).ok().and_then(|mut v| v.pop()).map_or_else(|| body_of(&f.body), |p| body_of(&p.form.body))
}

fn notes(conn: &Connection, dir: &Path, text: &str, ctx: &Ctx, forced: bool, rep: &mut ImportReport) -> Result<()> {
    let (mut exs, mut data) = (Vec::new(), HashMap::new());
    for r in thunderlook_items::notes(conn, dir)?.rows {
        let Some(n) = r.id.strip_prefix('n').and_then(|n| n.parse::<i64>().ok()) else { continue };
        let f = thunderlook_items::note_form(conn, dir, n)?;
        exs.push(Ex { id: r.id.clone(), uid: r.id.clone(), key: body_of(&f.body), title: note_title(&f.body) });
        data.insert(r.id.clone(), (canon_note(&f), f));
    }
    let mut m = Matcher::new(exs, ctx.map.clone());
    for (n, p) in thunderlook_items::vnote::parse(text)?.into_iter().enumerate() {
        warn(rep, crate::NOTES_FILE, &p.warnings);
        if p.form == NoteForm::default() {
            continue;
        }
        let (uid, body) = (p.meta.uid.clone(), body_of(&p.form.body));
        if ctx.deleted_here(uid.as_deref(), &m) {
            continue;
        }
        match m.find(uid.as_deref(), &body) {
            Some(i) => {
                let id = m.exs[i].id.clone();
                let (canon, existing) = &data[&id];
                if *canon == body {
                    rep.unchanged += 1;
                    ctx.see(rep, uid.as_deref(), Some(&id), ctx.fp(&body), None);
                } else {
                    let merged = NoteForm { body: body.clone(), ..existing.clone() };
                    let summary = format!("{} : texte", note_title(&body));
                    if forced {
                        // Le téléphone fait foi : remplacement coché d'office, la couleur existante est gardée (`..existing`).
                        ctx.modified(rep, &id, summary, uid.as_deref(), Payload::Note(merged), ctx.fp(&body));
                    } else {
                        ctx.propose(rep, &id, summary, uid.as_deref(), Payload::Note(merged), canon, &body);
                    }
                }
            }
            None => {
                let form = NoteForm { id: None, folder_id: String::new(), body: body.clone(), color: p.form.color };
                ctx.created(rep, &format!("#{n}"), note_title(&body), uid.as_deref(), Payload::Note(form), ctx.fp(&body));
            }
        }
    }
    ctx.deletions(&m, rep);
    Ok(())
}

fn paris(ms: i64) -> String {
    let utc = chrono::DateTime::from_timestamp_millis(ms).map(|t| t.naive_utc()).unwrap_or_default();
    TimeZone::PARIS.to_local(utc).format("%Y-%m-%dT%H:%M").to_string()
}

fn event_of(a: &Appointment, body: Option<String>) -> EventData {
    EventData {
        id: Some(format!("a{}", a.id)),
        calendar_id: format!("c{}", a.folder_id),
        subject: a.subject.clone(),
        location: a.location.clone().unwrap_or_default(),
        start: paris(a.start_ms),
        end: paris(a.end_ms),
        all_day: a.all_day,
        reminder: a.reminder_minutes.map(|m| m as i32),
        busy: match a.busy {
            Busy::Free => 0,
            Busy::Tentative => 1,
            Busy::Busy => 2,
            Busy::OutOfOffice => 3,
            Busy::WorkingElsewhere => 4,
        },
        body: body.map(|b| b.trim_end().to_string()).unwrap_or_default(),
        categories: a.categories.join(", "),
        private: a.private,
        series: None,
    }
}

fn norm_event(mut e: EventData) -> EventData {
    e.id = None;
    e.calendar_id.clear();
    e
}

fn event_key(e: &EventData) -> String {
    format!("{}|{}", lower(&e.subject), e.start)
}

pub(crate) fn canon_event(a: &Appointment, dir: &Path) -> EventData {
    let own = norm_event(event_of(a, thunderlook_calendar::description(a, dir)));
    let text = thunderlook_calendar::ics::export(&[a], "x", &|a| thunderlook_calendar::description(a, dir));
    match thunderlook_calendar::ics::parse(&text, 0, 0) {
        Ok((_, mut v)) if v.len() == 1 => {
            let b = v.remove(0);
            let body = b.description.clone();
            norm_event(event_of(&b, body))
        }
        _ => own,
    }
}

/// Rendez-vous périodique, de `ics` (son VCALENDAR) : début et fin de la première occurrence.
fn series_event(a: &Appointment, ics: String) -> EventData {
    EventData { series: Some(ics), ..norm_event(event_of(a, a.description.clone())) }
}

fn events(conn: &Connection, dir: &Path, text: &str, ctx: &Ctx, rep: &mut ImportReport) -> Result<()> {
    let cal = thunderlook_calendar::load(conn)?;
    let describe = |a: &Appointment| thunderlook_calendar::description(a, dir);
    let (mut exs, mut data) = (Vec::new(), HashMap::new());
    // Séries de la base : leur forme relue après un export (référence de la comparaison), et pourquoi elle n'est pas
    // relisible le cas échéant.
    let mut series_base: HashMap<String, (Appointment, Option<String>)> = HashMap::new();
    for a in &cal.appointments {
        let e = event_of(a, describe(a));
        let id = format!("a{}", a.id);
        exs.push(Ex { id: id.clone(), uid: event_uid(a), key: event_key(&e), title: a.subject.clone() });
        if a.recurrence.is_some() {
            series_base.insert(id, match ics_series::canonical(a, &describe) {
                Ok(b) => (b, None),
                Err((b, why)) => (b, Some(why)),
            });
        } else {
            data.insert(id, (canon_event(a, dir), e));
        }
    }
    let mut m = Matcher::new(exs, ctx.map.clone());
    // vCalendar 1.0 (MPE) : converti en iCalendar 2.0 ; les refus et pertes sont des avertissements.
    let converted;
    let text = if crate::vcal10::is_vcal10(text) {
        let c = crate::vcal10::to_ical(text);
        rep.warnings.extend(c.warnings.iter().map(|w| format!("{} : vCalendar 1.0, {w}", crate::CALENDAR_FILE)));
        converted = c.ics;
        converted.as_str()
    } else {
        text
    };
    let split = ics_series::split(text);
    let (_, phone) = thunderlook_calendar::ics::parse(&split.rest, 0, 0)?;
    for (n, a) in phone.iter().enumerate() {
        if ctx.deleted_here(a.uid.as_deref(), &m) {
            continue;
        }
        let e = norm_event(event_of(a, a.description.clone()));
        match m.find(a.uid.as_deref(), &event_key(&e)) {
            Some(i) if series_base.contains_key(&m.exs[i].id) => rep.warnings.push(format!(
                "{} : « {} » est une série dans ThunderLook mais un rendez-vous simple dans le fichier : non importé (la conversion d'une série en rendez-vous simple n'est pas prise en charge).",
                crate::CALENDAR_FILE, m.exs[i].title
            )),
            Some(i) => {
                let id = m.exs[i].id.clone();
                let (canon, existing) = &data[&id];
                let fields = diff_fields(canon, &e);
                if fields.is_empty() {
                    rep.unchanged += 1;
                    ctx.see(rep, a.uid.as_deref(), Some(&id), ctx.fp(&e), None);
                } else {
                    let summary = format!("{} : {}", m.exs[i].title, fields.join(", "));
                    ctx.propose(rep, &id, summary, a.uid.as_deref(), Payload::Event(overlay(existing, &e, &fields)), canon, &e);
                }
            }
            None => {
                let (fp, summary) = (ctx.fp(&e), format!("{} ({})", e.subject, e.start.replace('T', " ")));
                ctx.created(rep, &format!("#{n}"), summary, a.uid.as_deref(), Payload::Event(e), fp);
            }
        }
    }
    // Séries refusées : jamais importées de travers, toujours signalées (sauf celles que ThunderLook ne sait pas non plus
    // relire dans son propre export, pour la même raison : rien n'a changé). Retrouvées dans la base, elles ne sont pas
    // proposées à la suppression.
    for r in &split.rejected {
        let hit = m.find(Some(&r.uid), "").map(|i| m.exs[i].id.clone());
        if hit.and_then(|id| series_base.get(&id).and_then(|(_, why)| why.clone())).is_some_and(|why| why == r.reason) {
            rep.unchanged += 1;
        } else {
            rep.warnings.push(format!("{} : série « {} » non importée : {}.", crate::CALENDAR_FILE, r.subject, r.reason));
        }
    }
    for (n, s) in split.series.iter().enumerate() {
        let uid = Some(s.uid.as_str());
        if ctx.deleted_here(uid, &m) {
            continue;
        }
        let mut phone = s.appointment.clone();
        let subject = phone.subject.clone();
        let refuse = |rep: &mut ImportReport, why: &str| rep.warnings.push(format!("{} : série « {subject} » non importée : {why}.", crate::CALENDAR_FILE));
        match m.find(uid, &event_key(&event_of(&phone, None))) {
            Some(i) => {
                let id = m.exs[i].id.clone();
                let base = match series_base.get(&id) {
                    Some((b, _)) => b.clone(),
                    None => {
                        let a = cal.appointments.iter().find(|a| format!("a{}", a.id) == id).ok_or("rendez-vous introuvable")?;
                        Appointment { description: describe(a), ..a.clone() }
                    }
                };
                // Catégories et rappel que le téléphone n'envoie pas : ceux de ThunderLook.
                ics_series::keep_from(&base, &mut phone);
                let fields = ics_series::changed(&base, &phone);
                if fields.is_empty() {
                    rep.unchanged += 1;
                    ctx.see(rep, uid, Some(&id), ctx.fp(&s.ics), None);
                } else if let Some(why) = &s.inexact {
                    refuse(rep, why);
                } else {
                    let summary = format!("{} : série ({})", m.exs[i].title, fields.join(", "));
                    ctx.modified(rep, &id, summary, uid, Payload::Event(series_event(&phone, s.ics.clone())), ctx.fp(&s.ics));
                }
            }
            None => match &s.inexact {
                Some(why) => refuse(rep, why),
                None => {
                    let e = series_event(&phone, s.ics.clone());
                    let summary = format!("{} ({}, série)", e.subject, e.start.replace('T', " "));
                    ctx.created(rep, &format!("#s{n}"), summary, uid, Payload::Event(e), ctx.fp(&s.ics));
                }
            },
        }
    }
    ctx.deletions(&m, rep);
    Ok(())
}

/// Empreinte (celle de `propose`) de l'élément local `id` tel qu'il est maintenant : la valeur « ici » qu'une base
/// de comparaison retient après un règlement. `None` : élément introuvable, ou série (comparée autrement).
pub(crate) fn here_print(conn: &Connection, dir: &Path, item: ItemKind, id: &str) -> Option<String> {
    let n: i64 = id.get(1..)?.parse().ok()?;
    Some(match item {
        ItemKind::Contact => conflict::fingerprint(&canon_contact(&thunderlook_contacts::write::form_of(conn, dir, n).ok()?)),
        ItemKind::Task => conflict::fingerprint(&canon_task(&thunderlook_items::task_form(conn, dir, n).ok()?)),
        ItemKind::Note => conflict::fingerprint(&canon_note(&thunderlook_items::note_form(conn, dir, n).ok()?)),
        ItemKind::Event => {
            let cal = thunderlook_calendar::load(conn).ok()?;
            let a = cal.appointments.iter().find(|a| a.id == n).filter(|a| a.recurrence.is_none())?;
            conflict::fingerprint(&canon_event(a, dir))
        }
    })
}

// --- Application et annulation --------------------------------------------------------------------------------------

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Restore {
    item: ItemKind,
    id: String,
    /// L'élément a été supprimé (à recréer) ; sinon modifié (à remettre dans son état).
    deleted: bool,
    payload: Payload,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Created {
    item: ItemKind,
    id: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct UndoRecord {
    timestamp: String,
    restore: Vec<Restore>,
    created: Vec<Created>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ApplyReport {
    pub applied: usize,
    pub errors: Vec<String>,
    /// Conflits résolus en gardant la valeur de ThunderLook (rien d'écrit).
    pub kept: usize,
    /// Horodatage du fichier d'annulation, s'il y a de quoi annuler.
    pub undo: Option<String>,
    /// Clés des changements réglés (appliqués, ou conflits tranchés en gardant ThunderLook) : à donner à [`crate::settle`].
    #[serde(default)]
    pub done: Vec<String>,
}

fn num(id: &str, prefix: char) -> Result<i64> {
    id.strip_prefix(prefix).and_then(|n| n.parse().ok()).ok_or_else(|| format!("identifiant inconnu : {id}"))
}

fn prefix(item: ItemKind) -> char {
    match item {
        ItemKind::Contact => 'p',
        ItemKind::Task => 't',
        ItemKind::Note => 'n',
        ItemKind::Event => 'a',
    }
}

/// Dossier des nouveaux éléments : celui qui en contient le plus, sinon le premier dossier de la bonne classe.
fn default_folder(conn: &Connection, item: ItemKind) -> Result<i64> {
    let from_items = |msg: &str| thunderlook_items::default_folder(conn, msg).ok().flatten().and_then(|f| num(&f, 'f').ok());
    let by_class = |class: &str| conn.query_row("SELECT id FROM folder WHERE container_class LIKE ?1 ORDER BY id LIMIT 1", [class], |r| r.get::<_, i64>(0)).ok();
    let found = match item {
        ItemKind::Contact => thunderlook_contacts::view::folders(conn).ok().and_then(|f| f.first().and_then(|f| num(&f.id, 'k').ok())).or_else(|| by_class("IPF.Contact%")),
        ItemKind::Task => from_items("IPM.Task%").or_else(|| by_class("IPF.Task%")),
        ItemKind::Note => from_items("IPM.StickyNote%").or_else(|| by_class("IPF.StickyNote%")),
        ItemKind::Event => thunderlook_calendar::view::calendars(conn).ok().and_then(|l| thunderlook_calendar::view::default_shown(&l).first().and_then(|c| num(c, 'c').ok())),
    };
    found.ok_or_else(|| format!("Aucun dossier pour les nouveaux éléments ({}).", item.name()))
}

fn edit_of(e: &EventData) -> thunderlook_calendar::write::Edit {
    thunderlook_calendar::write::Edit {
        subject: e.subject.clone(),
        location: e.location.clone(),
        start: e.start.clone(),
        end: e.end.clone(),
        all_day: e.all_day,
        reminder: e.reminder,
        busy: e.busy,
        body: e.body.clone(),
        categories: e.categories.clone(),
        private: e.private,
        repeat: None,
    }
}

/// Périodicité d'une série (son VCALENDAR).
fn recurrence_of(ics: &str) -> Result<thunderlook_calendar::recur::Recurrence> {
    ics_series::parse_series(ics)?.appointment.recurrence.ok_or_else(|| "série sans périodicité".to_string())
}

/// État actuel d'un élément, pour pouvoir le rétablir.
fn snapshot(conn: &Connection, dir: &Path, item: ItemKind, id: &str) -> Result<Payload> {
    Ok(match item {
        ItemKind::Contact => Payload::Contact(thunderlook_contacts::write::form_of(conn, dir, num(id, 'p')?)?),
        ItemKind::Task => Payload::Task(thunderlook_items::task_form(conn, dir, num(id, 't')?)?),
        ItemKind::Note => Payload::Note(thunderlook_items::note_form(conn, dir, num(id, 'n')?)?),
        ItemKind::Event => {
            let n = num(id, 'a')?;
            let cal = thunderlook_calendar::load(conn)?;
            let a = cal.appointments.iter().find(|a| a.id == n).ok_or("rendez-vous introuvable")?;
            let mut e = event_of(a, thunderlook_calendar::description(a, dir));
            if a.recurrence.is_some() {
                e.series = Some(thunderlook_calendar::ics::export(&[a], "ThunderLook", &|a| thunderlook_calendar::description(a, dir)));
            }
            Payload::Event(e)
        }
    })
}

fn remove(conn: &mut Connection, item: ItemKind, id: &str) -> Result<()> {
    let n = num(id, prefix(item))?;
    match item {
        ItemKind::Contact => thunderlook_contacts::write::delete(conn, n),
        ItemKind::Task => thunderlook_items::task_delete(conn, n),
        ItemKind::Note => thunderlook_items::note_delete(conn, n),
        ItemKind::Event => thunderlook_calendar::write::delete(conn, n),
    }
}

/// Crée l'élément (nouveau, ou recréé par `undo`) ; rend son identifiant local.
fn create(conn: &mut Connection, dir: &Path, payload: &Payload) -> Result<String> {
    match payload {
        Payload::Contact(f) => {
            let folder = if f.folder_id.is_empty() { default_folder(conn, ItemKind::Contact)? } else { num(&f.folder_id, 'k')? };
            let f = ContactForm { id: None, folder_id: format!("k{folder}"), ..f.clone() };
            Ok(format!("p{}", thunderlook_contacts::write::create(conn, dir, folder, &f)?))
        }
        Payload::Task(f) => {
            let folder = if f.folder_id.is_empty() { default_folder(conn, ItemKind::Task)? } else { num(&f.folder_id, 'f')? };
            thunderlook_items::task_save(conn, dir, &TaskForm { id: None, folder_id: format!("f{folder}"), ..f.clone() })
        }
        Payload::Note(f) => {
            let folder = if f.folder_id.is_empty() { default_folder(conn, ItemKind::Note)? } else { num(&f.folder_id, 'f')? };
            thunderlook_items::note_save(conn, dir, &NoteForm { id: None, folder_id: format!("f{folder}"), ..f.clone() })
        }
        Payload::Event(e) => {
            let folder = if e.calendar_id.is_empty() { default_folder(conn, ItemKind::Event)? } else { num(&e.calendar_id, 'c')? };
            Ok(format!("a{}", match &e.series {
                Some(ics) => thunderlook_calendar::write::create_series(conn, dir, folder, &edit_of(e), &recurrence_of(ics)?)?,
                None => thunderlook_calendar::write::create(conn, dir, folder, &edit_of(e))?,
            }))
        }
        Payload::None => Err("rien à créer".into()),
    }
}

/// Remet à jour un élément existant (`id` local).
fn update(conn: &mut Connection, dir: &Path, id: &str, payload: &Payload) -> Result<()> {
    match payload {
        Payload::Contact(f) => thunderlook_contacts::write::update(conn, dir, num(id, 'p')?, &ContactForm { id: Some(id.to_string()), photo: None, ..f.clone() }),
        Payload::Task(f) => thunderlook_items::task_save(conn, dir, &TaskForm { id: Some(id.to_string()), ..f.clone() }).map(|_| ()),
        Payload::Note(f) => thunderlook_items::note_save(conn, dir, &NoteForm { id: Some(id.to_string()), ..f.clone() }).map(|_| ()),
        Payload::Event(e) => match &e.series {
            Some(ics) => thunderlook_calendar::write::replace_series(conn, dir, num(id, 'a')?, &edit_of(e), &recurrence_of(ics)?),
            None => thunderlook_calendar::write::update(conn, dir, num(id, 'a')?, &edit_of(e)),
        },
        Payload::None => Err("rien à modifier".into()),
    }
}

fn save_undo(data_dir: &Path, rec: &UndoRecord) -> Result<()> {
    let dir = undo_dir(data_dir);
    fs::create_dir_all(&dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    crate::write_atomic(&dir.join(format!("{}.json", rec.timestamp)), serde_json::to_string_pretty(rec).map_err(|e| e.to_string())?.as_bytes())
}

/// Applique les `changes` acceptés par l'utilisateur. Avant tout, l'état des éléments qui vont être modifiés ou
/// supprimés est consigné dans `data_dir/exchange-undo/<horodatage>.json` ; chaque échec est rapporté sans arrêter les autres.
pub fn apply(conn: &mut Connection, data_dir: &Path, changes: &[Change]) -> Result<ApplyReport> {
    let mut rec = UndoRecord { timestamp: chrono::Utc::now().format("%Y%m%dT%H%M%S%3f").to_string(), ..Default::default() };
    let mut report = ApplyReport::default();
    let mut todo: Vec<&Change> = Vec::new();
    for c in changes {
        if c.conflict.is_some() {
            match c.resolution {
                None => {
                    report.errors.push(format!("{} : conflit non résolu, ignoré.", c.summary));
                    continue;
                }
                Some(Resolution::KeepThunderlook) => {
                    report.kept += 1;
                    report.done.push(c.key.clone());
                    continue;
                }
                Some(Resolution::KeepPhone) => {}
            }
        }
        if c.kind == ChangeKind::New {
            todo.push(c);
            continue;
        }
        match snapshot(conn, data_dir, c.item, &c.id) {
            Ok(payload) => {
                rec.restore.push(Restore { item: c.item, id: c.id.clone(), deleted: c.kind == ChangeKind::Deleted, payload });
                todo.push(c);
            }
            Err(e) => report.errors.push(format!("{} : {e}", c.summary)),
        }
    }
    if !rec.restore.is_empty() {
        save_undo(data_dir, &rec)?;
    }
    let mut map = crate::read_map(data_dir);
    for c in todo {
        let item = c.item.name().to_string();
        let result = match c.kind {
            ChangeKind::New => create(conn, data_dir, &c.payload).map(|id| {
                rec.created.push(Created { item: c.item, id: id.clone() });
                Some(id)
            }),
            ChangeKind::Modified => update(conn, data_dir, &c.id, &c.payload).map(|()| Some(c.id.clone())),
            ChangeKind::Deleted => remove(conn, c.item, &c.id).map(|()| None),
        };
        match result {
            Ok(local) => {
                report.applied += 1;
                report.done.push(c.key.clone());
                if let (Some(uid), Some(local)) = (&c.ext_uid, local) {
                    map.entry(item).or_default().insert(uid.clone(), local);
                }
            }
            Err(e) => report.errors.push(format!("{} : {e}", c.summary)),
        }
    }
    crate::write_map(data_dir, &map)?;
    if !rec.restore.is_empty() || !rec.created.is_empty() {
        save_undo(data_dir, &rec)?;
        report.undo = Some(rec.timestamp);
    }
    Ok(report)
}

/// Horodatage du dernier import appliqué et pas encore annulé.
pub fn last_undo(data_dir: &Path) -> Option<String> {
    let mut stamps: Vec<String> = fs::read_dir(undo_dir(data_dir))
        .ok()?
        .filter_map(|e| e.ok()?.file_name().to_str()?.strip_suffix(".json").map(str::to_string))
        .collect();
    stamps.sort();
    stamps.pop()
}

/// Défait l'import `timestamp` : supprime ce qu'il a créé, remet les éléments modifiés dans leur état, recrée ceux qu'il
/// a supprimés (avec un nouvel identifiant : la base n'en réutilise pas). Rend le nombre d'éléments rétablis.
pub fn undo(conn: &mut Connection, data_dir: &Path, timestamp: &str) -> Result<usize> {
    let path = undo_dir(data_dir).join(format!("{timestamp}.json"));
    let rec: UndoRecord = serde_json::from_str(&fs::read_to_string(&path).map_err(|e| format!("{} : {e}", path.display()))?).map_err(|e| e.to_string())?;
    let mut map = crate::read_map(data_dir);
    let (mut done, mut errors) = (0, Vec::new());
    for c in &rec.created {
        // Déjà supprimé à la main depuis : rien à défaire.
        match remove(conn, c.item, &c.id) {
            Ok(()) => done += 1,
            Err(e) => errors.push(e),
        }
        // Dossier écrit par MPE : l'élément annulé est déjà réglé dans la base de comparaison, son identifiant reste
        // retenu (vers un élément qui n'existe plus) pour que le fichier ne le recrée pas au passage suivant.
        let settled = crate::phone::prints_of(data_dir, c.item);
        map.entry(c.item.name().to_string()).or_default().retain(|uid, local| *local != c.id || settled.contains_key(uid));
    }
    for r in &rec.restore {
        let result = if r.deleted {
            create(conn, data_dir, &r.payload).map(|new| {
                map.entry(r.item.name().to_string()).or_default().values_mut().filter(|l| **l == r.id).for_each(|l| *l = new.clone());
            })
        } else {
            update(conn, data_dir, &r.id, &r.payload)
        };
        match result {
            Ok(()) => done += 1,
            Err(e) => errors.push(e),
        }
    }
    crate::write_map(data_dir, &map)?;
    fs::rename(&path, path.with_extension("undone")).map_err(|e| e.to_string())?;
    if errors.is_empty() { Ok(done) } else { Err(format!("{done} rétabli(s), {} échec(s) : {}", errors.len(), errors.join(" ; "))) }
}
