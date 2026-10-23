//! `PidLidTaskRecurrence` [MS-OXOTASK] : `RecurrencePattern` seul (celui de `PidLidAppointmentRecur` [MS-OXOCAL], sans
//! `AppointmentRecurrencePattern`). Réutilise l'encodeur de `thunderlook_calendar::recur` et en retire la fin.

use chrono::{Datelike, Duration, Months, NaiveDate};
use thunderlook_calendar::{
    expand,
    recur::{End, Frequency, Pattern, Recurrence, local_to_minutes, minutes_to_local},
};

type Result<T> = std::result::Result<T, String>;

const NEVER_END: u32 = 0x5AE9_80DF;
/// Octets d'`AppointmentRecurrencePattern` pour une série sans exception : 4 versions et décalages (16), nombre
/// d'exceptions (2), deux blocs réservés vides (2 × 4).
const APPT_TAIL: usize = 26;

/// Octet 18 de `RecurrencePattern` : `SlidingFlag` ([MS-OXOCAL] 2.2.1.44.1), 1 pour une tâche qui recommence N jours (semaines,
/// mois, ans) après sa fin (« Regenerate new task » d'Outlook), 0 sinon.
const SLIDING_AT: usize = 18;
/// Préfixe de `TaskForm::repeat` pour une tâche qui recommence après sa fin : "regen_daily", "regen_weekly"…
pub const REGEN: &str = "regen_";

/// Champs simples d'une périodicité (ceux du formulaire).
#[derive(Debug, PartialEq, Eq)]
pub struct Fields {
    /// "daily", "weekly", "monthly", "yearly", "regen_daily"…"regen_yearly" ou "custom".
    pub repeat: &'static str,
    pub every: u32,
    /// "never", "count" ou "until".
    pub end: &'static str,
    pub count: u32,
    pub until: String,
}

fn midnight(d: NaiveDate) -> u32 {
    local_to_minutes(d.and_hms_opt(0, 0, 0).unwrap_or_default())
}

fn day(m: u32) -> NaiveDate {
    minutes_to_local(m).date()
}

/// Fréquence d'un choix du formulaire, avec ou sans le préfixe `regen_`.
pub fn frequency(name: &str) -> Option<Frequency> {
    Some(match name.strip_prefix(REGEN).unwrap_or(name) {
        "daily" => Frequency::Daily,
        "weekly" => Frequency::Weekly,
        "monthly" => Frequency::Monthly,
        "yearly" => Frequency::Yearly,
        _ => return None,
    })
}

/// Périodicité « tous les `every` jours/semaines/mois/ans » à partir de `anchor` (même jour de la semaine, du mois ou
/// de l'année), fin comprise. Même construction que les rendez-vous périodiques.
pub fn build(freq: Frequency, every: u32, end: &str, count: u32, until: &str, anchor: NaiveDate) -> Result<Recurrence> {
    let interval = every.clamp(1, 999);
    let month_index = (anchor.year() - 1601) * 12 + anchor.month0() as i32;
    let first_of_month = |idx: u32| midnight(NaiveDate::from_ymd_opt(1601 + (idx / 12) as i32, idx % 12 + 1, 1).unwrap_or_default());
    let (pattern, period, first) = match freq {
        Frequency::Daily => (Pattern::Day, 1_440 * interval, midnight(anchor) % (1_440 * interval)),
        Frequency::Weekly => {
            let monday = anchor - Duration::days(i64::from(anchor.weekday().num_days_from_monday()));
            (Pattern::Week { days: 1 << anchor.weekday().num_days_from_sunday() }, interval, midnight(monday) % (10_080 * interval))
        }
        Frequency::Monthly => (Pattern::Month { day: anchor.day() }, interval, first_of_month(month_index as u32 % interval)),
        Frequency::Yearly => (Pattern::Month { day: anchor.day() }, 12 * interval, first_of_month(anchor.month0())),
    };
    let mut r = Recurrence {
        frequency: freq,
        pattern,
        calendar_type: 0,
        first_date_time: first,
        period,
        end: End::Never,
        occurrences: 0,
        first_day_of_week: 1,
        deleted: vec![],
        modified: vec![],
        start_date: midnight(anchor),
        end_date: NEVER_END,
        start_offset: 0,
        end_offset: 0,
        exceptions: vec![],
        raw: Default::default(),
    };
    let horizon = anchor + Duration::days(366 * 120);
    match end {
        "count" => {
            let n = count.clamp(1, 9_999) as usize;
            let all = expand::dates(&r, anchor, horizon);
            r.end = End::Count(n.min(all.len().max(1)) as u32);
            r.occurrences = n as u32;
            r.end_date = midnight(all.get(n - 1).or(all.last()).copied().unwrap_or(anchor));
        }
        "until" => {
            let last = NaiveDate::parse_from_str(until, "%Y-%m-%d").map_err(|_| format!("date de fin invalide : {until}"))?;
            if last < anchor {
                return Err("La fin de la répétition est avant le début.".into());
            }
            r.end = End::Date;
            r.end_date = midnight(last);
            r.occurrences = expand::dates(&r, anchor, last).len() as u32;
        }
        _ => {}
    }
    Ok(r)
}

