//! Étape 3 du J9 : vue d'une période (Jour, Semaine de travail, Semaine, Mois) et navigateur de dates, calculés ici
//! en heure de Paris ; l'interface ne fait que les afficher.

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use thunderlook_model::calendar::{
    CalendarDay, CalendarEvent, CalendarInfo, CalendarQuery, CalendarView, CalendarViewKind, MONTHS, MONTHS_SHORT, NavDay, NavMonth, TimedEvent,
    WEEKDAYS,
};
use thunderlook_store::rusqlite::Connection;

use crate::{Appointment, Busy, Calendar, expand::{Occurrence, occurrences}, tz::TimeZone};

const TZ: TimeZone = TimeZone::PARIS;
const DAY_MIN: i64 = 1_440;

/// Calendriers de la base : dossiers de rendez-vous, avec leur fichier de données et leur nombre d'éléments.
pub fn calendars(conn: &Connection) -> crate::Result<Vec<CalendarInfo>> {
    let e = |x: thunderlook_store::rusqlite::Error| x.to_string();
    let mut stmt = conn
        .prepare(
            "SELECT f.id, f.name, s.display_name,
                    (SELECT COUNT(*) FROM message m WHERE m.folder_id = f.id)
             FROM folder f JOIN source s ON s.id = f.source_id
             WHERE f.container_class LIKE 'IPF.Appointment%' AND f.hidden = 0
             ORDER BY f.source_id, f.parent_id, f.position, f.id",
        )
        .map_err(e)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))
        .map_err(e)?;
    let mut out = Vec::new();
    for (i, r) in rows.enumerate() {
        let (id, name, section, count) = r.map_err(e)?;
        out.push(CalendarInfo { id: format!("c{id}"), name, section, color: (i % 8) as u8, count: count as u32, shown: false });
    }
    Ok(out)
}

/// Calendriers affichés par défaut : le premier « Calendrier » (ou « Calendar »), sinon le premier calendrier.
pub fn default_shown(list: &[CalendarInfo]) -> Vec<String> {
    list.iter()
        .find(|c| c.name.eq_ignore_ascii_case("Calendrier") || c.name.eq_ignore_ascii_case("Calendar"))
        .or(list.first())
        .map(|c| vec![c.id.clone()])
        .unwrap_or_default()
}

fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn local(ms: i64) -> NaiveDateTime {
    TZ.to_local(DateTime::from_timestamp_millis(ms).unwrap_or_default().naive_utc())
}

fn utc_ms(local: NaiveDateTime) -> i64 {
    TZ.to_utc(local).and_utc().timestamp_millis()
}

fn monday(d: NaiveDate) -> NaiveDate {
    d - Duration::days(i64::from(d.weekday().num_days_from_monday()))
}

fn add_months(d: NaiveDate, n: i32) -> NaiveDate {
    let k = d.year() * 12 + d.month0() as i32 + n;
    let (y, m) = (k.div_euclid(12), k.rem_euclid(12) as u32 + 1);
    let last = (28..=31).rev().find(|&x| NaiveDate::from_ymd_opt(y, m, x).is_some()).unwrap_or(28);
    NaiveDate::from_ymd_opt(y, m, d.day().min(last)).unwrap()
}

fn long_date(d: NaiveDate) -> String {
    format!("{} {} {} {}", WEEKDAYS[d.weekday().num_days_from_monday() as usize], d.day(), MONTHS[d.month0() as usize], d.year())
}

/// « 19 – 25 octobre 2026 », « 29 septembre – 5 octobre 2026 », « 29 décembre 2025 – 4 janvier 2026 ».
fn range_title(a: NaiveDate, b: NaiveDate) -> String {
    let m = |d: NaiveDate| MONTHS[d.month0() as usize];
    if a.year() != b.year() {
        format!("{} {} {} – {} {} {}", a.day(), m(a), a.year(), b.day(), m(b), b.year())
    } else if a.month() != b.month() {
        format!("{} {} – {} {} {}", a.day(), m(a), b.day(), m(b), b.year())
    } else {
        format!("{} – {} {} {}", a.day(), b.day(), m(b), b.year())
    }
}

fn hm(t: NaiveDateTime) -> String {
    format!("{:02}:{:02}", t.hour(), t.minute())
}

fn short_day(d: NaiveDate) -> String {
    format!("{} {}", d.day(), MONTHS_SHORT[d.month0() as usize])
}

/// Un morceau d'occurrence pour un jour donné.
struct Piece<'a> {
    a: &'a Appointment,
    o: &'a Occurrence,
    start: NaiveDateTime,
    end: NaiveDateTime,
}

