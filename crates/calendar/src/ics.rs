//! Étape 5 du J9 : iCalendar (RFC 5545). Export des rendez-vous comme Outlook les écrit (fuseau « Romance Standard
//! Time », RRULE, EXDATE, occurrences modifiées en VEVENT à RECURRENCE-ID) ; import vers des rendez-vous dont la
//! RRULE est traduite en périodicité [MS-OXOCAL], pour réutiliser l'expansion vérifiée contre Outlook.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};

use crate::{
    Appointment, Busy,
    expand::dates,
    recur::{End, Exception, Frequency, Pattern, Recurrence, local_to_minutes, minutes_to_local},
    tz::TimeZone,
};

const TZ: TimeZone = TimeZone::PARIS;
const TZID: &str = "Romance Standard Time";
/// `EndDate` d'une série sans fin (31/12/4500), comme Outlook.
const NEVER_END: u32 = 0x5AE9_80DF;
const DAYS: [&str; 7] = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"];

const VTIMEZONE: &str = "BEGIN:VTIMEZONE\r\nTZID:Romance Standard Time\r\nBEGIN:STANDARD\r\nDTSTART:16011028T030000\r\nRRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=10\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\nEND:STANDARD\r\nBEGIN:DAYLIGHT\r\nDTSTART:16010325T020000\r\nRRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=3\r\nTZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\n";

fn local(ms: i64) -> NaiveDateTime {
    TZ.to_local(DateTime::from_timestamp_millis(ms).unwrap_or_default().naive_utc())
}

fn utc_ms(t: NaiveDateTime) -> i64 {
    TZ.to_utc(t).and_utc().timestamp_millis()
}

/// Texte iCalendar (§3.3.11).
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace("\r\n", "\\n").replace('\n', "\\n")
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
}

fn dt_local(t: NaiveDateTime) -> String {
    t.format("%Y%m%dT%H%M%S").to_string()
}

fn date(t: NaiveDate) -> String {
    t.format("%Y%m%d").to_string()
}

fn when(name: &str, t: NaiveDateTime, all_day: bool) -> String {
    if all_day { format!("{name};VALUE=DATE:{}", date(t.date())) } else { format!("{name};TZID=\"{TZID}\":{}", dt_local(t)) }
}

fn byday_mask(mask: u32) -> String {
    (0..7).filter(|i| mask & (1 << i) != 0).map(|i| DAYS[i]).collect::<Vec<_>>().join(",")
}

fn rrule(r: &Recurrence, all_day: bool) -> String {
    let start = minutes_to_local(r.start_date);
    let mut parts = Vec::new();
    let nth = |n: u32| if n >= 5 { -1 } else { n as i32 };
    match (r.frequency, r.pattern) {
        (Frequency::Daily, _) => parts.push(format!("FREQ=DAILY;INTERVAL={}", (r.period / 1_440).max(1))),
        (_, Pattern::Week { days }) => {
            parts.push(format!("FREQ=WEEKLY;INTERVAL={};BYDAY={}", r.period.max(1), byday_mask(days)));
            parts.push(format!("WKST={}", DAYS[(r.first_day_of_week % 7) as usize]));
        }
        (f, p) => {
            let (freq, interval) = if f == Frequency::Yearly { ("YEARLY", r.period / 12) } else { ("MONTHLY", r.period) };
            parts.push(format!("FREQ={freq};INTERVAL={}", interval.max(1)));
            match p {
                Pattern::Month { day } => parts.push(format!("BYMONTHDAY={day}")),
                Pattern::MonthEnd => parts.push("BYMONTHDAY=-1".into()),
                Pattern::MonthNth { days, nth: n } if days.count_ones() == 1 => parts.push(format!("BYDAY={}{}", nth(n), byday_mask(days))),
                Pattern::MonthNth { days, nth: n } => parts.push(format!("BYDAY={};BYSETPOS={}", byday_mask(days), nth(n))),
                _ => {}
            }
            if freq == "YEARLY" {
                parts.push(format!("BYMONTH={}", start.month()));
            }
        }
    }
    // Outlook affiche ce que dit `EndDate` : quand `OccurrenceCount` ne lui correspond pas (série « tous les 2 ans,
    // 3 fois » qui s'arrête à la 2e), l'export écrit UNTIL pour donner les mêmes occurrences qu'à l'écran.
    let shown = |n: u32| dates(r, minutes_to_local(r.start_date).date(), minutes_to_local(r.end_date).date()).len() == n as usize;
    match r.end {
        End::Count(n) if shown(n) => parts.push(format!("COUNT={n}")),
        End::Count(_) | End::Date => {
            let last = minutes_to_local(r.end_date);
            if all_day {
                parts.push(format!("UNTIL={}", date(last.date())));
            } else {
                let t = last + Duration::minutes(i64::from(r.start_offset));
                parts.push(format!("UNTIL={}Z", dt_local(TZ.to_utc(t))));
            }
        }
        End::Never => {}
    }
    format!("RRULE:{}", parts.join(";"))
}

