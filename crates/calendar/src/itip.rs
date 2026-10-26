//! Lot BH : cœur iTIP (RFC 5546) sans interface. Lit un `text/calendar` (METHOD REQUEST / REPLY / CANCEL, plus PUBLISH
//! et les autres reconnus mais non traités), construit REQUEST, REPLY et CANCEL, applique une réponse à la liste des
//! participants. Aucun accès à la base ni au réseau : l'envoi du courrier et l'écriture du rendez-vous sont l'affaire
//! de l'interface (lot AW).
//!
//! Heures : les dates en UTC (`…Z`) sont exactes ; une heure locale (TZID ou flottante) est lue en heure de Paris, seul
//! fuseau des données (comme `ics.rs`), et un TZID autre que Paris est signalé dans `Invitation::warnings` plutôt que
//! converti de travers. Les séries (`RRULE`) sont conservées en texte brut, sans être traduites.
//! Les écritures sont toujours en UTC, donc sans `VTIMEZONE`.

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime};

use crate::tz::TimeZone;

const TZ: TimeZone = TimeZone::PARIS;
const PRODID: &str = "-//ThunderLook//iTIP//FR";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Publish,
    Request,
    Reply,
    Cancel,
    /// ADD, REFRESH, COUNTER, DECLINECOUNTER : lus mais non traités.
    Other,
}

impl Method {
    fn name(self) -> &'static str {
        match self {
            Method::Publish => "PUBLISH",
            Method::Request => "REQUEST",
            Method::Reply => "REPLY",
            Method::Cancel => "CANCEL",
            Method::Other => "X-OTHER",
        }
    }
}

/// `PARTSTAT` d'un participant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartStat {
    NeedsAction,
    Accepted,
    Declined,
    Tentative,
}

