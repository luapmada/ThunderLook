//! Règles côté client d'Outlook (J6) : lecture de `PR_RW_RULES_STREAM` (contenu associé
//! « IPM.RuleOrganizer » de la Boîte de réception) et des exports `.rwz`, qui ont le même format.
//!
//! Sources : spec communautaire de J. Engelhardt (CC-BY-SA-4.0) pour la disposition des éléments, et analyse
//! d'OUTLOOK.EXE par REA (`notes/rea/regles-outlook.md`) pour les versions, l'en-tête de règle, les étiquettes
//! d'objet MFC, le codage des longueurs et le type de valeur de chaque élément (`catalog.rs`).
//!
//! Chaque règle est bornée par son `bytecount` : un élément inconnu n'empêche pas de lire les règles suivantes,
//! et les octets bruts de chaque règle sont gardés pour ne rien perdre.

mod catalog;
mod describe;

use serde::{Deserialize, Serialize};

pub use catalog::CATALOG;
pub use describe::{describe, describe_element};

/// Nature d'un élément de règle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Header,
    Condition,
    Action,
    Exception,
}

/// Entrée du catalogue pour un identifiant d'élément.
pub fn catalog(id: u32) -> Option<(u8, Kind, &'static str)> {
    CATALOG.iter().find(|e| e.0 == id).map(|e| (e.1, e.2, e.3))
}

/// Versions du flux (`RW_Magic`) : un nombre, pas un motif (REA, `0x14084a418`).
pub const OL98: u32 = 0xED03C;
pub const OL2000: u32 = 0xE5EBD;
pub const OL2002: u32 = 1_000_000;
pub const OL2016: u32 = 0x140000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recipient {
    pub name: Option<String>,
    pub address: Option<String>,
}

/// Valeur d'un élément, selon son type (colonne « type » du catalogue).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Value {
    /// Types 0x0e (`0x64`) et 0x0f (`0x190`, bits 1 = arrivée, 4 = envoi).
    Flags { a: u32, flags: u32 },
    /// Types 0x01, 0x02, 0x10 : personnes, avec deux drapeaux de fin (1/0 pour « de », 0/0 pour « transférer »).
    Recipients { a: u32, recipients: Vec<Recipient>, tail: (u32, u32) },
    /// Type 0x09 : mots recherchés (sans casse, sous-chaîne).
    Words { flags: u32, text: String },
    /// Types 0x06, 0x07, 0x26 : importance ou sensibilité.
    Level { a: u32, level: u32 },
    /// Types 0x08 (catégories séparées par `;`), 0x05 et 0x1f (chemin), 0x0a (texte), 0x29 (flux RSS).
    Text { a: u32, text: String },
    /// Types 0x0c et 0x11 : indicateur (jours ou zéro, libellé de l'action).
    FlagFor { a: u32, n: u32, text: String, tail: u32 },
    /// Type 0x14 : taille en Ko.
    Size { a: u32, min_kb: u32, max_kb: u32 },
    /// Type 0x15 : dates (PT_APPTIME).
    Date { a: u32, test_after: u32, after_pad: u32, after: f64, test_before: u32, before_pad: u32, before: f64 },
    /// Type 0x17 : formulaire (nom affiché, classe de message).
    Form { a: u32, name: String, class: String },
    /// Type 0x1e : compte (nom, identifiant ASCII).
    Account { a: u32, name: String, id: String },
    /// Type 0x21 : ordinateur.
    Machine { a: u32, guid: Vec<u8> },
    /// Type 0x22 : carnet d'adresses.
    AddressBook { a: u32, entry_id: Vec<u8>, name: String },
    /// Type 0x04 : dossier de destination (EntryID du dossier et du magasin, nom affiché).
    Folder { a: u32, folder_entry_id: Vec<u8>, store_entry_id: Vec<u8>, name: String, tail: u32 },
    /// Type 0x12 : minutes.
    Minutes { a: u32, minutes: u32 },
    /// Type 0x1d : réponse par le serveur (EntryID du modèle, objet).
    ServerReply { a: u32, entry_id: Vec<u8>, subject: String },
    /// Type 0x27 : indicateur de suivi (1 aujourd'hui, 2 demain, 3 cette semaine, 4 semaine prochaine, 7 sans date, 10 terminé).
    FollowUp { a: u32, when: u32, text: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub id: u32,
    pub values: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    pub active: bool,
    /// Premier u32 de la règle : 24 bits bas = version, octet haut = « locator ».
    pub xr: u32,
    /// u32 qui suit « active » (bit 4 du drapeau interne d'Outlook), puis les trois u32 de la spec.
    pub unknown: u32,
    pub magic: [u32; 3],
    pub elements: Vec<Element>,
    /// Décodage incomplet : raison. Les éléments lus jusque-là sont gardés.
    pub error: Option<String>,
    /// Octets de la règle, depuis son premier u32 (rien n'est perdu).
    #[serde(with = "hexbytes")]
    pub raw: Vec<u8>,
}

