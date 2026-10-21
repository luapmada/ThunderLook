//! Sac de propriétés MAPI brut (format de `thunderlook-import-pst::props::propbag`) : `id u16`, `type u16`, valeur ;
//! valeurs fixes en clair, chaînes et binaires précédés de leur longueur `u32`, multivalués de leur nombre `u32`.

use std::collections::HashMap;

pub const PT_I2: u16 = 0x0002;
pub const PT_LONG: u16 = 0x0003;
pub const PT_BOOLEAN: u16 = 0x000B;
pub const PT_STRING8: u16 = 0x001E;
pub const PT_UNICODE: u16 = 0x001F;
pub const PT_SYSTIME: u16 = 0x0040;
pub const PT_BINARY: u16 = 0x0102;

/// Écart entre l'époque FILETIME (1601) et l'époque Unix, en intervalles de 100 ns.
const FILETIME_UNIX_OFFSET: i64 = 116_444_736_000_000_000;

/// Propriétés d'un sac, indexées par identifiant : (type, octets de la valeur sans sa longueur).
pub struct Bag<'a> {
    props: HashMap<u16, (u16, &'a [u8])>,
}

fn fixed_size(ty: u16) -> Option<usize> {
    match ty {
        0x0002 => Some(2),
        0x0003 | 0x0004 | 0x000A => Some(4),
        0x0005 | 0x0006 | 0x0007 | 0x0014 | 0x0040 | 0x000D => Some(8),
        0x000B => Some(1),
        0x0048 => Some(16),
        _ => None,
    }
}

impl<'a> Bag<'a> {
    /// Lit tout le sac ; un sac tronqué ou d'un type inconnu est une erreur (rien ne se perd en silence).
    pub fn parse(bag: &'a [u8]) -> Result<Self, String> {
        let u32_at = |i: usize| bag.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
        let mut props = HashMap::new();
        let mut i = 0;
        while i < bag.len() {
            let head = bag.get(i..i + 4).ok_or("sac tronqué (en-tête)")?;
            let id = u16::from_le_bytes([head[0], head[1]]);
            let ty = u16::from_le_bytes([head[2], head[3]]);
            i += 4;
            let (start, len) = match ty {
                0x0001 => (i, 0),
                PT_STRING8 | PT_UNICODE | PT_BINARY => (i + 4, u32_at(i).ok_or("sac tronqué (longueur)")?),
                0x000D => (i, 8),
                t if t & 0x1000 != 0 => {
                    let n = u32_at(i).ok_or("sac tronqué (nombre)")?;
                    let mut j = i + 4;
                    match t & !0x1000 {
                        PT_STRING8 | PT_UNICODE | PT_BINARY => {
                            for _ in 0..n {
                                j += 4 + u32_at(j).ok_or("sac tronqué (élément)")?;
                            }
                        }
                        base => j += n * fixed_size(base).ok_or_else(|| format!("type multivalué inconnu {t:#06x}"))?,
                    }
                    (i, j - i)
                }
                t => (i, fixed_size(t).ok_or_else(|| format!("type inconnu {t:#06x} (propriété {id:#06x})"))?),
            };
            let value = bag.get(start..start + len).ok_or_else(|| format!("sac tronqué (propriété {id:#06x})"))?;
            props.insert(id, (ty, value));
            i = start + len;
        }
        Ok(Self { props })
    }

    pub fn raw(&self, id: u16) -> Option<(u16, &'a [u8])> {
        self.props.get(&id).copied()
    }

    pub fn int(&self, id: u16) -> Option<i64> {
        let (ty, v) = self.raw(id)?;
        match ty {
            PT_I2 => Some(i64::from(i16::from_le_bytes(v.try_into().ok()?))),
            PT_LONG => Some(i64::from(i32::from_le_bytes(v.try_into().ok()?))),
            0x0014 => Some(i64::from_le_bytes(v.try_into().ok()?)),
            PT_BOOLEAN => Some(i64::from(v[0])),
            _ => None,
        }
    }

    pub fn bool(&self, id: u16) -> Option<bool> {
        self.int(id).map(|v| v != 0)
    }

    /// `PtypTime` → millisecondes Unix (négatives avant 1970 : anniversaires) ; les valeurs « jamais » (0,
    /// 4501-01-01) donnent `None`.
    pub fn time_ms(&self, id: u16) -> Option<i64> {
        let (ty, v) = self.raw(id)?;
        if ty != PT_SYSTIME {
            return None;
        }
        let ft = i64::from_le_bytes(v.try_into().ok()?);
        (ft > 0 && ft < 0x0CB3_4557_7F8B_0000).then(|| (ft - FILETIME_UNIX_OFFSET).div_euclid(10_000))
    }

    pub fn text(&self, id: u16) -> Option<String> {
        let (ty, v) = self.raw(id)?;
        let s = match ty {
            PT_UNICODE => String::from_utf16_lossy(&v.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()),
            PT_STRING8 => v.iter().map(|&b| cp1252(b)).collect(),
            _ => return None,
        };
        let s = s.trim_end_matches('\0').trim();
        (!s.is_empty()).then(|| s.to_string())
    }

    /// `PtypMultipleString` / `PtypMultipleString8` : éléments dans leur ordre.
    pub fn strings(&self, id: u16) -> Option<Vec<String>> {
        let (ty, v) = self.raw(id)?;
        if ty != 0x101F && ty != 0x101E {
            return None;
        }
        let n = u32::from_le_bytes(v.get(0..4)?.try_into().ok()?) as usize;
        let (mut i, mut out) = (4, Vec::with_capacity(n));
        for _ in 0..n {
            let len = u32::from_le_bytes(v.get(i..i + 4)?.try_into().ok()?) as usize;
            let s = v.get(i + 4..i + 4 + len)?;
            out.push(if ty == 0x101F {
                String::from_utf16_lossy(&s.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
            } else {
                s.iter().map(|&b| cp1252(b)).collect()
            });
            i += 4 + len;
        }
        Some(out)
    }

    pub fn binary(&self, id: u16) -> Option<&'a [u8]> {
        match self.raw(id)? {
            (PT_BINARY, v) => Some(v),
            _ => None,
        }
    }
}

/// Windows-1252 → caractère (chaînes 8 bits des PST français).
pub fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}', '\u{90}', '‘', '’', '“', '”',
        '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}', 'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[usize::from(b - 0x80)],
        _ => char::from(b),
    }
}
