//! Commandes Tauri du lot AW : invitations de réunion, sur le cœur iTIP du lot BH (`thunderlook_calendar::itip`).
//!
//! - Lecture : la partie `text/calendar` (ou la pièce jointe `.ics`) d'un message est lue ; l'interface en tire la barre
//!   Accepter / Provisoire / Refuser (REQUEST), le suivi d'une réponse (REPLY) ou l'avis d'annulation (CANCEL).
//! - Réponse : un REPLY est envoyé à l'organisateur par le circuit d'envoi ordinaire (Boîte d'envoi, puis passage).
//! - Organisateur : le rendez-vous garde ses participants dans `invitations.json` du dossier de données (Outlook n'est
//!   pas modifié) ; « Envoyer l'invitation » envoie un REQUEST, une réponse reçue met à jour l'état des participants.
//!
//! Le `.ics` part en pièce jointe d'un message ordinaire (le circuit d'envoi ne sait pas encore fabriquer la partie
//! `text/calendar; method=…` d'Outlook) : voir `REA-LIMITES.md`.

use std::{fs, path::PathBuf};

use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use thunderlook_calendar::{
    itip::{self, Invitation, Method, PartStat, Person},
    tz::TimeZone,
};
use thunderlook_mailbox::Mailbox;
use thunderlook_model::{ComposeKind, Draft, DraftAttachment, DraftAttachmentSource, calendar::AppointmentForm};

use crate::{Store, data_dir, start_sync, with_mailbox};

const TZ: TimeZone = TimeZone::PARIS;
const STORE_FILE: &str = "invitations.json";

// ---------------------------------------------------------------- vues pour l'interface

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AttendeeView {
    pub email: String,
    pub name: String,
    /// `needs-action`, `accepted`, `tentative` ou `declined`.
    pub status: String,
    pub status_label: String,
}

/// Invitation lue dans un message.
#[derive(Serialize, Debug, PartialEq)]
pub struct InviteView {
    /// `request`, `reply`, `cancel`, `publish` ou `other`.
    pub method: String,
    pub uid: String,
    pub summary: String,
    pub location: String,
    pub description: String,
    pub when_label: String,
    /// Début et fin locaux `AAAA-MM-JJTHH:MM`, pour pré-remplir un rendez-vous.
    pub start: String,
    pub end: String,
    pub all_day: bool,
    pub recurring: bool,
    pub organizer: String,
    pub attendees: Vec<AttendeeView>,
    /// Mon adresse parmi les participants (vide : je ne suis pas invité nommément, pas de réponse possible).
    pub me: String,
    pub my_status: String,
    /// Phrase de la barre d'information (« Camille a accepté », « Réunion annulée »…).
    pub headline: String,
    pub warnings: Vec<String>,
    /// REPLY reçu : l'invitation correspondante est suivie ici (« Mettre à jour le suivi » utile).
    pub tracked: bool,
}

// ---------------------------------------------------------------- suivi local des réunions organisées

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Meeting {
    /// Rendez-vous Outlook (`a<id>`) quand il est connu ; un nouveau rendez-vous n'en a pas encore.
    #[serde(default)]
    appointment_id: Option<String>,
    uid: String,
    sequence: i64,
    subject: String,
    /// Début local `AAAA-MM-JJTHH:MM`, avec l'objet : clé de secours d'un rendez-vous créé avant d'avoir un id.
    start: String,
    attendees: Vec<AttendeeView>,
}

fn store_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join(STORE_FILE))
}