fn uid(a: &Appointment) -> String {
    a.uid.clone().unwrap_or_else(|| format!("thunderlook-{}-{}", a.source_id, a.id))
}

fn common(out: &mut String, a: &Appointment, subject: &str, location: Option<&str>, description: Option<&str>) {
    if !a.categories.is_empty() {
        fold(out, &format!("CATEGORIES:{}", a.categories.iter().map(|c| escape(c)).collect::<Vec<_>>().join(",")));
    }
    fold(out, &format!("CLASS:{}", match a.sensitivity {
        2 => "PRIVATE",
        3 => "CONFIDENTIAL",
        _ => "PUBLIC",
    }));
    if let Some(d) = description.filter(|d| !d.trim().is_empty()) {
        fold(out, &format!("DESCRIPTION:{}", escape(d)));
    }
    if let Some(l) = location.filter(|l| !l.trim().is_empty()) {
        fold(out, &format!("LOCATION:{}", escape(l)));
    }
    fold(out, &format!("PRIORITY:{}", match a.importance {
        2 => 1,
        0 => 9,
        _ => 5,
    }));
    fold(out, &format!("SUMMARY;LANGUAGE=fr:{}", escape(subject)));
    fold(out, &format!("TRANSP:{}", if a.busy == Busy::Free { "TRANSPARENT" } else { "OPAQUE" }));
    fold(out, &format!("X-MICROSOFT-CDO-BUSYSTATUS:{}", match a.busy {
        Busy::Free => "FREE",
        Busy::Tentative => "TENTATIVE",
        Busy::OutOfOffice => "OOF",
        Busy::WorkingElsewhere => "WORKINGELSEWHERE",
        Busy::Busy => "BUSY",
    }));
    if let Some(m) = a.reminder_minutes {
        out.push_str(&format!("BEGIN:VALARM\r\nTRIGGER:-PT{m}M\r\nACTION:DISPLAY\r\nDESCRIPTION:Reminder\r\nEND:VALARM\r\n"));
    }
}