impl PartStat {
    fn name(self) -> &'static str {
        match self {
            PartStat::NeedsAction => "NEEDS-ACTION",
            PartStat::Accepted => "ACCEPTED",
            PartStat::Declined => "DECLINED",
            PartStat::Tentative => "TENTATIVE",
        }
    }

    fn from_name(s: &str) -> PartStat {
        match s.to_ascii_uppercase().as_str() {
            "ACCEPTED" => PartStat::Accepted,
            "DECLINED" => PartStat::Declined,
            "TENTATIVE" => PartStat::Tentative,
            _ => PartStat::NeedsAction,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    /// Adresse sans `mailto:`, en minuscules.
    pub email: String,
    pub name: String,
    pub partstat: PartStat,
    /// `RSVP=TRUE` : une réponse est attendue.
    pub rsvp: bool,
}

impl Person {
    pub fn new(email: &str, name: &str) -> Person {
        Person { email: email.trim().to_ascii_lowercase(), name: name.to_string(), partstat: PartStat::NeedsAction, rsvp: true }
    }
}

/// Un VEVENT d'un message iTIP (ou à écrire dans un message iTIP).
#[derive(Clone, Debug)]
pub struct Invitation {
    pub method: Method,
    pub uid: String,
    pub sequence: i64,
    pub summary: String,
    pub location: Option<String>,
    pub description: Option<String>,
    /// Début et fin en millisecondes Unix (UTC), même convention que `Appointment` ; pour une journée entière, minuit de Paris.
    pub start_ms: i64,
    pub end_ms: i64,
    pub all_day: bool,
    /// Horodatage du message (`DTSTAMP`), pour départager deux messages de même `SEQUENCE`.
    pub dtstamp_ms: Option<i64>,
    pub organizer: Option<Person>,
    pub attendees: Vec<Person>,
    /// `RRULE` brute (sans traduction) ; `None` pour un événement simple.
    pub rrule: Option<String>,
    /// `RECURRENCE-ID` : le message concerne UNE occurrence de la série `uid`.
    pub recurrence_id_ms: Option<i64>,
    /// Pertes et approximations de la lecture (TZID inconnu, date illisible…), à montrer à l'utilisateur.
    pub warnings: Vec<String>,
}

impl Invitation {
    /// Un événement neuf (méthode REQUEST) ; l'appelant renseigne le reste.
    pub fn new(uid: &str, summary: &str, start_ms: i64, end_ms: i64) -> Invitation {
        Invitation {
            method: Method::Request,
            uid: uid.to_string(),
            sequence: 0,
            summary: summary.to_string(),
            location: None,
            description: None,
            start_ms,
            end_ms,
            all_day: false,
            dtstamp_ms: None,
            organizer: None,
            attendees: Vec::new(),
            rrule: None,
            recurrence_id_ms: None,
            warnings: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------- lecture

struct Prop {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl Prop {
    fn param(&self, n: &str) -> Option<&str> {
        self.params.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str())
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(x) => out.push(x),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace("\r\n", "\\n").replace('\n', "\\n")
}

/// Découpe `NOM;PARAM=v;PARAM="a:b":valeur` (le premier `:` hors guillemets sépare la valeur).
fn parse_line(l: &str) -> Option<Prop> {
    let mut quoted = false;
    let (cut, _) = l.char_indices().find(|(_, c)| {
        if *c == '"' {
            quoted = !quoted;
        }
        *c == ':' && !quoted
    })?;
    let (head, value) = (&l[..cut], &l[cut + 1..]);
    // Les `;` entre guillemets ne séparent pas les paramètres.
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut q = false;
    for c in head.chars() {
        match c {
            '"' => {
                q = !q;
                cur.push(c);
            }
            ';' if !q => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    parts.push(cur);
    let name = parts.remove(0).to_ascii_uppercase();
    let params = parts
        .iter()
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.to_ascii_uppercase(), v.trim_matches('"').to_string()))
        .collect();
    Some(Prop { name, params, value: value.to_string() })
}

fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for l in text.lines() {
        match l.strip_prefix([' ', '\t']) {
            Some(rest) if !lines.is_empty() => lines.last_mut().unwrap().push_str(rest),
            _ => lines.push(l.trim_end_matches('\r').to_string()),
        }
    }
    lines
}

fn utc_ms(local: NaiveDateTime) -> i64 {
    TZ.to_utc(local).and_utc().timestamp_millis()
}

/// (instant en ms UTC, journée entière). `warnings` reçoit les approximations.
fn time_of(p: &Prop, warnings: &mut Vec<String>) -> Option<(i64, bool)> {
    let v = p.value.split(',').next()?.trim();
    if p.param("VALUE").is_some_and(|x| x.eq_ignore_ascii_case("DATE")) || v.len() == 8 {
        let d = NaiveDate::parse_from_str(v, "%Y%m%d").ok()?;
        return Some((utc_ms(d.and_hms_opt(0, 0, 0)?), true));
    }
    let t = NaiveDateTime::parse_from_str(v.trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()?;
    if v.ends_with('Z') {
        return Some((t.and_utc().timestamp_millis(), false));
    }
    if let Some(tzid) = p.param("TZID") {
        let paris = ["romance standard time", "europe/paris"];
        if !paris.contains(&tzid.to_ascii_lowercase().as_str()) {
            let w = format!("fuseau « {tzid} » non géré : heure lue comme heure de Paris");
            if !warnings.contains(&w) {
                warnings.push(w);
            }
        }
    }
    Some((utc_ms(t), false))
}

fn duration_ms(v: &str) -> Option<i64> {
    let (neg, v) = match v.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, v.strip_prefix('+').unwrap_or(v)),
    };
    let v = v.strip_prefix('P')?;
    let (mut total, mut num, mut in_time) = (Duration::zero(), String::new(), false);
    for c in v.chars() {
        if c == 'T' {
            in_time = true;
        } else if c.is_ascii_digit() {
            num.push(c);
        } else {
            let n: i64 = num.parse().ok()?;
            num.clear();
            total += match (c, in_time) {
                ('W', false) => Duration::weeks(n),
                ('D', false) => Duration::days(n),
                ('H', true) => Duration::hours(n),
                ('M', true) => Duration::minutes(n),
                ('S', true) => Duration::seconds(n),
                _ => return None,
            };
        }
    }
    Some(if neg { -total } else { total }.num_milliseconds())
}

fn address(v: &str) -> String {
    let v = v.trim();
    let v = if v.len() >= 7 && v[..7].eq_ignore_ascii_case("mailto:") { &v[7..] } else { v };
    v.trim().to_ascii_lowercase()
}

fn person(p: &Prop) -> Person {
    Person {
        email: address(&p.value),
        name: p.param("CN").map(str::to_string).unwrap_or_default(),
        partstat: p.param("PARTSTAT").map(PartStat::from_name).unwrap_or(PartStat::NeedsAction),
        rsvp: p.param("RSVP").is_some_and(|v| v.eq_ignore_ascii_case("TRUE")),
    }
}

/// Lit un `text/calendar` : un `Invitation` par VEVENT (série et occurrences modifiées séparées, distinguées par
/// `recurrence_id_ms`). Erreur si le texte n'a ni `BEGIN:VCALENDAR` ni VEVENT exploitable.
pub fn parse(text: &str) -> Result<Vec<Invitation>, String> {
    let lines = unfold(text);
    if !lines.iter().any(|l| l.trim().eq_ignore_ascii_case("BEGIN:VCALENDAR")) {
        return Err("pas un calendrier iCalendar (BEGIN:VCALENDAR absent)".into());
    }
    let mut method = Method::Publish;
    let mut depth_event = 0usize;
    let mut cur: Vec<Prop> = Vec::new();
    let mut events: Vec<Vec<Prop>> = Vec::new();
    for l in &lines {
        let Some(p) = parse_line(l) else { continue };
        match (p.name.as_str(), p.value.to_ascii_uppercase().as_str()) {
            ("BEGIN", "VEVENT") => {
                depth_event = 1;
                cur.clear();
            }
            ("END", "VEVENT") => {
                depth_event = 0;
                events.push(std::mem::take(&mut cur));
            }
            // Sous-composants du VEVENT (VALARM) : leurs propriétés ne se mélangent pas à celles de l'événement.
            ("BEGIN", _) if depth_event > 0 => depth_event += 1,
            ("END", _) if depth_event > 1 => depth_event -= 1,
            ("METHOD", m) if depth_event == 0 => {
                method = match m {
                    "REQUEST" => Method::Request,
                    "REPLY" => Method::Reply,
                    "CANCEL" => Method::Cancel,
                    "PUBLISH" => Method::Publish,
                    _ => Method::Other,
                }
            }
            _ if depth_event == 1 => cur.push(p),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for props in events {
        let mut warnings = Vec::new();
        let get = |n: &str| props.iter().find(|p| p.name == n);
        let Some(uid) = get("UID").map(|p| p.value.trim().to_string()).filter(|u| !u.is_empty()) else {
            continue;
        };
        let Some((start_ms, all_day)) = get("DTSTART").and_then(|p| time_of(p, &mut warnings)) else {
            continue;
        };
        let end_ms = match (get("DTEND").and_then(|p| time_of(p, &mut warnings)), get("DURATION").and_then(|p| duration_ms(&p.value))) {
            (Some((e, _)), _) => e,
            (None, Some(d)) => start_ms + d,
            (None, None) if all_day => start_ms + 24 * 3_600_000,
            _ => start_ms,
        };
        let text = |n: &str| get(n).map(|p| unescape(&p.value)).filter(|s| !s.trim().is_empty());
        out.push(Invitation {
            method,
            uid,
            sequence: get("SEQUENCE").and_then(|p| p.value.trim().parse().ok()).unwrap_or(0),
            summary: text("SUMMARY").unwrap_or_default(),
            location: text("LOCATION"),
            description: text("DESCRIPTION"),
            start_ms,
            end_ms,
            all_day,
            dtstamp_ms: get("DTSTAMP").and_then(|p| time_of(p, &mut Vec::new())).map(|t| t.0),
            organizer: get("ORGANIZER").map(person),
            attendees: props.iter().filter(|p| p.name == "ATTENDEE").map(person).collect(),
            rrule: get("RRULE").map(|p| p.value.trim().to_string()),
            recurrence_id_ms: get("RECURRENCE-ID").and_then(|p| time_of(p, &mut warnings)).map(|t| t.0),
            warnings,
        });
    }
    if out.is_empty() {
        return Err("aucun VEVENT exploitable (UID ou DTSTART manquant)".into());
    }
    Ok(out)
}

// ---------------------------------------------------------------- écriture

/// Ligne repliée à 75 octets (§3.1), sans couper un caractère.
fn fold(out: &mut String, line: &str) {
    let mut start = 0;
    let mut first = true;
    while start < line.len() {
        let room = if first { 75 } else { 74 };
        let mut end = (start + room).min(line.len());
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        if !first {
            out.push(' ');
        }
        out.push_str(&line[start..end]);
        out.push_str("\r\n");
        start = end;
        first = false;
    }
    if line.is_empty() {
        out.push_str("\r\n");
    }
}

fn utc_text(ms: i64) -> String {
    DateTime::from_timestamp_millis(ms).unwrap_or_default().format("%Y%m%dT%H%M%SZ").to_string()
}

fn date_text(ms: i64) -> String {
    TZ.to_local(DateTime::from_timestamp_millis(ms).unwrap_or_default().naive_utc()).format("%Y%m%d").to_string()
}

fn quoted(s: &str) -> String {
    s.replace('"', "'")
}

fn person_line(out: &mut String, name: &str, who: &Person, with_state: bool) {
    let mut l = String::from(name);
    if !who.name.is_empty() {
        l.push_str(&format!(";CN=\"{}\"", quoted(&who.name)));
    }
    if with_state {
        l.push_str(&format!(";PARTSTAT={}", who.partstat.name()));
        if who.rsvp {
            l.push_str(";RSVP=TRUE");
        }
    }
    l.push_str(&format!(":mailto:{}", who.email));
    fold(out, &l);
}

fn calendar(method: Method, body: &str) -> String {
    format!("BEGIN:VCALENDAR\r\nPRODID:{PRODID}\r\nVERSION:2.0\r\nCALSCALE:GREGORIAN\r\nMETHOD:{}\r\n{body}END:VCALENDAR\r\n", method.name())
}

/// Corps d'un VEVENT commun aux trois méthodes. `with_times`/`with_text` sont faux pour un REPLY minimal.
fn event(e: &Invitation, status: &str, stamp_ms: i64, full: bool) -> String {
    let mut s = String::from("BEGIN:VEVENT\r\n");
    fold(&mut s, &format!("UID:{}", e.uid));
    fold(&mut s, &format!("DTSTAMP:{}", utc_text(stamp_ms)));
    fold(&mut s, &format!("SEQUENCE:{}", e.sequence));
    if let Some(r) = e.recurrence_id_ms {
        if e.all_day {
            fold(&mut s, &format!("RECURRENCE-ID;VALUE=DATE:{}", date_text(r)));
        } else {
            fold(&mut s, &format!("RECURRENCE-ID:{}", utc_text(r)));
        }
    }
    if full {
        if e.all_day {
            fold(&mut s, &format!("DTSTART;VALUE=DATE:{}", date_text(e.start_ms)));
            fold(&mut s, &format!("DTEND;VALUE=DATE:{}", date_text(e.end_ms)));
        } else {
            fold(&mut s, &format!("DTSTART:{}", utc_text(e.start_ms)));
            fold(&mut s, &format!("DTEND:{}", utc_text(e.end_ms)));
        }
        if let Some(r) = &e.rrule {
            fold(&mut s, &format!("RRULE:{r}"));
        }
    }
    fold(&mut s, &format!("SUMMARY:{}", escape(&e.summary)));
    if full {
        if let Some(l) = &e.location {
            fold(&mut s, &format!("LOCATION:{}", escape(l)));
        }
        if let Some(d) = &e.description {
            fold(&mut s, &format!("DESCRIPTION:{}", escape(d)));
        }
    }
    if let Some(o) = &e.organizer {
        person_line(&mut s, "ORGANIZER", o, false);
    }
    fold(&mut s, &format!("STATUS:{status}"));
    s
}

/// REQUEST (invitation ou mise à jour) : l'organisateur est obligatoire, au moins un participant aussi.
/// `stamp_ms` est l'instant d'envoi (fourni par l'appelant : le module ne lit pas l'horloge, pour rester testable).
pub fn build_request(e: &Invitation, stamp_ms: i64) -> Result<String, String> {
    if e.organizer.is_none() {
        return Err("organisateur manquant".into());
    }
    if e.attendees.is_empty() {
        return Err("aucun participant".into());
    }
    if e.end_ms < e.start_ms {
        return Err("la fin précède le début".into());
    }
    let mut s = event(e, "CONFIRMED", stamp_ms, true);
    for a in &e.attendees {
        person_line(&mut s, "ATTENDEE;ROLE=REQ-PARTICIPANT", a, true);
    }
    s.push_str("END:VEVENT\r\n");
    Ok(calendar(Method::Request, &s))
}

/// CANCEL de l'événement (ou de l'occurrence `recurrence_id_ms`) ; `SEQUENCE` augmenté d'un cran, comme l'exige iTIP.
pub fn build_cancel(e: &Invitation, stamp_ms: i64) -> Result<String, String> {
    if e.organizer.is_none() {
        return Err("organisateur manquant".into());
    }
    let mut c = e.clone();
    c.sequence += 1;
    let mut s = event(&c, "CANCELLED", stamp_ms, true);
    for a in &c.attendees {
        person_line(&mut s, "ATTENDEE", a, false);
    }
    s.push_str("END:VEVENT\r\n");
    Ok(calendar(Method::Cancel, &s))
}

/// REPLY d'un participant à une REQUEST reçue. `me` doit figurer parmi les participants (comparaison d'adresses,
/// casse ignorée) : on ne répond pas pour quelqu'un qui n'est pas invité. Le REPLY ne porte que `me`.
/// Renvoie (adresse du destinataire = l'organisateur, texte `text/calendar`).
pub fn build_reply(request: &Invitation, me: &Person, answer: PartStat, stamp_ms: i64) -> Result<(String, String), String> {
    if request.method != Method::Request {
        return Err("on ne répond qu'à une invitation (METHOD:REQUEST)".into());
    }
    if answer == PartStat::NeedsAction {
        return Err("réponse « sans réponse » impossible".into());
    }
    let Some(org) = &request.organizer else {
        return Err("organisateur manquant : personne à qui répondre".into());
    };
    let mine = me.email.trim().to_ascii_lowercase();
    if !request.attendees.iter().any(|a| a.email == mine) {
        return Err(format!("{mine} ne figure pas parmi les participants"));
    }
    let mut r = request.clone();
    r.method = Method::Reply;
    let mut s = event(&r, "CONFIRMED", stamp_ms, false);
    // STATUS n'a pas de sens dans un REPLY : on le retire.
    s = s.replace("STATUS:CONFIRMED\r\n", "");
    let me = Person { email: mine.clone(), name: me.name.clone(), partstat: answer, rsvp: false };
    person_line(&mut s, "ATTENDEE", &me, true);
    s.push_str("END:VEVENT\r\n");
    Ok((org.email.clone(), calendar(Method::Reply, &s)))
}

// ---------------------------------------------------------------- application

/// Quoi faire d'une REQUEST reçue face à ce qu'on connaît déjà de l'événement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Incoming {
    /// Pas encore connu : créer.
    New,
    /// Connu, version plus récente : remplacer (les réponses données sont à redemander si l'heure a changé).
    Update,
    /// Connu, même version ou plus ancienne : ignorer.
    Stale,
}

/// Compare un message reçu à la version connue (`SEQUENCE`, puis `DTSTAMP`, §2.1.4 de la RFC 5546).
pub fn classify(known: Option<&Invitation>, received: &Invitation) -> Incoming {
    let Some(k) = known else { return Incoming::New };
    match received.sequence.cmp(&k.sequence) {
        std::cmp::Ordering::Greater => Incoming::Update,
        std::cmp::Ordering::Less => Incoming::Stale,
        std::cmp::Ordering::Equal => match (received.dtstamp_ms, k.dtstamp_ms) {
            (Some(r), Some(o)) if r > o => Incoming::Update,
            _ => Incoming::Stale,
        },
    }
}

/// Une mise à jour change-t-elle le moment ou le lieu (donc invalide les réponses déjà données) ?
pub fn reschedules(old: &Invitation, new: &Invitation) -> bool {
    old.start_ms != new.start_ms || old.end_ms != new.end_ms || old.all_day != new.all_day || old.rrule != new.rrule || old.location != new.location
}

/// Applique un REPLY reçu à l'événement de l'organisateur : met à jour le `PARTSTAT` du participant correspondant.
/// Erreurs : même UID requis, REPLY d'un inconnu refusé (rien n'est ajouté, un REPLY ne peut pas inviter),
/// réponse portant sur une `SEQUENCE` plus ancienne que la nôtre ignorée (`Ok(false)`). `Ok(true)` si modifié.
pub fn apply_reply(event: &mut Invitation, reply: &Invitation) -> Result<bool, String> {
    if reply.method != Method::Reply {
        return Err("ce n'est pas un REPLY".into());
    }
    if reply.uid != event.uid || reply.recurrence_id_ms != event.recurrence_id_ms {
        return Err("le REPLY concerne un autre événement".into());
    }
    if reply.sequence < event.sequence {
        return Ok(false);
    }
    let mut changed = false;
    for r in &reply.attendees {
        let Some(a) = event.attendees.iter_mut().find(|a| a.email == r.email) else {
            return Err(format!("{} ne figure pas parmi les participants de l'événement", r.email));
        };
        if a.partstat != r.partstat {
            a.partstat = r.partstat;
            changed = true;
        }
        a.rsvp = false;
    }
    Ok(changed)
}

/// Le message de l'organisateur concerne-t-il l'événement connu ? (même UID, même occurrence)
pub fn same_event(a: &Invitation, b: &Invitation) -> bool {
    a.uid == b.uid && a.recurrence_id_ms == b.recurrence_id_ms
}
