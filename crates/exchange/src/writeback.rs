//! Sens ThunderLook → téléphone en mode « fichiers seuls » : ThunderLook reporte SES modifications dans les fichiers que
//! MPE synchronise avec le téléphone (`contacts.vcf`, `calendrier.ics`), fiche par fiche.
//!
//! Le fichier de MPE n'est jamais régénéré : chaque fiche que ThunderLook n'a pas touchée reste octet pour octet ; seule
//! une fiche modifiée ici est réécrite (même `UID`, `REV` / `LAST-MODIFIED` à l'heure de l'écriture, propriétés `X-` de MPE
//! et groupes système d'Android repris de l'ancienne fiche), une fiche supprimée ici est retirée, une fiche créée ici
//! après la mise en route ([`Base::since`]) est ajoutée avec un `UID` neuf. MPE, réglé sur « synchroniser les deux sens »,
//! voit alors ces fiches changées et les reporte sur le téléphone à la synchro suivante.
//!
//! Garde-fous :
//! - rien n'est écrit si les fichiers ont changé depuis la dernière relecture (signature différente) : le téléphone a pu
//!   modifier une fiche, il faut relire d'abord ;
//! - une fiche n'est réécrite que si la base de comparaison la connaît (réglée) et que l'élément local a changé depuis :
//!   un conflit en attente n'est jamais écrasé ;
//! - les séries (rendez-vous périodiques) modifiées ici ne sont pas réécrites (comptées dans [`WriteReport::skipped`]) ;
//! - ce qui existait ici avant la mise en route et que le téléphone n'a pas n'est pas poussé (anciens rendez-vous supprimés
//!   du téléphone, doublons) ;
//! - les notes restent dans le sens téléphone → ThunderLook (`ImportOptions::notes_from_phone`).
//!
//! Après écriture, les fichiers sont relus et la base de comparaison est réglée sur ce qui vient d'être écrit : la
//! relecture suivante n'y voit pas de « modification du téléphone ».

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use serde::Serialize;
use thunderlook_calendar::{Appointment, Busy};
use thunderlook_contacts::vcard;
use thunderlook_store::rusqlite::Connection;

use crate::{CALENDAR_FILE, CONTACTS_FILE, ItemKind, Result, write_atomic};

/// Ce qu'un report a fait (ou ferait, à blanc).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteReport {
    pub modified: usize,
    pub created: usize,
    pub deleted: usize,
    /// Modifications non reportées (séries, élément illisible), avec la raison.
    pub skipped: Vec<String>,
    /// Faux : à blanc, rien n'a été écrit.
    pub written: bool,
}

impl WriteReport {
    pub fn is_empty(&self) -> bool {
        self.modified + self.created + self.deleted == 0
    }
}

/// Une fiche du fichier : ses lignes logiques (dépliées) et son texte d'origine, gardé tel quel si rien ne change.
struct Record {
    raw: String,
    uid: Option<String>,
}

/// Découpe `text` en : en-tête, fiches `BEGIN:<kind>`…`END:<kind>` (avec ce qui les sépare de la suivante), pied.
/// Les autres blocs (VTODO dans le calendrier) restent dans les morceaux « autres », intacts.
enum Piece {
    Other(String),
    Item(Record),
}

fn split(text: &str, kind: &str) -> Vec<Piece> {
    let (begin, end) = (format!("BEGIN:{kind}"), format!("END:{kind}"));
    let mut pieces = Vec::new();
    let mut other = String::new();
    let mut current: Option<String> = None;
    for line in text.split_inclusive('\n') {
        let t = line.trim_end();
        match &mut current {
            None if t.eq_ignore_ascii_case(&begin) => {
                if !other.is_empty() {
                    pieces.push(Piece::Other(std::mem::take(&mut other)));
                }
                current = Some(line.to_string());
            }
            None => other.push_str(line),
            Some(buf) => {
                buf.push_str(line);
                if t.eq_ignore_ascii_case(&end) {
                    let raw = current.take().unwrap_or_default();
                    let uid = logical_lines(&raw).iter().find_map(|l| prop(l, "UID").map(str::to_string));
                    pieces.push(Piece::Item(Record { raw, uid }));
                }
            }
        }
    }
    if let Some(buf) = current {
        other.push_str(&buf);
    }
    if !other.is_empty() {
        pieces.push(Piece::Other(other));
    }
    pieces
}