/// Fichier `.ics` des rendez-vous donnés ; `description` fournit le corps de chacun.
pub fn export(appointments: &[&Appointment], name: &str, description: &dyn Fn(&Appointment) -> Option<String>) -> String {
    let mut out = String::from("BEGIN:VCALENDAR\r\nPRODID:-//ThunderLook//Calendrier 0.9//FR\r\nVERSION:2.0\r\nMETHOD:PUBLISH\r\n");
    fold(&mut out, &format!("X-WR-CALNAME:{}", escape(name)));
    out.push_str(VTIMEZONE);
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    for a in appointments {
        let (start, end) = (local(a.start_ms), local(a.end_ms));
        let body = description(a);
        out.push_str("BEGIN:VEVENT\r\n");
        fold(&mut out, &when("DTSTART", start, a.all_day));
        fold(&mut out, &when("DTEND", end, a.all_day));
        fold(&mut out, &format!("DTSTAMP:{stamp}"));
        if let Some(r) = &a.recurrence {
            fold(&mut out, &rrule(r, a.all_day));
            // Occurrences supprimées : les dates d'origine qui ne sont pas celles d'une exception.
            let moved: Vec<NaiveDate> = r.exceptions.iter().map(|x| minutes_to_local(x.original_start).date()).collect();
            for d in r.deleted.iter().map(|m| minutes_to_local(*m).date()).filter(|d| !moved.contains(d)) {
                let t = d.and_hms_opt(0, 0, 0).unwrap() + Duration::minutes(i64::from(r.start_offset));
                fold(&mut out, &when("EXDATE", t, a.all_day));
            }
        }
        fold(&mut out, &format!("UID:{}", uid(a)));
        common(&mut out, a, &a.subject, a.location.as_deref(), body.as_deref());
        out.push_str("END:VEVENT\r\n");
        // Occurrences modifiées.
        for x in a.recurrence.iter().flat_map(|r| r.exceptions.iter()) {
            let all_day = x.all_day.unwrap_or(a.all_day);
            out.push_str("BEGIN:VEVENT\r\n");
            fold(&mut out, &when("DTSTART", minutes_to_local(x.start), all_day));
            fold(&mut out, &when("DTEND", minutes_to_local(x.end), all_day));
            fold(&mut out, &format!("DTSTAMP:{stamp}"));
            let original = minutes_to_local(x.original_start);
            fold(&mut out, &when("RECURRENCE-ID", original, a.all_day));
            fold(&mut out, &format!("UID:{}", uid(a)));
            let subject = x.subject.clone().unwrap_or_else(|| a.subject.clone());
            let location = x.location.clone().or_else(|| a.location.clone());
            common(&mut out, a, &subject, location.as_deref(), body.as_deref());
            out.push_str("END:VEVENT\r\n");
        }
    }
    out.push_str("END:VCALENDAR\r\n");
    out
}

/// Une propriété : nom, paramètres, valeur brute.
#[derive(Clone, Debug)]
struct Prop {
    name: String,
    params: HashMap<String, String>,
    value: String,
}

/// Lignes dépliées d'un fichier et propriétés de chaque VEVENT (sous-composants comme VALARM à part).
fn vevents(text: &str) -> Vec<(Vec<Prop>, Vec<Vec<Prop>>)> {
    let mut lines: Vec<String> = Vec::new();
    for l in text.lines() {
        match l.strip_prefix([' ', '\t']) {
            Some(rest) if !lines.is_empty() => lines.last_mut().unwrap().push_str(rest),
            _ => lines.push(l.trim_end_matches('\r').to_string()),
        }
    }
    let parse = |l: &str| -> Option<Prop> {
        // Le premier « : » hors guillemets sépare nom et paramètres de la valeur.
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
    };
    let mut out = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let (mut props, mut subs, mut sub): (Vec<Prop>, Vec<Vec<Prop>>, Vec<Prop>) = (Vec::new(), Vec::new(), Vec::new());
    for l in &lines {
        let Some(p) = parse(l) else { continue };
        match p.name.as_str() {
            "BEGIN" => {
                stack.push(p.value.to_ascii_uppercase());
                if stack.last().map(String::as_str) == Some("VEVENT") {
                    props.clear();
                    subs.clear();
                }
                sub.clear();
            }
            "END" => {
                let ended = stack.pop();
                match (ended.as_deref(), stack.last().map(String::as_str)) {
                    (Some("VEVENT"), _) => out.push((std::mem::take(&mut props), std::mem::take(&mut subs))),
                    (Some(_), Some("VEVENT")) => subs.push(std::mem::take(&mut sub)),
                    _ => {}
                }
            }
            _ => match stack.last().map(String::as_str) {
                Some("VEVENT") => props.push(p),
                Some(_) if stack.iter().any(|s| s == "VEVENT") => sub.push(p),
                _ => {}
            },
        }
    }
    out
}

/// Date ou date-heure iCalendar → heure locale de Paris, et « journée entière » pour une DATE.
fn time_of(p: &Prop) -> Option<(NaiveDateTime, bool)> {
    let v = p.value.split(',').next()?.trim();
    if p.params.get("VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE")) || v.len() == 8 {
        return Some((NaiveDate::parse_from_str(v, "%Y%m%d").ok()?.and_hms_opt(0, 0, 0)?, true));
    }
    let t = NaiveDateTime::parse_from_str(v.trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()?;
    // UTC → Paris ; un TZID ou une heure flottante sont pris en heure de Paris (seul fuseau des données).
    Some((if v.ends_with('Z') { TZ.to_local(t) } else { t }, false))
}

