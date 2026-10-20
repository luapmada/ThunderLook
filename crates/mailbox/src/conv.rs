//! Conversations (J3b) : calcul de `conversation_id` pour les bases importées avant le schéma v5, et
//! membres d'une conversation (tous dossiers du fichier de données, hors supprimés et indésirables).

use thunderlook_store::rusqlite::{Connection, params};

use crate::{Result, err};

/// `PidTagConversationIndexTracking`.
const TRACKING: u16 = 0x3016;

/// Valeur booléenne d'une propriété d'un sac MAPI brut (format de `thunderlook-import-pst::props::propbag`).
pub(crate) fn bag_bool(bag: &[u8], wanted: u16) -> Option<bool> {
    bag_value(bag, wanted, 0x000B).and_then(|v| v.first()).map(|b| *b != 0)
}

/// Valeur d'une propriété (identifiant et type) d'un sac MAPI brut : octets d'une valeur fixe, ou contenu d'une
/// chaîne ou d'un binaire sans sa longueur. `None` si absente ; le sac est lu sans allocation et un sac tronqué
/// arrête la lecture.
pub(crate) fn bag_value(bag: &[u8], wanted: u16, wanted_ty: u16) -> Option<&[u8]> {
    let u32_at = |i: usize| bag.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let fixed = |t: u16| match t {
        0x0002 => Some(2),
        0x0003 | 0x0004 | 0x000A => Some(4),
        0x0005 | 0x0006 | 0x0007 | 0x0014 | 0x0040 | 0x000D => Some(8),
        0x000B => Some(1),
        0x0048 => Some(16),
        _ => None,
    };
    let mut i = 0;
    while i + 4 <= bag.len() {
        let id = u16::from_le_bytes([bag[i], bag[i + 1]]);
        let ty = u16::from_le_bytes([bag[i + 2], bag[i + 3]]);
        i += 4;
        let (start, len) = match ty {
            0x0001 => (i, 0),
            0x001E | 0x001F | 0x0102 => (i + 4, u32_at(i)?),
            t if t & 0x1000 != 0 => {
                let n = u32_at(i)?;
                let mut j = i + 4;
                match t & !0x1000 {
                    0x001E | 0x001F | 0x0102 => {
                        for _ in 0..n {
                            j += 4 + u32_at(j)?;
                        }
                    }
                    base => j += n * fixed(base)?,
                }
                (i, j - i)
            }
            t => (i, fixed(t)?),
        };
        if id == wanted && ty == wanted_ty {
            return bag.get(start..start + len);
        }
        i = start + len;
    }
    None
}

/// Calcule `conversation_id` des messages qui n'en ont pas encore (bases antérieures au schéma v5).
/// Vide = pas de conversation. Renvoie le nombre de messages traités.
pub(crate) fn fill_conversation_ids(conn: &Connection) -> Result<usize> {
    let pending: bool = conn
        .query_row("SELECT EXISTS (SELECT 1 FROM message WHERE conversation_id IS NULL)", [], |r| r.get(0))
        .map_err(err)?;
    if !pending {
        return Ok(0);
    }
    let tx = conn.unchecked_transaction().map_err(err)?;
    let mut n = 0;
    {
        let mut read = tx
            .prepare("SELECT id, conversation_index, conversation_topic, props FROM message WHERE conversation_id IS NULL")
            .map_err(err)?;
        let mut write = tx.prepare("UPDATE message SET conversation_id = ?2 WHERE id = ?1").map_err(err)?;
        let mut rows = read.query([]).map_err(err)?;
        while let Some(r) = rows.next().map_err(err)? {
            let id: i64 = r.get(0).map_err(err)?;
            let index: Option<Vec<u8>> = r.get(1).map_err(err)?;
            let topic: Option<String> = r.get(2).map_err(err)?;
            let props = r.get_ref(3).map_err(err)?.as_blob().unwrap_or_default();
            let tracking = bag_bool(props, TRACKING).unwrap_or(false);
            let key = thunderlook_store::conversation_id(index.as_deref(), tracking, topic.as_deref());
            write.execute(params![id, key.map_or_else(Vec::new, |k| k.to_vec())]).map_err(err)?;
            n += 1;
        }
    }
    tx.commit().map_err(err)?;
    Ok(n)
}

/// Dossiers où l'on cherche les autres éléments d'une conversation : dossiers de courrier visibles de la
/// source, hors « Éléments supprimés » et « Courrier indésirable » et leurs sous-dossiers, plus le dossier
/// affiché. Rendus sous forme de liste SQL (`12,15,40`) : ce sont des entiers lus dans la base.
pub(crate) fn scope_folders(conn: &Connection, folder: i64) -> Result<String> {
    let mut stmt = conn
        .prepare(
            "WITH RECURSIVE excl(id) AS (
                SELECT id FROM folder WHERE role IN ('deleted', 'junk')
                UNION ALL SELECT x.id FROM folder x JOIN excl ON x.parent_id = excl.id)
             SELECT id FROM folder WHERE id = ?1 OR (source_id = (SELECT source_id FROM folder WHERE id = ?1)
                AND hidden = 0 AND id NOT IN (SELECT id FROM excl)
                AND (container_class IS NULL OR container_class = '' OR container_class LIKE 'IPF.Note%'
                     OR container_class LIKE 'IPF.Imap%'))",
        )
        .map_err(err)?;
    let ids: Vec<i64> = stmt.query_map([folder], |r| r.get(0)).map_err(err)?.collect::<std::result::Result<_, _>>().map_err(err)?;
    Ok(ids.iter().map(i64::to_string).collect::<Vec<_>>().join(","))
}