fn event(p: &Piece, day: NaiveDate, color: u8, all_day_band: bool) -> CalendarEvent {
    let (a, o) = (p.a, p.o);
    let last_day = if o.all_day && p.end.time() == chrono::NaiveTime::MIN { p.end.date() - Duration::days(1) } else { p.end.date() };
    let when = if o.all_day && last_day == p.start.date() {
        "Toute la journée".to_string()
    } else if all_day_band || p.start.date() != p.end.date() {
        if o.all_day {
            format!("{} – {}", short_day(p.start.date()), short_day(last_day))
        } else {
            format!("{} {} – {} {}", short_day(p.start.date()), hm(p.start), short_day(p.end.date()), hm(p.end))
        }
    } else {
        format!("{} – {}", hm(p.start), hm(p.end))
    };
    CalendarEvent {
        id: match o.original_date {
            Some(d) => format!("a{}@{}", a.id, iso(d)),
            None => format!("a{}", a.id),
        },
        calendar_id: format!("c{}", a.folder_id),
        color,
        subject: if o.subject.trim().is_empty() { "(sans objet)".into() } else { o.subject.split(['\r', '\n']).map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ") },
        location: o.location.clone().filter(|l| !l.trim().is_empty()),
        when,
        start_label: if o.all_day || all_day_band { String::new() } else { hm(p.start) },
        recurring: a.recurrence.is_some(),
        exception: o.exception,
        private: a.private,
        busy: match a.busy {
            Busy::Free => "free",
            Busy::Tentative => "tentative",
            Busy::Busy => "busy",
            Busy::OutOfOffice => "oof",
            Busy::WorkingElsewhere => "elsewhere",
        }
        .into(),
        categories: a.categories.clone(),
        reminder_minutes: a.reminder_minutes,
        organizer: a.organizer.clone(),
        attendees: a.attendees.clone(),
        is_meeting: a.is_meeting,
        continued: p.start.date() < day,
    }
}

/// Colonnes des rendez-vous qui se chevauchent dans une journée (glouton, groupe par groupe).
fn lay_out(mut items: Vec<(u32, u32, CalendarEvent)>) -> Vec<TimedEvent> {
    items.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    let mut out: Vec<TimedEvent> = Vec::new();
    let mut group_start = 0;
    let mut ends: Vec<u32> = Vec::new();
    let mut group_end = 0;
    for (start, end, event) in items {
        // Un rendez-vous de durée nulle occupe tout de même un quart d'heure à l'écran.
        let visible_end = end.max(start + 15);
        if !out.is_empty() && start >= group_end {
            let n = ends.len() as u8;
            out[group_start..].iter_mut().for_each(|t| t.columns = n);
            group_start = out.len();
            ends.clear();
        }
        let column = match ends.iter().position(|&e| e <= start) {
            Some(c) => {
                ends[c] = visible_end;
                c
            }
            None => {
                ends.push(visible_end);
                ends.len() - 1
            }
        };
        group_end = if out.len() == group_start { visible_end } else { group_end.max(visible_end) };
        out.push(TimedEvent { event, start_min: start, end_min: end, column: column as u8, columns: 1 });
    }
    let n = ends.len().max(1) as u8;
    out[group_start..].iter_mut().for_each(|t| t.columns = n);
    out
}

/// Période affichée : premier et dernier jour.
fn period(kind: CalendarViewKind, anchor: NaiveDate) -> (NaiveDate, NaiveDate) {
    match kind {
        CalendarViewKind::Day => (anchor, anchor),
        CalendarViewKind::WorkWeek => (monday(anchor), monday(anchor) + Duration::days(4)),
        CalendarViewKind::Week => (monday(anchor), monday(anchor) + Duration::days(6)),
        CalendarViewKind::Month => {
            let first = monday(anchor.with_day(1).unwrap());
            (first, first + Duration::days(41))
        }
    }
}

/// Occurrences des calendriers affichés qui chevauchent `[from, to]` (jours locaux inclus).
fn occurrences_in<'a>(cal: &'a Calendar, shown: &HashSet<i64>, from: NaiveDate, to: NaiveDate) -> Vec<(&'a Appointment, Occurrence)> {
    let (from_ms, to_ms) = (utc_ms(from.and_hms_opt(0, 0, 0).unwrap()), utc_ms((to + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap()));
    cal.appointments
        .iter()
        .filter(|a| shown.contains(&a.folder_id))
        .flat_map(|a| occurrences(a, from_ms, to_ms).into_iter().map(move |o| (a, o)))
        .collect()
}

pub fn build(cal: &Calendar, calendars: Vec<CalendarInfo>, query: &CalendarQuery, now: NaiveDateTime) -> CalendarView {
    let today = now.date();
    let anchor = NaiveDate::parse_from_str(&query.anchor, "%Y-%m-%d").unwrap_or(today);
    let kind = query.kind;
    let (first, last) = period(kind, anchor);
    let shown: HashSet<i64> = calendars.iter().filter(|c| c.shown).filter_map(|c| c.id[1..].parse().ok()).collect();
    let color = |folder: i64| calendars.iter().find(|c| c.id == format!("c{folder}")).map_or(0, |c| c.color);

    let occ = occurrences_in(cal, &shown, first, last);
    let mut all_day: BTreeMap<NaiveDate, Vec<CalendarEvent>> = BTreeMap::new();
    let mut timed: BTreeMap<NaiveDate, Vec<(u32, u32, CalendarEvent)>> = BTreeMap::new();
    for (a, o) in &occ {
        let (start, end) = (local(o.start_ms), local(o.end_ms));
        let p = Piece { a, o, start, end };
        // Comme la référence : journée entière, ou 24 h et plus, dans le bandeau du haut.
        let band = o.all_day || (end - start).num_minutes() >= DAY_MIN;
        let mut d = start.date().max(first);
        let stop = if end.time() == chrono::NaiveTime::MIN && end > start { end.date() - Duration::days(1) } else { end.date() };
        while d <= stop.min(last) {
            let ev = event(&p, d, color(a.folder_id), band);
            if band {
                all_day.entry(d).or_default().push(ev);
            } else {
                let s = if start.date() < d { 0 } else { start.hour() * 60 + start.minute() };
                let e = if end.date() > d { 1_440 } else { end.hour() * 60 + end.minute() };
                timed.entry(d).or_default().push((s, e, ev));
            }
            d += Duration::days(1);
        }
    }

    let month = anchor.month();
    let days: Vec<CalendarDay> = (0..=(last - first).num_days())
        .map(|i| first + Duration::days(i))
        .map(|d| {
            let mut band = all_day.remove(&d).unwrap_or_default();
            band.sort_by(|a, b| a.when.cmp(&b.when).then(a.subject.cmp(&b.subject)));
            let label = match kind {
                CalendarViewKind::Month if d.day() == 1 => short_day(d),
                CalendarViewKind::Month => d.day().to_string(),
                _ => format!("{} {}", WEEKDAYS[d.weekday().num_days_from_monday() as usize], d.day()),
            };
            CalendarDay {
                date: iso(d),
                label,
                today: d == today,
                in_month: kind != CalendarViewKind::Month || d.month() == month,
                all_day: band,
                timed: lay_out(timed.remove(&d).unwrap_or_default()),
            }
        })
        .collect();

    let (prev, next) = match kind {
        CalendarViewKind::Day => (anchor - Duration::days(1), anchor + Duration::days(1)),
        CalendarViewKind::WorkWeek | CalendarViewKind::Week => (anchor - Duration::days(7), anchor + Duration::days(7)),
        CalendarViewKind::Month => (add_months(anchor, -1), add_months(anchor, 1)),
    };
    let title = match kind {
        CalendarViewKind::Day => long_date(anchor),
        CalendarViewKind::WorkWeek | CalendarViewKind::Week => range_title(first, last),
        CalendarViewKind::Month => format!("{} {}", MONTHS[anchor.month0() as usize], anchor.year()),
    };

    // Navigateur : mois de la date de référence et le suivant, jours chargés en gras.
    let nav_first = monday(anchor.with_day(1).unwrap());
    let nav_last = monday(add_months(anchor.with_day(1).unwrap(), 1)) + Duration::days(41);
    let busy: HashSet<NaiveDate> = occurrences_in(cal, &shown, nav_first, nav_last)
        .iter()
        .flat_map(|(_, o)| {
            let (s, e) = (local(o.start_ms), local(o.end_ms));
            let stop = if e.time() == chrono::NaiveTime::MIN && e > s { e.date() - Duration::days(1) } else { e.date() };
            (0..=(stop - s.date()).num_days().clamp(0, 62)).map(move |i| s.date() + Duration::days(i))
        })
        .collect();
    let navigator = [anchor.with_day(1).unwrap(), add_months(anchor.with_day(1).unwrap(), 1)]
        .into_iter()
        .map(|m1| {
            let start = monday(m1);
            NavMonth {
                title: format!("{} {}", MONTHS[m1.month0() as usize], m1.year()),
                weeks: (0..6)
                    .map(|w| {
                        (0..7)
                            .map(|i| {
                                let d = start + Duration::days(w * 7 + i);
                                NavDay {
                                    date: iso(d),
                                    day: d.day(),
                                    in_month: d.month() == m1.month(),
                                    today: d == today,
                                    selected: d >= first && d <= last && (kind != CalendarViewKind::Month || d.month() == month),
                                    busy: busy.contains(&d),
                                }
                            })
                            .collect()
                    })
                    .collect(),
            }
        })
        .collect();

    CalendarView {
        kind,
        anchor: iso(anchor),
        title,
        prev: iso(prev),
        next: iso(next),
        today: iso(today),
        now_min: (today >= first && today <= last && kind != CalendarViewKind::Month).then(|| now.hour() * 60 + now.minute()),
        days,
        calendars,
        navigator,
        source_note: None,
    }
}

/// Option A du J9 : rappel que les calendriers affichés sont lus dans une copie du fichier de données, et de quand.
pub fn source_note(conn: &Connection, calendars: &[CalendarInfo]) -> crate::Result<Option<String>> {
    let ids: Vec<String> = calendars.iter().filter(|c| c.shown).map(|c| c.id[1..].to_string()).collect();
    if ids.is_empty() {
        return Ok(None);
    }
    // Date de lecture : dernier rechargement des calendriers (`tl-import --calendars`), sinon l'import de la source.
    let sql = format!(
        "SELECT COALESCE((SELECT json_extract(r.report_json, '$.originPath') FROM import_run r
                          WHERE r.source_id = s.id AND json_extract(r.report_json, '$.calendarRefresh') ORDER BY r.started_at DESC LIMIT 1), s.origin_path),
                COALESCE((SELECT MAX(r.started_at + r.duration_ms) FROM import_run r
                          WHERE r.source_id = s.id AND json_extract(r.report_json, '$.calendarRefresh')), s.imported_at)
         FROM source s WHERE s.id IN (SELECT source_id FROM folder WHERE id IN ({})) ORDER BY 2",
        ids.join(",")
    );
    let e = |x: thunderlook_store::rusqlite::Error| x.to_string();
    let mut stmt = conn.prepare(&sql).map_err(e)?;
    let rows: Vec<(String, i64)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map_err(e)?.collect::<Result<_, _>>().map_err(e)?;
    let parts: Vec<String> = rows
        .iter()
        .map(|(path, at)| {
            let name = std::path::Path::new(path).file_name().map_or(path.clone(), |n| n.to_string_lossy().into_owned());
            format!("« {name} », lu le {}", local(*at).format("%d/%m/%Y"))
        })
        .collect();
    Ok((!parts.is_empty()).then(|| format!("Lecture seule — Outlook fait foi. Source : {}.", parts.join(" ; "))))
}

