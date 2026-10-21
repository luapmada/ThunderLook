//! Table des propriétés nommées d'une source (`source.props`, écrite par `thunderlook-import-pst::props::NamedProps`) :
//! répété, `id u16`, GUID texte (36 octets), type (0 numéro `u32`, 1 nom : longueur `u32` + UTF-8).

use std::collections::HashMap;

pub const PSETID_APPOINTMENT: &str = "00062002-0000-0000-C000-000000000046";
pub const PSETID_COMMON: &str = "00062008-0000-0000-C000-000000000046";
pub const PSETID_MEETING: &str = "6ED8DA90-450B-101B-98DA-00AA003F1305";
pub const PS_PUBLIC_STRINGS: &str = "00020329-0000-0000-C000-000000000046";

#[derive(Default)]
pub struct Named {
    numbers: HashMap<(String, u32), u16>,
    names: HashMap<(String, String), u16>,
}

impl Named {
    pub fn parse(raw: &[u8]) -> Result<Self, String> {
        let mut numbers = HashMap::new();
        let mut names = HashMap::new();
        let mut i = 0;
        while i < raw.len() {
            let head = raw.get(i..i + 39).ok_or("table des propriétés nommées tronquée")?;
            let id = u16::from_le_bytes([head[0], head[1]]);
            let guid = std::str::from_utf8(&head[2..38]).map_err(|e| e.to_string())?.to_ascii_uppercase();
            let kind = head[38];
            i += 39;
            let word = raw.get(i..i + 4).ok_or("table des propriétés nommées tronquée")?;
            let value = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
            i += 4;
            match kind {
                0 => {
                    numbers.insert((guid, value), id);
                }
                1 => {
                    let s = raw.get(i..i + value as usize).ok_or("table des propriétés nommées tronquée")?;
                    names.insert((guid, String::from_utf8_lossy(s).into_owned()), id);
                    i += value as usize;
                }
                k => return Err(format!("type de nom inconnu {k}")),
            }
        }
        Ok(Self { numbers, names })
    }

    /// Identifiant local (0x8000 et plus) d'une propriété nommée par numéro, s'il existe dans cette source.
    pub fn id(&self, set: &str, lid: u32) -> Option<u16> {
        self.numbers.get(&(set.to_string(), lid)).copied()
    }

    /// Identifiant local d'une propriété nommée par nom (`Keywords` des catégories, par exemple).
    pub fn name(&self, set: &str, name: &str) -> Option<u16> {
        self.names.get(&(set.to_string(), name.to_string())).copied()
    }
}
