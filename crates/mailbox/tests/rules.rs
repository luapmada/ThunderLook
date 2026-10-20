//! Règles du J6 : import du flux d'un PST à l'ouverture et des `.rwz` à la demande (données synthétiques).

use std::{fs, path::PathBuf};

use thunderlook_mailbox::Mailbox;
use thunderlook_store::rusqlite::params;

struct Fixture {
    dir: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Flux minimal (version OL2016) : des règles sans élément, nommées.
fn stream(names: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    let u32s = |out: &mut Vec<u8>, v: &[u32]| v.iter().for_each(|x| out.extend(x.to_le_bytes()));
    u32s(&mut out, &[0x140000, 0x0614_0000, 0, 0, 0, 1, 0, 0, 1, 1, 0]);
    out.extend((names.len() as u16).to_le_bytes());
    for name in names {
        u32s(&mut out, &[0x140000]);
        let units: Vec<u16> = name.encode_utf16().collect();
        out.push(units.len() as u8);
        units.iter().for_each(|u| out.extend(u.to_le_bytes()));
        u32s(&mut out, &[1, 0, 0, 0, 0, 2]);
        out.extend(0u16.to_le_bytes());
    }
    u32s(&mut out, &[0]);
    out
}

fn setup(name: &str) -> (Fixture, PathBuf) {
    let dir = std::env::temp_dir().join(format!("tl-rules-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let c = thunderlook_store::open(&dir).unwrap();
    c.execute(
        "INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (1, 'pst', 'x', 'x', 'Courrier', 0, 0)",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position) VALUES (1, NULL, 0, 'Boîte de réception', 'inbox', NULL, 0, 0)",
        [],
    )
    .unwrap();
    // Sac MAPI : une propriété avant le flux, pour vérifier qu'elle est bien sautée.
    let rules = stream(&["Factures", "Lettres"]);
    let mut props = Vec::new();
    props.extend(0x001Au16.to_le_bytes());
    props.extend(0x001Fu16.to_le_bytes());
    props.extend(4u32.to_le_bytes());
    props.extend([b'I', 0, b'P', 0]);
    props.extend(0x6802u16.to_le_bytes());
    props.extend(0x0102u16.to_le_bytes());
    props.extend((rules.len() as u32).to_le_bytes());
    props.extend(&rules);
    c.execute(
        "INSERT INTO fai (folder_id, nid, message_class, subject, props) VALUES (1, 0, 'IPM.RuleOrganizer', '', ?1)",
        params![props],
    )
    .unwrap();
    (Fixture { dir: dir.clone() }, dir)
}

#[test]
fn pst_stream_imported_once_and_rwz_replaced() {
    let (_f, dir) = setup("import");
    let mut mb = Mailbox::open(&dir).unwrap();
    let rules = mb.rules().unwrap();
    assert_eq!(rules.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["Factures", "Lettres"]);
    assert_eq!((rules[0].origin.as_str(), rules[0].origin_label.as_str(), rules[0].error.as_deref()), ("pst:1", "Courrier", None));

    // La case « active » survit à une réouverture : le flux n'est pas relu.
    mb.set_rule_active(&rules[1].id, false).unwrap();
    drop(mb);
    let mut mb = Mailbox::open(&dir).unwrap();
    let rules = mb.rules().unwrap();
    assert_eq!(rules.iter().map(|r| r.active).collect::<Vec<_>>(), [true, false]);

    // Un .rwz forme son propre jeu ; le réimporter remplace ses règles.
    let rwz = dir.join("regles.rwz");
    fs::write(&rwz, stream(&["Ancienne"])).unwrap();
    assert_eq!(mb.import_rwz(&rwz).unwrap(), 1);
    fs::write(&rwz, stream(&["Nouvelle", "Autre"])).unwrap();
    assert_eq!(mb.import_rwz(&rwz).unwrap(), 2);
    let rules = mb.rules().unwrap();
    assert_eq!(rules.len(), 4);
    assert_eq!(rules.iter().filter(|r| r.origin_label == "regles.rwz").map(|r| r.name.as_str()).collect::<Vec<_>>(), ["Nouvelle", "Autre"]);

    assert!(mb.import_rwz(&dir.join("absent.rwz")).is_err());
    assert!(mb.set_rule_active("r999", true).is_err());
}

#[test]
fn folder_targets_by_entry_id_then_name() {
    use thunderlook_mailbox::{FolderMatch, FolderResolver};
    let (_f, dir) = setup("folders");
    let c = thunderlook_store::open(&dir).unwrap();
    let entry = |nid: u32| -> Vec<u8> { [0, 0, 0, 0].into_iter().chain([7u8; 16]).chain(nid.to_le_bytes()).collect() };
    let mut add = |name: &str, props: Vec<u8>, messages: usize| -> i64 {
        c.execute(
            "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position, props) VALUES (1, 1, 0, ?1, 'folder', NULL, 0, 0, ?2)",
            params![name, props],
        )
        .unwrap();
        let id = c.last_insert_rowid();
        for _ in 0..messages {
            c.execute(
                "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, message_flags, is_read,
                    flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
                 VALUES (?1, 0, 'IPM.Note', '', '', '', '', '', 0, 1, 0, 1, 0, 0, 0, 'text', '', '', x'')",
                params![id],
            )
            .unwrap();
        }
        id
    };
    // Sac MAPI avec PidTagEntryId (binaire de 24 octets).
    let bag = |e: &[u8]| -> Vec<u8> { [0xFFu8, 0x0F, 0x02, 0x01].into_iter().chain((e.len() as u32).to_le_bytes()).chain(e.iter().copied()).collect() };
    let factures = add("Factures", bag(&entry(0x8022)), 0);
    let petit = add("Listes", vec![], 1);
    let gros = add("listes", vec![], 3);
    let r = FolderResolver::load(&c).unwrap();
    assert_eq!(r.resolve(&entry(0x8022), "autre nom"), FolderMatch::EntryId(factures));
    assert_eq!(r.resolve(&entry(0x9999), "FACTURES"), FolderMatch::Name { folder: factures, homonyms: 1 });
    assert_eq!(r.resolve(&[], "Listes"), FolderMatch::Name { folder: gros, homonyms: 2 });
    assert_ne!(gros, petit);
    assert_eq!(r.resolve(&[], "Absent"), FolderMatch::Missing);
    assert_eq!(r.path(factures), "Courrier › Boîte de réception › Factures");
}