fn duration(v: &str) -> Option<Duration> {
    // P[n]W, P[n]D, PT[n]H[n]M[n]S et leurs combinaisons ; signe éventuel.
    let (neg, v) = match v.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, v.strip_prefix('+').unwrap_or(v)),
    };
    let v = v.strip_prefix('P')?;
    let (mut total, mut num, mut in_time) = (Duration::zero(), String::new(), false);
    for c in v.chars() {
        match c {
            'T' => in_time = true,
            '0'..='9' => num.push(c),
            u => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += match (u, in_time) {
                    ('W', _) => Duration::weeks(n),
                    ('D', _) => Duration::days(n),
                    ('H', true) => Duration::hours(n),
                    ('M', true) => Duration::minutes(n),
                    ('S', true) => Duration::seconds(n),
                    _ => return None,
                };
            }
        }
    }
    Some(if neg { -total } else { total })
}

/// RRULE → périodicité [MS-OXOCAL] à partir de la première occurrence `start` (heure locale).
fn recurrence(rule: &str, start: NaiveDateTime, end: NaiveDateTime) -> Result<Recurrence, String> {
    let kv: HashMap<String, String> =
        rule.split(';').filter_map(|p| p.split_once('=')).map(|(k, v)| (k.to_ascii_uppercase(), v.to_ascii_uppercase())).collect();
    let interval: u32 = kv.get("INTERVAL").and_then(|v| v.parse().ok()).unwrap_or(1).max(1);
    let day_bit = |d: &str| DAYS.iter().position(|x| *x == d).map(|i| 1u32 << i);
    // BYDAY : « MO,WE » ou « 2MO », « -1SU ».
    let byday: Vec<(i32, u32)> = kv
        .get("BYDAY")
        .map(|v| {
            v.split(',')
                .filter_map(|x| {
                    let split = x.len().checked_sub(2)?;
                    let (n, d) = x.split_at(split);
                    Some((n.parse().unwrap_or(0), day_bit(d)?))
                })
                .collect()
        })
        .unwrap_or_default();
    let mask: u32 = byday.iter().map(|(_, b)| b).fold(0, |a, b| a | b);
    let setpos: Option<i32> = kv.get("BYSETPOS").and_then(|v| v.parse().ok()).or_else(|| byday.first().map(|x| x.0).filter(|n| *n != 0));
    let nth = |n: i32| if n < 0 { 5 } else { n.clamp(1, 4) as u32 };
    let start_bit = 1u32 << start.weekday().num_days_from_sunday();
    let monthly = |kv: &HashMap<String, String>| -> Pattern {
        match (kv.get("BYMONTHDAY").and_then(|v| v.split(',').next().and_then(|x| x.parse::<i32>().ok())), setpos) {
            (Some(-1), _) => Pattern::MonthEnd,
            (Some(d), _) if d > 0 => Pattern::Month { day: d as u32 },
            (_, Some(n)) if mask != 0 => Pattern::MonthNth { days: mask, nth: nth(n) },
            _ => Pattern::Month { day: start.day() },
        }
    };
    let (frequency, pattern, period) = match kv.get("FREQ").map(String::as_str) {
        Some("DAILY") => (Frequency::Daily, Pattern::Day, interval * 1_440),
        Some("WEEKLY") => (Frequency::Weekly, Pattern::Week { days: if mask == 0 { start_bit } else { mask } }, interval),
        Some("MONTHLY") => (Frequency::Monthly, monthly(&kv), interval),
        Some("YEARLY") => (Frequency::Yearly, monthly(&kv), interval * 12),
        f => return Err(format!("RRULE : fréquence non prise en charge {f:?}")),
    };
    let first_day_of_week = kv.get("WKST").and_then(|d| DAYS.iter().position(|x| x == d)).unwrap_or(1) as u32;
    let start_min = start.hour() * 60 + start.minute();
    let mut r = Recurrence {
        frequency,
        pattern,
        calendar_type: 0,
        first_date_time: 0,
        period,
        end: End::Never,
        occurrences: 0,
        first_day_of_week,
        deleted: Vec::new(),
        modified: Vec::new(),
        start_date: local_to_minutes(start.date().and_hms_opt(0, 0, 0).unwrap()),
        end_date: NEVER_END,
        start_offset: start_min,
        end_offset: start_min + (end - start).num_minutes().max(0) as u32,
        exceptions: Vec::new(),
    };
    // La première date de la série doit être une occurrence du motif (sinon RFC 5545 la compte quand même).
    if let Some(first) = dates(&r, start.date(), start.date() + Duration::days(4_000)).first() {
        r.start_date = local_to_minutes(first.and_hms_opt(0, 0, 0).unwrap());
    }
    if let Some(c) = kv.get("COUNT").and_then(|v| v.parse::<u32>().ok()) {
        r.end = End::Count(c);
        let all = dates(&r, start.date(), start.date() + Duration::days(36_600));
        r.end_date = all.get(c.saturating_sub(1) as usize).or(all.last()).map(|d| local_to_minutes(d.and_hms_opt(0, 0, 0).unwrap())).unwrap_or(r.start_date);
    } else if let Some(u) = kv.get("UNTIL") {
        let until = time_of(&Prop { name: String::new(), params: HashMap::new(), value: u.clone() }).ok_or("RRULE : UNTIL illisible")?.0;
        r.end = End::Date;
        let last = dates(&r, start.date(), until.date()).into_iter().filter(|d| d.and_hms_opt(0, 0, 0).unwrap() + Duration::minutes(i64::from(start_min)) <= until || until.time() == chrono::NaiveTime::MIN).last();
        r.end_date = last.map(|d| local_to_minutes(d.and_hms_opt(0, 0, 0).unwrap())).unwrap_or(r.start_date);
    }
    Ok(r)
}

