//! Fuseau d'un rendez-vous périodique : `PidLidTimeZoneStruct` [MS-OXOCAL 2.2.1.39] (règle unique, sans historique).
//! Les dates d'une périodicité sont en heure locale de ce fuseau ; il faut ce fuseau pour les ramener en UTC.
//!
//! Lot AZ : fuseau de l'utilisateur ([`TimeZone::local`], Paris par défaut, choisi dans [`ZONES`]) pour l'affichage et la
//! saisie, et un second fuseau facultatif ([`second_zone`]) montré à côté des heures, comme Outlook. Les formats de
//! fichiers (`.ics` avec `VTIMEZONE` « Romance Standard Time », échange MPE, CSV) restent en heure de Paris
//! ([`TimeZone::PARIS`]) : c'est le fuseau qu'ils déclarent.

use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Weekday};

/// Date de changement d'heure au format SYSTEMTIME « relatif » : mois, n-ième jour de semaine (5 = dernier), heure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    pub month: u32,
    pub weekday: u32,
    pub week: u32,
    pub hour: u32,
    pub minute: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeZone {
    /// Minutes à ajouter à l'heure locale pour obtenir l'UTC (Paris : -60).
    pub bias: i32,
    pub standard_bias: i32,
    pub daylight_bias: i32,
    pub standard: Option<Transition>,
    pub daylight: Option<Transition>,
}

impl TimeZone {
    /// Europe/Paris (heure d'été du dernier dimanche de mars au dernier dimanche d'octobre).
    pub const PARIS: TimeZone = TimeZone {
        bias: -60,
        standard_bias: 0,
        daylight_bias: -60,
        standard: Some(Transition { month: 10, weekday: 0, week: 5, hour: 3, minute: 0 }),
        daylight: Some(Transition { month: 3, weekday: 0, week: 5, hour: 2, minute: 0 }),
    };

    /// Fuseau de l'utilisateur : celui choisi par [`set_zones`], Europe/Paris sinon.
    pub fn local() -> TimeZone {
        ZONES[PRIMARY.load(Ordering::Relaxed).min(ZONES.len() - 1)].tz
    }

    /// Fuseau de la liste [`ZONES`] par son identifiant (`Europe/Paris`…).
    pub fn by_id(id: &str) -> Option<TimeZone> {
        zone(id).map(|z| z.tz)
    }

    pub fn parse(b: &[u8]) -> Result<Self, String> {
        if b.len() < 48 {
            return Err(format!("PidLidTimeZoneStruct : {} octets au lieu de 48", b.len()));
        }
        let i32_at = |i: usize| i32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let u16_at = |i: usize| u32::from(u16::from_le_bytes([b[i], b[i + 1]]));
        // SYSTEMTIME : année, mois, jour de semaine, jour (rang de la semaine), heure, minute, seconde, ms.
        let transition = |at: usize| {
            let month = u16_at(at + 2);
            (month != 0).then(|| Transition { month, weekday: u16_at(at + 4), week: u16_at(at + 6), hour: u16_at(at + 8), minute: u16_at(at + 10) })
        };
        Ok(Self { bias: i32_at(0), standard_bias: i32_at(4), daylight_bias: i32_at(8), standard: transition(14), daylight: transition(32) })
    }

    fn transition_at(year: i32, t: &Transition) -> Option<NaiveDateTime> {
        let weekday = Weekday::try_from(((t.weekday + 6) % 7) as u8).ok()?;
        let first = NaiveDate::from_ymd_opt(year, t.month, 1)?;
        let offset = (7 + weekday.num_days_from_monday() as i64 - first.weekday().num_days_from_monday() as i64) % 7;
        let mut day = first + Duration::days(offset + 7 * (i64::from(t.week.clamp(1, 5)) - 1));
        while day.month() != t.month {
            day -= Duration::days(7);
        }
        day.and_hms_opt(t.hour, t.minute, 0)
    }

