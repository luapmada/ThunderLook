//! Autocomplétion des destinataires et Carnet d'adresses (lot BC) : contacts, listes de distribution, puis adresses
//! déjà vues dans le courrier (expéditeurs du courrier reçu, destinataires du courrier envoyé). Lecture seule.

use std::{collections::HashMap, path::Path};

use thunderlook_store::rusqlite::Connection;

use crate::{Result, view::sort_key};

/// Combien de messages envoyés au plus sont ouverts pour y lire les destinataires (les plus récents).
const SENT_SCAN: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Contact,
    List,
    Mail,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub kind: Kind,
    pub name: String,
    /// Adresse du contact ou de l'expéditeur ; vide pour une liste.
    pub address: String,
    /// Texte à insérer dans le champ : « Nom <adr> », ou les membres d'une liste séparés par « ; ».
    pub insert: String,
    /// Complément affiché : société, nombre de membres, nombre d'échanges.
    pub detail: String,
}

/// « Nom <adr> » ; le nom est mis entre guillemets s'il contient une virgule, un point-virgule ou un chevron.
pub fn format_one(name: &str, addr: &str) -> String {
    let name = name.trim();
    if name.is_empty() || name.eq_ignore_ascii_case(addr) {
        addr.to_string()
    } else if name.contains([',', ';', '<', '>', '"']) {
        format!("\"{}\" <{addr}>", name.replace('"', ""))
    } else {
        format!("{name} <{addr}>")
    }
}

/// Les mots de la requête (sans casse ni accents) doivent tous figurer dans « nom adresse ».
fn matches(words: &[String], name: &str, addr: &str) -> bool {
    let hay = sort_key(&format!("{name} {addr}"));
    words.iter().all(|w| hay.contains(w.as_str()))
}

fn starts(words: &[String], name: &str, addr: &str) -> bool {
    words.first().is_some_and(|w| sort_key(name).starts_with(w.as_str()) || sort_key(addr).starts_with(w.as_str()) || sort_key(name).split_whitespace().any(|p| p.starts_with(w.as_str())))
}

/// Propositions pour `query` (vide : tout le carnet), au plus `limit`. Contacts d'abord, puis listes, puis courrier ;
/// une adresse n'apparaît qu'une fois, sous sa meilleure origine. À l'intérieur d'une origine : début de mot d'abord.
pub fn suggest(conn: &Connection, data_dir: &Path, query: &str, limit: usize) -> Result<Vec<Suggestion>> {
    let words: Vec<String> = query.split_whitespace().map(sort_key).collect();
    let mut out: Vec<(bool, Suggestion)> = Vec::new();
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut push = |out: &mut Vec<(bool, Suggestion)>, s: Suggestion| {
        let pre = starts(&words, &s.name, &s.address);
        if s.address.is_empty() || seen.insert(s.address.to_lowercase(), ()).is_none() {
            out.push((pre, s));
        }
    };

    for c in crate::load(conn, None, None)?.contacts {
        let name = if c.display_name.trim().is_empty() { c.title() } else { c.display_name.trim().to_string() };
        for e in &c.emails {
            if e.address.contains('@') && matches(&words, &name, &e.address) {
                let detail = if c.company.is_empty() { e.address.clone() } else { format!("{} · {}", c.company, e.address) };
                push(&mut out, Suggestion { kind: Kind::Contact, name: name.clone(), address: e.address.clone(), insert: format_one(&name, &e.address), detail });
            }
        }
    }
    for f in crate::distlist::find(conn, None)? {
        let members: Vec<String> = f.list.members.iter().filter(|m| m.email.contains('@')).map(|m| format_one(&m.name, &m.email)).collect();
        if !members.is_empty() && matches(&words, &f.list.name, "") {
            let detail = format!("liste de distribution · {} membre{}", members.len(), if members.len() > 1 { "s" } else { "" });
            push(&mut out, Suggestion { kind: Kind::List, name: f.list.name.clone(), address: String::new(), insert: members.join("; "), detail });
        }
    }
    // Contacts et listes d'abord (déjà dans l'ordre de la base : triés ici), le courrier ensuite.
    let mine = out.len();
    for s in mail_addresses(conn, data_dir, &words)? {
        push(&mut out, s);
    }
    let (head, tail) = out.split_at_mut(mine);
    for part in [head, tail] {
        part.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.kind.cmp(&b.1.kind)).then_with(|| sort_key(&a.1.name).cmp(&sort_key(&b.1.name))));
    }
    out.truncate(limit);
    Ok(out.into_iter().map(|(_, s)| s).collect())
}

