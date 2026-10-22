//! Propriétés d'un élément enregistré par Outlook au format `.msg` ([MS-OXMSG]) : toutes celles de la racine, avec
//! leurs valeurs brutes, propriétés nommées traduites dans la table de la source. C'est le seul moyen, hors du
//! processus d'Outlook, d'obtenir la périodicité (`PidLidAppointmentRecur`) et le fuseau (`PidLidTimeZoneStruct`)
//! d'un rendez-vous : `PropertyAccessor` les refuse.

use std::{collections::HashMap, io::Read, path::Path};

use thunderlook_calendar::{bag::Value, named::Named};

use crate::com::Result;

const PS_MAPI: &str = "00020328-0000-0000-C000-000000000046";
const PS_PUBLIC_STRINGS: &str = "00020329-0000-0000-C000-000000000046";
/// Corps (texte, HTML, RTF compressé) : gardés dans le `.eml`, comme à l'import.
const SKIPPED: [u16; 3] = [0x1000, 0x1013, 0x1009];

/// Nom d'une propriété nommée : numéro ou chaîne, dans un jeu de propriétés.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Name {
    Number(String, u32),
    String(String, String),
}

fn guid_text(b: &[u8]) -> String {
    let d1 = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let d2 = u16::from_le_bytes([b[4], b[5]]);
    let d3 = u16::from_le_bytes([b[6], b[7]]);
    format!("{d1:08X}-{d2:04X}-{d3:04X}-{:02X}{:02X}-{}", b[8], b[9], b[10..16].iter().map(|x| format!("{x:02X}")).collect::<String>())
}

fn fixed_size(ty: u16) -> Option<usize> {
    match ty {
        0x0002 => Some(2),
        0x0003 | 0x0004 | 0x000A => Some(4),
        0x0005 | 0x0006 | 0x0007 | 0x0014 | 0x0040 => Some(8),
        0x000B => Some(1),
        0x0048 => Some(16),
        _ => None,
    }
}

struct File(cfb::CompoundFile<std::fs::File>);

impl File {
    fn stream(&mut self, path: &str) -> Option<Vec<u8>> {
        let mut s = self.0.open_stream(path).ok()?;
        let mut out = Vec::new();
        s.read_to_end(&mut out).ok()?;
        Some(out)
    }

    /// Table des propriétés nommées du fichier : identifiant (0x8000 + index) → nom.
    fn names(&mut self) -> HashMap<u16, Name> {
        let guids = self.stream("/__nameid_version1.0/__substg1.0_00020102").unwrap_or_default();
        let entries = self.stream("/__nameid_version1.0/__substg1.0_00030102").unwrap_or_default();
        let strings = self.stream("/__nameid_version1.0/__substg1.0_00040102").unwrap_or_default();
        let mut out = HashMap::new();
        for e in entries.chunks_exact(8) {
            let key = u32::from_le_bytes([e[0], e[1], e[2], e[3]]);
            let index_kind = u32::from_le_bytes([e[4], e[5], e[6], e[7]]);
            let is_string = index_kind & 1 == 1;
            let guid_index = ((index_kind & 0xFFFF) >> 1) as usize;
            let prop_index = (index_kind >> 16) as u16;
            let set = match guid_index {
                1 => PS_MAPI.to_string(),
                2 => PS_PUBLIC_STRINGS.to_string(),
                n if n >= 3 => match guids.get((n - 3) * 16..(n - 2) * 16) {
                    Some(g) => guid_text(g),
                    None => continue,
                },
                _ => continue,
            };
            let name = if is_string {
                let at = key as usize;
                let Some(len) = strings.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize) else { continue };
                let Some(s) = strings.get(at + 4..at + 4 + len) else { continue };
                let units: Vec<u16> = s.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                Name::String(set, String::from_utf16_lossy(&units))
            } else {
                Name::Number(set, key)
            };
            out.insert(0x8000 + prop_index, name);
        }
        out
    }
}

/// Valeur codée comme dans un sac (voir `thunderlook_calendar::bag`) à partir d'une entrée du flux des propriétés.
fn value(f: &mut File, id: u16, ty: u16, inline: &[u8]) -> Option<Value> {
    let tag = format!("/__substg1.0_{id:04X}{ty:04X}");
    let len_prefixed = |b: Vec<u8>| {
        let mut v = (b.len() as u32).to_le_bytes().to_vec();
        v.extend(b);
        v
    };
    let encoded = match ty {
        0x0001 => Vec::new(),
        t if fixed_size(t).is_some() => inline.get(..fixed_size(t)?)?.to_vec(),
        0x001E | 0x0102 => len_prefixed(f.stream(&tag)?),
        // Chaînes Unicode : sans le zéro final que le fichier peut porter.
        0x001F => {
            let mut b = f.stream(&tag)?;
            while b.len() >= 2 && b[b.len() - 2..] == [0, 0] {
                b.truncate(b.len() - 2);
            }
            len_prefixed(b)
        }
        t if t & 0x1000 != 0 => {
            let base = t & !0x1000;
            let lengths = f.stream(&tag)?;
            match fixed_size(base) {
                // Multivalué fixe : valeurs à la suite.
                Some(size) => {
                    let n = lengths.len() / size;
                    let mut v = (n as u32).to_le_bytes().to_vec();
                    v.extend(&lengths[..n * size]);
                    v
                }
                // Multivalué variable : un flux par valeur (`-0000000N`), longueurs sur 4 (chaînes) ou 8 octets.
                None => {
                    let step = if base == 0x0102 { 8 } else { 4 };
                    let n = lengths.len() / step;
                    let mut v = (n as u32).to_le_bytes().to_vec();
                    for i in 0..n {
                        let mut item = f.stream(&format!("{tag}-{i:08X}")).unwrap_or_default();
                        if base == 0x001F {
                            while item.len() >= 2 && item[item.len() - 2..] == [0, 0] {
                                item.truncate(item.len() - 2);
                            }
                        }
                        v.extend((item.len() as u32).to_le_bytes());
                        v.extend(item);
                    }
                    v
                }
            }
        }
        _ => return None,
    };
    Some(Value::Raw(ty, encoded))
}

/// Toutes les propriétés de la racine d'un `.msg`, identifiants traduits dans la table `named` de la source.
/// Les propriétés nommées inconnues de la source sont ignorées (leur nombre est rendu).
pub fn read(path: &Path, named: &Named) -> Result<(Vec<(u16, Value)>, usize)> {
    let mut f = File(cfb::open(path).map_err(|e| format!("{} : {e}", path.display()))?);
    let names = f.names();
    let props = f.stream("/__properties_version1.0").ok_or("flux des propriétés absent du .msg")?;
    let mut out = Vec::new();
    let mut unknown = 0;
    for e in props.get(32..).unwrap_or_default().chunks_exact(16) {
        let tag = u32::from_le_bytes([e[0], e[1], e[2], e[3]]);
        let (id, ty) = ((tag >> 16) as u16, (tag & 0xFFFF) as u16);
        if SKIPPED.contains(&id) || ty == 0x000D {
            continue;
        }
        let local = if id < 0x8000 {
            Some(id)
        } else {
            match names.get(&id) {
                Some(Name::Number(set, lid)) => named.id(set, *lid),
                Some(Name::String(set, n)) => named.name(set, n),
                None => None,
            }
        };
        let Some(local) = local else {
            unknown += 1;
            continue;
        };
        if let Some(v) = value(&mut f, id, ty, &e[8..16]) {
            out.push((local, v));
        }
    }
    Ok((out, unknown))
}
