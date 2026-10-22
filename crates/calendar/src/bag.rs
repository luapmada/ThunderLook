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
        let mut props = HashMap::new();
        for e in scan(bag)? {
            props.insert(e.id, (e.ty, &bag[e.start..e.end]));
        }
        Ok(Self { props })
    }
}

/// Une propriété dans le sac : `encoded..end` est sa valeur telle qu'écrite (longueur comprise), `start..end` la
/// valeur seule.
struct Entry {
    id: u16,
    ty: u16,
    encoded: usize,
    start: usize,
    end: usize,
}

fn scan(bag: &[u8]) -> Result<Vec<Entry>, String> {
    let u32_at = |i: usize| bag.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let mut out = Vec::new();
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
        bag.get(start..start + len).ok_or_else(|| format!("sac tronqué (propriété {id:#06x})"))?;
        out.push(Entry { id, ty, encoded: i, start, end: start + len });
        i = start + len;
    }
    Ok(out)
}

/// Valeur à écrire dans un sac ([`merge`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Long(i32),
    Bool(bool),
    /// FILETIME (intervalles de 100 ns depuis 1601, UTC).
    Time(i64),
    Unicode(String),
    Binary(Vec<u8>),
    Strings(Vec<String>),
    /// Type MAPI et valeur déjà codée comme dans un sac (longueur ou nombre compris) : recopie d'autres types.
    Raw(u16, Vec<u8>),
}

impl Value {
    /// Millisecondes Unix → FILETIME.
    pub fn time_ms(ms: i64) -> Self {
        Self::Time(ms * 10_000 + FILETIME_UNIX_OFFSET)
    }

    fn encode(&self, out: &mut Vec<u8>) -> u16 {
        let utf16 = |out: &mut Vec<u8>, s: &str| {
            let w: Vec<u16> = s.encode_utf16().collect();
            out.extend_from_slice(&((w.len() * 2) as u32).to_le_bytes());
            w.iter().for_each(|c| out.extend_from_slice(&c.to_le_bytes()));
        };
        match self {
            Value::Long(v) => {
                out.extend_from_slice(&v.to_le_bytes());
                PT_LONG
            }
            Value::Bool(v) => {
                out.push(u8::from(*v));
                PT_BOOLEAN
            }
            Value::Time(v) => {
                out.extend_from_slice(&v.to_le_bytes());
                PT_SYSTIME
            }
            Value::Unicode(s) => {
                utf16(out, s);
                PT_UNICODE
            }
            Value::Binary(b) => {
                out.extend_from_slice(&(b.len() as u32).to_le_bytes());
                out.extend_from_slice(b);
                PT_BINARY
            }
            Value::Strings(v) => {
                out.extend_from_slice(&(v.len() as u32).to_le_bytes());
                v.iter().for_each(|s| utf16(out, s));
                0x1000 | PT_UNICODE
            }
            Value::Raw(ty, v) => {
                out.extend_from_slice(v);
                *ty
            }
        }
    }
}

/// Sac `old` dont les propriétés de `set` sont remplacées (`Some`) ou retirées (`None`) ; les autres sont recopiées
/// telles quelles, dans leur ordre.
pub fn merge(old: &[u8], set: &[(u16, Option<Value>)]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(old.len() + 256);
    for e in scan(old)? {
        if set.iter().any(|(id, _)| *id == e.id) {
            continue;
        }
        out.extend_from_slice(&e.id.to_le_bytes());
        out.extend_from_slice(&e.ty.to_le_bytes());
        out.extend_from_slice(&old[e.encoded..e.end]);
    }
    for (id, v) in set {
        if let Some(v) = v {
            let mut value = Vec::new();
            let ty = v.encode(&mut value);
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&ty.to_le_bytes());
            out.extend_from_slice(&value);
        }
    }
    Ok(out)
}

impl<'a> Bag<'a> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_replaces_removes_and_keeps() {
        let old = merge(&[], &[(0x0037, Some(Value::Unicode("Objet".into()))), (0x8001, Some(Value::Long(7))), (0x8002, Some(Value::Bool(true)))]).unwrap();
        let new = merge(
            &old,
            &[
                (0x8001, Some(Value::Long(9))),
                (0x8002, None),
                (0x8003, Some(Value::Strings(vec!["A".into(), "Bé".into()]))),
                (0x8004, Some(Value::time_ms(1_000))),
                (0x8005, Some(Value::Binary(vec![1, 2, 3]))),
            ],
        )
        .unwrap();
        let b = Bag::parse(&new).unwrap();
        assert_eq!(b.text(0x0037).as_deref(), Some("Objet"));
        assert_eq!(b.int(0x8001), Some(9));
        assert_eq!(b.int(0x8002), None);
        assert_eq!(b.strings(0x8003), Some(vec!["A".to_string(), "Bé".to_string()]));
        assert_eq!(b.time_ms(0x8004), Some(1_000));
        assert_eq!(b.binary(0x8005), Some(&[1u8, 2, 3][..]));
    }
}
