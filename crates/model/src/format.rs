//! Formats français (dates, nombres, tailles), sans dépendance à une bibliothèque de localisation.

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Timelike};

const WEEKDAYS_SHORT: [&str; 7] = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."];
const WEEKDAYS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
const WEEKDAYS_CAP: [&str; 7] = ["Lundi", "Mardi", "Mercredi", "Jeudi", "Vendredi", "Samedi", "Dimanche"];
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

/// Taille lisible : « 812 o », « 14 Ko », « 2,3 Mo ».
pub fn size(bytes: u64) -> String {
    const K: f64 = 1024.0;
    let b = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} o")
    } else if b < K * K {
        format!("{} Ko", (b / K).round() as u64)
    } else if b < K * K * K {
        format!("{} Mo", decimal(b / K / K))
    } else {
        format!("{} Go", decimal(b / K / K / K))
    }
}

/// Une décimale, virgule française, sans « ,0 ».
fn decimal(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    if r.fract() == 0.0 { format!("{}", r as u64) } else { format!("{r:.1}").replace('.', ",") }
}

/// Tranche de taille pour le regroupement de la liste triée par taille.
pub fn size_group(bytes: u64) -> &'static str {
    match bytes {
        0..10_240 => "Minuscule (moins de 10 Ko)",
        10_240..25_600 => "Petit (10 à 25 Ko)",
        25_600..102_400 => "Moyen (25 à 100 Ko)",
        102_400..512_000 => "Grand (100 à 500 Ko)",
        512_000..1_048_576 => "Très grand (500 Ko à 1 Mo)",
        1_048_576..5_242_880 => "Énorme (1 à 5 Mo)",
        _ => "Gigantesque (plus de 5 Mo)",
    }
}

fn days_between(earlier: NaiveDate, later: NaiveDate) -> i64 {
    (later - earlier).num_days()
}

/// Groupe de date d'un message par rapport à `now` (semaines du lundi au dimanche).
/// Les groupes se suivent dans l'ordre chronologique, donc restent contigus dans une liste triée par date.
pub fn date_group<Tz: TimeZone>(received: &DateTime<Tz>, now: &DateTime<Tz>) -> String {
    let (r, n) = (received.date_naive(), now.date_naive());
    let diff = days_between(r, n);
    let since_monday = i64::from(n.weekday().num_days_from_monday());
    let label = match diff {
        i64::MIN..0 => "Plus tard",
        0 => "Aujourd'hui",
        1 => "Hier",
        d if d <= since_monday => return WEEKDAYS_CAP[r.weekday().num_days_from_monday() as usize].to_string(),
        d if d <= since_monday + 7 => "La semaine dernière",
        d if d <= since_monday + 14 => "Il y a deux semaines",
        d if d <= since_monday + 21 => "Il y a trois semaines",
        _ if (r.year(), r.month()) == (n.year(), n.month()) => "Plus tôt ce mois-ci",
        _ if r.with_day(1).and_then(|d| d.checked_add_months(chrono::Months::new(1)))
            == n.with_day(1) =>
        {
            "Le mois dernier"
        }
        _ => "Plus ancien",
    };
    label.to_string()
}

/// Libellé court de la liste : heure si aujourd'hui, jour + heure dans la semaine écoulée, sinon date.
pub fn list_time<Tz: TimeZone>(received: &DateTime<Tz>, now: &DateTime<Tz>) -> String {
    let diff = days_between(received.date_naive(), now.date_naive());
    let hm = format!("{:02}:{:02}", received.hour(), received.minute());
    match diff {
        0 => hm,
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

/// Date sans heure : « 16 octobre 2026 ».
pub fn long_day<Tz: TimeZone>(d: &DateTime<Tz>) -> String {
    format!("{} {} {}", d.day(), MONTHS[d.month0() as usize], d.year())
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
        let g = |y, m, d| date_group(&at(y, m, d, 8, 0), &now);
        assert_eq!(g(2026, 10, 17), "Plus tard");
        assert_eq!(g(2026, 10, 16), "Aujourd'hui");
        assert_eq!(g(2026, 10, 15), "Hier");
        assert_eq!(g(2026, 10, 12), "Lundi");
        assert_eq!(g(2026, 10, 11), "La semaine dernière"); // dimanche
        assert_eq!(g(2026, 10, 5), "La semaine dernière"); // lundi précédent
        assert_eq!(g(2026, 10, 4), "Il y a deux semaines");
        assert_eq!(g(2026, 9, 28), "Il y a deux semaines"); // lundi, deux semaines avant
        assert_eq!(g(2026, 9, 21), "Il y a trois semaines");
        assert_eq!(g(2026, 9, 20), "Le mois dernier");
        assert_eq!(g(2026, 8, 31), "Plus ancien");
        let late = at(2026, 10, 30, 10, 0);
        assert_eq!(date_group(&at(2026, 10, 2, 8, 0), &late), "Plus tôt ce mois-ci");
        let jan = at(2027, 1, 29, 10, 0);
        assert_eq!(date_group(&at(2026, 12, 20, 8, 0), &jan), "Le mois dernier");
    }

    #[test]
    fn labels() {
        let now = at(2026, 10, 16, 10, 0);
        assert_eq!(list_time(&at(2026, 10, 16, 9, 5), &now), "09:05");
        assert_eq!(list_time(&at(2026, 10, 12, 17, 0), &now), "lun. 17:00");
        assert_eq!(list_time(&at(2026, 10, 4, 11, 20), &now), "04/10/2026");
        assert_eq!(list_time(&at(2026, 10, 18, 11, 20), &now), "18/10/2026");
        assert_eq!(full_date(&at(2026, 10, 16, 9, 12)), "vendredi 16 octobre 2026 à 09:12");
        assert_eq!(long_day(&at(2026, 10, 16, 9, 12)), "16 octobre 2026");
        assert_eq!(thousands(20543), "20\u{202f}543");
        assert_eq!(thousands(999), "999");
    }

    #[test]
    fn sizes() {
        assert_eq!(size(812), "812 o");
        assert_eq!(size(14 * 1024 + 300), "14 Ko");
        assert_eq!(size(2_411_724), "2,3 Mo");
        assert_eq!(size(3 * 1024 * 1024), "3 Mo");
        assert_eq!(size_group(5_000), "Minuscule (moins de 10 Ko)");
        assert_eq!(size_group(6_000_000), "Gigantesque (plus de 5 Mo)");
    }
}
