//! Séries (RRULE) d'un `.ics` venu du téléphone → rendez-vous périodiques de ThunderLook (lot AA).
//!
//! [`split`] sépare le fichier : les VEVENT simples (rendus tels quels pour `ics::parse`) et, par UID, les séries (VEVENT
//! maître à RRULE + occurrences modifiées à RECURRENCE-ID). Une série est traduite en périodicité [MS-OXOCAL] (la même
//! que celle qu'Outlook écrit, donc l'expansion vérifiée contre Outlook s'applique). Ce que le modèle ne sait pas
//! représenter n'est JAMAIS importé de travers : la série est refusée ([`Rejected`], avec la raison) ou, quand seule
//! l'exactitude d'un cas limite est en jeu, marquée [`Series::inexact`] (à n'accepter que si elle est identique à ce que
//! ThunderLook a exporté).
//!
//! Limites du modèle (les refus) : fréquences autres que DAILY/WEEKLY/MONTHLY/YEARLY, `BYxxx` hors `BYDAY`/`BYMONTHDAY`/
//! `BYSETPOS`/`BYMONTH` (un seul mois, un seul jour du mois), `BYDAY` sans rang en mensuel, rang hors 1-4 / -1, `RDATE`,
//! `RANGE=THISANDFUTURE`, fuseau autre que l'heure de Paris, série à heure fixe en UTC qui traverse un changement
//! d'heure (ThunderLook répète à heure LOCALE fixe), occurrence modifiée dont la description, les catégories ou la
//! confidentialité diffèrent de la série. Cas inexacts : jour 29-31 du mois (RFC 5545 saute les mois trop courts,
//! Outlook et ThunderLook prennent le dernier jour) et 29 février annuel (idem : 28 février les années non bissextiles).

use std::collections::{BTreeSet, HashMap};

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};

use crate::{
    Appointment, Busy, expand, ics,
    recur::{End, Exception, Frequency, Pattern, Recurrence, local_to_minutes},
    tz::TimeZone,
};

const TZ: TimeZone = TimeZone::PARIS;
/// `EndDate` d'une série sans fin (31/12/4500), comme Outlook.
const NEVER_END: u32 = 0x5AE9_80DF;
const DAYS: [&str; 7] = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"];
/// Fuseaux qui ont, pour ThunderLook, les règles de l'heure de Paris.
const PARIS_ZONES: [&str; 14] = [
    "europe/paris",
    "europe/brussels",
    "europe/madrid",
    "europe/berlin",
    "europe/rome",
    "europe/amsterdam",
    "europe/luxembourg",
    "europe/zurich",
    "europe/vienna",
    "romance standard time",
    "w. europe standard time",
    "central europe standard time",
    "central european standard time",
    "cet",
];

/// Une série lue du fichier.
#[derive(Clone, Debug)]
pub struct Series {
    pub uid: String,
    /// Rendez-vous maître : `recurrence` renseignée (exceptions et dates supprimées comprises) ; début et fin : ceux de la
    /// première occurrence.
    pub appointment: Appointment,
    /// Le VCALENDAR de la série seule (maître + occurrences modifiées), relisible par [`parse_series`].
    pub ics: String,
    /// Cas limite où la lecture de ThunderLook diffère de la RFC 5545 : ne pas importer tel quel (raison).
    pub inexact: Option<String>,
}

/// Une série refusée, et pourquoi (affiché tel quel à l'utilisateur).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rejected {
    pub uid: String,
    pub subject: String,
    pub reason: String,
}

pub struct Split {
    /// Le fichier sans les séries ni leurs occurrences modifiées : pour `ics::parse`.
    pub rest: String,
    pub series: Vec<Series>,
    pub rejected: Vec<Rejected>,
}

// --- Lignes -----------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Prop {
    name: String,
    params: HashMap<String, String>,
    value: String,
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

fn prop_of(l: &str) -> Option<Prop> {
    let mut quoted = false;
    let split = l.char_indices().find(|(_, c)| {
        if *c == '"' {
            quoted = !quoted;
        }
        *c == ':' && !quoted
    })?;
    let (head, value) = (&l[..split.0], &l[split.0 + 1..]);
    let mut parts = head.split(';');
    let name = parts.next()?.to_ascii_uppercase();
    let params = parts.filter_map(|p| p.split_once('=')).map(|(k, v)| (k.to_ascii_uppercase(), v.trim_matches('"').to_string())).collect();
    Some(Prop { name, params, value: value.to_string() })
}

struct Item {
    raw: String,
    /// Propriété du VEVENT lui-même (pas celles d'un VALARM).
    prop: Option<Prop>,
}

struct Block {
    start: usize,
    end: usize,
    items: Vec<Item>,
}

