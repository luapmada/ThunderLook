//! `PidLidAppointmentRecur` [MS-OXOCAL 2.2.1.44] : RecurrencePattern, puis AppointmentRecurrencePattern (heures de
//! début et de fin, exceptions avec leurs champs modifiés). Toutes les dates sont en minutes depuis le 1601-01-01,
//! en heure locale du fuseau du rendez-vous.

use chrono::{Duration, NaiveDate, NaiveDateTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    /// Tous les `period` minutes (Daily : multiple de 1 440).
    Day,
    /// Jours de la semaine : bit 0 = dimanche … bit 6 = samedi.
    Week { days: u32 },
    /// Jour du mois (31 = dernier jour si le mois est plus court).
    Month { day: u32 },
    /// N-ième (1-4, 5 = dernier) jour parmi `days` (même masque que `Week`).
    MonthNth { days: u32, nth: u32 },
    /// Dernier jour du mois.
    MonthEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Date,
    Count(u32),
    Never,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Exception {
    pub start: u32,
    pub end: u32,
    pub original_start: u32,
    pub subject: Option<String>,
    pub location: Option<String>,
    pub busy_status: Option<u32>,
    pub reminder_delta: Option<u32>,
    pub reminder_set: Option<bool>,
    /// `PidLidAppointmentSubType` de l'occurrence : journée entière.
    pub all_day: Option<bool>,
    pub has_attachment: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recurrence {
    pub frequency: Frequency,
    pub pattern: Pattern,
    /// Calendrier non grégorien (hégirien, etc.) : non pris en charge pour l'expansion.
    pub calendar_type: u16,
    pub first_date_time: u32,
    pub period: u32,
    pub end: End,
    /// `OccurrenceCount` tel qu'écrit (le nombre d'occurrences, même pour une série qui finit à une date).
    pub occurrences: u32,
    pub first_day_of_week: u32,
    pub deleted: Vec<u32>,
    pub modified: Vec<u32>,
    pub start_date: u32,
    pub end_date: u32,
    pub start_offset: u32,
    pub end_offset: u32,
    pub exceptions: Vec<Exception>,
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let s = self.b.get(self.i..self.i + n).ok_or_else(|| format!("périodicité tronquée à l'octet {}", self.i))?;
        self.i += n;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16, String> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Result<u32, String> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn list(&mut self) -> Result<Vec<u32>, String> {
        let n = self.u32()?;
        if n > 100_000 {
            return Err(format!("liste de {n} dates"));
        }
        (0..n).map(|_| self.u32()).collect()
    }
    fn skip_block(&mut self) -> Result<(), String> {
        let n = self.u32()? as usize;
        self.take(n).map(|_| ())
    }
}

const OVERRIDE_SUBJECT: u16 = 0x0001;
const OVERRIDE_MEETING_TYPE: u16 = 0x0002;
const OVERRIDE_REMINDER_DELTA: u16 = 0x0004;
const OVERRIDE_REMINDER: u16 = 0x0008;
const OVERRIDE_LOCATION: u16 = 0x0010;
const OVERRIDE_BUSY: u16 = 0x0020;
const OVERRIDE_ATTACHMENT: u16 = 0x0040;
const OVERRIDE_SUBTYPE: u16 = 0x0080;
const OVERRIDE_COLOR: u16 = 0x0100;

impl Recurrence {
    pub fn parse(b: &[u8]) -> Result<Self, String> {
        let mut r = Reader { b, i: 0 };
        let (reader, writer) = (r.u16()?, r.u16()?);
        if reader != 0x3004 || writer != 0x3004 {
            return Err(format!("versions de périodicité inattendues {reader:#x}/{writer:#x}"));
        }
        let frequency = match r.u16()? {
            0x200A => Frequency::Daily,
            0x200B => Frequency::Weekly,
            0x200C => Frequency::Monthly,
            0x200D => Frequency::Yearly,
            f => return Err(format!("fréquence inconnue {f:#x}")),
        };
        let pattern_type = r.u16()?;
        let calendar_type = r.u16()?;
        let first_date_time = r.u32()?;
        let period = r.u32()?;
        let _sliding = r.u32()?;
        let pattern = match pattern_type {
            0x0000 => Pattern::Day,
            0x0001 => Pattern::Week { days: r.u32()? },
            0x0002 | 0x000A => Pattern::Month { day: r.u32()? },
            0x0003 | 0x000B => {
                let days = r.u32()?;
                Pattern::MonthNth { days, nth: r.u32()? }
            }
            0x0004 | 0x000C => {
                r.u32()?;
                Pattern::MonthEnd
            }
            p => return Err(format!("type de périodicité inconnu {p:#x}")),
        };
        let end_type = r.u32()?;
        let count = r.u32()?;
        let end = match end_type {
            0x2021 => End::Date,
            0x2022 => End::Count(count),
            0x2023 | 0xFFFF_FFFF => End::Never,
            e => return Err(format!("type de fin inconnu {e:#x}")),
        };
        let first_day_of_week = r.u32()?;
        let deleted = r.list()?;
        let modified = r.list()?;
        let start_date = r.u32()?;
        let end_date = r.u32()?;
        // AppointmentRecurrencePattern.
        let _reader2 = r.u32()?;
        let writer2 = r.u32()?;
        let start_offset = r.u32()?;
        let end_offset = r.u32()?;
        let n = r.u16()?;
        let mut exceptions = Vec::with_capacity(usize::from(n));
        let mut flags = Vec::with_capacity(usize::from(n));
        for _ in 0..n {
            let mut e = Exception { start: r.u32()?, end: r.u32()?, original_start: r.u32()?, ..Default::default() };
            let f = r.u16()?;
            if f & OVERRIDE_SUBJECT != 0 {
                let _len = r.u16()?;
                let len2 = r.u16()?;
                e.subject = Some(r.take(usize::from(len2))?.iter().map(|&c| crate::bag::cp1252(c)).collect());
            }
            if f & OVERRIDE_MEETING_TYPE != 0 {
                r.u32()?;
            }
            if f & OVERRIDE_REMINDER_DELTA != 0 {
                e.reminder_delta = Some(r.u32()?);
            }
            if f & OVERRIDE_REMINDER != 0 {
                e.reminder_set = Some(r.u32()? != 0);
            }
            if f & OVERRIDE_LOCATION != 0 {
                let _len = r.u16()?;
                let len2 = r.u16()?;
                e.location = Some(r.take(usize::from(len2))?.iter().map(|&c| crate::bag::cp1252(c)).collect());
            }
            if f & OVERRIDE_BUSY != 0 {
                e.busy_status = Some(r.u32()?);
            }
            if f & OVERRIDE_ATTACHMENT != 0 {
                e.has_attachment = r.u32()? != 0;
            }
            if f & OVERRIDE_SUBTYPE != 0 {
                e.all_day = Some(r.u32()? != 0);
            }
            if f & OVERRIDE_COLOR != 0 {
                r.u32()?;
            }
            exceptions.push(e);
            flags.push(f);
        }
        r.skip_block()?;
        // ExtendedException : objet et lieu en Unicode, qui remplacent les versions 8 bits ; ChangeHighlight à partir
        // de la version d'écriture 0x3009.
        {
            for (e, f) in exceptions.iter_mut().zip(&flags) {
                if writer2 >= 0x3009 {
                    let size = r.u32()? as usize;
                    r.take(size)?;
                }
                r.skip_block()?;
                if f & (OVERRIDE_SUBJECT | OVERRIDE_LOCATION) != 0 {
                    r.take(12)?;
                    let wide = |r: &mut Reader| -> Result<String, String> {
                        let n = usize::from(r.u16()?);
                        let s = r.take(n * 2)?;
                        Ok(String::from_utf16_lossy(&s.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()))
                    };
                    if f & OVERRIDE_SUBJECT != 0 {
                        e.subject = Some(wide(&mut r)?);
                    }
                    if f & OVERRIDE_LOCATION != 0 {
                        e.location = Some(wide(&mut r)?);
                    }
                    r.skip_block()?;
                }
            }
        }
        r.skip_block()?;
        Ok(Self {
            frequency,
            pattern,
            calendar_type,
            first_date_time,
            period,
            end,
            occurrences: count,
            first_day_of_week,
            deleted,
            modified,
            start_date,
            end_date,
            start_offset,
            end_offset,
            exceptions,
        })
    }
}

impl Recurrence {
    /// Encodage de la périodicité d'une série sans exception (rendez-vous créé dans ThunderLook).
    pub fn encode(&self) -> Vec<u8> {
        let mut b: Vec<u8> = Vec::new();
        let w16 = |b: &mut Vec<u8>, v: u16| b.extend_from_slice(&v.to_le_bytes());
        let w32 = |b: &mut Vec<u8>, v: u32| b.extend_from_slice(&v.to_le_bytes());
        w16(&mut b, 0x3004);
        w16(&mut b, 0x3004);
        w16(
            &mut b,
            match self.frequency {
                Frequency::Daily => 0x200A,
                Frequency::Weekly => 0x200B,
                Frequency::Monthly => 0x200C,
                Frequency::Yearly => 0x200D,
            },
        );
        w16(
            &mut b,
            match self.pattern {
                Pattern::Day => 0x0000,
                Pattern::Week { .. } => 0x0001,
                Pattern::Month { .. } => 0x0002,
                Pattern::MonthNth { .. } => 0x0003,
                Pattern::MonthEnd => 0x0004,
            },
        );
        w16(&mut b, self.calendar_type);
        w32(&mut b, self.first_date_time);
        w32(&mut b, self.period);
        w32(&mut b, 0);
        match self.pattern {
            Pattern::Day => {}
            Pattern::Week { days } => w32(&mut b, days),
            Pattern::Month { day } => w32(&mut b, day),
            Pattern::MonthNth { days, nth } => {
                w32(&mut b, days);
                w32(&mut b, nth);
            }
            Pattern::MonthEnd => w32(&mut b, 1),
        }
        let (end_type, count) = match self.end {
            End::Date => (0x2021, 0),
            End::Count(n) => (0x2022, n),
            End::Never => (0x2023, self.occurrences),
        };
        let count = if matches!(self.end, End::Date) { self.occurrences } else { count };
        w32(&mut b, end_type);
        w32(&mut b, count);
        w32(&mut b, self.first_day_of_week);
        w32(&mut b, self.deleted.len() as u32);
        self.deleted.iter().for_each(|d| w32(&mut b, *d));
        w32(&mut b, self.modified.len() as u32);
        self.modified.iter().for_each(|d| w32(&mut b, *d));
        w32(&mut b, self.start_date);
        w32(&mut b, self.end_date);
        // AppointmentRecurrencePattern sans exception.
        w32(&mut b, 0x3006);
        w32(&mut b, 0x3009);
        w32(&mut b, self.start_offset);
        w32(&mut b, self.end_offset);
        w16(&mut b, 0);
        w32(&mut b, 0);
        w32(&mut b, 0);
        b
    }

    /// Ajoute une occurrence supprimée (minuit local de son jour, en minutes depuis 1601) à une périodicité déjà
    /// encodée, sans toucher au reste (exceptions comprises). La liste reste triée.
    pub fn with_deleted_instance(blob: &[u8], day_minutes: u32) -> Result<Vec<u8>, String> {
        let r = Self::parse(blob)?;
        if r.deleted.contains(&day_minutes) {
            return Ok(blob.to_vec());
        }
        // Position du nombre d'occurrences supprimées : après l'en-tête et le motif.
        let pattern_len = match r.pattern {
            Pattern::Day => 0,
            Pattern::Week { .. } | Pattern::Month { .. } | Pattern::MonthEnd => 4,
            Pattern::MonthNth { .. } => 8,
        };
        let count_at = 5 * 2 + 4 * 3 + pattern_len + 4 * 3;
        let at = |i: usize| blob.get(i..i + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
        if at(count_at) != Some(r.deleted.len() as u32) {
            return Err("périodicité d'une forme inattendue".into());
        }
        let mut deleted = r.deleted.clone();
        deleted.push(day_minutes);
        deleted.sort_unstable();
        let list_end = count_at + 4 + 4 * r.deleted.len();
        let mut out = blob[..count_at].to_vec();
        out.extend_from_slice(&(deleted.len() as u32).to_le_bytes());
        deleted.iter().for_each(|d| out.extend_from_slice(&d.to_le_bytes()));
        out.extend_from_slice(&blob[list_end..]);
        Ok(out)
    }
}

/// Minutes depuis 1601-01-01 (heure locale) → date et heure.
pub fn minutes_to_local(m: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(1601, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap() + Duration::minutes(i64::from(m))
}

pub fn local_to_minutes(t: NaiveDateTime) -> u32 {
    (t - NaiveDate::from_ymd_opt(1601, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap()).num_minutes() as u32
}