impl Rule {
    pub fn elements_of(&self, kind: Kind) -> impl Iterator<Item = &Element> {
        self.elements.iter().filter(move |e| catalog(e.id).is_some_and(|c| c.1 == kind))
    }

    /// Règle appliquée à l'arrivée des messages (et non à l'envoi).
    pub fn on_arrival(&self) -> bool {
        self.elements.iter().find(|e| e.id == 0x190).and_then(|e| e.values.first()).is_none_or(|v| match v {
            Value::Flags { flags, .. } => flags & 1 != 0,
            _ => true,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuleStream {
    /// Version du flux (`RW_Magic`), et le u32 qui la suit.
    pub version: u32,
    pub version2: u32,
    pub header: Vec<u32>,
    pub rules: Vec<Rule>,
    /// Dernier dossier de modèles utilisé (affichage seulement).
    pub template_dir: String,
}

mod hexbytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(b: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&b.iter().map(|x| format!("{x:02x}")).collect::<String>())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(serde::de::Error::custom)).collect()
    }
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
    unicode: bool,
}

type R<T> = Result<T, String>;

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        let s = self.b.get(self.p..self.p + n).ok_or_else(|| format!("flux tronqué à l'octet {}", self.p))?;
        self.p += n;
        Ok(s)
    }

    fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> R<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> R<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f64(&mut self) -> R<f64> {
        let b = self.take(8)?;
        Ok(f64::from_le_bytes(b.try_into().expect("8 octets")))
    }

    /// Longueur : un octet, ou `0xFF` suivi d'un u16 (forme non minimale acceptée, comme Outlook).
    fn len(&mut self) -> R<usize> {
        let n = self.u8()?;
        Ok(if n == 0xFF { self.u16()? as usize } else { n as usize })
    }

    /// Chaîne TCHAR : UTF-16LE à partir d'OL2002, octets Windows-1252 avant.
    fn tstr(&mut self) -> R<String> {
        let n = self.len()?;
        if self.unicode {
            let b = self.take(2 * n)?;
            let units: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            Ok(String::from_utf16_lossy(&units))
        } else {
            Ok(latin(self.take(n)?))
        }
    }

    /// Chaîne d'octets (classe de message, identifiant de compte).
    fn astr(&mut self) -> R<String> {
        let n = self.len()?;
        Ok(latin(self.take(n)?))
    }

    fn bytes32(&mut self) -> R<Vec<u8>> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }
}

/// Windows-1252 pour les caractères courants ; le reste comme Latin-1.
fn latin(b: &[u8]) -> String {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“',
        '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    b.iter().map(|&c| if (0x80..0xA0).contains(&c) { HIGH[(c - 0x80) as usize] } else { c as char }).collect()
}

