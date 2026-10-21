//! Étape 1 du J9, contrôle croisé : chaque événement d'un export `.ics` d'Outlook est cherché par son UID parmi les
//! rendez-vous lus dans la base ; début, objet, journée entière et périodicité doivent concorder.
//! Usage : `cargo run -p thunderlook-calendar --example crosscheck_ics -- <dossier de données> <fichier .ics>`

use std::{collections::HashMap, path::Path};

use chrono::NaiveDateTime;
use thunderlook_calendar::{recur::Frequency, tz::TimeZone};

/// Lignes dépliées (RFC 5545 §3.1) d'un VEVENT : (nom avec paramètres, valeur).
type Event = Vec<(String, String)>;

fn events(text: &str) -> Vec<Event> {
    let mut lines: Vec<String> = Vec::new();
    for l in text.lines() {
        match l.strip_prefix([' ', '\t']) {
            Some(rest) if !lines.is_empty() => lines.last_mut().unwrap().push_str(rest),
            _ => lines.push(l.to_string()),
        }
    }
    let (mut out, mut cur, mut depth) = (Vec::new(), Vec::new(), 0);
    for l in lines {
        match l.as_str() {
            "BEGIN:VEVENT" => {
                depth = 1;
                cur = Vec::new();
            }
            "END:VEVENT" => {
                depth = 0;
                out.push(std::mem::take(&mut cur));
            }
            l if l.starts_with("BEGIN:") && depth > 0 => depth += 1,
            l if l.starts_with("END:") && depth > 1 => depth -= 1,
            _ if depth == 1 => {
                if let Some((k, v)) = l.split_once(':') {
                    cur.push((k.to_string(), v.to_string()));
                }
            }
            _ => {}
        }
    }
    out
}

fn get<'a>(e: &'a Event, name: &str) -> Option<(&'a str, &'a str)> {
    e.iter().find(|(k, _)| k == name || k.starts_with(&format!("{name};"))).map(|(k, v)| (k.as_str(), v.as_str()))
}

/// DTSTART → millisecondes UTC ; journée entière si `VALUE=DATE`.
fn start_ms(key: &str, value: &str) -> Option<(i64, bool)> {
    if key.contains("VALUE=DATE") {
        let d = NaiveDateTime::parse_from_str(&format!("{value}T000000"), "%Y%m%dT%H%M%S").ok()?;
        return Some((TimeZone::PARIS.to_utc(d).and_utc().timestamp_millis(), true));
    }
    let t = NaiveDateTime::parse_from_str(value.trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()?;
    let utc = if value.ends_with('Z') { t } else { TimeZone::PARIS.to_utc(t) };
    Some((utc.and_utc().timestamp_millis(), false))
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let conn = thunderlook_store::open(Path::new(&args[0])).map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(&args[1]).map_err(|e| e.to_string())?;
    let cal = thunderlook_calendar::load(&conn)?;
    let mut by_uid: HashMap<&str, Vec<&thunderlook_calendar::Appointment>> = HashMap::new();
    for a in &cal.appointments {
        if let Some(u) = &a.uid {
            by_uid.entry(u.as_str()).or_default().push(a);
        }
    }
    let evs = events(&text);
    let (mut masters, mut found, mut ok, mut exc, mut exc_ok) = (0, 0, 0, 0, 0);
    let mut problems: Vec<String> = Vec::new();
    for e in &evs {
        let uid = get(e, "UID").map(|x| x.1).unwrap_or("");
        // Texte iCalendar (RFC 5545 §3.3.11) : « \, », « \; », « \\ », « \n ».
        let summary = get(e, "SUMMARY").map(|x| x.1).unwrap_or("").replace("\\n", "\n").replace("\\N", "\n").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\");
        let summary = summary.as_str();
        let Some((k, v)) = get(e, "DTSTART") else { continue };
        let Some((start, all_day)) = start_ms(k, v) else {
            problems.push(format!("DTSTART illisible : {v}"));
            continue;
        };
        let candidates = by_uid.get(uid).cloned().unwrap_or_default();
        if let Some((_, rid)) = get(e, "RECURRENCE-ID") {
            // Occurrence modifiée : la série doit porter une exception à cette date d'origine.
            exc += 1;
            let date = &rid[..8];
            let hit = candidates.iter().filter_map(|a| a.recurrence.as_ref()).any(|r| {
                r.exceptions.iter().any(|x| thunderlook_calendar::recur::minutes_to_local(x.original_start).format("%Y%m%d").to_string() == date)
            });
            if hit {
                exc_ok += 1;
            } else {
                problems.push(format!("exception absente : « {summary} » au {date}"));
            }
            continue;
        }
        masters += 1;
        // Même UID dans plusieurs calendriers (copie) : il suffit qu'un des rendez-vous concorde.
        let Some(_) = candidates.first() else {
            problems.push(format!("UID introuvable : « {summary} » ({v})"));
            continue;
        };
        found += 1;
        let rrule = get(e, "RRULE").map(|x| x.1);
        let matches = |a: &&thunderlook_calendar::Appointment| {
            let freq_ok = match (rrule, &a.recurrence) {
                (None, None) => true,
                (Some(rr), Some(r)) => {
                    let f = match r.frequency {
                        Frequency::Daily => "DAILY",
                        Frequency::Weekly => "WEEKLY",
                        Frequency::Monthly => "MONTHLY",
                        Frequency::Yearly => "YEARLY",
                    };
                    // Outlook écrit une série annuelle en MONTHLY;INTERVAL=12 dans le blob, YEARLY dans l'iCalendar.
                    rr.contains(&format!("FREQ={f}")) || (f == "MONTHLY" && rr.contains("FREQ=YEARLY"))
                }
                _ => false,
            };
            a.start_ms == start && a.all_day == all_day && a.subject.replace("\r\n", "\n").trim() == summary.trim() && freq_ok
        };
        if candidates.iter().any(matches) {
            ok += 1;
        } else {
            let a = candidates[0];
            problems.push(format!(
                "écart « {summary} » : ics début {start} jour={all_day} rrule={rrule:?} | base début {} jour={} objet « {} » série={:?}",
                a.start_ms,
                a.all_day,
                a.subject,
                a.recurrence.as_ref().map(|r| r.frequency)
            ));
        }
    }
    println!("événements : {} ; principaux : {masters} ; trouvés par UID : {found} ; concordants : {ok}", evs.len());
    println!("occurrences modifiées : {exc} ; retrouvées dans la série : {exc_ok}");
    for p in problems.iter().take(25) {
        println!("  {p}");
    }
    if problems.len() > 25 {
        println!("  … {} autres", problems.len() - 25);
    }
    Ok(())
}