/// Lignes logiques : continuation par espace ou tabulation (RFC), et lignes quoted-printable terminées par `=`.
fn logical_lines(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in raw.lines() {
        if (line.starts_with(' ') || line.starts_with('\t')) && !out.is_empty() {
            out.last_mut().unwrap().push_str(&line[1..]);
        } else if out.last().is_some_and(|l| l.contains("QUOTED-PRINTABLE") && l.ends_with('=')) {
            let last = out.last_mut().unwrap();
            last.pop();
            last.push_str(line);
        } else {
            out.push(line.to_string());
        }
    }
    out
}

/// Valeur de la propriété `name` (sans paramètres) de la ligne, si c'est elle.
fn prop<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let (head, value) = line.split_once(':')?;
    let key = head.split(';').next()?;
    key.eq_ignore_ascii_case(name).then_some(value.trim())
}

fn prop_name(line: &str) -> String {
    line.split([':', ';']).next().unwrap_or("").to_ascii_uppercase()
}

fn utc_stamp() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

/// `UID` neuf, au format de MPE (GUID en minuscules).
fn new_uid(seed: &str) -> String {
    let h = crate::sha256::hex(format!("{seed}|{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()).as_bytes());
    format!("{}-{}-4{}-a{}-{}", &h[0..8], &h[8..12], &h[13..16], &h[17..20], &h[20..32])
}

// --- Contacts -------------------------------------------------------------------------------------------------------

/// Fiche vCard 2.1 de `form` sous l'identifiant `uid`, complétée de ce que MPE avait mis dans l'ancienne fiche et que
/// ThunderLook ne connaît pas (propriétés `X-`, groupes système, photo si ThunderLook n'en a pas).
fn contact_record(form: &thunderlook_model::contacts::ContactForm, uid: &str, old: Option<&str>) -> String {
    let mut f = form.clone();
    f.id = Some(uid.to_string());
    let old_lines = old.map(logical_lines).unwrap_or_default();
    let system: Vec<String> = old_lines
        .iter()
        .filter_map(|l| prop(l, "CATEGORIES"))
        .flat_map(|v| v.split([',', ';']).map(str::trim).map(str::to_string).collect::<Vec<_>>())
        .filter(|c| c.eq_ignore_ascii_case("My Contacts") || c.to_ascii_lowercase().starts_with("system group:"))
        .collect();
    let text = vcard::export(std::slice::from_ref(&f), vcard::Version::V21);
    let mut lines: Vec<String> = Vec::new();
    let mut has_photo = false;
    let mut has_uid = false;
    for l in logical_lines(&text) {
        let name = prop_name(&l);
        if name == "END" || name == "REV" {
            continue;
        }
        has_photo |= name == "PHOTO";
        has_uid |= name == "UID";
        if name == "CATEGORIES" && !system.is_empty() {
            lines.push(format!("{};{}", l, system.join(";")));
            continue;
        }
        if !l.trim().is_empty() {
            lines.push(l);
        }
    }
    if !has_uid {
        lines.push(format!("UID:{uid}"));
    }
    if !system.is_empty() && !lines.iter().any(|l| prop_name(l) == "CATEGORIES") {
        lines.push(format!("CATEGORIES:{}", system.join(";")));
    }
    lines.push(format!("REV:{}", utc_stamp()));
    for l in &old_lines {
        let name = prop_name(l);
        if name.starts_with("X-") || (name == "PHOTO" && !has_photo) {
            lines.push(l.clone());
        }
    }
    let mut out = String::new();
    for l in lines {
        out.push_str(&fold(&l));
        out.push_str("\r\n");
        if prop_name(&l) == "PHOTO" {
            out.push_str("\r\n");
        }
    }
    out.push_str("END:VCARD\r\n");
    out
}

// --- Rendez-vous ----------------------------------------------------------------------------------------------------

fn ics_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace("\r\n", "\\n").replace('\n', "\\n")
}

/// Pliage à 75 octets (RFC 5545 § 3.1), sans couper un caractère.
pub(crate) fn fold(line: &str) -> String {
    let mut out = String::new();
    let mut width = 0;
    for ch in line.chars() {
        let n = ch.len_utf8();
        if width + n > 75 {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(ch);
        width += n;
    }
    out
}

fn utc_of(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).unwrap_or_default().format("%Y%m%dT%H%M%SZ").to_string()
}

fn local_date_of(ms: i64) -> String {
    let utc = chrono::DateTime::from_timestamp_millis(ms).unwrap_or_default().naive_utc();
    thunderlook_calendar::tz::TimeZone::PARIS.to_local(utc).format("%Y%m%d").to_string()
}

/// `VEVENT` d'un rendez-vous simple, à la manière de MPE (heures en UTC, journées entières en dates).
fn event_record(a: &Appointment, body: Option<&str>, uid: &str, old: Option<&str>) -> String {
    let mut lines = vec!["BEGIN:VEVENT".to_string(), format!("SUMMARY:{}", ics_escape(&a.subject))];
    if a.all_day {
        lines.push(format!("DTSTART;VALUE=DATE:{}", local_date_of(a.start_ms)));
        lines.push(format!("DTEND;VALUE=DATE:{}", local_date_of(a.end_ms)));
    } else {
        lines.push(format!("DTSTART:{}", utc_of(a.start_ms)));
        lines.push(format!("DTEND:{}", utc_of(a.end_ms)));
    }
    if let Some(l) = a.location.as_deref().filter(|l| !l.trim().is_empty()) {
        lines.push(format!("LOCATION:{}", ics_escape(l)));
    }
    if let Some(d) = body.filter(|d| !d.trim().is_empty()) {
        lines.push(format!("DESCRIPTION:{}", ics_escape(d.trim_end())));
    }
    lines.push(format!("CLASS:{}", if a.private { "PRIVATE" } else { "DEFAULT" }));
    lines.push(format!("TRANSP:{}", if a.busy == Busy::Free { "TRANSPARENT" } else { "OPAQUE" }));
    if let Some(m) = a.reminder_minutes {
        lines.extend(["BEGIN:VALARM".into(), "ACTION:DISPLAY".into(), format!("TRIGGER;VALUE=DURATION:-PT{m}M"), "END:VALARM".into()]);
    }
    lines.push(format!("LAST-MODIFIED:{}", utc_stamp()));
    let old_lines = old.map(logical_lines).unwrap_or_default();
    let calendar = old_lines.iter().find(|l| prop_name(l) == "X-CALENDAR").cloned().unwrap_or_else(|| "X-CALENDAR:My calendar@LOCAL/NULL;My calendar".into());
    lines.push(calendar);
    lines.push(format!("UID:{uid}"));
    lines.push("END:VEVENT".into());
    lines.iter().map(|l| format!("{}\r\n", fold(l))).collect()
}

// --- Report ---------------------------------------------------------------------------------------------------------

/// Instant (ms) à partir duquel un élément créé ici est poussé vers le téléphone : fixé à la première écriture.
fn since_ms(base: &crate::phone::Base) -> i64 {
    chrono::DateTime::parse_from_rfc3339(&base.since).map(|t| t.timestamp_millis()).unwrap_or(i64::MAX)
}

fn created_after(conn: &Connection, folder: i64, ms: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare("SELECT id FROM message WHERE folder_id = ?1 AND created_at > ?2").map_err(|e| e.to_string())?;
    let rows = stmt.query_map((folder, ms), |r| r.get::<_, i64>(0)).map_err(|e| e.to_string())?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

fn exists(conn: &Connection, n: i64) -> bool {
    conn.query_row("SELECT 1 FROM message WHERE id = ?1", [n], |_| Ok(())).is_ok()
}

/// Reporte les modifications de ThunderLook dans les fichiers de `dir` (dossier écrit par MPE). `dry` : calcule sans
/// rien écrire. Refusé si les fichiers ont changé depuis la dernière relecture.
pub fn write_back(conn: &Connection, data_dir: &Path, dir: &Path, dry: bool) -> Result<WriteReport> {
    if crate::read_manifest(dir).is_some() {
        return Err("dossier d'échange de ThunderLook (manifeste) : le report fichier par fichier ne s'applique qu'aux fichiers de MPE".into());
    }
    let signature = crate::files_signature(dir);
    if crate::last_signature(data_dir).as_deref() != Some(signature.as_str()) {
        return Err("les fichiers ont changé depuis la dernière relecture : relire d'abord".into());
    }
    let mut base = crate::phone::read_base(data_dir);
    if base.since.is_empty() && !dry {
        base.since = chrono::Utc::now().to_rfc3339();
        crate::phone::write_base(data_dir, &base)?;
    }
    let since = since_ms(&base);
    // Fiches que la relecture propose encore (modifiées sur le téléphone, en conflit, supprimées) : jamais réécrites ici.
    let pending: HashSet<(ItemKind, String)> = {
        let rep = crate::import_changes(conn, data_dir, dir)?;
        let mut p: HashSet<(ItemKind, String)> = rep.seen.iter().filter(|s| s.key.is_some()).map(|s| (s.item, s.uid.clone())).collect();
        let map = crate::read_map(data_dir);
        for c in rep.changes.iter().chain(&rep.conflicts).filter(|c| c.kind == crate::ChangeKind::Deleted) {
            if let Some((uid, _)) = map.get(c.item.name()).and_then(|m| m.iter().find(|(_, l)| **l == c.id)) {
                p.insert((c.item, uid.clone()));
            }
        }
        p
    };
    let mut map = crate::read_map(data_dir);
    let mut report = WriteReport::default();
    let mut written: HashSet<(ItemKind, String)> = HashSet::new();
    let mut outputs: Vec<(String, String)> = Vec::new();

    // Contacts.
    let path = dir.join(CONTACTS_FILE);
    if let Ok(bytes) = fs::read(&path) {
        let text = String::from_utf8(bytes.clone()).unwrap_or_else(|_| bytes.iter().map(|&b| b as char).collect());
        let prints = base.items.get(ItemKind::Contact.name()).cloned().unwrap_or_default();
        let ids = map.entry(ItemKind::Contact.name().to_string()).or_default();
        let mut out = String::new();
        let mut known: HashSet<String> = ids.values().cloned().collect();
        let mut changed = false;
        for piece in split(&text, "VCARD") {
            let r = match piece {
                Piece::Other(s) => {
                    out.push_str(&s);
                    continue;
                }
                Piece::Item(r) => r,
            };
            let Some(uid) = r.uid.clone() else { out.push_str(&r.raw); continue };
            if pending.contains(&(ItemKind::Contact, uid.clone())) {
                out.push_str(&r.raw);
                continue;
            }
            let (Some(local), Some(print)) = (ids.get(&uid).cloned(), prints.get(&uid)) else { out.push_str(&r.raw); continue };
            let Some(n) = local.get(1..).and_then(|s| s.parse::<i64>().ok()) else { out.push_str(&r.raw); continue };
            if !exists(conn, n) {
                report.deleted += 1;
                changed = true;
                written.insert((ItemKind::Contact, uid));
                continue;
            }
            match crate::import::here_print(conn, data_dir, ItemKind::Contact, &local) {
                Some(here) if here != print.here => match thunderlook_contacts::write::form_of(conn, data_dir, n) {
                    Ok(form) => {
                        out.push_str(&contact_record(&form, &uid, Some(&r.raw)));
                        report.modified += 1;
                        changed = true;
                        written.insert((ItemKind::Contact, uid));
                    }
                    Err(e) => {
                        report.skipped.push(format!("contact {local} : {e}"));
                        out.push_str(&r.raw);
                    }
                },
                _ => out.push_str(&r.raw),
            }
        }
        // Créés ici depuis la mise en route, dans le dossier des contacts du téléphone.
        if let Ok(folder) = crate::import::default_folder(conn, ItemKind::Contact) {
            for n in created_after(conn, folder, since)? {
                let local = format!("p{n}");
                if !known.insert(local.clone()) {
                    continue;
                }
                let Ok(form) = thunderlook_contacts::write::form_of(conn, data_dir, n) else { continue };
                let uid = new_uid(&local);
                let record = contact_record(&form, &uid, None);
                insert_before_end(&mut out, &record, "END:VCARD", false);
                ids.insert(uid.clone(), local);
                report.created += 1;
                changed = true;
                written.insert((ItemKind::Contact, uid));
            }
        }
        if changed {
            outputs.push((CONTACTS_FILE.to_string(), out));
        }
    }

    // Rendez-vous.
    let path = dir.join(CALENDAR_FILE);
    if let Ok(bytes) = fs::read(&path) {
        let text = String::from_utf8(bytes.clone()).unwrap_or_else(|_| bytes.iter().map(|&b| b as char).collect());
        let prints = base.items.get(ItemKind::Event.name()).cloned().unwrap_or_default();
        let cal = thunderlook_calendar::load(conn)?;
        let by_id: HashMap<i64, &Appointment> = cal.appointments.iter().map(|a| (a.id, a)).collect();
        let ids = map.entry(ItemKind::Event.name().to_string()).or_default();
        let mut known: HashSet<String> = ids.values().cloned().collect();
        let mut out = String::new();
        let mut changed = false;
        for piece in split(&text, "VEVENT") {
            let r = match piece {
                Piece::Other(s) => {
                    out.push_str(&s);
                    continue;
                }
                Piece::Item(r) => r,
            };
            let Some(uid) = r.uid.clone() else { out.push_str(&r.raw); continue };
            if pending.contains(&(ItemKind::Event, uid.clone())) {
                out.push_str(&r.raw);
                continue;
            }
            let (Some(local), Some(print)) = (ids.get(&uid).cloned(), prints.get(&uid)) else { out.push_str(&r.raw); continue };
            let Some(n) = local.get(1..).and_then(|s| s.parse::<i64>().ok()) else { out.push_str(&r.raw); continue };
            let Some(a) = by_id.get(&n) else {
                if exists(conn, n) {
                    out.push_str(&r.raw);
                } else {
                    report.deleted += 1;
                    changed = true;
                    written.insert((ItemKind::Event, uid));
                }
                continue;
            };
            if a.recurrence.is_some() {
                // Les séries ne sont pas comparées par empreinte simple : jamais réécrites ici.
                out.push_str(&r.raw);
                continue;
            }
            let here = crate::conflict::fingerprint(&crate::import::canon_event(a, data_dir));
            if here != print.here {
                let body = thunderlook_calendar::description(a, data_dir);
                out.push_str(&event_record(a, body.as_deref(), &uid, Some(&r.raw)));
                report.modified += 1;
                changed = true;
                written.insert((ItemKind::Event, uid));
            } else {
                out.push_str(&r.raw);
            }
        }
        if let Ok(folder) = crate::import::default_folder(conn, ItemKind::Event) {
            for n in created_after(conn, folder, since)? {
                let local = format!("a{n}");
                if !known.insert(local.clone()) {
                    continue;
                }
                let Some(a) = by_id.get(&n) else { continue };
                if a.recurrence.is_some() {
                    report.skipped.push(format!("« {} » : série créée dans ThunderLook, non envoyée au téléphone", a.subject));
                    continue;
                }
                let uid = new_uid(&local);
                let body = thunderlook_calendar::description(a, data_dir);
                insert_before_end(&mut out, &event_record(a, body.as_deref(), &uid, None), "END:VCALENDAR", true);
                ids.insert(uid.clone(), local);
                report.created += 1;
                changed = true;
                written.insert((ItemKind::Event, uid));
            }
        }
        if changed {
            outputs.push((CALENDAR_FILE.to_string(), out));
        }
    }

    if dry || outputs.is_empty() {
        return Ok(report);
    }
    // Les fichiers n'ont pas bougé pendant le calcul (MPE a pu écrire entre-temps).
    if crate::files_signature(dir) != signature {
        return Err("MPE a réécrit les fichiers pendant le report : rien n'est écrit, relire d'abord".into());
    }
    for (name, text) in &outputs {
        write_atomic(&dir.join(name), text.as_bytes())?;
    }
    crate::write_map(data_dir, &map)?;
    report.written = true;

    // Base de comparaison réglée sur ce qui vient d'être écrit.
    let rep = crate::import_changes(conn, data_dir, dir)?;
    let done: Vec<String> = rep.seen.iter().filter(|s| written.contains(&(s.item, s.uid.clone()))).filter_map(|s| s.key.clone()).collect();
    crate::settle(conn, data_dir, &rep, &done)?;
    crate::set_signature(data_dir, &crate::files_signature(dir))?;
    Ok(report)
}

/// Ajoute `record` juste avant la dernière ligne `end` (fin du calendrier), ou à la fin du texte (`inside` faux).
fn insert_before_end(out: &mut String, record: &str, end: &str, inside: bool) {
    if inside && let Some(i) = out.to_ascii_uppercase().rfind(end) {
        out.insert_str(i, record);
        return;
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str("\r\n");
    }
    out.push_str(record);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoupe_garde_les_fiches_intactes() {
        let text = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nSUMMARY:a\r\nUID:1\r\nEND:VEVENT\r\nBEGIN:VTODO\r\nUID:t\r\nEND:VTODO\r\nBEGIN:VEVENT\r\nSUMMARY:b\r\n folded\r\nUID:2\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let pieces = split(text, "VEVENT");
        let back: String = pieces.iter().map(|p| match p { Piece::Other(s) => s.as_str(), Piece::Item(r) => r.raw.as_str() }).collect();
        assert_eq!(back, text);
        let uids: Vec<_> = pieces.iter().filter_map(|p| match p { Piece::Item(r) => r.uid.clone(), _ => None }).collect();
        assert_eq!(uids, ["1", "2"]);
    }

    #[test]
    fn fiche_contact_reprend_ce_que_mpe_avait_mis() {
        let old = "BEGIN:VCARD\r\nVERSION:2.1\r\nN:Dupont;Jean;;;\r\nCATEGORIES:System Group: My Contacts;TOURS\r\nUID:abc\r\nREV:20261025\r\nX-ACCOUNT:vnd.sec.contact.phone;vnd.sec.contact.phone\r\nEND:VCARD\r\n";
        let form = thunderlook_model::contacts::ContactForm { given_name: "Jean".into(), surname: "Dupond".into(), categories: "TOURS".into(), ..Default::default() };
        let rec = contact_record(&form, "abc", Some(old));
        assert!(rec.contains("UID:abc\r\n"), "{rec}");
        assert!(rec.contains("X-ACCOUNT:vnd.sec.contact.phone"), "{rec}");
        assert!(rec.contains("System Group: My Contacts"), "{rec}");
        assert!(rec.contains("Dupond"), "{rec}");
        assert!(!rec.contains("REV:20261025"), "{rec}");
        assert!(rec.ends_with("END:VCARD\r\n"));
        let parsed = vcard::parse(&rec).unwrap();
        assert_eq!(parsed[0].uid.as_deref(), Some("abc"));
        assert_eq!(parsed[0].form.surname, "Dupond");
    }

    #[test]
    fn nouvel_uid_au_format_guid() {
        let u = new_uid("p1");
        assert_eq!(u.len(), 36);
        assert_eq!(u.matches('-').count(), 4);
    }
}