    /// Décalage (minutes, à ajouter à l'heure locale pour obtenir l'UTC) en vigueur à cette heure locale.
    pub fn bias_at(&self, local: NaiveDateTime) -> i32 {
        let (Some(std), Some(dst)) = (&self.standard, &self.daylight) else { return self.bias + self.standard_bias };
        let year = local.year();
        let (Some(to_dst), Some(to_std)) = (Self::transition_at(year, dst), Self::transition_at(year, std)) else {
            return self.bias + self.standard_bias;
        };
        let in_dst = if to_dst < to_std { local >= to_dst && local < to_std } else { local >= to_dst || local < to_std };
        self.bias + if in_dst { self.daylight_bias } else { self.standard_bias }
    }

    pub fn to_utc(&self, local: NaiveDateTime) -> NaiveDateTime {
        local + Duration::minutes(i64::from(self.bias_at(local)))
    }

    /// UTC → heure locale : le décalage retenu est celui qui, appliqué en sens inverse, redonne cet instant.
    pub fn to_local(&self, utc: NaiveDateTime) -> NaiveDateTime {
        for extra in [self.daylight_bias, self.standard_bias] {
            let local = utc - Duration::minutes(i64::from(self.bias + extra));
            if self.to_utc(local) == utc {
                return local;
            }
        }
        utc - Duration::minutes(i64::from(self.bias + self.standard_bias))
    }
}

/// Un fuseau proposé à l'utilisateur : identifiant IANA, libellé comme la liste de Windows/Outlook, règle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zone {
    pub id: &'static str,
    pub label: &'static str,
    pub tz: TimeZone,
}

const fn fixed(bias: i32) -> TimeZone {
    TimeZone { bias, standard_bias: 0, daylight_bias: 0, standard: None, daylight: None }
}

/// Heure d'été européenne (dernier dimanche de mars → dernier dimanche d'octobre, à 01:00 UTC) ; `bias` en minutes.
const fn europe(bias: i32) -> TimeZone {
    let utc_hour = -bias / 60;
    TimeZone {
        bias,
        standard_bias: 0,
        daylight_bias: -60,
        standard: Some(Transition { month: 10, weekday: 0, week: 5, hour: (utc_hour + 2) as u32, minute: 0 }),
        daylight: Some(Transition { month: 3, weekday: 0, week: 5, hour: (utc_hour + 1) as u32, minute: 0 }),
    }
}

/// Heure d'été nord-américaine depuis 2007 (2e dimanche de mars → 1er dimanche de novembre, à 02:00 locale).
const fn america(bias: i32) -> TimeZone {
    TimeZone {
        bias,
        standard_bias: 0,
        daylight_bias: -60,
        standard: Some(Transition { month: 11, weekday: 0, week: 1, hour: 2, minute: 0 }),
        daylight: Some(Transition { month: 3, weekday: 0, week: 2, hour: 2, minute: 0 }),
    }
}

