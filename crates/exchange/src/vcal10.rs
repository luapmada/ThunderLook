//! vCalendar 1.0 (`VERSION:1.0`, celui que MPE écrit dans `Organizer.dat`) → iCalendar 2.0, pour la lecture de
//! `calendrier.ics` (lot AE).
//!
//! La conversion produit un VCALENDAR 2.0 que relisent tels quels `ics_series::split` (séries) et `ics::parse`
//! (rendez-vous simples) : les refus du modèle (lot AA) s'appliquent donc aux séries venues de 1.0 comme aux autres.
//! Un rendez-vous dont le sens ne peut pas être conservé (règle de récurrence hors modèle, date illisible, `RDATE`…) est
//! refusé en entier, avec un avertissement : jamais importé de travers, jamais ignoré en silence.
//!
//! Lu : `DTSTART`/`DTEND` (UTC `Z`, locaux, journée entière), `SUMMARY`/`LOCATION`/`DESCRIPTION` (quoted-printable,
//! UTF-8 ou WINDOWS-1252), `CATEGORIES` (séparées par `;`), `CLASS`, `TRANSP`, `AALARM`/`DALARM`/`PALARM`/`MALARM`,
//! `RRULE` 1.0 (`D`, `W`, `MD`, `MP`, `YM`, `YD`, fin `#N` ou date), `EXDATE`, identifiant (`UID`, sinon
//! `X-OUTLOOK-UID`, sinon `X-IRMC-LUID`). Les autres propriétés sont écartées (participants : signalés).

use chrono::{Datelike, NaiveDate, NaiveDateTime};

pub struct Converted {
    pub ics: String,
    /// Sans nom de fichier : l'appelant les préfixe.
    pub warnings: Vec<String>,
}

/// Le texte est-il un vCalendar 1.0 ? (`VERSION:1.0` avant le premier composant.)
pub fn is_vcal10(text: &str) -> bool {
    text.trim_start_matches('\u{feff}')
        .lines()
        .map(|l| l.trim().to_ascii_uppercase())
        .take_while(|l| l != "BEGIN:VEVENT" && l != "BEGIN:VTODO")
        .any(|l| l == "VERSION:1.0")
}

#[derive(Clone, Debug)]
struct Prop {
    name: String,
    qp: bool,
    charset: Option<String>,
    value: String,
}

/// Lignes logiques : repliement par espace/tabulation, coupures douces du quoted-printable (`=` en fin de ligne).
fn unfold(text: &str) -> Vec<String> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    let pending = |l: &str| l.ends_with('=') && l.split(':').next().is_some_and(|h| h.to_ascii_uppercase().contains("QUOTED-PRINTABLE"));
    let mut out: Vec<String> = Vec::new();
    let mut open_qp = false;
    for raw in text.split('\n') {
        if open_qp {
            if let Some(cur) = out.last_mut() {
                cur.pop();
                cur.push_str(raw);
                open_qp = pending(cur);
                continue;
            }
        }
        if raw.starts_with([' ', '\t']) && !out.is_empty() {
            let cur = out.last_mut().expect("non vide");
            cur.push_str(&raw[1..]);
            open_qp = pending(cur);
        } else if !raw.trim().is_empty() {
            out.push(raw.to_string());
            open_qp = pending(raw);
        }
    }
    out
}

fn prop_of(line: &str) -> Option<Prop> {
    let (head, value) = line.split_once(':')?;
    let mut parts = head.split(';');
    let name = parts.next()?.rsplit('.').next()?.trim().to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }
    let (mut qp, mut charset) = (false, None);
    for p in parts {
        let up = p.trim().to_ascii_uppercase();
        match up.split_once('=') {
            Some(("ENCODING", v)) => qp |= v == "QUOTED-PRINTABLE",
            Some(("CHARSET", v)) => charset = Some(v.to_string()),
            None if up == "QUOTED-PRINTABLE" => qp = true,
            _ => {}
        }
    }
    Some(Prop { name, qp, charset, value: value.to_string() })
}

const CP1252: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ',
    '\u{9d}', 'ž', 'Ÿ',
];

