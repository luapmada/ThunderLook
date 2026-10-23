//! Listes de distribution (`IPM.DistList`), en lecture seule. Propriétés de [MS-OXOCNTC] 2.2.2 (jeu `PSETID_Address`) :
//! `PidLidDistributionListName` (0x8053), `PidLidDistributionListOneOffMembers` (0x8054) et `PidLidDistributionListMembers`
//! (0x8055), tableaux d'EntryID. Seuls les EntryID « one-off » ([MS-OXCDATA] 2.2.5.1 : nom, type et adresse en clair)
//! donnent un nom et une adresse ; un EntryID de contact ou de liste (renvoi vers un autre élément) n'est pas résolu.

use std::collections::HashMap;

use thunderlook_calendar::{
    bag::{Bag, cp1252},
    named::Named,
};
use thunderlook_model::contacts::{DistList, DistMember};
use thunderlook_store::rusqlite::Connection;

use crate::{PSETID_ADDRESS, Result};

/// GUID du fournisseur « one-off » (812B1FA4-BEA3-1019-9D6E-00DD010F5402) tel qu'écrit dans un EntryID.
const ONE_OFF_PROVIDER: [u8; 16] = [0xA4, 0x1F, 0x2B, 0x81, 0xA3, 0xBE, 0x19, 0x10, 0x9D, 0x6E, 0x00, 0xDD, 0x01, 0x0F, 0x54, 0x02];
const PT_MV_BINARY: u16 = 0x1102;

/// Liste lue, avec le dossier qui la contient et le nombre de membres que l'on ne sait pas résoudre.
pub struct Found {
    pub folder_id: i64,
    pub list: DistList,
    pub unresolved: u32,
}

/// Éléments d'une propriété `PtypMultipleBinary` (nombre, puis longueur et octets de chaque élément).
fn multi_binary<'a>(bag: &Bag<'a>, id: u16) -> Option<Vec<&'a [u8]>> {
    let (ty, v) = bag.raw(id)?;
    if ty != PT_MV_BINARY {
        return None;
    }
    let n = u32::from_le_bytes(v.get(0..4)?.try_into().ok()?) as usize;
    let (mut i, mut out) = (4, Vec::new());
    for _ in 0..n {
        let len = u32::from_le_bytes(v.get(i..i + 4)?.try_into().ok()?) as usize;
        out.push(v.get(i + 4..i + 4 + len)?);
        i += 4 + len;
    }
    Some(out)
}

/// Une chaîne terminée par un octet nul (ou deux pour l'Unicode) à partir de `at` ; rend la chaîne et l'indice suivant.
fn cstr(b: &[u8], at: usize, unicode: bool) -> Option<(String, usize)> {
    if unicode {
        let w: Vec<u16> = b.get(at..)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
        let next = at + (w.len() + 1) * 2;
        (next <= b.len()).then(|| (String::from_utf16_lossy(&w), next))
    } else {
        let end = at + b.get(at..)?.iter().position(|&c| c == 0)?;
        Some((b[at..end].iter().map(|&c| cp1252(c)).collect(), end + 1))
    }
}

/// Membre d'un EntryID « one-off » : drapeaux (4 octets nuls), fournisseur, version (2), drapeaux (2, bit 0x8000 :
/// chaînes Unicode), puis nom affiché, type d'adresse et adresse. `None` pour tout autre EntryID.
fn one_off(e: &[u8]) -> Option<DistMember> {
    if e.get(4..20)? != ONE_OFF_PROVIDER {
        return None;
    }
    let unicode = u16::from_le_bytes([*e.get(22)?, *e.get(23)?]) & 0x8000 != 0;
    let (name, i) = cstr(e, 24, unicode)?;
    let (_kind, i) = cstr(e, i, unicode)?;
    let (email, _) = cstr(e, i, unicode)?;
    Some(DistMember { name: name.trim().to_string(), email: email.trim().to_string() })
}