/// Éléments d'une conversation dans la portée, du plus récent au plus ancien.
pub(crate) fn members(conn: &Connection, key: &[u8; 16], scope: &str) -> Result<Vec<i64>> {
    let mut stmt = conn
        .prepare_cached(&format!(
            "SELECT id FROM message INDEXED BY message_conv
             WHERE conversation_id = ?1 AND length(conversation_id) = 16 AND folder_id IN ({scope})
             ORDER BY received_at DESC, id DESC"
        ))
        .map_err(err)?;
    stmt.query_map([&key[..]], |r| r.get(0)).map_err(err)?.collect::<std::result::Result<_, _>>().map_err(err)
}

/// Résumé d'une conversation pour son en-tête.
pub(crate) struct Summary {
    pub count: u32,
    pub unread: u32,
    pub unread_here: u32,
    pub flagged: bool,
    pub done: bool,
    pub high: bool,
    pub attachments: bool,
    /// Expéditeurs distincts, du plus récent au plus ancien (au plus [`MAX_SENDERS`]).
    pub senders: Vec<String>,
    pub categories: Vec<String>,
}

pub(crate) const MAX_SENDERS: usize = 8;

/// Résumé d'une conversation, calculé dans la base : une conversation peut compter des dizaines de
/// milliers d'éléments (emballement de liste de diffusion).
pub(crate) fn summary(conn: &Connection, key: &[u8; 16], scope: &str, folder: i64) -> Result<Summary> {
    let filter = format!("m.conversation_id = ?1 AND length(m.conversation_id) = 16 AND m.folder_id IN ({scope})");
    let (count, unread, unread_here, flagged, done, high, attachments) = conn
        .prepare_cached(&format!(
            "SELECT COUNT(*), IFNULL(SUM(m.is_read = 0), 0), IFNULL(SUM(m.is_read = 0 AND m.folder_id = ?2), 0),
                    IFNULL(MAX(m.flag_status = 2), 0), IFNULL(MAX(m.flag_status = 1), 0), IFNULL(MAX(m.importance = 2), 0),
                    EXISTS (SELECT 1 FROM message m INDEXED BY message_conv JOIN attachment a ON a.message_id = m.id
                            WHERE {filter} AND a.inline = 0 AND a.hidden = 0)
             FROM message m INDEXED BY message_conv WHERE {filter}"
        ))
        .map_err(err)?
        .query_row(params![&key[..], folder], |r| {
            Ok((r.get::<_, u32>(0)?, r.get::<_, u32>(1)?, r.get::<_, u32>(2)?, r.get::<_, bool>(3)?, r.get::<_, bool>(4)?, r.get::<_, bool>(5)?, r.get::<_, bool>(6)?))
        })
        .map_err(err)?;
    let mut senders: Vec<String> = Vec::new();
    {
        // Les 500 derniers éléments suffisent à trouver les participants récents.
        let mut stmt = conn
            .prepare_cached(&format!(
                "SELECT m.from_name, m.from_addr FROM message m INDEXED BY message_conv WHERE {filter}
                 ORDER BY m.received_at DESC, m.id DESC LIMIT 500"
            ))
            .map_err(err)?;
        let mut rows = stmt.query([&key[..]]).map_err(err)?;
        while let Some(r) = rows.next().map_err(err)? {
            let (name, addr): (String, String) = (r.get(0).map_err(err)?, r.get(1).map_err(err)?);
            let who = if name.trim().is_empty() { addr } else { name };
            if !who.trim().is_empty() && !senders.iter().any(|s| crate::fold(s) == crate::fold(&who)) {
                senders.push(who);
                if senders.len() == MAX_SENDERS {
                    break;
                }
            }
        }
    }
    let categories = conn
        .prepare_cached(&format!(
            "SELECT DISTINCT c.name FROM message m INDEXED BY message_conv JOIN message_category c ON c.message_id = m.id
             WHERE {filter} ORDER BY c.name"
        ))
        .map_err(err)?
        .query_map([&key[..]], |r| r.get(0))
        .map_err(err)?
        .collect::<std::result::Result<_, _>>()
        .map_err(err)?;
    Ok(Summary { count, unread, unread_here, flagged, done, high, attachments, senders, categories })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(out: &mut Vec<u8>, id: u16, ty: u16, value: &[u8]) {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&ty.to_le_bytes());
        out.extend_from_slice(value);
    }

    #[test]
    fn reads_boolean_after_other_types() {
        let mut bag = Vec::new();
        entry(&mut bag, 0x0037, 0x001F, &[4, 0, 0, 0, b'a', 0, b'b', 0]);
        entry(&mut bag, 0x0E08, 0x0003, &42u32.to_le_bytes());
        entry(&mut bag, 0x8001, 0x101F, &[2, 0, 0, 0, 2, 0, 0, 0, b'x', 0, 0, 0, 0, 0]);
        entry(&mut bag, 0x8002, 0x1003, &[1, 0, 0, 0, 7, 0, 0, 0]);
        entry(&mut bag, 0x0001, 0x0001, &[]);
        entry(&mut bag, 0x3016, 0x000B, &[1]);
        assert_eq!(bag_bool(&bag, 0x3016), Some(true));
        assert_eq!(bag_bool(&bag, 0x3017), None);
        assert_eq!(bag_bool(&bag[..bag.len() - 1], 0x3016), None);
        // Type inconnu : lecture arrêtée sans paniquer.
        let mut odd = Vec::new();
        entry(&mut odd, 0x1234, 0x7777, &[0; 8]);
        entry(&mut odd, 0x3016, 0x000B, &[1]);
        assert_eq!(bag_bool(&odd, 0x3016), None);
    }
}