/// Fuseaux proposés (règle actuelle seulement, sans historique, comme `PidLidTimeZoneStruct`). Le premier est le défaut.
pub const ZONES: &[Zone] = &[
    Zone { id: "Europe/Paris", label: "(UTC+01:00) Paris, Bruxelles, Madrid, Berlin, Rome", tz: TimeZone::PARIS },
    Zone { id: "Europe/London", label: "(UTC+00:00) Londres, Dublin, Lisbonne", tz: europe(0) },
    Zone { id: "Europe/Athens", label: "(UTC+02:00) Athènes, Helsinki, Bucarest", tz: europe(-120) },
    Zone { id: "Europe/Moscow", label: "(UTC+03:00) Moscou", tz: fixed(-180) },
    Zone { id: "UTC", label: "(UTC) Temps universel coordonné", tz: fixed(0) },
    Zone { id: "Atlantic/Reykjavik", label: "(UTC+00:00) Reykjavik", tz: fixed(0) },
    Zone { id: "Africa/Casablanca", label: "(UTC+01:00) Casablanca (sans le changement du ramadan)", tz: fixed(-60) },
    Zone { id: "Africa/Algiers", label: "(UTC+01:00) Alger, Tunis", tz: fixed(-60) },
    Zone { id: "Africa/Dakar", label: "(UTC+00:00) Dakar, Abidjan", tz: fixed(0) },
    Zone { id: "America/New_York", label: "(UTC-05:00) New York, Montréal, Toronto", tz: america(300) },
    Zone { id: "America/Chicago", label: "(UTC-06:00) Chicago", tz: america(360) },
    Zone { id: "America/Denver", label: "(UTC-07:00) Denver", tz: america(420) },
    Zone { id: "America/Los_Angeles", label: "(UTC-08:00) Los Angeles, Vancouver", tz: america(480) },
    Zone { id: "America/Miquelon", label: "(UTC-03:00) Saint-Pierre-et-Miquelon", tz: america(180) },
    Zone { id: "America/Guadeloupe", label: "(UTC-04:00) Guadeloupe, Martinique", tz: fixed(240) },
    Zone { id: "America/Cayenne", label: "(UTC-03:00) Guyane", tz: fixed(180) },
    Zone { id: "Indian/Reunion", label: "(UTC+04:00) La Réunion", tz: fixed(-240) },
    Zone { id: "Indian/Mayotte", label: "(UTC+03:00) Mayotte", tz: fixed(-180) },
    Zone { id: "Pacific/Noumea", label: "(UTC+11:00) Nouvelle-Calédonie", tz: fixed(-660) },
    Zone { id: "Pacific/Tahiti", label: "(UTC-10:00) Tahiti", tz: fixed(600) },
    Zone { id: "Asia/Dubai", label: "(UTC+04:00) Dubaï", tz: fixed(-240) },
    Zone { id: "Asia/Kolkata", label: "(UTC+05:30) Inde", tz: fixed(-330) },
    Zone { id: "Asia/Shanghai", label: "(UTC+08:00) Pékin, Hong Kong, Singapour", tz: fixed(-480) },
    Zone { id: "Asia/Tokyo", label: "(UTC+09:00) Tokyo", tz: fixed(-540) },
];

/// Indices dans [`ZONES`] : fuseau de l'utilisateur, second fuseau (`usize::MAX` = aucun).
static PRIMARY: AtomicUsize = AtomicUsize::new(0);
static SECOND: AtomicUsize = AtomicUsize::new(usize::MAX);

pub fn zone(id: &str) -> Option<&'static Zone> {
    ZONES.iter().find(|z| z.id.eq_ignore_ascii_case(id))
}

fn index(id: &str) -> Result<usize, String> {
    ZONES.iter().position(|z| z.id.eq_ignore_ascii_case(id)).ok_or_else(|| format!("fuseau inconnu : {id}"))
}

/// Règle le fuseau de l'utilisateur et le second fuseau (vide ou `None` = aucun). Rien ne change en cas d'erreur.
pub fn set_zones(primary: &str, second: Option<&str>) -> Result<(), String> {
    let p = index(primary)?;
    let s = match second.filter(|s| !s.is_empty()) {
        Some(id) => index(id)?,
        None => usize::MAX,
    };
    PRIMARY.store(p, Ordering::Relaxed);
    SECOND.store(s, Ordering::Relaxed);
    Ok(())
}

/// Fuseau de l'utilisateur (entrée de [`ZONES`]).
pub fn primary_zone() -> &'static Zone {
    &ZONES[PRIMARY.load(Ordering::Relaxed).min(ZONES.len() - 1)]
}

/// Second fuseau affiché à côté des heures, s'il est choisi.
pub fn second_zone() -> Option<&'static Zone> {
    ZONES.get(SECOND.load(Ordering::Relaxed))
}