fn cp1252(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| if (0x80..0xA0).contains(&b) { CP1252[(b - 0x80) as usize] } else { b as char }).collect()
}

fn qp_bytes(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let (mut out, mut i) = (Vec::with_capacity(b.len()), 0);
    while i < b.len() {
        if b[i] == b'=' && i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() {
            out.push(u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("00"), 16).unwrap_or(b'?'));
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

/// Texte d'une propriété ; octets non UTF-8 (ou jeu inconnu) lus en WINDOWS-1252, avec avertissement.
fn text_of(p: &Prop, warnings: &mut Vec<String>) -> String {
    if !p.qp {
        return p.value.clone();
    }
    let bytes = qp_bytes(&p.value);
    let latin = |b: &[u8]| b.iter().map(|&c| c as char).collect::<String>();
    let text = match p.charset.as_deref().unwrap_or("UTF-8") {
        "UTF-8" | "UTF8" | "US-ASCII" | "ASCII" => String::from_utf8(bytes.clone()).unwrap_or_else(|_| {
            warnings.push(format!("{} : octets non UTF-8 lus en WINDOWS-1252", p.name));
            cp1252(&bytes)
        }),
        "WINDOWS-1252" | "CP1252" => cp1252(&bytes),
        "ISO-8859-1" | "LATIN1" | "ISO8859-1" => latin(&bytes),
        other => {
            warnings.push(format!("{} : jeu de caractères {other} inconnu, lu en UTF-8 ou WINDOWS-1252", p.name));
            String::from_utf8(bytes.clone()).unwrap_or_else(|_| cp1252(&bytes))
        }
    };
    text.replace('\r', "")
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace('\n', "\\n")
}

/// Repli à 75 octets, sur une frontière de caractère.
fn fold(out: &mut String, line: &str) {
    let (mut n, mut max) = (0, 75);
    for c in line.chars() {
        if n + c.len_utf8() > max {
            out.push_str("\r\n ");
            (n, max) = (0, 74);
        }
        out.push(c);
        n += c.len_utf8();
    }
    out.push_str("\r\n");
}

// --- Dates ------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
struct Date {
    at: NaiveDateTime,
    utc: bool,
    date_only: bool,
}

fn date(v: &str) -> Option<Date> {
    let v = v.trim();
    if v.len() == 8 {
        return Some(Date { at: NaiveDate::parse_from_str(v, "%Y%m%d").ok()?.and_hms_opt(0, 0, 0)?, utc: false, date_only: true });
    }
    let utc = v.ends_with(['Z', 'z']);
    let at = NaiveDateTime::parse_from_str(v.trim_end_matches(['Z', 'z']), "%Y%m%dT%H%M%S").ok()?;
    Some(Date { at, utc, date_only: false })
}

fn render(d: &Date) -> String {
    if d.date_only { d.at.format("%Y%m%d").to_string() } else { format!("{}{}", d.at.format("%Y%m%dT%H%M%S"), if d.utc { "Z" } else { "" }) }
}

// --- Récurrence -------------------------------------------------------------------------------------------------------

const DAYS: [&str; 7] = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"];

fn is_day(t: &str) -> bool {
    DAYS.contains(&t)
}

/// `D1 #0`, `W1 MO TU #10`, `MD1 15 20261231T000000Z`, `MP1 1+ MO #0`, `YM1 10 #0`, `YD1 #3` → RRULE 2.0.
fn rrule(rule: &str, start: &Date) -> Result<String, String> {
    let up = rule.to_ascii_uppercase();
    let tokens: Vec<&str> = up.split_whitespace().collect();
    let (head, rest) = tokens.split_first().ok_or("règle de récurrence vide")?;
    let split = head.find(|c: char| c.is_ascii_digit()).ok_or_else(|| format!("règle de récurrence « {rule} » illisible"))?;
    let (kind, interval) = head.split_at(split);
    let interval: u32 = interval.parse().ok().filter(|n| *n >= 1).ok_or_else(|| format!("intervalle de « {rule} » illisible"))?;
    // Fin : `#N` (0 = sans fin), une date ; absente, la norme 1.0 compte 2 occurrences.
    let (mods, end) = match rest.split_last() {
        Some((last, mods)) if last.starts_with('#') => {
            let n: u32 = last[1..].parse().map_err(|_| format!("fin « {last} » illisible"))?;
            (mods, if n == 0 { String::new() } else { format!(";COUNT={n}") })
        }
        Some((last, mods)) if date(last).is_some() => (mods, format!(";UNTIL={}", render(&date(last).expect("date")))),
        _ => (rest, ";COUNT=2".to_string()),
    };
    let every = if interval > 1 { format!(";INTERVAL={interval}") } else { String::new() };
    let int = |t: &str| t.parse::<i32>().map_err(|_| format!("« {t} » illisible dans « {rule} »"));
    let body = match kind {
        "D" if mods.is_empty() => format!("FREQ=DAILY{every}"),
        "D" => return Err(format!("modificateurs inattendus dans « {rule} »")),
        "W" => {
            if let Some(bad) = mods.iter().find(|t| !is_day(t)) {
                return Err(format!("jour « {bad} » illisible dans « {rule} »"));
            }
            format!("FREQ=WEEKLY{every}{}", if mods.is_empty() { String::new() } else { format!(";BYDAY={}", mods.join(",")) })
        }
        "MD" => {
            let days = mods
                .iter()
                .map(|t| match *t {
                    "LD" => Ok("-1".to_string()),
                    t if t.ends_with('-') => int(&t[..t.len() - 1]).map(|n| (-n).to_string()),
                    t => int(t.trim_end_matches('+')).map(|n| n.to_string()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            format!("FREQ=MONTHLY{every}{}", if days.is_empty() { String::new() } else { format!(";BYMONTHDAY={}", days.join(",")) })
        }
        "MP" => {
            let (mut rank, mut by) = (None, Vec::new());
            for t in mods {
                if is_day(t) {
                    by.push(format!("{}{t}", rank.ok_or_else(|| format!("jour sans rang dans « {rule} »"))?));
                } else {
                    let n = int(&t[..t.len().saturating_sub(1)])?;
                    rank = Some(match t.chars().last() {
                        Some('+') => n,
                        Some('-') => -n,
                        _ => return Err(format!("rang « {t} » illisible dans « {rule} »")),
                    });
                }
            }
            if by.is_empty() {
                return Err(format!("« {rule} » sans jour de la semaine"));
            }
            format!("FREQ=MONTHLY{every};BYDAY={}", by.join(","))
        }
        "YM" => {
            let months = mods.iter().map(|t| int(t).map(|n| n.to_string())).collect::<Result<Vec<_>, _>>()?;
            format!("FREQ=YEARLY{every}{}", if months.is_empty() { String::new() } else { format!(";BYMONTH={}", months.join(",")) })
        }
        "YD" => {
            // Un seul jour de l'année, celui du début : l'annuel du modèle (même date chaque année).
            let days = mods.iter().map(|t| int(t)).collect::<Result<Vec<_>, _>>()?;
            if days.iter().any(|d| *d != start.at.ordinal() as i32) || days.len() > 1 {
                return Err(format!("« {rule} » : jour de l'année différent de celui du début, non pris en charge"));
            }
            format!("FREQ=YEARLY{every}")
        }
        _ => return Err(format!("règle de récurrence « {rule} » non reconnue")),
    };
    Ok(format!("{body}{end}"))
}

// --- Rendez-vous ------------------------------------------------------------------------------------------------------

/// Minutes entre le rappel (`AALARM:20261023T210000Z;;;`) et le début ; `None` : pas de rappel.
fn alarm(p: &Prop, start: &Date) -> Result<Option<i64>, String> {
    let run = p.value.split(';').next().unwrap_or("").trim();
    if run.is_empty() {
        return Ok(None);
    }
    let run = date(run).ok_or_else(|| format!("{} « {run} » illisible", p.name))?;
    if run.utc != start.utc && !start.date_only {
        return Err(format!("{} : heure du rappel et du début dans des repères différents (UTC / locale)", p.name));
    }
    if start.date_only && run.utc {
        return Err(format!("{} en UTC sur une journée entière", p.name));
    }
    let minutes = (start.at - run.at).num_minutes();
    if minutes < 0 {
        return Err(format!("{} après le début du rendez-vous", p.name));
    }
    Ok(Some(minutes))
}

fn event(props: &[Prop], warnings: &mut Vec<String>) -> Result<String, String> {
    let get = |n: &str| props.iter().find(|p| p.name == n);
    let all = |n: &'static str| props.iter().filter(move |p| p.name == n);
    let one = |n: &str| get(n).map(|p| p.value.trim().to_string()).filter(|v| !v.is_empty());
    let mut local = Vec::new();
    let text = |n: &str, local: &mut Vec<String>| get(n).map(|p| text_of(p, local)).filter(|v| !v.trim().is_empty());

    let start_raw = one("DTSTART").ok_or("sans DTSTART")?;
    let start = date(&start_raw).ok_or_else(|| format!("DTSTART « {start_raw} » illisible"))?;
    let end = match one("DTEND") {
        None => None,
        Some(v) => Some(date(&v).ok_or_else(|| format!("DTEND « {v} » illisible"))?),
    };
    if end.is_some_and(|e| e.date_only != start.date_only) {
        return Err("DTSTART et DTEND de natures différentes (journée entière / heure)".into());
    }
    // Journée entière : `DTEND` y est exclusif en iCal 2.0 ; une fin absente, égale ou antérieure au début = un jour.
    let end = match end {
        Some(e) if !(start.date_only && e.at <= start.at) => e,
        _ if start.date_only => Date { at: start.at + chrono::Duration::days(1), ..start },
        _ => start,
    };

    if all("RRULE").count() > 1 {
        return Err("plusieurs règles de récurrence".into());
    }
    if get("RDATE").is_some() {
        return Err("RDATE (dates ajoutées à la main) non pris en charge".into());
    }
    if get("EXRULE").is_some() {
        return Err("EXRULE non pris en charge".into());
    }
    let recurrence = get("RRULE").map(|p| rrule(&p.value, &start)).transpose()?;
    let mut exdates = Vec::new();
    for p in all("EXDATE") {
        for v in p.value.split([';', ',']).map(str::trim).filter(|v| !v.is_empty()) {
            exdates.push(render(&date(v).ok_or_else(|| format!("EXDATE « {v} » illisible"))?));
        }
    }
    if recurrence.is_none() && !exdates.is_empty() {
        return Err("EXDATE sans règle de récurrence".into());
    }

    let mut out = String::from("BEGIN:VEVENT\r\n");
    let uid = one("UID").or_else(|| one("X-OUTLOOK-UID")).or_else(|| one("X-IRMC-LUID"));
    if let Some(uid) = &uid {
        fold(&mut out, &format!("UID:{uid}"));
    }
    for n in ["X-OUTLOOK-UID", "X-IRMC-LUID"] {
        if let Some(v) = one(n).filter(|v| Some(v) != uid.as_ref()) {
            fold(&mut out, &format!("{n}:{v}"));
        }
    }
    let prefix = if start.date_only { ";VALUE=DATE" } else { "" };
    fold(&mut out, &format!("DTSTART{prefix}:{}", render(&start)));
    fold(&mut out, &format!("DTEND{prefix}:{}", render(&end)));
    for n in ["SUMMARY", "LOCATION", "DESCRIPTION"] {
        if let Some(v) = text(n, &mut local) {
            fold(&mut out, &format!("{n}:{}", escape(&v)));
        }
    }
    if let Some(p) = get("CATEGORIES") {
        let v = text_of(p, &mut local);
        let cats: Vec<String> = v.split(';').map(|c| escape(c.trim())).filter(|c| !c.is_empty()).collect();
        if !cats.is_empty() {
            fold(&mut out, &format!("CATEGORIES:{}", cats.join(",")));
        }
    }
    match one("CLASS").map(|v| v.to_ascii_uppercase()).as_deref() {
        Some("1" | "PRIVATE") => out.push_str("CLASS:PRIVATE\r\n"),
        Some("2" | "CONFIDENTIAL") => out.push_str("CLASS:CONFIDENTIAL\r\n"),
        _ => {}
    }
    match one("TRANSP").map(|v| v.to_ascii_uppercase()).as_deref() {
        Some("1" | "TRANSPARENT") => out.push_str("TRANSP:TRANSPARENT\r\n"),
        Some("0" | "OPAQUE") => out.push_str("TRANSP:OPAQUE\r\n"),
        _ => {}
    }
    if let Some(r) = recurrence {
        fold(&mut out, &format!("RRULE:{r}"));
    }
    if !exdates.is_empty() {
        fold(&mut out, &format!("EXDATE:{}", exdates.join(",")));
    }
    let alarms: Vec<&Prop> = props.iter().filter(|p| matches!(p.name.as_str(), "AALARM" | "DALARM" | "PALARM" | "MALARM")).collect();
    for p in alarms.iter().take(1) {
        match alarm(p, &start) {
            Ok(Some(m)) => out.push_str(&format!("BEGIN:VALARM\r\nTRIGGER:-PT{m}M\r\nACTION:DISPLAY\r\nDESCRIPTION:Reminder\r\nEND:VALARM\r\n")),
            Ok(None) => {}
            Err(why) => local.push(format!("rappel non importé ({why})")),
        }
    }
    out.push_str("END:VEVENT\r\n");
    warnings.append(&mut local);
    Ok(out)
}

/// Convertit un vCalendar 1.0 en iCalendar 2.0 (voir le module).
pub fn to_ical(text: &str) -> Converted {
    let mut warnings = Vec::new();
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//ThunderLook//vCalendar 1.0//FR\r\n");
    let mut current: Option<Vec<Prop>> = None;
    let (mut zone, mut in_todo, mut todos, mut guests, mut local_times) = (false, false, 0usize, 0usize, 0usize);
    for line in &unfold(text) {
        let up = line.trim().to_ascii_uppercase();
        if up == "BEGIN:VEVENT" {
            current = Some(Vec::new());
        } else if up == "END:VEVENT" {
            let Some(props) = current.take() else { continue };
            guests += usize::from(props.iter().any(|p| p.name == "ATTENDEE"));
            let label = props.iter().find(|p| p.name == "SUMMARY").map(|p| text_of(p, &mut Vec::new())).unwrap_or_default();
            let when = props.iter().find(|p| p.name == "DTSTART").map(|p| p.value.trim().to_string()).unwrap_or_default();
            local_times += usize::from(date(&when).is_some_and(|d| !d.utc && !d.date_only));
            let mut mine = Vec::new();
            match event(&props, &mut mine) {
                Ok(v) => {
                    out.push_str(&v);
                    warnings.extend(mine.into_iter().map(|w| format!("« {label} » ({when}) : {w}")));
                }
                Err(why) => warnings.push(format!("rendez-vous « {label} » ({when}) non importé : {why}")),
            }
        } else if up == "BEGIN:VTODO" {
            (in_todo, todos) = (true, todos + 1);
        } else if up == "END:VTODO" {
            in_todo = false;
        } else if let Some(props) = current.as_mut() {
            props.extend(prop_of(line));
        } else if !in_todo {
            zone |= prop_of(line).is_some_and(|p| matches!(p.name.as_str(), "TZ" | "DAYLIGHT"));
        }
    }
    out.push_str("END:VCALENDAR\r\n");
    if todos > 0 {
        warnings.push(format!("{todos} tâche(s) (VTODO) du calendrier non importée(s) : les tâches se lisent dans taches.ics"));
    }
    if guests > 0 {
        warnings.push(format!("participants (ATTENDEE) de {guests} rendez-vous non importés"));
    }
    if zone && local_times > 0 {
        warnings.push(format!("{local_times} heure(s) sans « Z » lue(s) en heure de Paris : les propriétés TZ/DAYLIGHT du fichier ne sont pas appliquées"));
    }
    Converted { ics: out, warnings }
}
