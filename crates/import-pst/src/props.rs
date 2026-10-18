//! Lecture des valeurs MAPI, propriétés nommées et sac de propriétés brut.

use std::{collections::HashMap, io, rc::Rc};

use outlook_pst::{
    ltp::prop_context::{GuidValue, PropertyValue},
    messaging::{
        named_prop::{NamedPropertyGuid, NamedPropertyId},
        store::Store,
    },
};

/// Écart entre l'époque FILETIME (1601) et l'époque Unix, en intervalles de 100 ns.
const FILETIME_UNIX_OFFSET: i64 = 116_444_736_000_000_000;

pub fn text(value: Option<&PropertyValue>) -> Option<String> {
    let s = match value? {
        PropertyValue::Unicode(v) => String::from_utf16_lossy(v.buffer()),
        PropertyValue::String8(v) => thunderlook_mime::decode(v.buffer(), 1252),
        _ => return None,
    };
    let s = s.trim_end_matches('\0');
    (!s.is_empty()).then(|| s.to_string())
}

pub fn binary(value: Option<&PropertyValue>) -> Option<&[u8]> {
    match value? {
        PropertyValue::Binary(v) => Some(v.buffer()),
        _ => None,
    }
}

pub fn int(value: Option<&PropertyValue>) -> Option<i64> {
    match value? {
        PropertyValue::Integer16(v) => Some(i64::from(*v)),
        PropertyValue::Integer32(v) => Some(i64::from(*v)),
        PropertyValue::Integer64(v) => Some(*v),
        PropertyValue::Boolean(v) => Some(i64::from(*v)),
        _ => None,
    }
}

pub fn boolean(value: Option<&PropertyValue>) -> bool {
    int(value).is_some_and(|v| v != 0)
}

/// `PtypTime` → millisecondes Unix. Les valeurs « jamais » (0, 4501-01-01) donnent `None`.
pub fn time_ms(value: Option<&PropertyValue>) -> Option<i64> {
    match value? {
        PropertyValue::Time(ft) if *ft > FILETIME_UNIX_OFFSET && *ft < 0x0CB3_4557_7F8B_0000 => {
            Some((ft - FILETIME_UNIX_OFFSET) / 10_000)
        }
        _ => None,
    }
}

pub fn strings(value: Option<&PropertyValue>) -> Vec<String> {
    match value {
        Some(PropertyValue::MultipleUnicode(v)) => v
            .iter()
            .map(|s| String::from_utf16_lossy(s.buffer()).trim_end_matches('\0').to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        Some(PropertyValue::MultipleString8(v)) => v
            .iter()
            .map(|s| thunderlook_mime::decode(s.buffer(), 1252).trim_end_matches('\0').to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        other => text(other).into_iter().collect(),
    }
}

/// Identifiant de nœud contenu dans un EntryID de PST ([MS-PST] 2.4.3.2 : 4 octets d'indicateurs,
/// 16 octets d'UID du fournisseur, puis le NID).
pub fn entry_id_nid(bytes: &[u8]) -> Option<u32> {
    bytes.get(20..24).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

pub fn guid_string(g: &GuidValue) -> String {
    let d = g.data4();
    format!(
        "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        g.data1(), g.data2(), g.data3(), d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]
    )
}

const PS_PUBLIC_STRINGS: &str = "00020329-0000-0000-C000-000000000046";
const PS_MAPI: &str = "00020328-0000-0000-C000-000000000046";
const PSETID_COMMON: &str = "00062008-0000-0000-C000-000000000046";
const PSETID_TASK: &str = "00062003-0000-0000-C000-000000000046";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum NameKey {
    Number(u32),
    Name(String),
}

/// Table des propriétés nommées du PST : (jeu, nom ou numéro) → identifiant 0x8000 + index.
pub struct NamedProps {
    map: HashMap<(String, NameKey), u16>,
    /// Sérialisation de toute la table (pour `source.props`) : relire les sacs bruts plus tard.
    pub raw: Vec<u8>,
    pub keywords: Option<u16>,
    pub task_due_date: Option<u16>,
    pub flag_request: Option<u16>,
}

impl NamedProps {
    pub fn read(store: &Rc<dyn Store>) -> io::Result<Self> {
        let map_obj = store.named_property_map()?;
        let props = map_obj.properties();
        let guids = props.stream_guid()?;
        let mut map = HashMap::new();
        let mut raw = Vec::new();
        for entry in props.stream_entry()? {
            let guid = match entry.guid() {
                NamedPropertyGuid::None => continue,
                NamedPropertyGuid::Mapi => PS_MAPI.to_string(),
                NamedPropertyGuid::PublicStrings => PS_PUBLIC_STRINGS.to_string(),
                // L'index de la table des GUID commence à 3 ; `GuidIndex(i)` porte déjà i = index - 3.
                NamedPropertyGuid::GuidIndex(i) => match guids.get(usize::from(i)) {
                    Some(g) => guid_string(g),
                    None => continue,
                },
            };
            let key = match entry.id() {
                NamedPropertyId::Number(n) => NameKey::Number(n),
                NamedPropertyId::StringOffset(off) => match props.lookup_string(off) {
                    Ok(s) => NameKey::Name(s.to_string()),
                    Err(_) => continue,
                },
            };
            // `prop_id()` rend déjà l'identifiant complet (0x8000 + wPropIdx).
            let prop_id = entry.prop_id() | 0x8000;
            // Format : id u16, GUID texte (36 octets), type (0 numéro / 1 nom), valeur.
            raw.extend_from_slice(&prop_id.to_le_bytes());
            raw.extend_from_slice(guid.as_bytes());
            match &key {
                NameKey::Number(n) => {
                    raw.push(0);
                    raw.extend_from_slice(&n.to_le_bytes());
                }
                NameKey::Name(s) => {
                    raw.push(1);
                    raw.extend_from_slice(&(s.len() as u32).to_le_bytes());
                    raw.extend_from_slice(s.as_bytes());
                }
            }
            map.insert((guid, key), prop_id);
        }
        let find = |g: &str, k: NameKey| map.get(&(g.to_string(), k)).copied();
        Ok(Self {
            keywords: find(PS_PUBLIC_STRINGS, NameKey::Name("Keywords".into())),
            task_due_date: find(PSETID_TASK, NameKey::Number(0x8105)),
            flag_request: find(PSETID_COMMON, NameKey::Number(0x8530)),
            map,
            raw,
        })
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
}

/// Sac de propriétés brut. Format (petit-boutiste), répété : `id u16`, `type u16`, valeur.
/// Valeurs fixes en clair ; chaînes et binaires précédés de leur longueur `u32` (UTF-16LE pour
/// `PtypString`) ; multivalués précédés du nombre d'éléments `u32` ; `PtypObject` = NID `u32` + taille `u32`.
pub fn propbag<'a>(props: impl Iterator<Item = (&'a u16, &'a PropertyValue)>, skip: &[u16]) -> Vec<u8> {
    let mut out = Vec::new();
    for (id, value) in props {
        if skip.contains(id) {
            continue;
        }
        let ty: u16 = outlook_pst::ltp::prop_type::PropertyType::from(value).into();
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&ty.to_le_bytes());
        write_value(&mut out, value);
    }
    out
}

fn put_bytes(out: &mut Vec<u8>, b: &[u8]) {
    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    out.extend_from_slice(b);
}

fn put_utf16(out: &mut Vec<u8>, s: &[u16]) {
    out.extend_from_slice(&((s.len() * 2) as u32).to_le_bytes());
    for c in s {
        out.extend_from_slice(&c.to_le_bytes());
    }
}

fn put_guid(out: &mut Vec<u8>, g: &GuidValue) {
    out.extend_from_slice(&g.data1().to_le_bytes());
    out.extend_from_slice(&g.data2().to_le_bytes());
    out.extend_from_slice(&g.data3().to_le_bytes());
    out.extend_from_slice(g.data4());
}

fn put_count(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u32).to_le_bytes());
}