/// Écart (minutes) à ajouter à une heure du fuseau `from` pour l'avoir dans le fuseau `to`, à cette heure locale.
pub fn delta_minutes(from: &TimeZone, to: &TimeZone, local: NaiveDateTime) -> i32 {
    let utc = from.to_utc(local);
    (to.to_local(utc) - local).num_minutes() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paris_offsets() {
        let at = |s: &str| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
        let tz = TimeZone::PARIS;
        assert_eq!(tz.to_utc(at("2023-01-15 10:00")), at("2023-01-15 09:00"));
        assert_eq!(tz.to_utc(at("2023-07-15 10:00")), at("2023-07-15 08:00"));
        // 2023 : heure d'été le 26 mars, retour le 29 octobre.
        assert_eq!(tz.to_utc(at("2023-03-26 01:59")), at("2023-03-26 00:59"));
        assert_eq!(tz.to_utc(at("2023-03-26 03:00")), at("2023-03-26 01:00"));
        assert_eq!(tz.to_utc(at("2023-10-29 03:00")), at("2023-10-29 02:00"));
        for s in ["2023-01-15 10:00", "2023-07-15 10:00", "2023-03-26 03:00", "2023-10-29 03:00"] {
            assert_eq!(tz.to_local(tz.to_utc(at(s))), at(s));
        }
    }

    #[test]
    fn europe_rules_match_paris() {
        assert_eq!(europe(-60), TimeZone::PARIS);
        let at = |s: &str| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
        // Londres : BST du 29 mars 2026 (01:00 → 02:00) au 25 octobre 2026 (02:00 → 01:00).
        let london = TimeZone::by_id("Europe/London").unwrap();
        assert_eq!(london.to_utc(at("2026-01-10 12:00")), at("2026-01-10 12:00"));
        assert_eq!(london.to_utc(at("2026-07-10 12:00")), at("2026-07-10 11:00"));
        assert_eq!(london.to_utc(at("2026-03-29 00:59")), at("2026-03-29 00:59"));
        assert_eq!(london.to_utc(at("2026-03-29 02:00")), at("2026-03-29 01:00"));
        // Athènes : même instant de bascule que Paris (01:00 UTC).
        let athens = TimeZone::by_id("Europe/Athens").unwrap();
        assert_eq!(athens.to_utc(at("2026-03-29 04:00")), at("2026-03-29 01:00"));
        assert_eq!(athens.to_utc(at("2026-03-29 02:59")), at("2026-03-29 00:59"));
    }

    #[test]
    fn america_and_overseas() {
        let at = |s: &str| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
        // New York 2026 : heure d'été du 8 mars au 1er novembre.
        let ny = TimeZone::by_id("America/New_York").unwrap();
        assert_eq!(ny.to_utc(at("2026-03-07 12:00")), at("2026-03-07 17:00"));
        assert_eq!(ny.to_utc(at("2026-03-08 12:00")), at("2026-03-08 16:00"));
        assert_eq!(ny.to_utc(at("2026-10-31 12:00")), at("2026-10-31 16:00"));
        assert_eq!(ny.to_utc(at("2026-11-02 12:00")), at("2026-11-02 17:00"));
        let reunion = TimeZone::by_id("indian/reunion").unwrap();
        assert_eq!(reunion.to_utc(at("2026-07-01 12:00")), at("2026-07-01 08:00"));
        // Paris → New York : 6 h d'écart d'ordinaire, 5 h entre les deux changements d'heure (fin mars 2026).
        let paris = TimeZone::PARIS;
        assert_eq!(delta_minutes(&paris, &ny, at("2026-01-15 12:00")), -360);
        assert_eq!(delta_minutes(&paris, &ny, at("2026-03-20 12:00")), -300);
        assert_eq!(delta_minutes(&paris, &reunion, at("2026-07-15 12:00")), 120);
    }

    #[test]
    fn zones_are_unique_and_settable() {
        let mut ids: Vec<&str> = ZONES.iter().map(|z| z.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), ZONES.len());
        assert!(set_zones("Mars/Olympus", None).is_err());
        // Réglage global : un seul test le touche, et le remet comme il était.
        assert_eq!(TimeZone::local(), TimeZone::PARIS);
        set_zones("Europe/Paris", Some("America/New_York")).unwrap();
        assert_eq!(second_zone().map(|z| z.id), Some("America/New_York"));
        set_zones("Europe/Paris", None).unwrap();
        assert_eq!(second_zone(), None);
    }
}