/// `XR_PropValArray` : index de propriétés de 16 octets, puis les données (chaînes UTF-16 terminées par NUL).
fn recipient(r: &mut Reader) -> R<Recipient> {
    let _reserved = r.u32()?; // 0 selon la spec ; 0xC0 observé
    let n = r.u32()? as usize;
    let size = r.u32()? as usize;
    let block = r.take(size)?;
    let string_at = |off: usize| -> Option<String> {
        let tail = block.get(off..)?;
        let units: Vec<u16> = tail.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|u| *u != 0).collect();
        Some(String::from_utf16_lossy(&units))
    };
    let (mut name, mut smtp, mut email) = (None, None, None);
    for i in 0..n {
        let e = block.get(16 * i..16 * i + 16).ok_or("index de propriétés tronqué")?;
        let tag = u32::from_le_bytes([e[0], e[1], e[2], e[3]]);
        let b = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as usize;
        match tag {
            0x3001_001F => name = string_at(b),
            0x39FE_001F => smtp = string_at(b),
            0x3003_001F => email = string_at(b),
            _ => {}
        }
    }
    Ok(Recipient { name, address: smtp.or(email) })
}

fn value(r: &mut Reader, ty: u8) -> R<Value> {
    Ok(match ty {
        0x0E | 0x0F => Value::Flags { a: r.u32()?, flags: r.u32()? },
        0x01 | 0x02 | 0x10 => {
            let a = r.u32()?;
            let n = r.u32()?;
            let mut recipients = Vec::with_capacity(n as usize);
            for _ in 0..n {
                recipients.push(recipient(r)?);
            }
            Value::Recipients { a, recipients, tail: (r.u32()?, r.u32()?) }
        }
        0x09 => Value::Words { flags: r.u32()?, text: r.tstr()? },
        0x06 | 0x07 | 0x26 => Value::Level { a: r.u32()?, level: r.u32()? },
        0x08 | 0x05 | 0x1F | 0x0A | 0x29 => Value::Text { a: r.u32()?, text: r.tstr()? },
        0x0C | 0x11 => Value::FlagFor { a: r.u32()?, n: r.u32()?, text: r.tstr()?, tail: r.u32()? },
        0x14 => Value::Size { a: r.u32()?, min_kb: r.u32()?, max_kb: r.u32()? },
        0x15 => Value::Date {
            a: r.u32()?,
            test_after: r.u32()?,
            after_pad: r.u32()?,
            after: r.f64()?,
            test_before: r.u32()?,
            before_pad: r.u32()?,
            before: r.f64()?,
        },
        0x17 => Value::Form { a: r.u32()?, name: r.tstr()?, class: r.astr()? },
        0x1E => Value::Account { a: r.u32()?, name: r.tstr()?, id: r.astr()? },
        0x21 => Value::Machine { a: r.u32()?, guid: r.take(16)?.to_vec() },
        0x22 => Value::AddressBook { a: r.u32()?, entry_id: r.bytes32()?, name: r.tstr()? },
        0x04 => Value::Folder { a: r.u32()?, folder_entry_id: r.bytes32()?, store_entry_id: r.bytes32()?, name: r.tstr()?, tail: r.u32()? },
        0x12 => Value::Minutes { a: r.u32()?, minutes: r.u32()? },
        0x1D => Value::ServerReply { a: r.u32()?, entry_id: r.bytes32()?, subject: r.tstr()? },
        0x27 => Value::FollowUp { a: r.u32()?, when: r.u32()?, text: r.tstr()? },
        other => return Err(format!("type de valeur {other:#04x} non décodé")),
    })
}