/// Toutes les listes de distribution de la base, ou celles d'un dossier.
pub fn find(conn: &Connection, folder: Option<i64>) -> Result<Vec<Found>> {
    let e = |x: thunderlook_store::rusqlite::Error| x.to_string();
    let mut named: HashMap<i64, Named> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT id, props FROM source WHERE props IS NOT NULL").map_err(e)?;
        let mut rows = stmt.query([]).map_err(e)?;
        while let Some(r) = rows.next().map_err(e)? {
            let raw: Vec<u8> = r.get(1).map_err(e)?;
            named.insert(r.get(0).map_err(e)?, Named::parse(&raw)?);
        }
    }
    let mut stmt = conn
        .prepare(
            "SELECT m.id, f.source_id, m.folder_id, m.subject, m.props
             FROM message m JOIN folder f ON f.id = m.folder_id
             WHERE (m.message_class = 'IPM.DistList' OR m.message_class LIKE 'IPM.DistList.%') AND (?1 IS NULL OR m.folder_id = ?1)",
        )
        .map_err(e)?;
    let mut rows = stmt.query([folder]).map_err(e)?;
    let (mut out, none) = (Vec::new(), Named::default());
    while let Some(r) = rows.next().map_err(e)? {
        let (id, source, folder_id, subject): (i64, i64, i64, String) = (r.get(0).map_err(e)?, r.get(1).map_err(e)?, r.get(2).map_err(e)?, r.get(3).map_err(e)?);
        let props: Option<Vec<u8>> = r.get(4).map_err(e)?;
        let n = named.get(&source).unwrap_or(&none);
        let (mut name, mut members, mut total) = (subject, Vec::new(), 0usize);
        if let Some(Ok(bag)) = props.as_deref().map(Bag::parse) {
            let lid = |l| n.id(PSETID_ADDRESS, l);
            if let Some(t) = lid(0x8053).and_then(|i| bag.text(i)).or_else(|| bag.text(0x3001)) {
                name = t;
            }
            // Les « one-off » listent tous les membres ; à défaut, ceux de la liste générale qui le sont.
            let general = lid(0x8055).and_then(|i| multi_binary(&bag, i)).unwrap_or_default();
            let oneoff = lid(0x8054).and_then(|i| multi_binary(&bag, i));
            total = general.len().max(oneoff.as_ref().map_or(0, Vec::len));
            members = oneoff.unwrap_or(general).into_iter().filter_map(one_off).collect();
        }
        let unresolved = (total - members.len().min(total)) as u32;
        out.push(Found { folder_id, list: DistList { id: format!("d{id}"), name, members }, unresolved });
    }
    out.sort_by(|a, b| crate::view::sort_key(&a.list.name).cmp(&crate::view::sort_key(&b.list.name)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use thunderlook_calendar::bag::{self, Value};

    fn entry(name: &str, kind: &str, email: &str, unicode: bool) -> Vec<u8> {
        let mut e = vec![0, 0, 0, 0];
        e.extend_from_slice(&ONE_OFF_PROVIDER);
        e.extend_from_slice(&[0, 0]);
        e.extend_from_slice(&(if unicode { 0x8000u16 } else { 0 }).to_le_bytes());
        for s in [name, kind, email] {
            if unicode {
                s.encode_utf16().chain([0]).for_each(|c| e.extend_from_slice(&c.to_le_bytes()));
            } else {
                e.extend(s.chars().map(|c| if c == 'é' { 0xE9 } else { c as u8 }).chain([0]));
            }
        }
        e
    }

    fn mv(items: &[Vec<u8>]) -> Value {
        let mut v = (items.len() as u32).to_le_bytes().to_vec();
        for i in items {
            v.extend_from_slice(&(i.len() as u32).to_le_bytes());
            v.extend_from_slice(i);
        }
        Value::Raw(PT_MV_BINARY, v)
    }

    #[test]
    fn entryid_one_off_unicode_et_8_bits() {
        let m = one_off(&entry("Léa Martin", "SMTP", "lea@exemple.fr", true)).unwrap();
        assert_eq!((m.name.as_str(), m.email.as_str()), ("Léa Martin", "lea@exemple.fr"));
        let m = one_off(&entry("Zoé", "SMTP", "zoe@exemple.fr", false)).unwrap();
        assert_eq!((m.name.as_str(), m.email.as_str()), ("Zoé", "zoe@exemple.fr"));
        // Contact référencé (autre fournisseur) ou entrée tronquée : pas un membre lisible.
        let mut other = entry("X", "SMTP", "x@y.fr", true);
        other[4] ^= 0xFF;
        assert!(one_off(&other).is_none());
        assert!(one_off(&entry("X", "SMTP", "x@y.fr", true)[..30]).is_none());
    }

    #[test]
    fn listes_lues_dans_la_base() {
        let dir = std::env::temp_dir().join(format!("tl-distlist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let conn = thunderlook_store::open(&dir).unwrap();
        let mut names = Vec::new();
        for (id, lid) in [(0x8000u16, 0x8053u32), (0x8001, 0x8054), (0x8002, 0x8055)] {
            names.extend_from_slice(&id.to_le_bytes());
            names.extend_from_slice(PSETID_ADDRESS.as_bytes());
            names.push(0);
            names.extend_from_slice(&lid.to_le_bytes());
        }
        conn.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at, props) VALUES (1, 'tl', '', '', 't', 0, 0, ?1)", [names]).unwrap();
        conn.execute("INSERT INTO folder (id, source_id, nid, name, role, hidden, position) VALUES (1, 1, 1, 'Contacts', 'folder', 0, 0)", []).unwrap();
        let add = |nid: i64, class: &str, subject: &str, props: Vec<u8>| {
            conn.execute(
                "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, created_at, modified_at, received_at,
                    message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
                 VALUES (1, ?1, ?2, ?3, '', '', '', '', 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 'text', '', '', ?4)",
                thunderlook_store::rusqlite::params![nid, class, subject, props],
            )
            .unwrap();
        };
        let a = entry("Léa Martin", "SMTP", "lea@exemple.fr", true);
        let b = entry("Paul", "SMTP", "paul@exemple.fr", true);
        let contact_ref = vec![0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 3, 0, 0, 0];
        let props = bag::merge(&[], &[(0x8000, Some(Value::Unicode("Équipe".into()))), (0x8001, Some(mv(&[a.clone(), b.clone()]))), (0x8002, Some(mv(&[a.clone(), contact_ref.clone(), b.clone()])))]).unwrap();
        add(1, "IPM.DistList", "sujet", props);
        // Sans propriété « one-off » : les membres de la liste générale qui le sont ; nom de repli = objet.
        let props = bag::merge(&[], &[(0x8002, Some(mv(&[a, contact_ref])))]).unwrap();
        add(2, "IPM.DistList", "Anciens", props);
        add(3, "IPM.Contact", "pas une liste", Vec::new());
        let lists = find(&conn, None).unwrap();
        assert_eq!(lists.len(), 2);
        assert_eq!((lists[0].list.name.as_str(), lists[0].list.members.len(), lists[0].unresolved), ("Anciens", 1, 1));
        let team = &lists[1];
        assert_eq!((team.list.name.as_str(), team.unresolved), ("Équipe", 1));
        assert_eq!(team.list.members.iter().map(|m| m.email.as_str()).collect::<Vec<_>>(), ["lea@exemple.fr", "paul@exemple.fr"]);
        assert!(find(&conn, Some(99)).unwrap().is_empty());
    }
}
