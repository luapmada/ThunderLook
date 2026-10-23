//! Étape 2 du J9 : occurrences d'un rendez-vous sur une plage. Les dates d'une série sont calculées en heure locale
//! de son fuseau ([MS-OXOCAL 2.2.1.44.1] : `StartDate`, `EndDate`, `Period`, motif), puis ramenées en UTC ;
//! les occurrences supprimées ou modifiées (`DeletedInstanceDates`) sont retirées et les exceptions ajoutées.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};

use crate::{
    Appointment,
    recur::{Frequency, Pattern, Recurrence, minutes_to_local},
    tz::TimeZone,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    pub appointment_id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub all_day: bool,
    pub subject: String,
    pub location: Option<String>,
    /// Occurrence modifiée (exception) d'une série.
    pub exception: bool,
    /// Date d'origine (locale) d'une occurrence de série, pour la retrouver ; `None` hors série.
    pub original_date: Option<NaiveDate>,
}

const MINUTES_PER_DAY: u32 = 1_440;

fn day(m: u32) -> NaiveDate {
    minutes_to_local(m).date()
}

fn days_in_month(y: i32, m: u32) -> u32 {
    let next = if m == 12 { NaiveDate::from_ymd_opt(y + 1, 1, 1) } else { NaiveDate::from_ymd_opt(y, m + 1, 1) };
    next.unwrap().pred_opt().unwrap().day()
}

/// Bit du masque de jours [MS-OXOCAL] : bit 0 = dimanche.
fn bit(d: NaiveDate) -> u32 {
    1 << d.weekday().num_days_from_sunday()
}

/// Date de l'occurrence dans un mois donné, selon le motif mensuel ou annuel.
fn in_month(pattern: &Pattern, y: i32, m: u32) -> Option<NaiveDate> {
    let last = days_in_month(y, m);
    match *pattern {
        Pattern::Month { day } => NaiveDate::from_ymd_opt(y, m, day.clamp(1, last)),
        Pattern::MonthEnd => NaiveDate::from_ymd_opt(y, m, last),
        Pattern::MonthNth { days, nth } => {
            let matching: Vec<NaiveDate> = (1..=last).filter_map(|d| NaiveDate::from_ymd_opt(y, m, d)).filter(|d| bit(*d) & days != 0).collect();
            if nth >= 5 { matching.last().copied() } else { matching.get(nth as usize - 1).copied() }
        }
        _ => None,
    }
}

/// Dates locales des occurrences de la série entre `from` et `to` inclus (avant retrait des supprimées).
pub fn dates(r: &Recurrence, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    let (first, last) = (day(r.start_date), day(r.end_date).min(to));
    let mut out = Vec::new();
    if last < first {
        return out;
    }
    match (r.frequency, &r.pattern) {
        (Frequency::Daily, Pattern::Day) => {
            let step = i64::from((r.period / MINUTES_PER_DAY).max(1));
            // Premier pas qui atteint `from`, sans parcourir les années antérieures.
            let skip = ((from - first).num_days().max(0) / step) * step;
            let mut d = first + Duration::days(skip);
            while d <= last {
                if d >= from {
                    out.push(d);
                }
                d += Duration::days(step);
            }
        }
        (_, Pattern::Week { days }) => {
            // Semaines comptées depuis celle de `StartDate`, qui commence au `FirstDOW` de la série.
            let back = (7 + first.weekday().num_days_from_sunday() - r.first_day_of_week % 7) % 7;
            let week0 = first - Duration::days(i64::from(back));
            let step = i64::from(r.period.max(1)) * 7;
            let skip = (((from - week0).num_days().max(0)) / step) * step;
            let mut w = week0 + Duration::days(skip);
            while w <= last {
                for i in 0..7 {
                    let d = w + Duration::days(i);
                    if bit(d) & days != 0 && d >= first && d >= from && d <= last {
                        out.push(d);
                    }
                }
                w += Duration::days(step);
            }
        }
        (Frequency::Monthly | Frequency::Yearly, p) => {
            let step = r.period.max(1);
            let index = |d: NaiveDate| d.year() * 12 + d.month0() as i32;
            let base = index(first);
            let skip = (((index(from) - base).max(0) as u32) / step) * step;
            let mut k = base + skip as i32;
            while k <= index(last) {
                if let Some(d) = in_month(p, k.div_euclid(12), k.rem_euclid(12) as u32 + 1)
                    && d >= first
                    && d >= from
                    && d <= last
                {
                    out.push(d);
                }
                k += step as i32;
            }
        }
        _ => {}
    }
    out
}

fn ms(tz: &TimeZone, local: NaiveDateTime) -> i64 {
    tz.to_utc(local).and_utc().timestamp_millis()
}