/// `RecurrencePattern` seul.
pub fn encode(r: &Recurrence) -> Vec<u8> {
    let mut b = r.encode();
    b.truncate(b.len().saturating_sub(APPT_TAIL));
    b
}

/// `RecurrencePattern` d'une tâche qui recommence après sa fin : sans fin, `SlidingFlag` = 1.
pub fn encode_regen(r: &Recurrence) -> Vec<u8> {
    let mut b = encode(r);
    if let Some(x) = b.get_mut(SLIDING_AT..SLIDING_AT + 4) {
        x.copy_from_slice(&1u32.to_le_bytes());
    }
    b
}

/// La périodicité vient-elle d'une tâche qui recommence après sa fin ?
pub fn is_regen(blob: &[u8]) -> bool {
    blob.get(SLIDING_AT..SLIDING_AT + 4).is_some_and(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]) == 1)
}

pub fn decode(blob: &[u8]) -> Result<Recurrence> {
    let mut b = blob.to_vec();
    // AppointmentRecurrencePattern vide : versions 0x3006 / 0x3009, décalages 0, aucune exception, blocs vides.
    for v in [0x3006u32, 0x3009, 0, 0] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&[0; 2 + 4 + 4]);
    Recurrence::parse(&b)
}

/// Champs de formulaire d'une périodicité relue (`regen` : voir `is_regen`) ; "custom" quand elle n'y entre pas.
pub fn fields(r: &Recurrence, regen: bool) -> Fields {
    if regen {
        let base = fields(r, false);
        let repeat = match base.repeat {
            "daily" => "regen_daily",
            "weekly" => "regen_weekly",
            "monthly" => "regen_monthly",
            "yearly" => "regen_yearly",
            _ => "custom",
        };
        return Fields { repeat, end: "never", count: 0, until: String::new(), ..base };
    }
    let (repeat, every) = match (r.frequency, r.pattern) {
        (Frequency::Daily, Pattern::Day) => ("daily", r.period / 1_440),
        (Frequency::Weekly, Pattern::Week { days }) if days.count_ones() == 1 => ("weekly", r.period),
        (Frequency::Monthly, Pattern::Month { .. }) => ("monthly", r.period),
        (Frequency::Yearly, Pattern::Month { .. }) if r.period % 12 == 0 => ("yearly", r.period / 12),
        _ => ("custom", 1),
    };
    let (end, count, until) = match r.end {
        End::Never => ("never", 0, String::new()),
        End::Count(n) => ("count", n, String::new()),
        End::Date => ("until", 0, day(r.end_date).format("%Y-%m-%d").to_string()),
    };
    Fields { repeat, every: every.max(1), end, count, until }
}

/// Prochaine échéance d'une tâche qui recommence après sa fin : `from` (jour de la fin) + `every` jours/semaines/mois/ans.
/// En fin de mois, le jour est ramené au dernier jour du mois (31 janvier + 1 mois = 28 ou 29 février).
pub fn next_regen(freq: Frequency, every: u32, from: NaiveDate) -> Option<NaiveDate> {
    let n = every.clamp(1, 999);
    match freq {
        Frequency::Daily => from.checked_add_signed(Duration::days(i64::from(n))),
        Frequency::Weekly => from.checked_add_signed(Duration::days(7 * i64::from(n))),
        Frequency::Monthly => from.checked_add_months(Months::new(n)),
        Frequency::Yearly => from.checked_add_months(Months::new(12 * n)),
    }
}