fn text(props: &[Prop], name: &str) -> Option<String> {
    props.iter().find(|p| p.name == name).map(|p| unescape(&p.value)).filter(|s| !s.trim().is_empty())
}

/// Calendrier importé : nom (`X-WR-CALNAME`) et rendez-vous. `folder_id` et les identifiants sont fournis par
/// l'appelant (calendriers importés : négatifs, pour ne pas croiser ceux de la base).
pub fn parse(text_in: &str, folder_id: i64, first_id: i64) -> Result<(Option<String>, Vec<Appointment>), String> {
    let name = text_in
        .lines()
        .find_map(|l| l.strip_prefix("X-WR-CALNAME:"))
        .map(|s| unescape(s.trim()));
    let mut masters: Vec<Appointment> = Vec::new();
    let mut by_uid: HashMap<String, usize> = HashMap::new();
    let mut overrides: BTreeMap<String, Vec<(NaiveDateTime, Vec<Prop>)>> = BTreeMap::new();
    let mut next = first_id;
    for (props, subs) in vevents(text_in) {
        let get = |n: &str| props.iter().find(|p| p.name == n);
        let Some((start, all_day)) = get("DTSTART").and_then(time_of) else { continue };
        let end = match (get("DTEND").and_then(time_of), get("DURATION").and_then(|p| duration(&p.value))) {
            (Some((e, _)), _) => e,
            (None, Some(d)) => start + d,
            (None, None) if all_day => start + Duration::days(1),
            _ => start,
        };
        let uid = get("UID").map(|p| p.value.clone()).unwrap_or_else(|| format!("sans-uid-{next}"));
        if let Some(rid) = get("RECURRENCE-ID").and_then(time_of) {
            overrides.entry(uid).or_default().push((rid.0, props.clone()));
            continue;
        }
        let busy = match get("X-MICROSOFT-CDO-BUSYSTATUS").map(|p| p.value.to_ascii_uppercase()).as_deref() {
            Some("FREE") => Busy::Free,
            Some("TENTATIVE") => Busy::Tentative,
            Some("OOF") => Busy::OutOfOffice,
            Some("WORKINGELSEWHERE") => Busy::WorkingElsewhere,
            Some(_) => Busy::Busy,
            None if get("TRANSP").is_some_and(|p| p.value.eq_ignore_ascii_case("TRANSPARENT")) => Busy::Free,
            None => Busy::Busy,
        };
        let reminder = subs.iter().find_map(|s| s.iter().find(|p| p.name == "TRIGGER").and_then(|p| duration(&p.value))).map(|d| -d.num_minutes());
        let mut recurrence = match get("RRULE") {
            Some(p) => Some(recurrence(&p.value, start, end)?),
            None => None,
        };
        if let Some(r) = &mut recurrence {
            for ex in props.iter().filter(|p| p.name == "EXDATE") {
                for v in ex.value.split(',') {
                    if let Some((t, _)) = time_of(&Prop { name: String::new(), params: ex.params.clone(), value: v.to_string() }) {
                        r.deleted.push(local_to_minutes(t.date().and_hms_opt(0, 0, 0).unwrap()));
                    }
                }
            }
        }
        let a = Appointment {
            id: next,
            uid: Some(uid.clone()),
            source_id: 0,
            folder_id,
            subject: text(&props, "SUMMARY").unwrap_or_default(),
            location: text(&props, "LOCATION"),
            start_ms: utc_ms(start),
            end_ms: utc_ms(end),
            all_day,
            busy,
            recurrence,
            time_zone: None,
            time_zone_name: get("DTSTART").and_then(|p| p.params.get("TZID").cloned()),
            reminder_minutes: reminder,
            reminder_delta: reminder,
            categories: get("CATEGORIES").map(|p| p.value.split(',').map(unescape).filter(|c| !c.is_empty()).collect()).unwrap_or_default(),
            is_meeting: get("ORGANIZER").is_some(),
            organizer: get("ORGANIZER").and_then(|p| p.params.get("CN").cloned()).unwrap_or_default(),
            attendees: props.iter().filter(|p| p.name == "ATTENDEE").filter_map(|p| p.params.get("CN").cloned()).collect::<Vec<_>>().join("; "),
            response_status: 0,
            private: get("CLASS").is_some_and(|p| p.value.eq_ignore_ascii_case("PRIVATE")),
            sensitivity: match get("CLASS").map(|p| p.value.to_ascii_uppercase()).as_deref() {
                Some("PRIVATE") => 2,
                Some("CONFIDENTIAL") => 3,
                _ => 0,
            },
            importance: match get("PRIORITY").and_then(|p| p.value.parse::<u32>().ok()) {
                Some(1..=4) => 2,
                Some(6..=9) => 0,
                _ => 1,
            },
            eml_path: None,
            description: text(&props, "DESCRIPTION"),
        };
        next -= 1;
        by_uid.insert(uid, masters.len());
        masters.push(a);
    }
    // Occurrences modifiées : exceptions de leur série (date d'origine retirée, nouvelle date ajoutée).
    for (uid, list) in overrides {
        let Some(&i) = by_uid.get(&uid) else { continue };
        let master = &mut masters[i];
        let master_subject = master.subject.clone();
        let master_location = master.location.clone();
        let Some(r) = &mut master.recurrence else { continue };
        for (rid, props) in list {
            let get = |n: &str| props.iter().find(|p| p.name == n);
            let Some((start, all_day)) = get("DTSTART").and_then(time_of) else { continue };
            let end = get("DTEND").and_then(time_of).map(|x| x.0).unwrap_or(start + Duration::minutes(i64::from(r.end_offset - r.start_offset)));
            let original_day = local_to_minutes(rid.date().and_hms_opt(0, 0, 0).unwrap());
            r.deleted.push(original_day);
            r.modified.push(local_to_minutes(start.date().and_hms_opt(0, 0, 0).unwrap()));
            let subject = text(&props, "SUMMARY");
            let location = text(&props, "LOCATION");
            r.exceptions.push(Exception {
                start: local_to_minutes(start),
                end: local_to_minutes(end),
                original_start: local_to_minutes(rid),
                subject: subject.filter(|s| *s != master_subject),
                location: location.filter(|l| Some(l) != master_location.as_ref()),
                all_day: Some(all_day),
                ..Default::default()
            });
        }
        r.deleted.sort_unstable();
        r.deleted.dedup();
    }
    Ok((name, masters))
}