fn load(app: &AppHandle) -> Vec<Meeting> {
    store_path(app).ok().and_then(|p| fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save(app: &AppHandle, meetings: &[Meeting]) -> Result<(), String> {
    let path = store_path(app)?;
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(meetings).map_err(|e| e.to_string())?;
    fs::write(&tmp, text).map_err(|e| format!("{} : {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("{} : {e}", path.display()))
}

/// Réunion suivie pour ce rendez-vous : par id, sinon par objet et début (rendez-vous créé avant d'avoir un id).
fn find(meetings: &[Meeting], id: Option<&str>, subject: &str, start: &str) -> Option<usize> {
    id.and_then(|id| meetings.iter().position(|m| m.appointment_id.as_deref() == Some(id)))
        .or_else(|| meetings.iter().position(|m| m.subject == subject && m.start == start && (id.is_none() || m.appointment_id.is_none())))
}

// ---------------------------------------------------------------- utilitaires purs (testés)

fn status_code(p: PartStat) -> &'static str {
    match p {
        PartStat::NeedsAction => "needs-action",
        PartStat::Accepted => "accepted",
        PartStat::Tentative => "tentative",
        PartStat::Declined => "declined",
    }
}

fn status_of(code: &str) -> PartStat {
    match code {
        "accepted" => PartStat::Accepted,
        "tentative" => PartStat::Tentative,
        "declined" => PartStat::Declined,
        _ => PartStat::NeedsAction,
    }
}

fn status_label(p: PartStat) -> &'static str {
    match p {
        PartStat::NeedsAction => "Pas de réponse",
        PartStat::Accepted => "Accepté",
        PartStat::Tentative => "Provisoire",
        PartStat::Declined => "Refusé",
    }
}

fn attendee_view(p: &Person) -> AttendeeView {
    AttendeeView { email: p.email.clone(), name: p.name.clone(), status: status_code(p.partstat).into(), status_label: status_label(p.partstat).into() }
}

fn display(p: &Person) -> String {
    if p.name.trim().is_empty() { p.email.clone() } else { format!("{} <{}>", p.name, p.email) }
}

fn local(ms: i64) -> NaiveDateTime {
    TZ.to_local(DateTime::from_timestamp_millis(ms).unwrap_or_default().naive_utc())
}

fn local_text(ms: i64) -> String {
    local(ms).format("%Y-%m-%dT%H:%M").to_string()
}

const DAYS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
const MONTHS: [&str; 12] = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];

fn day_label(d: NaiveDate) -> String {
    format!("{} {} {} {}", DAYS[d.weekday().num_days_from_monday() as usize], d.day(), MONTHS[d.month0() as usize], d.year())
}

/// « mardi 28 octobre 2026, 14:00 – 15:00 », « … (journée entière) », ou deux jours si la réunion déborde.
pub fn when_label(start_ms: i64, end_ms: i64, all_day: bool) -> String {
    let (s, e) = (local(start_ms), local(end_ms));
    if all_day {
        let last = (e - chrono::Duration::days(1)).date().max(s.date());
        return if last == s.date() {
            format!("{} (journée entière)", day_label(s.date()))
        } else {
            format!("du {} au {} (journées entières)", day_label(s.date()), day_label(last))
        };
    }
    let hm = |t: NaiveDateTime| format!("{:02}:{:02}", t.hour(), t.minute());
    if s.date() == e.date() {
        format!("{}, {} – {}", day_label(s.date()), hm(s), hm(e))
    } else {
        format!("{}, {} – {}, {}", day_label(s.date()), hm(s), day_label(e.date()), hm(e))
    }
}

/// `AAAA-MM-JJTHH:MM` (heure de Paris) ou `AAAA-MM-JJ` en millisecondes UTC.
pub fn parse_local(s: &str) -> Option<i64> {
    let s = s.trim();
    let t = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .ok()
        .or_else(|| NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()?.and_hms_opt(0, 0, 0))?;
    Some(TZ.to_utc(t).and_utc().timestamp_millis())
}

/// Liste « Nom <adresse>; adresse, … » en participants (doublons et adresses sans @ écartés).
pub fn parse_people(list: &str) -> Vec<Person> {
    let mut out: Vec<Person> = Vec::new();
    for raw in list.split([';', ',', '\n']) {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let (name, addr) = match (raw.rfind('<'), raw.rfind('>')) {
            (Some(a), Some(b)) if a < b => (raw[..a].trim().trim_matches('"').trim(), raw[a + 1..b].trim()),
            _ => ("", raw),
        };
        if !addr.contains('@') || addr.contains(char::is_whitespace) {
            continue;
        }
        let p = Person::new(addr, name);
        if !out.iter().any(|o| o.email == p.email) {
            out.push(p);
        }
    }
    out
}

/// Préfixe d'objet de la réponse, comme Outlook en français [à vérifier sur un Outlook FR].
fn reply_prefix(answer: PartStat) -> &'static str {
    match answer {
        PartStat::Accepted => "Accepté",
        PartStat::Tentative => "Provisoire",
        PartStat::Declined => "Refusé",
        PartStat::NeedsAction => "",
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\n', "<br>")
}

/// Vue d'une invitation lue, pour l'adresse `mine` (toutes mes adresses, en minuscules).
pub fn view_of(inv: &Invitation, mine: &[String], tracked: bool) -> InviteView {
    let me = inv.attendees.iter().find(|a| mine.contains(&a.email));
    let organizer = inv.organizer.as_ref().map(display).unwrap_or_default();
    let headline = match inv.method {
        Method::Request if inv.sequence > 0 => format!("Mise à jour de la réunion envoyée par {organizer}."),
        Method::Request => format!("Invitation à une réunion envoyée par {organizer}."),
        Method::Reply => match inv.attendees.first() {
            Some(a) => {
                let who = if a.name.trim().is_empty() { a.email.clone() } else { a.name.clone() };
                match a.partstat {
                    PartStat::Accepted => format!("{who} a accepté la réunion."),
                    PartStat::Tentative => format!("{who} a accepté provisoirement la réunion."),
                    PartStat::Declined => format!("{who} a refusé la réunion."),
                    PartStat::NeedsAction => format!("{who} n'a pas encore répondu."),
                }
            }
            None => "Réponse à une réunion (sans participant).".into(),
        },
        Method::Cancel => format!("Réunion annulée par {organizer}. Supprimez-la de votre calendrier si elle y figure."),
        Method::Publish => "Événement publié (pas de réponse attendue).".into(),
        Method::Other => "Message de calendrier non pris en charge (ADD, REFRESH, COUNTER…).".into(),
    };
    InviteView {
        method: match inv.method {
            Method::Request => "request",
            Method::Reply => "reply",
            Method::Cancel => "cancel",
            Method::Publish => "publish",
            Method::Other => "other",
        }
        .into(),
        uid: inv.uid.clone(),
        summary: inv.summary.clone(),
        location: inv.location.clone().unwrap_or_default(),
        description: inv.description.clone().unwrap_or_default(),
        when_label: when_label(inv.start_ms, inv.end_ms, inv.all_day),
        start: local_text(inv.start_ms),
        end: local_text(inv.end_ms),
        all_day: inv.all_day,
        recurring: inv.rrule.is_some(),
        organizer,
        attendees: inv.attendees.iter().map(attendee_view).collect(),
        me: me.map(|m| m.email.clone()).unwrap_or_default(),
        my_status: me.map(|m| status_code(m.partstat).to_string()).unwrap_or_default(),
        headline,
        warnings: inv.warnings.clone(),
        tracked,
    }
}

// ---------------------------------------------------------------- accès au message

/// Premier calendrier iTIP du message : parties `text/calendar` ou pièces jointes `.ics`, reconnues à leur contenu.
fn invitation_of(mb: &Mailbox, id: &str) -> Result<Option<Invitation>, String> {
    let view = mb.message_view(id, false)?;
    for a in &view.attachments {
        if a.blocked || a.message {
            continue;
        }
        let data = mb.attachment(id, a.index)?;
        if data.bytes.len() > 2 * 1024 * 1024 {
            continue;
        }
        let text = String::from_utf8_lossy(&data.bytes);
        if !text.contains("BEGIN:VCALENDAR") {
            continue;
        }
        if let Ok(mut list) = itip::parse(&text) {
            // Série d'abord (occurrences modifiées ensuite) : la barre porte sur la réunion entière.
            list.sort_by_key(|i| i.recurrence_id_ms.is_some());
            return Ok(list.into_iter().next());
        }
    }
    Ok(None)
}

/// Mes adresses : celles des comptes, plus celles déduites pour la source du message.
fn my_addresses(app: &AppHandle, mb: &Mailbox, id: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Ok(conn) = data_dir(app).and_then(|d| thunderlook_store::open(&d).map_err(|e| e.to_string())) {
        if let Ok(mut stmt) = conn.prepare("SELECT lower(email) FROM account WHERE email LIKE '%@%'") {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                out.extend(rows.flatten());
            }
        }
        let n: Option<i64> = id.strip_prefix('m').and_then(|n| n.parse().ok());
        let source: Option<i64> =
            n.and_then(|n| conn.query_row("SELECT f.source_id FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1", [n], |r| r.get(0)).ok());
        if let Some(s) = source {
            out.extend(mb.my_addresses(s).unwrap_or_default());
        }
    }
    out.iter_mut().for_each(|a| *a = a.trim().to_ascii_lowercase());
    out.sort();
    out.dedup();
    out
}