/// Éléments d'une règle : nombre (u16), puis pour chacun une étiquette d'objet MFC (`0xFFFF` + schéma + nom de
/// classe la première fois, `0x8001` ensuite) et l'élément (identifiant, nombre de valeurs, valeurs).
fn elements(r: &mut Reader, out: &mut Vec<Element>) -> R<()> {
    let n = r.u16()?;
    for _ in 0..n {
        match r.u16()? {
            0xFFFF => {
                let _schema = r.u16()?;
                let len = r.u16()? as usize;
                let class = r.take(len)?;
                if class != b"CRuleElement" {
                    return Err(format!("classe inattendue : {}", latin(class)));
                }
            }
            0x8001 => {}
            tag => return Err(format!("étiquette d'objet {tag:#06x} inattendue")),
        }
        let id = r.u32()?;
        let ty = catalog(if id == 0xEA { 0xEB } else { id }).ok_or_else(|| format!("élément {id:#x} absent du catalogue"))?.0;
        let count = r.u32()?;
        let mut values = Vec::with_capacity(count as usize);
        for _ in 0..count {
            values.push(value(r, ty).map_err(|e| format!("élément {id:#x} : {e}"))?);
        }
        out.push(Element { id, values });
    }
    Ok(())
}

/// Lit un flux de règles (`.rwz` ou `PR_RW_RULES_STREAM`).
pub fn parse(bytes: &[u8]) -> Result<RuleStream, String> {
    let mut r = Reader { b: bytes, p: 0, unicode: true };
    let version = r.u32()?;
    let modern = version >= OL2002;
    // OL98 et OL2000 : 4 octets de version ; à partir d'OL2002 : 8.
    let version2 = if modern { r.u32()? } else { 0 };
    r.unicode = modern;
    let header: Vec<u32> = (0..if modern { 9 } else { 8 }).map(|_| r.u32()).collect::<R<_>>()?;
    let n = r.u16()?;
    let mut rules = Vec::with_capacity(n as usize);
    for k in 0..n {
        let start = r.p;
        let xr = if modern { r.u32()? } else { 0 };
        let name = r.tstr()?;
        let active = r.u32()? != 0;
        let unknown = if modern { r.u32()? } else { 0 };
        let magic = [r.u32()?, r.u32()?, r.u32()?];
        let mut rule = Rule { name, active, xr, unknown, magic, elements: Vec::new(), error: None, raw: Vec::new() };
        if modern {
            let count = r.u32()? as usize;
            let end = r.p.checked_add(count).filter(|e| *e <= bytes.len()).ok_or(format!("règle {k} : taille {count} hors du flux"))?;
            let mut inner = Reader { b: &bytes[..end], p: r.p, unicode: true };
            match elements(&mut inner, &mut rule.elements) {
                Ok(()) if inner.p == end => {}
                Ok(()) => rule.error = Some(format!("{} octets non lus en fin de règle", end - inner.p)),
                Err(e) => rule.error = Some(e),
            }
            r.p = end;
        } else {
            // Sans taille de règle, une erreur interrompt la lecture du flux.
            elements(&mut r, &mut rule.elements).map_err(|e| format!("règle {k} « {} » : {e}", rule.name))?;
        }
        rule.raw = bytes[start..r.p].to_vec();
        rules.push(rule);
    }
    let template_dir = match r.u32() {
        Ok(len) => {
            let b = r.take(if modern { 2 * len as usize } else { len as usize })?;
            if modern {
                String::from_utf16_lossy(&b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
            } else {
                latin(b)
            }
        }
        Err(_) => String::new(),
    };
    Ok(RuleStream { version, version2, header, rules, template_dir })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Règle synthétique : « de x@example.org », déplacer vers « Factures », arrêter.
    fn sample() -> Vec<u8> {
        fn tstr(out: &mut Vec<u8>, s: &str) {
            let units: Vec<u16> = s.encode_utf16().collect();
            out.push(units.len() as u8);
            units.iter().for_each(|u| out.extend(u.to_le_bytes()));
        }
        let u32s = |out: &mut Vec<u8>, v: &[u32]| v.iter().for_each(|x| out.extend(x.to_le_bytes()));
        let mut body = Vec::new();
        body.extend(4u16.to_le_bytes());
        body.extend(0xFFFFu16.to_le_bytes());
        body.extend(0u16.to_le_bytes());
        body.extend(12u16.to_le_bytes());
        body.extend(b"CRuleElement");
        u32s(&mut body, &[0x190, 1, 0, 1]);
        body.extend(0x8001u16.to_le_bytes());
        // De : un destinataire, nom et adresse SMTP dans le bloc de propriétés.
        u32s(&mut body, &[0xCB, 1, 0, 1]);
        let mut block = Vec::new();
        let name: Vec<u8> = "Camille".encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
        let addr: Vec<u8> = "x@example.org".encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
        u32s(&mut block, &[0x3001_001F, 0, 32, 0, 0x39FE_001F, 0, 32 + name.len() as u32, 0]);
        block.extend(&name);
        block.extend(&addr);
        u32s(&mut body, &[0xC0, 2, block.len() as u32]);
        body.extend(&block);
        u32s(&mut body, &[1, 0]);
        body.extend(0x8001u16.to_le_bytes());
        u32s(&mut body, &[0x12C, 1, 0, 3]);
        body.extend([1, 2, 3]);
        u32s(&mut body, &[2]);
        body.extend([9, 9]);
        tstr(&mut body, "Factures");
        u32s(&mut body, &[0]);
        body.extend(0x8001u16.to_le_bytes());
        u32s(&mut body, &[0x142, 0]);
        let mut out = Vec::new();
        u32s(&mut out, &[OL2016, 0x0614_0000, 0, 0, 0, 1, 0, 0, 1, 1, 0]);
        out.extend(1u16.to_le_bytes());
        u32s(&mut out, &[OL2016]);
        tstr(&mut out, "Factures de Camille");
        u32s(&mut out, &[1, 0, 0, 0, 0, body.len() as u32]);
        out.extend(&body);
        u32s(&mut out, &[0, 2, 0, 0, 0, 0]);
        out
    }

    #[test]
    fn parses_synthetic_rule() {
        let s = parse(&sample()).unwrap();
        assert_eq!((s.version, s.rules.len()), (OL2016, 1));
        let r = &s.rules[0];
        assert_eq!((r.name.as_str(), r.active, r.error.as_deref()), ("Factures de Camille", true, None));
        assert_eq!(r.elements.iter().map(|e| e.id).collect::<Vec<_>>(), [0x190, 0xCB, 0x12C, 0x142]);
        assert!(r.on_arrival());
        let Value::Recipients { recipients, tail, .. } = &r.elements[1].values[0] else { panic!() };
        assert_eq!(recipients[0], Recipient { name: Some("Camille".into()), address: Some("x@example.org".into()) });
        assert_eq!(*tail, (1, 0));
        let Value::Folder { name, folder_entry_id, .. } = &r.elements[2].values[0] else { panic!() };
        assert_eq!((name.as_str(), folder_entry_id.as_slice()), ("Factures", &[1, 2, 3][..]));
        assert_eq!(
            describe(r),
            vec![
                "Appliquer cette règle après l'arrivée du message".to_string(),
                "de Camille <x@example.org>".into(),
                "le déplacer dans le dossier Factures".into(),
                "arrêter de traiter plus de règles".into(),
            ]
        );
    }

    #[test]
    fn unknown_element_keeps_following_rules() {
        let mut b = sample();
        // Identifiant inconnu à la place de « arrêter » : la règle est marquée, le flux reste lisible.
        let pos = b.windows(4).rposition(|w| w == 0x142u32.to_le_bytes()).unwrap();
        b[pos..pos + 4].copy_from_slice(&0x999u32.to_le_bytes());
        let s = parse(&b).unwrap();
        assert!(s.rules[0].error.as_deref().unwrap().contains("0x999"));
        assert_eq!(s.rules[0].elements.len(), 3);
    }
}