/// Heure locale (Paris) d'un instant Unix en millisecondes, pour l'appelant.
pub fn local_now(now_ms: i64) -> NaiveDateTime {
    local(now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(id: &str) -> CalendarEvent {
        CalendarEvent {
            id: id.into(),
            calendar_id: String::new(),
            color: 0,
            subject: id.into(),
            location: None,
            when: String::new(),
            start_label: String::new(),
            recurring: false,
            exception: false,
            private: false,
            busy: "busy".into(),
            categories: vec![],
            reminder_minutes: None,
            organizer: String::new(),
            attendees: String::new(),
            is_meeting: false,
            continued: false,
        }
    }

    #[test]
    fn overlapping_events_share_columns() {
        // 9-10, 9:30-11, 10-10:30 : un groupe de deux colonnes ; 14-15 seul.
        let out = lay_out(vec![(540, 600, ev("a")), (570, 660, ev("b")), (600, 630, ev("c")), (840, 900, ev("d"))]);
        let cols: Vec<(String, u8, u8)> = out.iter().map(|t| (t.event.id.clone(), t.column, t.columns)).collect();
        assert_eq!(cols, [("a".into(), 0, 2), ("b".into(), 1, 2), ("c".into(), 0, 2), ("d".into(), 0, 1)]);
    }

    #[test]
    fn periods_and_titles() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap();
        assert_eq!(period(CalendarViewKind::WorkWeek, d), (NaiveDate::from_ymd_opt(2026, 10, 19).unwrap(), NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()));
        assert_eq!(period(CalendarViewKind::Month, d).0, NaiveDate::from_ymd_opt(2026, 9, 28).unwrap());
        assert_eq!(range_title(NaiveDate::from_ymd_opt(2025, 12, 29).unwrap(), NaiveDate::from_ymd_opt(2026, 1, 4).unwrap()), "29 décembre 2025 – 4 janvier 2026");
        assert_eq!(add_months(NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(), 1), NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
        assert_eq!(long_date(d), "mercredi 21 octobre 2026");
    }
}