/// Premier jour de la série après `after` ; `None` si la série est finie.
pub fn next_after(r: &Recurrence, after: NaiveDate) -> Option<NaiveDate> {
    let from = after + Duration::days(1);
    expand::dates(r, from, after + Duration::days(366 * 120)).into_iter().find(|d| *d >= from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, dd: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, dd).unwrap()
    }

    #[test]
    fn pattern_round_trip_without_appointment_part() {
        for (f, every, end, count, until) in [
            (Frequency::Daily, 3, "never", 0, ""),
            (Frequency::Weekly, 2, "count", 5, ""),
            (Frequency::Monthly, 1, "until", 0, "2027-03-31"),
            (Frequency::Yearly, 1, "count", 3, ""),
        ] {
            let r = build(f, every, end, count, until, d(2026, 10, 23)).unwrap();
            let blob = encode(&r);
            // RecurrencePattern : 0x3004 0x3004, fréquence 0x200A-D en tête ; pas d'octets d'AppointmentRecurrencePattern.
            assert_eq!(&blob[..4], &[0x04, 0x30, 0x04, 0x30]);
            let back = decode(&blob).unwrap();
            assert_eq!(encode(&back), blob);
            let x = fields(&back, false);
            assert_eq!((x.every, x.end, x.count), (every, end, if end == "count" { count } else { 0 }));
            assert_eq!(x.until, until);
        }
    }

    #[test]
    fn next_occurrence() {
        let r = build(Frequency::Weekly, 1, "count", 3, "", d(2026, 10, 23)).unwrap();
        assert_eq!(next_after(&r, d(2026, 10, 23)), Some(d(2026, 10, 30)));
        assert_eq!(next_after(&r, d(2026, 11, 6)), None);
        let m = build(Frequency::Monthly, 1, "never", 0, "", d(2026, 10, 31)).unwrap();
        assert!(next_after(&m, d(2026, 10, 31)).is_some());
        let y = build(Frequency::Yearly, 1, "never", 0, "", d(2026, 10, 23)).unwrap();
        assert_eq!(next_after(&y, d(2026, 10, 23)), Some(d(2027, 10, 23)));
    }

    #[test]
    fn custom_pattern_is_flagged() {
        let mut r = build(Frequency::Weekly, 1, "never", 0, "", d(2026, 10, 23)).unwrap();
        r.pattern = Pattern::Week { days: 0b0101010 };
        assert_eq!(fields(&decode(&encode(&r)).unwrap(), false).repeat, "custom");
    }

    #[test]
    fn regenerating_flag_round_trip() {
        let r = build(Frequency::Daily, 3, "never", 0, "", d(2026, 10, 23)).unwrap();
        let plain = encode(&r);
        assert!(!is_regen(&plain));
        let blob = encode_regen(&r);
        assert!(is_regen(&blob));
        // Seul le SlidingFlag change.
        assert_eq!(blob.iter().zip(&plain).filter(|(a, b)| a != b).count(), 1);
        let back = decode(&blob).unwrap();
        let x = fields(&back, is_regen(&blob));
        assert_eq!((x.repeat, x.every, x.end), ("regen_daily", 3, "never"));
        assert_eq!(encode_regen(&back), blob);
        assert_eq!(fields(&back, false).repeat, "daily");
    }

    #[test]
    fn regenerated_dates_follow_month_ends() {
        assert_eq!(next_regen(Frequency::Daily, 10, d(2026, 10, 23)), Some(d(2026, 11, 2)));
        assert_eq!(next_regen(Frequency::Weekly, 2, d(2026, 12, 25)), Some(d(2027, 1, 8)));
        assert_eq!(next_regen(Frequency::Monthly, 1, d(2026, 1, 31)), Some(d(2026, 2, 28)));
        assert_eq!(next_regen(Frequency::Monthly, 1, d(2028, 1, 31)), Some(d(2028, 2, 29)));
        assert_eq!(next_regen(Frequency::Monthly, 3, d(2026, 11, 30)), Some(d(2027, 2, 28)));
        assert_eq!(next_regen(Frequency::Yearly, 1, d(2028, 2, 29)), Some(d(2029, 2, 28)));
        assert_eq!(next_regen(Frequency::Yearly, 4, d(2028, 2, 29)), Some(d(2032, 2, 29)));
    }

    #[test]
    fn fixed_series_on_month_ends() {
        // Série du 31 : les mois courts sont sautés ou ramenés selon `expand`, mais toujours une date par mois au plus.
        let m = build(Frequency::Monthly, 1, "never", 0, "", d(2026, 1, 31)).unwrap();
        let mut day = d(2026, 1, 31);
        for _ in 0..12 {
            let next = next_after(&m, day).unwrap();
            assert!(next > day);
            day = next;
        }
        assert!(day < d(2027, 3, 1));
        let y = build(Frequency::Yearly, 1, "never", 0, "", d(2028, 2, 29)).unwrap();
        assert!(next_after(&y, d(2028, 2, 29)).unwrap() <= d(2029, 3, 1));
    }
}
