//! Incident du 21/10 : deux comptes ont des dossiers de même nom IMAP ; les règles copiées vers un compte doivent
//! viser ses propres dossiers, jamais ceux de l'autre (données synthétiques, sans réseau).

use std::fs;

use thunderlook_mailbox::{FolderMatch, FolderResolver, Mailbox};
use thunderlook_store::rusqlite::{Connection, params};

fn account(c: &Connection, id: i64, address: &str) -> (i64, i64) {
    c.execute(
        "INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (?1, 'imap', ?2, '', ?2, 0, 0)",
        params![id, address],
    )
    .unwrap();
    c.execute(
        "INSERT INTO account (source_id, name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, save_sent, created_at)
         VALUES (?1, '', ?2, ?2, 'imap.example.org', 993, 'tls', 'smtp.example.org', 465, 'tls', 1, 0)",
        params![id, address],
    )
    .unwrap();
    let folder = |parent: Option<i64>, name: &str, role: &str, imap: Option<&str>| -> i64 {
        c.execute(
            "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position, imap_name) VALUES (?1, ?2, 0, ?3, ?4, 'IPF.Note', 0, 0, ?5)",
            params![id, parent, name, role, imap],
        )
        .unwrap();
        c.last_insert_rowid()
    };
    let root = folder(None, "", "root", None);
    let top = folder(Some(root), address, "ipm", None);
    let inbox = folder(Some(top), "Boîte de réception", "inbox", Some("INBOX"));
    let target = folder(Some(inbox), "Accusés", "folder", Some("INBOX.Accus&AOk-s"));
    (inbox, target)
}

/// Flux d'une règle « déplacer vers Accusés » (cible d'un PST : EntryID quelconque).
fn stream() -> Vec<u8> {
    let u32s = |out: &mut Vec<u8>, v: &[u32]| v.iter().for_each(|x| out.extend(x.to_le_bytes()));
    let tstr = |out: &mut Vec<u8>, s: &str| {
        let units: Vec<u16> = s.encode_utf16().collect();
        out.push(units.len() as u8);
        units.iter().for_each(|u| out.extend(u.to_le_bytes()));
    };
    let mut body = Vec::new();
    body.extend(2u16.to_le_bytes());
    body.extend(0xFFFFu16.to_le_bytes());
    body.extend(0u16.to_le_bytes());
    body.extend(12u16.to_le_bytes());
    body.extend(b"CRuleElement");
    u32s(&mut body, &[0x190, 1, 0, 1]);
    body.extend(0x8001u16.to_le_bytes());
    u32s(&mut body, &[0x12C, 1, 0, 3]);
    body.extend([1, 2, 3]);
    u32s(&mut body, &[0]);
    tstr(&mut body, "Accusés");
    u32s(&mut body, &[0]);
    let mut out = Vec::new();
    u32s(&mut out, &[0x140000, 0x0614_0000, 0, 0, 0, 1, 0, 0, 1, 1, 0]);
    out.extend(1u16.to_le_bytes());
    u32s(&mut out, &[0x140000]);
    tstr(&mut out, "AR");
    u32s(&mut out, &[1, 0, 0, 0, 0, body.len() as u32]);
    out.extend(body);
    u32s(&mut out, &[0]);
    out
}

#[test]
fn copied_rules_target_their_own_account() {
    let dir = std::env::temp_dir().join(format!("tl-account-rules-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let c = thunderlook_store::open(&dir).unwrap();
    let (_, paul_target) = account(&c, 3, "paul@example.org");
    let (_, other_target) = account(&c, 4, "contact@example.org");
    drop(c);
    let rwz = dir.join("regles.rwz");
    fs::write(&rwz, stream()).unwrap();
    let mut mb = Mailbox::open(&dir).unwrap();
    mb.import_rwz(&rwz).unwrap();
    let origin = mb.rules().unwrap()[0].origin.clone();
    // Copie vers les deux comptes, dans l'ordre de l'incident (le second a les mêmes noms de dossiers).
    assert_eq!(mb.copy_rules_to_account(&origin, 3).unwrap(), (1, vec![]));
    assert_eq!(mb.copy_rules_to_account(&origin, 4).unwrap(), (1, vec![]));
    let c = thunderlook_store::open(&dir).unwrap();
    let resolver = FolderResolver::load(&c).unwrap();
    for (source, expected) in [(3, paul_target), (4, other_target)] {
        let json: String = c.query_row("SELECT rule_json FROM rule WHERE origin = ?1", [format!("account:{source}")], |r| r.get(0)).unwrap();
        let rule: thunderlook_rules::Rule = serde_json::from_str(&json).unwrap();
        let thunderlook_rules::Value::Folder { folder_entry_id, name, .. } = &rule.elements[1].values[0] else { panic!() };
        assert_eq!(resolver.resolve(folder_entry_id, name), FolderMatch::EntryId(expected), "compte {source}");
    }
    drop((c, mb));
    let _ = fs::remove_dir_all(&dir);
}
