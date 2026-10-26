//! Lot AZ : jours fériés de France métropolitaine (11 jours, loi : article L3133-1 du Code du travail), calculés pour
//! n'importe quelle année. Pâques par l'algorithme grégorien « anonyme » (Meeus/Jones/Butcher). Alsace-Moselle
//! (Vendredi saint, 26 décembre) et outre-mer (abolition de l'esclavage) ne sont pas comptés.

use chrono::{Datelike, Duration, NaiveDate};

/// Dimanche de Pâques (calendrier grégorien).
pub fn easter(year: i32) -> NaiveDate {
    let a = year % 19;
    let (b, c) = (year / 100, year % 100);
    let (d, e) = (b / 4, b % 4);
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let (i, k) = (c / 4, c % 4);
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    NaiveDate::from_ymd_opt(year, month as u32, day as u32).unwrap_or_default()
}

/// Jours fériés de l'année, dans l'ordre, avec le nom affiché.
pub fn france(year: i32) -> Vec<(NaiveDate, &'static str)> {
    let d = |m, j| NaiveDate::from_ymd_opt(year, m, j).unwrap_or_default();
    let p = easter(year);
    let mut out = vec![
        (d(1, 1), "Jour de l'An"),
        (p + Duration::days(1), "Lundi de Pâques"),
        (d(5, 1), "Fête du Travail"),
        (d(5, 8), "Victoire 1945"),
        (p + Duration::days(39), "Ascension"),
        (p + Duration::days(50), "Lundi de Pentecôte"),
        (d(7, 14), "Fête nationale"),
        (d(8, 15), "Assomption"),
        (d(11, 1), "Toussaint"),
        (d(11, 11), "Armistice 1918"),
        (d(12, 25), "Noël"),
    ];
    out.sort_by_key(|(day, _)| *day);
    out
}

/// Jours fériés entre `from` et `to` (inclus).
pub fn between(from: NaiveDate, to: NaiveDate) -> Vec<(NaiveDate, &'static str)> {
    (from.year()..=to.year()).flat_map(france).filter(|(d, _)| *d >= from && *d <= to).collect()
}

/// Comparaison souple d'un objet de rendez-vous avec un nom de jour férié (casse, accents, apostrophes), pour ne pas
/// doubler un jour férié déjà présent dans le calendrier (Outlook les ajoute comme rendez-vous).
pub fn same_name(subject: &str, holiday: &str) -> bool {
    let fold = |s: &str| -> String {
        s.chars()
            .flat_map(char::to_lowercase)
            .map(|c| match c {
                'à' | 'â' | 'ä' => 'a',
                'é' | 'è' | 'ê' | 'ë' => 'e',
                'î' | 'ï' => 'i',
                'ô' | 'ö' => 'o',
                'ù' | 'û' | 'ü' => 'u',
                'ç' => 'c',
                c => c,
            })
            .filter(|c| c.is_alphanumeric())
            .collect()
    };
    let (s, h) = (fold(subject), fold(holiday));
    !h.is_empty() && s.contains(&h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn easter_dates() {
        // Dates connues (calendrier grégorien).
        for (y, m, d) in [(2024, 3, 31), (2025, 4, 20), (2026, 4, 5), (2027, 3, 28), (2019, 4, 21), (2000, 4, 23), (2038, 4, 25)] {
            assert_eq!(easter(y), ymd(y, m, d), "{y}");
        }
    }

    #[test]
    fn france_2026() {
        let days: Vec<String> = france(2026).iter().map(|(d, n)| format!("{} {n}", d.format("%m-%d"))).collect();
        assert_eq!(
            days,
            [
                "01-01 Jour de l'An",
                "04-06 Lundi de Pâques",
                "05-01 Fête du Travail",
                "05-08 Victoire 1945",
                "05-14 Ascension",
                "05-25 Lundi de Pentecôte",
                "07-14 Fête nationale",
                "08-15 Assomption",
                "11-01 Toussaint",
                "11-11 Armistice 1918",
                "12-25 Noël",
            ]
        );
        assert_eq!(between(ymd(2026, 12, 20), ymd(2027, 1, 5)).len(), 2);
    }

    #[test]
    fn names_are_folded() {
        assert!(same_name("Jour de l’an", "Jour de l'An"));
        assert!(same_name("Fête Nationale", "Fête nationale"));
        assert!(same_name("FETE DU TRAVAIL (férié)", "Fête du Travail"));
        assert!(!same_name("Réunion", "Noël"));
    }
}