fn write_value(out: &mut Vec<u8>, value: &PropertyValue) {
    use PropertyValue as V;
    match value {
        V::Null => {}
        V::Integer16(v) => out.extend_from_slice(&v.to_le_bytes()),
        V::Integer32(v) | V::ErrorCode(v) => out.extend_from_slice(&v.to_le_bytes()),
        V::Floating32(v) => out.extend_from_slice(&v.to_le_bytes()),
        V::Floating64(v) | V::FloatingTime(v) => out.extend_from_slice(&v.to_le_bytes()),
        V::Currency(v) | V::Integer64(v) | V::Time(v) => out.extend_from_slice(&v.to_le_bytes()),
        V::Boolean(v) => out.push(u8::from(*v)),
        V::String8(v) => put_bytes(out, v.buffer()),
        V::Unicode(v) => put_utf16(out, v.buffer()),
        V::Guid(g) => put_guid(out, g),
        V::Binary(v) => put_bytes(out, v.buffer()),
        V::Object(o) => {
            out.extend_from_slice(&u32::from(o.node()).to_le_bytes());
            out.extend_from_slice(&o.size().to_le_bytes());
        }
        V::MultipleInteger16(v) => {
            put_count(out, v.len());
            v.iter().for_each(|x| out.extend_from_slice(&x.to_le_bytes()));
        }
        V::MultipleInteger32(v) => {
            put_count(out, v.len());
            v.iter().for_each(|x| out.extend_from_slice(&x.to_le_bytes()));
        }
        V::MultipleFloating32(v) => {
            put_count(out, v.len());
            v.iter().for_each(|x| out.extend_from_slice(&x.to_le_bytes()));
        }
        V::MultipleFloating64(v) | V::MultipleFloatingTime(v) => {
            put_count(out, v.len());
            v.iter().for_each(|x| out.extend_from_slice(&x.to_le_bytes()));
        }
        V::MultipleCurrency(v) | V::MultipleInteger64(v) | V::MultipleTime(v) => {
            put_count(out, v.len());
            v.iter().for_each(|x| out.extend_from_slice(&x.to_le_bytes()));
        }
        V::MultipleString8(v) => {
            put_count(out, v.len());
            v.iter().for_each(|x| put_bytes(out, x.buffer()));
        }
        V::MultipleUnicode(v) => {
            put_count(out, v.len());
            v.iter().for_each(|x| put_utf16(out, x.buffer()));
        }
        V::MultipleGuid(v) => {
            put_count(out, v.len());
            v.iter().for_each(|g| put_guid(out, g));
        }
        V::MultipleBinary(v) => {
            put_count(out, v.len());
            v.iter().for_each(|x| put_bytes(out, x.buffer()));
        }
    }
}