/// Compte d'envoi (`s<source>`) du message, comme `compose` pour une réponse.
fn account_of_message(app: &AppHandle, id: &str) -> Option<String> {
    let conn = thunderlook_store::open(&data_dir(app).ok()?).ok()?;
    let n: i64 = id.strip_prefix('m')?.parse().ok()?;
    let s: i64 = conn.query_row("SELECT f.source_id FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1", [n], |r| r.get(0)).ok()?;
    conn.query_row("SELECT 1 FROM account WHERE source_id = ?1", [s], |_| Ok(())).ok()?;
    Some(format!("s{s}"))
}

/// Premier compte de messagerie : (source, nom, adresse) de l'organisateur.
fn default_account(app: &AppHandle) -> Result<(i64, String, String), String> {
    let conn = thunderlook_store::open(&data_dir(app)?).map_err(|e| e.to_string())?;
    conn.query_row("SELECT source_id, name, email FROM account WHERE email LIKE '%@%' ORDER BY id LIMIT 1", [], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
    })
    .map_err(|_| "Aucun compte de messagerie : ajoutez-en un (Envoyer/Recevoir › Paramètres du compte).".to_string())
}

/// Écrit le `.ics` à joindre dans `invitations/` du dossier de données ; renvoie la pièce jointe du brouillon.
fn ics_attachment(app: &AppHandle, uid: &str, kind: &str, text: &str) -> Result<DraftAttachment, String> {
    let dir = data_dir(app)?.join("invitations");
    fs::create_dir_all(&dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    let safe: String = uid.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).take(60).collect();
    let path = dir.join(format!("{kind}-{safe}-{}.ics", chrono::Utc::now().timestamp_millis()));
    fs::write(&path, text).map_err(|e| format!("{} : {e}", path.display()))?;
    let name = if kind == "reponse" { "reponse.ics" } else { "invitation.ics" };
    Ok(DraftAttachment {
        name: name.into(),
        size_label: format!("{} octets", text.len()),
        source: DraftAttachmentSource::File { path: path.to_string_lossy().into_owned() },
    })
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ---------------------------------------------------------------- commandes

/// Invitation contenue dans le message `id`, ou `None` si le message n'en porte pas.
#[tauri::command(async)]
pub fn invite_read(app: AppHandle, store: State<Store>, id: String) -> Result<Option<InviteView>, String> {
    let (inv, mine) = with_mailbox(&app, &store, |mb| Ok((invitation_of(mb, &id)?, my_addresses(&app, mb, &id))))?;
    let Some(inv) = inv else { return Ok(None) };
    let tracked = inv.method == Method::Reply && load(&app).iter().any(|m| m.uid == inv.uid);
    Ok(Some(view_of(&inv, &mine, tracked)))
}

/// Répond à l'invitation du message `id` : `answer` vaut `accepted`, `tentative` ou `declined`.
/// Le REPLY part vers l'organisateur avec le commentaire facultatif ; renvoie le message d'état.
#[tauri::command(async)]
pub fn invite_respond(app: AppHandle, store: State<Store>, id: String, answer: String, comment: String) -> Result<String, String> {
    let answer = status_of(&answer);
    if answer == PartStat::NeedsAction {
        return Err("Réponse inconnue.".into());
    }
    let from_account = account_of_message(&app, &id);
    let sent = with_mailbox(&app, &store, |mb| {
        let inv = invitation_of(mb, &id)?.ok_or("Ce message ne contient pas d'invitation.")?;
        let mine = my_addresses(&app, mb, &id);
        let me = inv
            .attendees
            .iter()
            .find(|a| mine.contains(&a.email))
            .cloned()
            .ok_or("Vous ne figurez pas parmi les participants : impossible de répondre au nom d'une autre adresse.")?;
        let (to, ics) = itip::build_reply(&inv, &me, answer, now_ms())?;
        let mut draft = mb.compose(ComposeKind::Reply, Some(&id))?;
        draft.to = to;
        draft.cc.clear();
        draft.bcc.clear();
        draft.subject = format!("{} : {}", reply_prefix(answer), inv.summary);
        let note = comment.trim();
        draft.html = if note.is_empty() { "<p></p>".into() } else { format!("<p>{}</p>", escape_html(note)) };
        draft.styles.clear();
        draft.attachments = vec![ics_attachment(&app, &inv.uid, "reponse", &ics)?];
        draft.from_account = from_account.clone();
        mb.send_draft(&draft)?;
        Ok(format!("Réponse « {} » envoyée à {}.", status_label(answer), draft.to))
    })?;
    start_sync(&app);
    Ok(sent)
}

/// Applique la réponse (REPLY) du message `id` au suivi de la réunion organisée correspondante.
#[tauri::command(async)]
pub fn invite_apply_reply(app: AppHandle, store: State<Store>, id: String) -> Result<String, String> {
    let reply = with_mailbox(&app, &store, |mb| invitation_of(mb, &id))?.ok_or("Ce message ne contient pas de réponse à une réunion.")?;
    let mut meetings = load(&app);
    let m = meetings.iter_mut().find(|m| m.uid == reply.uid).ok_or("Cette réunion n'a pas été envoyée depuis ThunderLook : rien à mettre à jour.")?;
    let mut event = meeting_invitation(m);
    let changed = itip::apply_reply(&mut event, &reply)?;
    if !changed {
        return Ok("Suivi déjà à jour.".into());
    }
    m.attendees = event.attendees.iter().map(attendee_view).collect();
    save(&app, &meetings)?;
    Ok("Suivi des réponses mis à jour.".into())
}

fn meeting_invitation(m: &Meeting) -> Invitation {
    let mut e = Invitation::new(&m.uid, &m.subject, 0, 0);
    e.method = Method::Request;
    e.sequence = m.sequence;
    e.attendees = m
        .attendees
        .iter()
        .map(|a| Person { email: a.email.clone(), name: a.name.clone(), partstat: status_of(&a.status), rsvp: true })
        .collect();
    e
}

/// Participants suivis d'un rendez-vous (`id`, sinon objet et début) ; vide s'il n'a jamais été envoyé.
#[tauri::command(async)]
pub fn invite_attendees(app: AppHandle, id: Option<String>, subject: String, start: String) -> Vec<AttendeeView> {
    let meetings = load(&app);
    find(&meetings, id.as_deref(), &subject, &start).map(|i| meetings[i].attendees.clone()).unwrap_or_default()
}

/// Envoie (ou renvoie, mise à jour) l'invitation du rendez-vous `form` aux participants `attendees`
/// (« Nom <adresse>; … »). `orig_subject` / `orig_start` : objet et début à l'ouverture de la boîte, pour retrouver la
/// réunion déjà envoyée même si l'utilisateur les a changés. Les réponses reçues sont gardées si l'heure ne change pas.
#[tauri::command(async)]
pub fn invite_send(
    app: AppHandle,
    store: State<Store>,
    form: AppointmentForm,
    attendees: String,
    orig_subject: String,
    orig_start: String,
) -> Result<String, String> {
    if form.recurring || !form.repeat.is_empty() {
        return Err("Invitations pour une série : pas encore prises en charge (envoyez un rendez-vous simple).".into());
    }
    let people = parse_people(&attendees);
    if people.is_empty() {
        return Err("Ajoutez au moins un participant (adresse de messagerie).".into());
    }
    let start_ms = parse_local(&form.start).ok_or("Début illisible.")?;
    let mut end_ms = parse_local(&form.end).ok_or("Fin illisible.")?;
    if form.all_day {
        // Journée entière : fin exclusive, au moins le lendemain du premier jour.
        end_ms = end_ms.max(start_ms + 24 * 3_600_000);
    }
    let (_, org_name, org_email) = default_account(&app)?;
    let mut meetings = load(&app);
    let found = find(&meetings, form.id.as_deref(), &orig_subject, &orig_start);
    let previous = found.map(|i| meetings[i].clone());
    let uid = previous.as_ref().map(|m| m.uid.clone()).unwrap_or_else(|| format!("{:x}-{}@thunderlook.local", now_ms(), people.len()));
    let mut inv = Invitation::new(&uid, &form.subject, start_ms, end_ms);
    inv.sequence = previous.as_ref().map(|m| m.sequence + 1).unwrap_or(0);
    inv.all_day = form.all_day;
    inv.location = Some(form.location.clone()).filter(|s| !s.trim().is_empty());
    inv.description = Some(form.body.clone()).filter(|s| !s.trim().is_empty());
    inv.organizer = Some(Person { rsvp: false, ..Person::new(&org_email, &org_name) });
    let moved = previous.as_ref().is_some_and(|m| m.start != form.start);
    inv.attendees = people
        .into_iter()
        .map(|mut p| {
            if !moved {
                if let Some(old) = previous.as_ref().and_then(|m| m.attendees.iter().find(|a| a.email == p.email)) {
                    p.partstat = status_of(&old.status);
                }
            }
            p
        })
        .collect();
    let ics = itip::build_request(&inv, now_ms())?;
    let mut draft = Draft { kind: ComposeKind::New, ..Draft::default() };
    draft.to = inv.attendees.iter().map(display).collect::<Vec<_>>().join("; ");
    draft.subject = form.subject.clone();
    let mut html = format!("<p><b>Quand :</b> {}</p>", escape_html(&when_label(start_ms, end_ms, form.all_day)));
    if let Some(l) = &inv.location {
        html.push_str(&format!("<p><b>Où :</b> {}</p>", escape_html(l)));
    }
    if let Some(d) = &inv.description {
        html.push_str(&format!("<p>{}</p>", escape_html(d)));
    }
    draft.html = html;
    draft.attachments = vec![ics_attachment(&app, &uid, "invitation", &ics)?];
    with_mailbox(&app, &store, |mb| mb.send_draft(&draft))?;
    let record = Meeting {
        appointment_id: form.id.clone().or(previous.as_ref().and_then(|m| m.appointment_id.clone())),
        uid,
        sequence: inv.sequence,
        subject: form.subject.clone(),
        start: form.start.clone(),
        attendees: inv.attendees.iter().map(attendee_view).collect(),
    };
    match found {
        Some(i) => meetings[i] = record,
        None => meetings.push(record),
    }
    save(&app, &meetings)?;
    start_sync(&app);
    let n = inv.attendees.len();
    Ok(if inv.sequence > 0 {
        format!("Mise à jour envoyée à {n} participant(s).")
    } else {
        format!("Invitation envoyée à {n} participant(s).")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const REQ: &str = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:abc\r\nSEQUENCE:0\r\n\
DTSTART:20261028T130000Z\r\nDTEND:20261028T140000Z\r\nSUMMARY:Point chantier\r\nLOCATION:Fenioux\r\n\
ORGANIZER;CN=Camille:mailto:camille@example.org\r\n\
ATTENDEE;CN=Paul;PARTSTAT=NEEDS-ACTION;RSVP=TRUE:mailto:Paul@Example.org\r\n\
ATTENDEE;PARTSTAT=ACCEPTED:mailto:luc@example.org\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn request_view_for_me() {
        let inv = itip::parse(REQ).unwrap().remove(0);
        let v = view_of(&inv, &["paul@example.org".into()], false);
        assert_eq!(v.method, "request");
        assert_eq!(v.me, "paul@example.org");
        assert_eq!(v.my_status, "needs-action");
        assert_eq!(v.when_label, "mercredi 28 octobre 2026, 14:00 – 15:00");
        assert_eq!(v.start, "2026-10-28T14:00");
        assert_eq!(v.organizer, "Camille <camille@example.org>");
        assert_eq!(v.attendees[1].status_label, "Accepté");
        assert!(v.headline.starts_with("Invitation"));
        let other = view_of(&inv, &["autre@example.org".into()], false);
        assert!(other.me.is_empty());
    }

    #[test]
    fn reply_headline() {
        let reply = REQ.replace("METHOD:REQUEST", "METHOD:REPLY").replace("PARTSTAT=NEEDS-ACTION", "PARTSTAT=DECLINED");
        let inv = itip::parse(&reply).unwrap().remove(0);
        assert_eq!(view_of(&inv, &[], true).headline, "Paul a refusé la réunion.");
    }

    #[test]
    fn labels_and_parsing() {
        let s = parse_local("2026-10-28T14:00").unwrap();
        assert_eq!(local_text(s), "2026-10-28T14:00");
        let d = parse_local("2026-12-24").unwrap();
        assert_eq!(when_label(d, d + 86_400_000, true), "jeudi 24 décembre 2026 (journée entière)");
        assert_eq!(when_label(d, d + 2 * 86_400_000, true), "du jeudi 24 décembre 2026 au vendredi 25 décembre 2026 (journées entières)");
        let p = parse_people("Camille <Camille@Example.org>; luc@example.org, pas une adresse; luc@example.org");
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].email.as_str(), p[0].name.as_str()), ("camille@example.org", "Camille"));
        assert_eq!(p[1].email, "luc@example.org");
    }

    #[test]
    fn meeting_lookup() {
        let m = |id: Option<&str>| Meeting {
            appointment_id: id.map(String::from),
            uid: "u".into(),
            sequence: 0,
            subject: "S".into(),
            start: "2026-10-28T14:00".into(),
            attendees: vec![],
        };
        let list = vec![m(Some("a1")), m(None)];
        assert_eq!(find(&list, Some("a1"), "x", "y"), Some(0));
        assert_eq!(find(&list, Some("a2"), "S", "2026-10-28T14:00"), Some(1));
        assert_eq!(find(&list, None, "S", "2026-10-28T14:00"), Some(0));
        assert_eq!(find(&list, None, "T", "2026-10-28T14:00"), None);
    }
}