/// Occurrences du rendez-vous qui chevauchent `[from_ms, to_ms[` (UTC), triées par début.
pub fn occurrences(a: &Appointment, from_ms: i64, to_ms: i64) -> Vec<Occurrence> {
    let overlaps = |s: i64, e: i64| s < to_ms && (e > from_ms || (e == s && s >= from_ms));
    let Some(r) = &a.recurrence else {
        return if overlaps(a.start_ms, a.end_ms) {
            vec![Occurrence {
                appointment_id: a.id,
                start_ms: a.start_ms,
                end_ms: a.end_ms,
                all_day: a.all_day,
                subject: a.subject.clone(),
                location: a.location.clone(),
                exception: false,
                original_date: None,
            }]
        } else {
            Vec::new()
        };
    };
    let tz = a.time_zone.unwrap_or(TimeZone::PARIS);
    // Marge d'un jour de chaque côté pour le décalage horaire, plus la durée d'une occurrence.
    let span = i64::from(r.end_offset.saturating_sub(r.start_offset)) / i64::from(MINUTES_PER_DAY) + 1;
    let to_date = |m: i64| chrono::DateTime::from_timestamp_millis(m).map(|t| t.naive_utc().date());
    let (Some(from), Some(to)) = (to_date(from_ms), to_date(to_ms)) else { return Vec::new() };
    let (from, to) = (from - Duration::days(span + 1), to + Duration::days(1));
    let deleted: std::collections::HashSet<NaiveDate> = r.deleted.iter().map(|m| day(*m)).collect();
    let mut out: Vec<Occurrence> = dates(r, from, to)
        .into_iter()
        .filter(|d| !deleted.contains(d))
        .filter_map(|d| {
            let midnight = d.and_hms_opt(0, 0, 0)?;
            let start = ms(&tz, midnight + Duration::minutes(i64::from(r.start_offset)));
            let end = ms(&tz, midnight + Duration::minutes(i64::from(r.end_offset)));
            overlaps(start, end).then(|| Occurrence {
                appointment_id: a.id,
                start_ms: start,
                end_ms: end,
                all_day: a.all_day,
                subject: a.subject.clone(),
                location: a.location.clone(),
                exception: false,
                original_date: Some(d),
            })
        })
        .collect();
    for x in &r.exceptions {
        let (start, end) = (ms(&tz, minutes_to_local(x.start)), ms(&tz, minutes_to_local(x.end)));
        if overlaps(start, end) {
            out.push(Occurrence {
                appointment_id: a.id,
                start_ms: start,
                end_ms: end,
                all_day: x.all_day.unwrap_or(a.all_day),
                subject: x.subject.clone().unwrap_or_else(|| a.subject.clone()),
                location: x.location.clone().or_else(|| a.location.clone()),
                exception: true,
                original_date: Some(day(x.original_start)),
            });
        }
    }
    out.sort_by_key(|o| o.start_ms);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recur::{End, local_to_minutes};

    fn series(frequency: Frequency, pattern: Pattern, period: u32, start: &str, end: &str) -> Recurrence {
        let m = |s: &str| local_to_minutes(NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap().and_hms_opt(0, 0, 0).unwrap());
        Recurrence {
            frequency,
            pattern,
            calendar_type: 0,
            first_date_time: 0,
            period,
            end: End::Date,
            occurrences: 0,
            first_day_of_week: 1,
            deleted: vec![],
            modified: vec![],
            start_date: m(start),
            end_date: m(end),
            start_offset: 600,
            end_offset: 660,
            exceptions: vec![],
            raw: Default::default(),
        }
    }

    fn all(r: &Recurrence) -> Vec<String> {
        dates(r, NaiveDate::from_ymd_opt(1900, 1, 1).unwrap(), NaiveDate::from_ymd_opt(2100, 1, 1).unwrap())
            .iter()
            .map(|d| d.to_string())
            .collect()
    }

    #[test]
    fn weekly_every_two_weeks_on_monday_and_thursday() {
        // Du jeudi 5 janvier 2023, une semaine sur deux (semaine commençant le lundi).
        let r = series(Frequency::Weekly, Pattern::Week { days: 0b0010010 }, 2, "2023-01-05", "2023-02-02");
        assert_eq!(all(&r), ["2023-01-05", "2023-01-16", "2023-01-19", "2023-01-30", "2023-02-02"]);
    }

    #[test]
    fn monthly_31st_and_last_friday() {
        let r = series(Frequency::Monthly, Pattern::Month { day: 31 }, 1, "2023-01-31", "2023-04-30");
        assert_eq!(all(&r), ["2023-01-31", "2023-02-28", "2023-03-31", "2023-04-30"]);
        let r = series(Frequency::Monthly, Pattern::MonthNth { days: 1 << 5, nth: 5 }, 1, "2023-01-27", "2023-03-31");
        assert_eq!(all(&r), ["2023-01-27", "2023-02-24", "2023-03-31"]);
    }

    #[test]
    fn yearly_february_29th_and_daily_every_three_days() {
        let r = series(Frequency::Yearly, Pattern::Month { day: 29 }, 12, "2024-02-29", "2026-02-28");
        assert_eq!(all(&r), ["2024-02-29", "2025-02-28", "2026-02-28"]);
        let r = series(Frequency::Daily, Pattern::Day, 3 * MINUTES_PER_DAY, "2023-01-30", "2023-02-06");
        assert_eq!(all(&r), ["2023-01-30", "2023-02-02", "2023-02-05"]);
    }
}