/// Adresses du courrier : expéditeurs (hors dossiers Envoyés, Brouillons, Boîte d'envoi) du plus fréquent au plus rare,
/// puis destinataires des derniers messages envoyés (lus dans les `.eml`, la base ne garde que les noms affichés).
fn mail_addresses(conn: &Connection, data_dir: &Path, words: &[String]) -> Result<Vec<Suggestion>> {
    let e = |x: thunderlook_store::rusqlite::Error| x.to_string();
    let mut found: HashMap<String, (String, u32)> = HashMap::new();
    let mut note = |name: &str, addr: &str| {
        if addr.contains('@') && matches(words, name, addr) {
            let slot = found.entry(addr.to_lowercase()).or_insert_with(|| (String::new(), 0));
            slot.1 += 1;
            if slot.0.is_empty() && !name.trim().is_empty() && !name.contains('@') {
                slot.0 = name.trim().to_string();
            }
        }
    };
    let mut stmt = conn
        .prepare(
            "SELECT m.from_name, m.from_addr FROM message m JOIN folder f ON f.id = m.folder_id
             WHERE m.from_addr LIKE '%@%' AND f.role NOT IN ('sent', 'drafts', 'outbox', 'deleted', 'junk')",
        )
        .map_err(e)?;
    let mut rows = stmt.query([]).map_err(e)?;
    while let Some(r) = rows.next().map_err(e)? {
        note(&r.get::<_, String>(0).map_err(e)?, &r.get::<_, String>(1).map_err(e)?);
    }
    let mut stmt = conn
        .prepare(
            "SELECT m.eml_path FROM message m JOIN folder f ON f.id = m.folder_id
             WHERE f.role = 'sent' AND m.eml_path <> '' ORDER BY COALESCE(m.sent_at, m.received_at, 0) DESC LIMIT ?1",
        )
        .map_err(e)?;
    let paths = stmt.query_map([SENT_SCAN as i64], |r| r.get::<_, String>(0)).map_err(e)?.collect::<std::result::Result<Vec<_>, _>>().map_err(e)?;
    for p in paths {
        let Ok(raw) = std::fs::read(data_dir.join(&p)) else { continue };
        let Some(msg) = mail_parser::MessageParser::default().parse_headers(&raw) else { continue };
        for list in [msg.to(), msg.cc(), msg.bcc()] {
            for a in list.iter().flat_map(|l| l.iter()) {
                if let Some(addr) = a.address() {
                    note(a.name().unwrap_or(""), addr);
                }
            }
        }
    }
    Ok(found
        .into_iter()
        .map(|(addr, (name, n))| Suggestion {
            kind: Kind::Mail,
            insert: format_one(&name, &addr),
            detail: format!("{addr} · {n} message{}", if n > 1 { "s" } else { "" }),
            name: if name.is_empty() { addr.clone() } else { name },
            address: addr,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write;
    use thunderlook_model::contacts::ContactForm;

    fn fixture(name: &str) -> (std::path::PathBuf, Connection) {
        let dir = std::env::temp_dir().join(format!("tl-suggest-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let conn = thunderlook_store::open(&dir).unwrap();
        let mut names = Vec::new();
        for (k, lid) in [0x8080u32, 0x8082, 0x8083, 0x8084, 0x8090, 0x8092, 0x8093, 0x8094, 0x80A0, 0x80A2, 0x80A3, 0x80A4].into_iter().enumerate() {
            names.extend_from_slice(&(0x8000 + k as u16).to_le_bytes());
            names.extend_from_slice(crate::PSETID_ADDRESS.as_bytes());
            names.push(0);
            names.extend_from_slice(&lid.to_le_bytes());
        }
        conn.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at, props) VALUES (1, 'tl', '', '', 't', 0, 0, ?1)", [names]).unwrap();
        for (id, name, role) in [(1, "Contacts", "folder"), (2, "Boîte de réception", "inbox"), (3, "Éléments envoyés", "sent")] {
            conn.execute("INSERT INTO folder (id, source_id, nid, name, role, hidden, position) VALUES (?1, 1, ?1, ?2, ?3, 0, 0)", (id, name, role)).unwrap();
        }
        (dir, conn)
    }

    fn mail(conn: &Connection, folder: i64, from: (&str, &str), eml: &str) {
        conn.execute(
            "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, message_flags, is_read, flag_status,
             importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
             VALUES (?1, abs(random()), 'IPM.Note', 's', ?2, ?3, '', '', 0, 1, 0, 1, 0, 0, 0, 'text', '', ?4, x'')",
            (folder, from.0, from.1, eml),
        )
        .unwrap();
    }

    #[test]
    fn format_guillemets_si_virgule() {
        assert_eq!(format_one("Dupont, Jean", "j@x.fr"), "\"Dupont, Jean\" <j@x.fr>");
        assert_eq!(format_one("Jean", "j@x.fr"), "Jean <j@x.fr>");
        assert_eq!(format_one("", "j@x.fr"), "j@x.fr");
    }

    #[test]
    fn contacts_courrier_recu_et_envoye() {
        let (dir, mut conn) = fixture("tout");
        write::create(&mut conn, &dir, 1, &ContactForm { given_name: "Élodie".into(), surname: "Martin".into(), company: "ADI".into(), email1: "elodie@adi.fr".into(), ..Default::default() }).unwrap();
        mail(&conn, 2, ("Elodie Martin (perso)", "elodie@adi.fr"), "");
        mail(&conn, 2, ("Émile Zola", "emile@exemple.fr"), "");
        mail(&conn, 2, ("Émile Zola", "emile@exemple.fr"), "");
        std::fs::create_dir_all(dir.join("eml")).unwrap();
        std::fs::write(dir.join("eml/envoye.eml"), "From: moi@x.fr\r\nTo: \"Durand, Eve\" <eve@durand.fr>, emile@exemple.fr\r\nCc: Paul <paul@x.fr>\r\nSubject: s\r\n\r\ncorps").unwrap();
        mail(&conn, 3, ("Moi", "moi@x.fr"), "eml/envoye.eml");

        // Sans accent ni casse, le contact passe avant la même adresse vue dans le courrier.
        let r = suggest(&conn, &dir, "elodie", 10).unwrap();
        assert_eq!(r.len(), 1, "{r:?}");
        assert_eq!((r[0].kind, r[0].insert.as_str(), r[0].detail.as_str()), (Kind::Contact, "Élodie Martin <elodie@adi.fr>", "ADI · elodie@adi.fr"));

        // Expéditeurs (hors Envoyés) et destinataires des messages envoyés ; l'expéditeur « Moi » n'est pas proposé.
        let r = suggest(&conn, &dir, "", 50).unwrap();
        let addrs: Vec<&str> = r.iter().map(|s| s.address.as_str()).collect();
        for a in ["elodie@adi.fr", "emile@exemple.fr", "eve@durand.fr", "paul@x.fr"] {
            assert!(addrs.contains(&a), "{a} manque : {addrs:?}");
        }
        assert!(!addrs.contains(&"moi@x.fr"));
        assert_eq!(addrs.iter().filter(|a| **a == "emile@exemple.fr").count(), 1);
        let eve = r.iter().find(|s| s.address == "eve@durand.fr").unwrap();
        assert_eq!((eve.kind, eve.insert.as_str()), (Kind::Mail, "\"Durand, Eve\" <eve@durand.fr>"));
        let emile = r.iter().find(|s| s.address == "emile@exemple.fr").unwrap();
        assert!(emile.detail.ends_with("3 messages"), "{}", emile.detail);
        // Le contact reste devant le courrier.
        assert_eq!(r[0].kind, Kind::Contact);

        // Recherche par fragment d'adresse, et limite.
        assert_eq!(suggest(&conn, &dir, "durand.fr", 10).unwrap().len(), 1);
        assert_eq!(suggest(&conn, &dir, "", 2).unwrap().len(), 2);
        assert!(suggest(&conn, &dir, "zzz", 10).unwrap().is_empty());
    }
}
