//! Fuseau d'un rendez-vous périodique : `PidLidTimeZoneStruct` [MS-OXOCAL 2.2.1.39] (règle unique, sans historique).
//! Les dates d'une périodicité sont en heure locale de ce fuseau ; il faut ce fuseau pour les ramener en UTC.

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
    }
}