impl Block {
    fn get<'a>(&'a self, name: &'a str) -> Option<&'a Prop> {
        self.all(name).next()
    }
    fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Prop> {
        self.items.iter().filter_map(|i| i.prop.as_ref()).filter(move |p| p.name == name)
    }
    fn has(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
    fn uid(&self) -> String {
        self.get("UID").map(|p| p.value.clone()).unwrap_or_default()
    }
    fn subject(&self) -> String {
        self.get("SUMMARY").map(|p| plain(&p.value)).unwrap_or_default()
    }
    /// Le VEVENT en VCALENDAR minimal, sans les propriétés `skip`.
    fn text(&self, skip: &[&str]) -> String {
        let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\n");
        for i in &self.items {
            if i.prop.as_ref().is_some_and(|p| skip.contains(&p.name.as_str())) {
                continue;
            }
            out.push_str(&i.raw);
            out.push_str("\r\n");
        }
        out.push_str("END:VEVENT\r\nEND:VCALENDAR\r\n");
        out
    }
}

fn plain(s: &str) -> String {
    s.replace("\\n", " ").replace("\\N", " ").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\")
}

fn blocks(lines: &[String]) -> Vec<Block> {
    let mut out = Vec::new();
    let mut cur: Option<(usize, usize, Vec<Item>)> = None;
    for (i, l) in lines.iter().enumerate() {
        let up = l.to_ascii_uppercase();
        match cur.take() {
            None => {
                if up == "BEGIN:VEVENT" {
                    cur = Some((i, 0, Vec::new()));
                }
            }
            Some((start, depth, mut items)) => {
                if up == "END:VEVENT" && depth == 0 {
                    out.push(Block { start, end: i, items });
                    continue;
                }
                let marker = up.starts_with("BEGIN:") || up.starts_with("END:");
                items.push(Item { raw: l.clone(), prop: if depth == 0 && !marker { prop_of(l) } else { None } });
                let depth = if up.starts_with("BEGIN:") { depth + 1 } else if up.starts_with("END:") { depth.saturating_sub(1) } else { depth };
                cur = Some((start, depth, items));
            }
        }
    }
    out
}

// --- Séparation -------------------------------------------------------------------------------------------------------

/// Sépare séries et rendez-vous simples. N'échoue jamais : ce qui n'est pas lisible est dans `rejected`.
pub fn split(text: &str) -> Split {
    let lines = unfold(text);
    let all = blocks(&lines);
    let series_uids: BTreeSet<String> = all.iter().filter(|b| b.has("RRULE") && !b.has("RECURRENCE-ID")).map(Block::uid).collect();
    let mut removed = vec![false; lines.len()];
    let mut groups: Vec<(String, Vec<&Block>)> = Vec::new();
    let (mut series, mut rejected) = (Vec::new(), Vec::new());
    for b in &all {
        let uid = b.uid();
        if series_uids.contains(&uid) {
            removed[b.start..=b.end].iter_mut().for_each(|r| *r = true);
            match groups.iter_mut().find(|(u, _)| *u == uid) {
                Some((_, g)) => g.push(b),
                None => groups.push((uid, vec![b])),
            }
        } else if b.has("RECURRENCE-ID") {
            removed[b.start..=b.end].iter_mut().for_each(|r| *r = true);
            rejected.push(Rejected { uid, subject: b.subject(), reason: "occurrence modifiée dont la série est absente du fichier".into() });
        }
    }
    for (uid, group) in groups {
        let subject = group.iter().find(|b| !b.has("RECURRENCE-ID")).map(|b| b.subject()).unwrap_or_default();
        match build(&uid, &group) {
            Ok(s) => series.push(s),
            Err(reason) => rejected.push(Rejected { uid, subject, reason }),
        }
    }
    let mut rest = String::new();
    for (i, l) in lines.iter().enumerate() {
        if !removed[i] {
            rest.push_str(l);
            rest.push_str("\r\n");
        }
    }
    Split { rest, series, rejected }
}

/// Relit une série seule (le champ `ics` d'une [`Series`], ou l'export d'un rendez-vous périodique).
pub fn parse_series(text: &str) -> Result<Series, String> {
    let mut s = split(text);
    if let Some(r) = s.rejected.into_iter().next() {
        return Err(r.reason);
    }
    if s.series.len() == 1 { Ok(s.series.remove(0)) } else { Err(format!("{} série(s) au lieu d'une", s.series.len())) }
}

// --- Dates ------------------------------------------------------------------------------------------------------------

struct Dt {
    local: NaiveDateTime,
    utc: bool,
    date_only: bool,
}

/// DATE ou DATE-TIME (`Z` : UTC, converti en heure de Paris ; sinon heure locale / flottante).
fn dt(v: &str) -> Option<Dt> {
    let v = v.trim();
    if v.len() == 8 {
        return Some(Dt { local: NaiveDate::parse_from_str(v, "%Y%m%d").ok()?.and_hms_opt(0, 0, 0)?, utc: false, date_only: true });
    }
    let t = NaiveDateTime::parse_from_str(v.trim_end_matches(['Z', 'z']), "%Y%m%dT%H%M%S").ok()?;
    let utc = v.ends_with(['Z', 'z']);
    Some(Dt { local: if utc { TZ.to_local(t) } else { t }, utc, date_only: false })
}

fn midnight(d: NaiveDate) -> u32 {
    local_to_minutes(d.and_hms_opt(0, 0, 0).unwrap_or_default())
}

fn at(d: NaiveDate, minutes: u32) -> NaiveDateTime {
    d.and_hms_opt(0, 0, 0).unwrap_or_default() + Duration::minutes(i64::from(minutes))
}

fn utc_ms(local: NaiveDateTime) -> i64 {
    TZ.to_utc(local).and_utc().timestamp_millis()
}

fn local_of(ms: i64) -> NaiveDateTime {
    TZ.to_local(chrono::DateTime::from_timestamp_millis(ms).unwrap_or_default().naive_utc())
}

fn day_of(m: u32) -> NaiveDate {
    crate::recur::minutes_to_local(m).date()
}

// --- RRULE → périodicité ----------------------------------------------------------------------------------------------

fn is_occurrence(r: &Recurrence, d: NaiveDate) -> bool {
    expand::dates(r, d, d).contains(&d)
}

fn nth(n: i32) -> Result<u32, String> {
    match n {
        1..=4 => Ok(n as u32),
        -1 => Ok(5),
        _ => Err(format!("rang {n} d'un jour de la semaine non pris en charge (1 à 4, ou -1)")),
    }
}

/// `MO`, `2MO`, `-1FR` → (rang, bit du jour). Bit 0 = dimanche.
fn byday(item: &str) -> Result<(i32, u32), String> {
    let split = item.len().checked_sub(2).filter(|i| item.is_char_boundary(*i)).ok_or_else(|| format!("BYDAY={item} illisible"))?;
    let (n, d) = item.split_at(split);
    let bit = DAYS.iter().position(|x| *x == d).map(|i| 1u32 << i).ok_or_else(|| format!("BYDAY={item} illisible"))?;
    let n = if n.is_empty() { 0 } else { n.parse::<i32>().map_err(|_| format!("BYDAY={item} illisible"))? };
    Ok((n, bit))
}

/// Périodicité d'une RRULE. `start`/`end` : première occurrence (DTSTART/DTEND) en heure locale. Rend aussi l'avertissement
/// d'inexactitude (voir le module).
fn recurrence(rule: &str, start: NaiveDateTime, end: NaiveDateTime) -> Result<(Recurrence, Option<String>), String> {
    let mut kv: HashMap<String, String> = HashMap::new();
    for part in rule.split(';').filter(|p| !p.trim().is_empty()) {
        let (k, v) = part.split_once('=').ok_or_else(|| format!("RRULE illisible : {part}"))?;
        if kv.insert(k.to_ascii_uppercase(), v.to_ascii_uppercase()).is_some() {
            return Err(format!("RRULE : {k} en double"));
        }
    }
    let freq = kv.get("FREQ").cloned().ok_or("RRULE sans FREQ")?;
    let allowed: &[&str] = match freq.as_str() {
        "DAILY" => &["FREQ", "INTERVAL", "COUNT", "UNTIL", "WKST"],
        "WEEKLY" => &["FREQ", "INTERVAL", "COUNT", "UNTIL", "WKST", "BYDAY"],
        "MONTHLY" => &["FREQ", "INTERVAL", "COUNT", "UNTIL", "WKST", "BYDAY", "BYMONTHDAY", "BYSETPOS"],
        "YEARLY" => &["FREQ", "INTERVAL", "COUNT", "UNTIL", "WKST", "BYDAY", "BYMONTHDAY", "BYSETPOS", "BYMONTH"],
        f => return Err(format!("fréquence {f} non prise en charge (jour, semaine, mois, an seulement)")),
    };
    if let Some(k) = kv.keys().filter(|k| !allowed.contains(&k.as_str())).min() {
        return Err(format!("paramètre {k} de la RRULE non pris en charge avec FREQ={freq}"));
    }
    if kv.contains_key("COUNT") && kv.contains_key("UNTIL") {
        return Err("COUNT et UNTIL ensemble".into());
    }
    let single = |name: &str| -> Result<Option<i32>, String> {
        match kv.get(name) {
            None => Ok(None),
            Some(v) if v.contains(',') => Err(format!("plusieurs valeurs de {name} non prises en charge")),
            Some(v) => v.parse::<i32>().map(Some).map_err(|_| format!("{name}={v} illisible")),
        }
    };
    let interval = match kv.get("INTERVAL") {
        None => 1,
        Some(v) => v.parse::<u32>().ok().filter(|n| (1..=999).contains(n)).ok_or_else(|| format!("INTERVAL={v} non pris en charge (1 à 999)"))?,
    };
    let wkst = match kv.get("WKST") {
        None => 1,
        Some(v) => DAYS.iter().position(|x| x == v).ok_or_else(|| format!("WKST={v} illisible"))? as u32,
    };
    let days: Vec<(i32, u32)> = match kv.get("BYDAY") {
        None => Vec::new(),
        Some(v) => v.split(',').map(byday).collect::<Result<_, _>>()?,
    };
    let mask = days.iter().fold(0u32, |a, (_, b)| a | b);
    let mut inexact = None;
    let (frequency, pattern, period, month) = match freq.as_str() {
        "DAILY" => (Frequency::Daily, Pattern::Day, interval * 1_440, start.month()),
        "WEEKLY" => {
            if days.iter().any(|(n, _)| *n != 0) {
                return Err("BYDAY avec rang en hebdomadaire non pris en charge".into());
            }
            (Frequency::Weekly, Pattern::Week { days: if mask == 0 { 1 << start.weekday().num_days_from_sunday() } else { mask } }, interval, start.month())
        }
        _ => {
            let yearly = freq == "YEARLY";
            let month = match single("BYMONTH")? {
                Some(m) if (1..=12).contains(&m) => m as u32,
                Some(m) => return Err(format!("BYMONTH={m} illisible")),
                None => start.month(),
            };
            let setpos = single("BYSETPOS")?;
            let monthday = single("BYMONTHDAY")?;
            let pattern = match (monthday, days.is_empty()) {
                (Some(-1), true) => Pattern::MonthEnd,
                (Some(d), true) if (1..=31).contains(&d) => Pattern::Month { day: d as u32 },
                (Some(d), true) => return Err(format!("BYMONTHDAY={d} non pris en charge (1 à 31, ou -1 pour le dernier jour)")),
                (Some(_), false) => return Err("BYMONTHDAY avec BYDAY non pris en charge".into()),
                (None, false) => {
                    if days.len() == 1 && days[0].0 != 0 && setpos.is_none() {
                        Pattern::MonthNth { days: mask, nth: nth(days[0].0)? }
                    } else if let (Some(p), true) = (setpos, days.iter().all(|(n, _)| *n == 0)) {
                        Pattern::MonthNth { days: mask, nth: nth(p)? }
                    } else {
                        return Err("BYDAY sans rang unique (« tous les lundis du mois », rangs multiples) non pris en charge".into());
                    }
                }
                (None, true) if setpos.is_some() => return Err("BYSETPOS sans BYDAY".into()),
                (None, true) => Pattern::Month { day: start.day() },
            };
            if let Pattern::Month { day } = pattern {
                if yearly && month == 2 && day == 29 {
                    inexact = Some("29 février : la RFC 5545 saute les années non bissextiles, ThunderLook tombe le 28 février".to_string());
                } else if !yearly && day >= 29 {
                    inexact = Some(format!("jour {day} du mois : la RFC 5545 saute les mois trop courts, ThunderLook tombe le dernier jour du mois (utiliser « dernier jour du mois » sur le téléphone)"));
                }
            }
            if yearly { (Frequency::Yearly, pattern, interval * 12, month) } else { (Frequency::Monthly, pattern, interval, start.month()) }
        }
    };
    let start_offset = start.hour() * 60 + start.minute();
    let mut r = Recurrence {
        frequency,
        pattern,
        calendar_type: 0,
        first_date_time: 0,
        period,
        end: End::Never,
        occurrences: 0,
        first_day_of_week: wkst,
        deleted: Vec::new(),
        modified: Vec::new(),
        start_date: midnight(start.date()),
        end_date: NEVER_END,
        start_offset,
        end_offset: start_offset + (end - start).num_minutes().max(0) as u32,
        exceptions: Vec::new(),
        raw: Default::default(),
    };
    // Première occurrence : DTSTART lui-même s'il suit le motif, sinon la première date du motif qui le suit. Annuel :
    // le motif part du mois de BYMONTH de l'année de DTSTART (l'expansion compte les années depuis ce mois).
    if frequency == Frequency::Yearly {
        r.start_date = midnight(NaiveDate::from_ymd_opt(start.year(), month, 1).unwrap_or(start.date()));
    }
    let first = expand::dates(&r, start.date(), start.date() + Duration::days(366 * 40)).first().copied().ok_or("aucune occurrence")?;
    r.start_date = midnight(first);
    let m_index = |d: NaiveDate| ((d.year() - 1601) * 12 + d.month0() as i32) as u32;
    let first_of_month = |idx: u32| midnight(NaiveDate::from_ymd_opt(1601 + (idx / 12) as i32, idx % 12 + 1, 1).unwrap_or_default());
    r.first_date_time = match frequency {
        Frequency::Daily => midnight(first) % period,
        Frequency::Weekly => {
            let back = (7 + first.weekday().num_days_from_sunday() - wkst % 7) % 7;
            midnight(first - Duration::days(i64::from(back))) % (10_080 * interval)
        }
        Frequency::Monthly => first_of_month(m_index(first) % interval),
        Frequency::Yearly => first_of_month(first.month0()),
    };
    let horizon = first + Duration::days(366 * 120);
    if let Some(v) = kv.get("COUNT") {
        let n = v.parse::<u32>().ok().filter(|n| (1..=9_999).contains(n)).ok_or_else(|| format!("COUNT={v} non pris en charge (1 à 9999)"))?;
        let all = expand::dates(&r, first, horizon);
        r.end = End::Count(n.min(all.len().max(1) as u32));
        r.occurrences = n;
        r.end_date = midnight(all.get(n as usize - 1).or(all.last()).copied().unwrap_or(first));
    } else if let Some(v) = kv.get("UNTIL") {
        let u = dt(v).ok_or_else(|| format!("UNTIL={v} illisible"))?;
        // Une date seule vaut toute la journée.
        let until = if u.date_only { u.local + Duration::hours(24) - Duration::seconds(1) } else { u.local };
        let all: Vec<NaiveDate> = expand::dates(&r, first, until.date()).into_iter().filter(|d| at(*d, start_offset) <= until).collect();
        let last = all.last().copied().ok_or("UNTIL avant la première occurrence")?;
        r.end = End::Date;
        r.end_date = midnight(last);
        r.occurrences = all.len() as u32;
    }
    Ok((r, inexact))
}

// --- Série complète ---------------------------------------------------------------------------------------------------

fn busy_code(b: Busy) -> u32 {
    match b {
        Busy::Free => 0,
        Busy::Tentative => 1,
        Busy::Busy => 2,
        Busy::OutOfOffice => 3,
        Busy::WorkingElsewhere => 4,
    }
}

fn simple(b: &Block, skip: &[&str]) -> Result<Appointment, String> {
    let (_, mut v) = ics::parse(&b.text(skip), 0, 0)?;
    if v.len() == 1 { Ok(v.remove(0)) } else { Err("VEVENT sans DTSTART".into()) }
}

fn build(uid: &str, group: &[&Block]) -> Result<Series, String> {
    let masters: Vec<&&Block> = group.iter().filter(|b| !b.has("RECURRENCE-ID")).collect();
    let [master] = masters.as_slice() else { return Err("plusieurs définitions de la série (même UID)".into()) };
    let master = **master;
    if master.has("RDATE") || master.has("EXRULE") {
        return Err("RDATE / EXRULE non pris en charge".into());
    }
    if master.all("RRULE").count() > 1 {
        return Err("plusieurs RRULE".into());
    }
    let dtstart = master.get("DTSTART").ok_or("série sans DTSTART")?;
    if let Some(z) = dtstart.params.get("TZID")
        && !PARIS_ZONES.contains(&z.to_ascii_lowercase().as_str())
    {
        return Err(format!("fuseau « {z} » non pris en charge (heure de Paris et équivalents seulement)"));
    }
    let mut m = simple(master, &["RRULE", "EXDATE", "RDATE", "EXRULE"])?;
    let (start, end) = (local_of(m.start_ms), local_of(m.end_ms));
    let utc = dt(&dtstart.value).is_some_and(|d| d.utc) && !m.all_day;
    let rule = master.get("RRULE").map(|p| p.value.clone()).unwrap_or_default();
    let (mut r, inexact) = recurrence(&rule, start, end)?;
    let first = day_of(r.start_date);
    if utc {
        // Heure fixe en UTC : ThunderLook répète à heure locale fixe. Équivalent seulement si le décalage ne change pas.
        let last = day_of(r.end_date).min(first + Duration::days(366 * 2));
        let offsets: BTreeSet<i64> = expand::dates(&r, first, last).into_iter().map(|d| (utc_ms(at(d, r.start_offset)) - at(d, r.start_offset).and_utc().timestamp_millis()) / 60_000).collect();
        if offsets.len() > 1 {
            return Err("heure fixée en UTC sur une période qui traverse un changement d'heure (ThunderLook répète à heure locale fixe) : recréer la série en heure de Paris sur le téléphone".into());
        }
    }
    // Occurrences modifiées (ou annulées) : exceptions de la série.
    let mut seen = BTreeSet::new();
    for o in group.iter().filter(|b| b.has("RECURRENCE-ID")) {
        let rid = o.get("RECURRENCE-ID").ok_or("RECURRENCE-ID absent")?;
        if rid.params.get("RANGE").is_some_and(|v| !v.eq_ignore_ascii_case("THISONLY")) {
            return Err("RECURRENCE-ID avec RANGE=THISANDFUTURE non pris en charge (modifier « cette occurrence et les suivantes » sur le téléphone n'est pas représentable)".into());
        }
        let rid = dt(&rid.value).ok_or("RECURRENCE-ID illisible")?;
        let day = rid.local.date();
        if !seen.insert(day) {
            return Err(format!("deux occurrences modifiées le {day}"));
        }
        if !is_occurrence(&r, day) {
            return Err(format!("occurrence modifiée le {day} : ce jour n'est pas une occurrence de la série"));
        }
        r.deleted.push(midnight(day));
        if o.get("STATUS").is_some_and(|p| p.value.eq_ignore_ascii_case("CANCELLED")) {
            continue;
        }
        let x = simple(o, &["RECURRENCE-ID", "RRULE"]).map_err(|e| format!("occurrence modifiée le {day} : {e}"))?;
        let diff = |what: &str| format!("occurrence modifiée le {day} : {what} différente de la série (non représentable)");
        if x.description.as_deref().map(str::trim).filter(|d| !d.is_empty()).is_some_and(|d| m.description.as_deref().map(str::trim) != Some(d)) {
            return Err(diff("description"));
        }
        if !x.categories.is_empty() && x.categories != m.categories {
            return Err(diff("catégorie"));
        }
        if x.sensitivity != m.sensitivity && x.sensitivity != 0 {
            return Err(diff("confidentialité"));
        }
        let (xs, xe) = (local_of(x.start_ms), local_of(x.end_ms));
        let original_start = if rid.date_only { midnight(day) + r.start_offset } else { local_to_minutes(rid.local) };
        let has_busy = o.has("TRANSP") || o.has("X-MICROSOFT-CDO-BUSYSTATUS");
        let reminder = x.reminder_minutes.filter(|v| Some(*v) != m.reminder_minutes);
        r.modified.push(midnight(xs.date()));
        r.exceptions.push(Exception {
            start: local_to_minutes(xs),
            end: local_to_minutes(xe),
            original_start,
            subject: Some(x.subject.clone()).filter(|s| !s.is_empty() && *s != m.subject),
            location: x.location.clone().filter(|l| Some(l) != m.location.as_ref()),
            busy_status: Some(x.busy).filter(|b| has_busy && *b != m.busy).map(busy_code),
            reminder_set: reminder.map(|_| true),
            reminder_delta: reminder.map(|v| v as u32),
            all_day: Some(x.all_day).filter(|v| *v != m.all_day),
            ..Default::default()
        });
    }
    // EXDATE : seulement des jours qui sont des occurrences.
    for ex in master.all("EXDATE") {
        for v in ex.value.split(',') {
            let d = dt(v).ok_or_else(|| format!("EXDATE={v} illisible"))?.local.date();
            if is_occurrence(&r, d) {
                r.deleted.push(midnight(d));
            }
        }
    }
    r.deleted.sort_unstable();
    r.deleted.dedup();
    r.modified.sort_unstable();
    r.modified.dedup();
    r.exceptions.sort_by_key(|x| x.original_start);
    // Rendez-vous : début et fin de la première occurrence.
    m.start_ms = utc_ms(at(first, r.start_offset));
    m.end_ms = utc_ms(at(first, r.end_offset));
    m.time_zone = None;
    m.recurrence = Some(r);
    m.uid = Some(uid.to_string());
    let mut ics = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n");
    for b in group {
        ics.push_str("BEGIN:VEVENT\r\n");
        for i in &b.items {
            ics.push_str(&i.raw);
            ics.push_str("\r\n");
        }
        ics.push_str("END:VEVENT\r\n");
    }
    ics.push_str("END:VCALENDAR\r\n");
    Ok(Series { uid: uid.to_string(), appointment: m, ics, inexact })
}

// --- Comparaison ------------------------------------------------------------------------------------------------------

/// Le rendez-vous `a` tel que le téléphone le relirait après un export : aller-retour par le même format. Sert de
/// référence à la comparaison (ce que le format ne porte pas ne produit jamais de « modification »). `Err(raison)` :
/// l'export de ThunderLook lui-même n'est pas relisible (la série n'est alors comparée qu'à elle-même et à ses refus).
pub fn canonical(a: &Appointment, description: &dyn Fn(&Appointment) -> Option<String>) -> Result<Appointment, (Appointment, String)> {
    let text = ics::export(&[a], "ThunderLook", description);
    parse_series(&text).map(|s| s.appointment).map_err(|reason| {
        let mut b = a.clone();
        b.description = description(a);
        (b, reason)
    })
}

/// Ce que la mise à jour d'une série conserve de l'existante quand le téléphone ne l'envoie pas : catégories, rappel.
pub fn keep_from(existing: &Appointment, phone: &mut Appointment) {
    if phone.categories.is_empty() {
        phone.categories = existing.categories.clone();
    }
    if phone.reminder_minutes.is_none() {
        phone.reminder_minutes = existing.reminder_minutes;
        phone.reminder_delta = existing.reminder_delta;
    }
}

fn parts(a: &Appointment) -> Vec<(&'static str, String)> {
    let mut v = vec![
        ("objet", a.subject.trim().to_string()),
        ("lieu", a.location.as_deref().unwrap_or("").trim().to_string()),
        ("disponibilité", format!("{:?}", a.busy)),
        ("rappel", format!("{:?}", a.reminder_minutes)),
        ("catégories", a.categories.join("\u{1}")),
        ("description", a.description.as_deref().unwrap_or("").trim().to_string()),
        ("confidentialité", (a.sensitivity >= 2).to_string()),
    ];
    match &a.recurrence {
        None => {
            let (s, e) = (local_of(a.start_ms), local_of(a.end_ms));
            v.push(("horaire", format!("{}|{}|{}", s, e, a.all_day)));
            v.push(("périodicité", "aucune".into()));
        }
        Some(r) => {
            v.push(("horaire", format!("{}|{}|{}", r.start_offset, r.end_offset, a.all_day)));
            v.push(("périodicité", format!("{:?}|{:?}|{}|{}|{}", r.frequency, r.pattern, r.period, r.start_date, r.first_day_of_week)));
            v.push(("fin de série", format!("{:?}|{}", r.end, if r.end == End::Never { 0 } else { r.end_date })));
            let mut deleted = r.deleted.clone();
            deleted.sort_unstable();
            deleted.dedup();
            v.push(("occurrences supprimées", format!("{deleted:?}")));
            let mut xs: Vec<String> = r
                .exceptions
                .iter()
                .map(|x| format!("{}|{}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}", x.original_start, x.start, x.end, x.subject, x.location, x.busy_status, x.reminder_set, x.reminder_delta, x.all_day))
                .collect();
            xs.sort();
            v.push(("occurrences modifiées", xs.join(";")));
        }
    }
    v
}

/// Noms des aspects qui diffèrent entre deux rendez-vous (périodicité comprise) ; vide : identiques.
pub fn changed(a: &Appointment, b: &Appointment) -> Vec<&'static str> {
    parts(a).into_iter().zip(parts(b)).filter(|(x, y)| x.1 != y.1).map(|(x, _)| x.0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cal(events: &str) -> String {
        format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{events}END:VCALENDAR\r\n")
    }

    fn series(events: &str) -> Series {
        let s = split(&cal(events));
        assert!(s.rejected.is_empty(), "{:?}", s.rejected);
        assert_eq!(s.series.len(), 1);
        s.series.into_iter().next().unwrap()
    }

    fn rejected(events: &str) -> String {
        let s = split(&cal(events));
        assert!(s.series.is_empty(), "série acceptée à tort");
        s.rejected.into_iter().next().expect("aucun refus").reason
    }

    /// Jours locaux des occurrences (hors supprimées), d'après l'expansion de l'agenda.
    fn days(a: &Appointment) -> Vec<String> {
        let r = a.recurrence.as_ref().unwrap();
        let from = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        let skip: Vec<NaiveDate> = r.deleted.iter().map(|m| day_of(*m)).collect();
        expand::dates(r, from, NaiveDate::from_ymd_opt(2040, 1, 1).unwrap()).into_iter().filter(|d| !skip.contains(d)).map(|d| d.to_string()).collect()
    }

    fn ev(extra: &str) -> String {
        format!("BEGIN:VEVENT\r\nUID:u1\r\nSUMMARY:Test\r\n{extra}END:VEVENT\r\n")
    }

    #[test]
    fn weekly_byday_count_and_exdate() {
        let s = series(&ev("DTSTART;TZID=Europe/Paris:20261026T100000\r\nDTEND;TZID=Europe/Paris:20261026T110000\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE;COUNT=5\r\nEXDATE;TZID=Europe/Paris:20261028T100000\r\n"));
        assert_eq!(days(&s.appointment), ["2026-10-26", "2026-11-02", "2026-11-04", "2026-11-09"]);
        assert_eq!(s.appointment.recurrence.as_ref().unwrap().end, End::Count(5));
    }

    #[test]
    fn daily_interval_and_until_in_utc() {
        // 10:00 Paris (09:00 UTC en hiver), tous les 2 jours, UNTIL en UTC à 09:00Z le 7/11 : le 7/11 est inclus.
        let s = series(&ev("DTSTART;TZID=Europe/Paris:20261101T100000\r\nDTEND;TZID=Europe/Paris:20261101T103000\r\nRRULE:FREQ=DAILY;INTERVAL=2;UNTIL=20261107T090000Z\r\n"));
        assert_eq!(days(&s.appointment), ["2026-11-01", "2026-11-03", "2026-11-05", "2026-11-07"]);
        // Une minute plus tôt : le 7/11 à 10:00 dépasse UNTIL.
        let s = series(&ev("DTSTART;TZID=Europe/Paris:20261101T100000\r\nDTEND;TZID=Europe/Paris:20261101T103000\r\nRRULE:FREQ=DAILY;INTERVAL=2;UNTIL=20261107T085900Z\r\n"));
        assert_eq!(days(&s.appointment), ["2026-11-01", "2026-11-03", "2026-11-05"]);
        // Date seule : toute la journée.
        let s = series(&ev("DTSTART;TZID=Europe/Paris:20261101T100000\r\nDTEND;TZID=Europe/Paris:20261101T103000\r\nRRULE:FREQ=DAILY;UNTIL=20261103\r\n"));
        assert_eq!(days(&s.appointment), ["2026-11-01", "2026-11-02", "2026-11-03"]);
    }

    #[test]
    fn monthly_last_day_and_ordinal_weekdays() {
        let s = series(&ev("DTSTART;TZID=Europe/Paris:20260131T180000\r\nDTEND;TZID=Europe/Paris:20260131T190000\r\nRRULE:FREQ=MONTHLY;BYMONTHDAY=-1;COUNT=4\r\n"));
        assert_eq!(days(&s.appointment), ["2026-01-31", "2026-02-28", "2026-03-31", "2026-04-30"]);
        assert!(s.inexact.is_none());
        // Deuxième mardi, puis dernier vendredi.
        let s = series(&ev("DTSTART;TZID=Europe/Paris:20261013T180000\r\nDTEND;TZID=Europe/Paris:20261013T190000\r\nRRULE:FREQ=MONTHLY;BYDAY=2TU;COUNT=3\r\n"));
        assert_eq!(days(&s.appointment), ["2026-10-13", "2026-11-10", "2026-12-08"]);
        let s = series(&ev("DTSTART;TZID=Europe/Paris:20261030T180000\r\nDTEND;TZID=Europe/Paris:20261030T190000\r\nRRULE:FREQ=MONTHLY;BYDAY=FR;BYSETPOS=-1;COUNT=3\r\n"));
        assert_eq!(days(&s.appointment), ["2026-10-30", "2026-11-27", "2026-12-25"]);
    }

    #[test]
    fn day_31_and_february_29_are_flagged_inexact() {
        let s = series(&ev("DTSTART;TZID=Europe/Paris:20260131T180000\r\nDTEND;TZID=Europe/Paris:20260131T190000\r\nRRULE:FREQ=MONTHLY;COUNT=4\r\n"));
        assert!(s.inexact.is_some_and(|w| w.contains("31")));
        let s = series(&ev("DTSTART;TZID=Europe/Paris:20240229T090000\r\nDTEND;TZID=Europe/Paris:20240229T100000\r\nRRULE:FREQ=YEARLY;COUNT=3\r\n"));
        assert!(s.inexact.is_some_and(|w| w.contains("29 février")));
        assert_eq!(days(&s.appointment), ["2024-02-29", "2025-02-28", "2026-02-28"]);
        // Annuel un autre jour : exact.
        let s = series(&ev("DTSTART;TZID=Europe/Paris:20240228T090000\r\nDTEND;TZID=Europe/Paris:20240228T100000\r\nRRULE:FREQ=YEARLY;COUNT=3\r\n"));
        assert!(s.inexact.is_none());
    }

    #[test]
    fn yearly_by_month_starts_in_that_month() {
        let s = series(&ev("DTSTART;TZID=Europe/Paris:20260105T090000\r\nDTEND;TZID=Europe/Paris:20260105T100000\r\nRRULE:FREQ=YEARLY;BYMONTH=6;BYMONTHDAY=15;COUNT=2\r\n"));
        assert_eq!(days(&s.appointment), ["2026-06-15", "2027-06-15"]);
        assert_eq!(local_of(s.appointment.start_ms).date().to_string(), "2026-06-15");
    }

    #[test]
    fn exceptions_cancellations_and_exdate() {
        let s = series(&format!(
            "{}{}{}",
            ev("DTSTART;TZID=Europe/Paris:20261026T100000\r\nDTEND;TZID=Europe/Paris:20261026T110000\r\nRRULE:FREQ=WEEKLY;COUNT=5\r\nEXDATE;TZID=Europe/Paris:20261109T100000\r\n"),
            "BEGIN:VEVENT\r\nUID:u1\r\nSUMMARY:Déplacé\r\nRECURRENCE-ID;TZID=Europe/Paris:20261102T100000\r\nDTSTART;TZID=Europe/Paris:20261103T140000\r\nDTEND;TZID=Europe/Paris:20261103T150000\r\nEND:VEVENT\r\n",
            "BEGIN:VEVENT\r\nUID:u1\r\nSUMMARY:Test\r\nSTATUS:CANCELLED\r\nRECURRENCE-ID;TZID=Europe/Paris:20261116T100000\r\nDTSTART;TZID=Europe/Paris:20261116T100000\r\nDTEND;TZID=Europe/Paris:20261116T110000\r\nEND:VEVENT\r\n",
        ));
        let r = s.appointment.recurrence.as_ref().unwrap();
        assert_eq!(days(&s.appointment), ["2026-10-26", "2026-11-23"]);
        assert_eq!(r.exceptions.len(), 1);
        assert_eq!(r.exceptions[0].subject.as_deref(), Some("Déplacé"));
        assert_eq!(minutes_to_local_str(r.exceptions[0].start), "2026-11-03 14:00");
        assert_eq!(r.deleted.len(), 3);
    }

    fn minutes_to_local_str(m: u32) -> String {
        crate::recur::minutes_to_local(m).format("%Y-%m-%d %H:%M").to_string()
    }

    #[test]
    fn refusals_are_explicit() {
        let base = "DTSTART;TZID=Europe/Paris:20261026T100000\r\nDTEND;TZID=Europe/Paris:20261026T110000\r\n";
        assert!(rejected(&ev(&format!("{base}RRULE:FREQ=HOURLY;COUNT=3\r\n"))).contains("HOURLY"));
        assert!(rejected(&ev(&format!("{base}RRULE:FREQ=WEEKLY;BYWEEKNO=3\r\n"))).contains("BYWEEKNO"));
        assert!(rejected(&ev(&format!("{base}RRULE:FREQ=MONTHLY;BYDAY=MO\r\n"))).contains("BYDAY"));
        assert!(rejected(&ev(&format!("{base}RRULE:FREQ=MONTHLY;BYDAY=5MO\r\n"))).contains("rang 5"));
        assert!(rejected(&ev(&format!("{base}RRULE:FREQ=MONTHLY;BYMONTHDAY=1,15\r\n"))).contains("BYMONTHDAY"));
        assert!(rejected(&ev(&format!("{base}RRULE:FREQ=DAILY;COUNT=2;UNTIL=20261201\r\n"))).contains("COUNT et UNTIL"));
        assert!(rejected(&ev(&format!("{base}RRULE:FREQ=DAILY;COUNT=3\r\nRDATE:20261201T100000\r\n"))).contains("RDATE"));
        assert!(rejected(&ev("DTSTART;TZID=America/New_York:20261026T100000\r\nDTEND;TZID=America/New_York:20261026T110000\r\nRRULE:FREQ=DAILY;COUNT=3\r\n")).contains("fuseau"));
        // THISANDFUTURE.
        let r = rejected(&format!(
            "{}BEGIN:VEVENT\r\nUID:u1\r\nRECURRENCE-ID;RANGE=THISANDFUTURE;TZID=Europe/Paris:20261102T100000\r\nDTSTART;TZID=Europe/Paris:20261102T120000\r\nDTEND;TZID=Europe/Paris:20261102T130000\r\nEND:VEVENT\r\n",
            ev(&format!("{base}RRULE:FREQ=WEEKLY;COUNT=5\r\n"))
        ));
        assert!(r.contains("THISANDFUTURE"));
        // Occurrence modifiée avec une autre description.
        let r = rejected(&format!(
            "{}BEGIN:VEVENT\r\nUID:u1\r\nDESCRIPTION:Autre\r\nRECURRENCE-ID;TZID=Europe/Paris:20261102T100000\r\nDTSTART;TZID=Europe/Paris:20261102T100000\r\nDTEND;TZID=Europe/Paris:20261102T110000\r\nEND:VEVENT\r\n",
            ev(&format!("{base}RRULE:FREQ=WEEKLY;COUNT=5\r\n"))
        ));
        assert!(r.contains("description"));
        // Exception sur un jour qui n'est pas une occurrence, et exception sans série.
        let r = rejected(&format!(
            "{}BEGIN:VEVENT\r\nUID:u1\r\nRECURRENCE-ID;TZID=Europe/Paris:20261103T100000\r\nDTSTART;TZID=Europe/Paris:20261103T100000\r\nDTEND;TZID=Europe/Paris:20261103T110000\r\nEND:VEVENT\r\n",
            ev(&format!("{base}RRULE:FREQ=WEEKLY;COUNT=5\r\n"))
        ));
        assert!(r.contains("pas une occurrence"));
        let r = rejected("BEGIN:VEVENT\r\nUID:zz\r\nRECURRENCE-ID:20261103T100000\r\nDTSTART:20261103T100000\r\nEND:VEVENT\r\n");
        assert!(r.contains("absente"));
    }

    #[test]
    fn utc_series_across_daylight_saving_time_is_refused() {
        // 09:00Z en octobre = 11:00 Paris (été), 10:00 Paris après le 25/10 : l'heure locale dériverait.
        let r = rejected(&ev("DTSTART:20261019T090000Z\r\nDTEND:20261019T100000Z\r\nRRULE:FREQ=WEEKLY;COUNT=4\r\n"));
        assert!(r.contains("changement d'heure"));
        // Sans changement d'heure sur la période : acceptée, à l'heure de Paris.
        let s = series(&ev("DTSTART:20261102T090000Z\r\nDTEND:20261102T100000Z\r\nRRULE:FREQ=WEEKLY;COUNT=4\r\n"));
        assert_eq!(local_of(s.appointment.start_ms).to_string(), "2026-11-02 10:00:00");
        // Sans fin : traverse forcément un changement d'heure.
        assert!(rejected(&ev("DTSTART:20261102T090000Z\r\nDTEND:20261102T100000Z\r\nRRULE:FREQ=WEEKLY\r\n")).contains("changement d'heure"));
    }

    #[test]
    fn local_time_series_keeps_its_hour_across_daylight_saving_time() {
        let s = series(&ev("DTSTART;TZID=Romance Standard Time:20260323T100000\r\nDTEND;TZID=Romance Standard Time:20260323T110000\r\nRRULE:FREQ=WEEKLY;COUNT=3\r\n"));
        let occ = expand::occurrences(&s.appointment, 0, 4_102_444_800_000);
        let hours: Vec<String> = occ.iter().map(|o| format!("{}", TZ.to_local(chrono::DateTime::from_timestamp_millis(o.start_ms).unwrap().naive_utc()).format("%m-%d %H:%M"))).collect();
        assert_eq!(hours, ["03-23 10:00", "03-30 10:00", "04-06 10:00"]);
        // En UTC : 09:00Z puis 08:00Z (le 29/03/2026 est le changement d'heure).
        assert_eq!(occ.iter().map(|o| chrono::DateTime::from_timestamp_millis(o.start_ms).unwrap().format("%H:%M").to_string()).collect::<Vec<_>>(), ["09:00", "08:00", "08:00"]);
    }

    #[test]
    fn simple_events_stay_in_rest_and_parse_is_unaffected() {
        let text = cal(&format!(
            "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Simple\r\nDTSTART:20261101T080000Z\r\nDTEND:20261101T090000Z\r\nEND:VEVENT\r\n{}",
            ev("DTSTART;TZID=Europe/Paris:20261026T100000\r\nDTEND;TZID=Europe/Paris:20261026T110000\r\nRRULE:FREQ=HOURLY\r\n")
        ));
        let s = split(&text);
        assert_eq!(s.rejected.len(), 1);
        let (_, v) = ics::parse(&s.rest, 0, 0).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].subject, "Simple");
    }

    /// Série d'agenda → export → relecture : mêmes occurrences, mêmes exceptions, rien de changé.
    fn round_trip(a: &Appointment) -> Appointment {
        let text = ics::export(&[a], "t", &|_| None);
        parse_series(&text).unwrap().appointment
    }

    fn occurrences(a: &Appointment) -> Vec<(i64, i64, bool)> {
        expand::occurrences(a, 0, 4_102_444_800_000).into_iter().map(|o| (o.start_ms, o.end_ms, o.exception)).collect()
    }

    fn base(frequency: Frequency, pattern: Pattern, period: u32, start: &str, end: End, end_date: &str) -> Appointment {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let first = d(start);
        let r = Recurrence {
            frequency,
            pattern,
            calendar_type: 0,
            first_date_time: 0,
            period,
            end,
            occurrences: 0,
            first_day_of_week: 1,
            deleted: vec![],
            modified: vec![],
            start_date: midnight(first),
            end_date: if end == End::Never { NEVER_END } else { midnight(d(end_date)) },
            start_offset: 600,
            end_offset: 660,
            exceptions: vec![],
            raw: Default::default(),
        };
        Appointment {
            id: 1,
            uid: Some("U-1".into()),
            source_id: 1,
            folder_id: 1,
            subject: "Série".into(),
            location: None,
            start_ms: utc_ms(at(first, 600)),
            end_ms: utc_ms(at(first, 660)),
            all_day: false,
            busy: Busy::Busy,
            recurrence: Some(r),
            time_zone: None,
            time_zone_name: None,
            reminder_minutes: Some(15),
            reminder_delta: Some(15),
            categories: vec!["Mairie".into()],
            is_meeting: false,
            organizer: String::new(),
            attendees: String::new(),
            response_status: 0,
            private: false,
            sensitivity: 0,
            importance: 1,
            eml_path: None,
            description: None,
        }
    }

    #[test]
    fn export_round_trip_end_of_month_leap_day_and_daylight_saving_time() {
        // Fin de mois : jusqu'au 30/04, avec EXDATE de février et une exception en mars.
        let mut a = base(Frequency::Monthly, Pattern::MonthEnd, 1, "2026-01-31", End::Date, "2026-04-30");
        {
            let r = a.recurrence.as_mut().unwrap();
            r.deleted.push(midnight(NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()));
            r.deleted.push(midnight(NaiveDate::from_ymd_opt(2026, 3, 31).unwrap()));
            r.modified.push(midnight(NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()));
            r.exceptions.push(Exception { start: local_to_minutes(at(NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(), 900)), end: local_to_minutes(at(NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(), 960)), original_start: local_to_minutes(at(NaiveDate::from_ymd_opt(2026, 3, 31).unwrap(), 600)), subject: Some("Décalé".into()), ..Default::default() });
        }
        let b = round_trip(&a);
        assert_eq!(occurrences(&b), occurrences(&a));
        assert_eq!(days(&b), ["2026-01-31", "2026-04-30"]);
        assert!(changed(&a, &b).is_empty() || changed(&a, &b) == ["description"], "{:?}", changed(&a, &b));
        // 29 février annuel : exact à l'aller-retour, signalé inexact.
        let a = base(Frequency::Yearly, Pattern::Month { day: 29 }, 12, "2024-02-29", End::Date, "2026-02-28");
        let text = ics::export(&[&a], "t", &|_| None);
        let s = parse_series(&text).unwrap();
        assert!(s.inexact.is_some());
        assert_eq!(occurrences(&s.appointment), occurrences(&a));
        assert_eq!(days(&s.appointment), ["2024-02-29", "2025-02-28", "2026-02-28"]);
        // Changement d'heure : hebdomadaire à 10:00 locales du 23/03 au 6/04/2026.
        let a = base(Frequency::Weekly, Pattern::Week { days: 1 << 1 }, 1, "2026-03-23", End::Date, "2026-04-06");
        let b = round_trip(&a);
        assert_eq!(occurrences(&b), occurrences(&a));
        assert_eq!(occurrences(&b).len(), 3);
    }

    #[test]
    fn canonical_matches_phone_copy_and_detects_edits() {
        let a = base(Frequency::Weekly, Pattern::Week { days: 1 << 1 }, 1, "2026-10-26", End::Count(4), "2026-11-16");
        let canon = canonical(&a, &|_| None).unwrap();
        let text = ics::export(&[&a], "t", &|_| None);
        let phone = parse_series(&text.replace("\r\n", "\n")).unwrap().appointment;
        assert!(changed(&canon, &phone).is_empty());
        let edited = parse_series(&text.replace("SUMMARY;LANGUAGE=fr:Série", "SUMMARY;LANGUAGE=fr:Autre")).unwrap().appointment;
        assert_eq!(changed(&canon, &edited), ["objet"]);
        let mut shorter = parse_series(&text.replace("COUNT=4", "COUNT=3")).unwrap().appointment;
        assert_eq!(changed(&canon, &shorter), ["fin de série"]);
        // Catégories et rappel absents du téléphone : conservés.
        shorter.categories.clear();
        shorter.reminder_minutes = None;
        keep_from(&canon, &mut shorter);
        assert_eq!((shorter.categories, shorter.reminder_minutes), (canon.categories.clone(), canon.reminder_minutes));
    }
}
