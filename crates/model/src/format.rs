//! Formats français (dates, nombres), sans dépendance à une bibliothèque de localisation.

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Timelike};

use crate::DateGroup;

const WEEKDAYS_SHORT: [&str; 7] = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."];
const WEEKDAYS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
const MONTHS: [&str; 12] = [
    "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre",
    "décembre",
];

/// Espace fine insécable, séparateur de milliers en français.
const NNBSP: char = '\u{202f}';

pub fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 * 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(NNBSP);
        }
        out.push(c);
    }
    out
}

fn days_between(earlier: NaiveDate, later: NaiveDate) -> i64 {
    (later - earlier).num_days()
}

/// Groupe de date d'un message par rapport à `now` (semaines du lundi au dimanche).
pub fn date_group<Tz: TimeZone>(received: &DateTime<Tz>, now: &DateTime<Tz>) -> DateGroup {
    let diff = days_between(received.date_naive(), now.date_naive());
    let since_monday = i64::from(now.weekday().num_days_from_monday());
    match diff {
        i64::MIN..=0 => DateGroup::Today,
        1 => DateGroup::Yesterday,
        d if d <= since_monday => DateGroup::ThisWeek,
        d if d <= since_monday + 7 => DateGroup::LastWeek,
        _ => DateGroup::Older,
    }
}

/// Libellé court de la liste : heure si aujourd'hui, jour + heure dans la semaine écoulée, sinon date.
pub fn list_time<Tz: TimeZone>(received: &DateTime<Tz>, now: &DateTime<Tz>) -> String {
    let diff = days_between(received.date_naive(), now.date_naive());
    let hm = format!("{:02}:{:02}", received.hour(), received.minute());
    match diff {
        i64::MIN..=0 => hm,
        1..=6 => format!("{} {hm}", WEEKDAYS_SHORT[received.weekday().num_days_from_monday() as usize]),
        _ => format!("{:02}/{:02}/{}", received.day(), received.month(), received.year()),
    }
}

/// Libellé complet : « vendredi 16 octobre 2026 à 09:12 ».
pub fn full_date<Tz: TimeZone>(received: &DateTime<Tz>) -> String {
    format!(
        "{} {} {} {} à {:02}:{:02}",
        WEEKDAYS[received.weekday().num_days_from_monday() as usize],
        received.day(),
        MONTHS[received.month0() as usize],
        received.year(),
        received.hour(),
        received.minute()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<FixedOffset> {
        FixedOffset::east_opt(2 * 3600).unwrap().with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    #[test]
    fn groups_relative_to_friday() {
        let now = at(2026, 10, 16, 10, 0); // vendredi
        assert_eq!(date_group(&at(2026, 10, 16, 8, 0), &now), DateGroup::Today);
        assert_eq!(date_group(&at(2026, 10, 15, 8, 0), &now), DateGroup::Yesterday);
        assert_eq!(date_group(&at(2026, 10, 12, 8, 0), &now), DateGroup::ThisWeek); // lundi
        assert_eq!(date_group(&at(2026, 10, 11, 8, 0), &now), DateGroup::LastWeek); // dimanche
        assert_eq!(date_group(&at(2026, 10, 5, 8, 0), &now), DateGroup::LastWeek); // lundi précédent
        assert_eq!(date_group(&at(2026, 10, 4, 8, 0), &now), DateGroup::Older);
    }

    #[test]
    fn labels() {
        let now = at(2026, 10, 16, 10, 0);
        assert_eq!(list_time(&at(2026, 10, 16, 9, 5), &now), "09:05");
        assert_eq!(list_time(&at(2026, 10, 12, 17, 0), &now), "lun. 17:00");
        assert_eq!(list_time(&at(2026, 10, 4, 11, 20), &now), "04/10/2026");
        assert_eq!(full_date(&at(2026, 10, 16, 9, 12)), "vendredi 16 octobre 2026 à 09:12");
        assert_eq!(thousands(20543), "20\u{202f}543");
        assert_eq!(thousands(999), "999");
    }
}
