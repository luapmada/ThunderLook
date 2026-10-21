//! « Exécuter maintenant » (J6) sur un dossier de données synthétique : ordre des règles, « arrêter », exception,
//! catégories, déplacement par EntryID, copie par nom, suppression, envoi non exécuté, simulation sans effet.

use std::{fs, path::PathBuf};

use thunderlook_mailbox::Mailbox;
use thunderlook_model::{RuleRunRequest, RunFilter};
use thunderlook_store::rusqlite::{Connection, params};

struct Fixture {
    dir: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn u32s(out: &mut Vec<u8>, v: &[u32]) {
    v.iter().for_each(|x| out.extend(x.to_le_bytes()));
}

fn tstr(out: &mut Vec<u8>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    out.push(units.len() as u8);
    units.iter().for_each(|u| out.extend(u.to_le_bytes()));
}

fn words(w: &str) -> Vec<u8> {
    let mut v = 0u32.to_le_bytes().to_vec();
    tstr(&mut v, w);
    v
}

fn folder_value(entry_id: &[u8], name: &str) -> Vec<u8> {
    let mut v = Vec::new();
    u32s(&mut v, &[0, entry_id.len() as u32]);
    v.extend(entry_id);
    u32s(&mut v, &[2]);
    v.extend([9, 9]);
    tstr(&mut v, name);
    u32s(&mut v, &[0]);
    v
}

fn recipient(addr: &str) -> Vec<u8> {
    let mut block = Vec::new();
    let a: Vec<u8> = addr.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
    u32s(&mut block, &[0x39FE_001F, 0, 16, 0]);
    block.extend(&a);
    let mut v = Vec::new();
    u32s(&mut v, &[0, 1, 0xC0, 1, block.len() as u32]);
    v.extend(&block);
    u32s(&mut v, &[1, 0]);
    v
}

/// Règle : en-tête « à l'arrivée », puis les éléments (identifiant, valeurs déjà codées).
fn rule(name: &str, elements: &[(u32, Vec<Vec<u8>>)]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend((elements.len() as u16 + 1).to_le_bytes());
    body.extend(0xFFFFu16.to_le_bytes());
    body.extend(0u16.to_le_bytes());
    body.extend(12u16.to_le_bytes());
    body.extend(b"CRuleElement");
    u32s(&mut body, &[0x190, 1, 0, 1]);
    for (id, values) in elements {
        body.extend(0x8001u16.to_le_bytes());
        u32s(&mut body, &[*id, values.len() as u32]);
        values.iter().for_each(|v| body.extend(v));
    }
    let mut out = Vec::new();
    u32s(&mut out, &[0x140000]);
    tstr(&mut out, name);
    u32s(&mut out, &[1, 0, 0, 0, 0, body.len() as u32]);
    out.extend(body);
    out
}

fn stream(rules: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    u32s(&mut out, &[0x140000, 0x0614_0000, 0, 0, 0, 1, 0, 0, 1, 1, 0]);
    out.extend((rules.len() as u16).to_le_bytes());
    rules.iter().for_each(|r| out.extend(r));
    u32s(&mut out, &[0]);
    out
}

fn entry_id(nid: u32) -> Vec<u8> {
    [0u8; 4].into_iter().chain([7u8; 16]).chain(nid.to_le_bytes()).collect()
}

fn folder(c: &Connection, parent: Option<i64>, name: &str, role: &str, entry: Option<&[u8]>) -> i64 {
    let props: Vec<u8> = entry.map_or_else(Vec::new, |e| {
        [0xFFu8, 0x0F, 0x02, 0x01].into_iter().chain((e.len() as u32).to_le_bytes()).chain(e.iter().copied()).collect()
    });
    c.execute(
        "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position, props) VALUES (1, ?1, 0, ?2, ?3, NULL, 0, 0, ?4)",
        params![parent, name, role, props],
    )
    .unwrap();
    c.last_insert_rowid()
}

fn message(c: &Connection, dir: &PathBuf, folder: i64, from: &str, subject: &str, body: &str, t: i64) -> i64 {
    c.execute(
        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, received_at,
            message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
         VALUES (?1, 0, 'IPM.Note', ?2, ?3, ?3, 'Paul', '', 1760000000000 + ?4, 0, 0, 0, 1, 0, 100, 0, 'text', '', '', x'')",
        params![folder, subject, from, t],
    )
    .unwrap();
    let id = c.last_insert_rowid();
    let rel = thunderlook_store::eml_relative_path(1, id);
    let path = thunderlook_store::resolve(dir, &rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, format!("Subject: {subject}\r\nFrom: {from}\r\nTo: paul@example.org\r\n\r\n{body}\r\n")).unwrap();
    c.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, rel]).unwrap();
    id
}

fn where_is(c: &Connection, id: i64) -> (i64, bool, Vec<String>) {
    let (f, read) = c.query_row("SELECT folder_id, is_read FROM message WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    let cats = c
        .prepare("SELECT name FROM message_category WHERE message_id = ?1")
        .unwrap()
        .query_map([id], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    (f, read, cats)
}

#[test]
fn run_now_simulates_then_applies() {
    let dir = std::env::temp_dir().join(format!("tl-rule-run-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let _f = Fixture { dir: dir.clone() };
    let c = thunderlook_store::open(&dir).unwrap();
    c.execute(
        "INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (1, 'pst', 'x', 'x', 'Courrier', 0, 0)",
        [],
    )
    .unwrap();
    let root = folder(&c, None, "", "root", None);
    let ipm = folder(&c, Some(root), "IPM", "ipm", None);
    let inbox = folder(&c, Some(ipm), "Boîte de réception", "inbox", None);
    let deleted = folder(&c, Some(ipm), "Éléments supprimés", "deleted", None);
    let factures = folder(&c, Some(inbox), "Factures", "folder", Some(&entry_id(0x8022)));
    let archives = folder(&c, Some(ipm), "Archives 2025", "folder", None);

    let m1 = message(&c, &dir, inbox, "compta@a.fr", "Facture 12", "petite relance", 5);
    let m2 = message(&c, &dir, inbox, "compta@a.fr", "Facture brouillon", "rien", 4);
    let m3 = message(&c, &dir, inbox, "camille@b.fr", "Point", "Petite RELANCE amicale", 3);
    let m4 = message(&c, &dir, inbox, "promo@shop.fr", "Soldes", "", 2);
    let m5 = message(&c, &dir, inbox, "camille@b.fr", "Bonjour", "", 1);

    let rules = stream(&[
        // Objet « facture », sauf « brouillon » : catégorie, déplacer (EntryID), arrêter.
        rule(
            "Factures",
            &[
                (0xCD, vec![words("facture")]),
                (0x1F9, vec![words("brouillon")]),
                (0x133, vec![{
                    let mut v = 0u32.to_le_bytes().to_vec();
                    tstr(&mut v, "Compta");
                    v
                }]),
                (0x12C, vec![folder_value(&entry_id(0x8022), "Factures")]),
                (0x142, vec![]),
            ],
        ),
        // « relance » dans l'objet ou le corps : lu, copie dans un dossier retrouvé par le nom.
        rule("Relances", &[(0xCF, vec![words("relance")]), (0x14C, vec![]), (0x139, vec![folder_value(&entry_id(0x9999), "archives 2025")])]),
        // Adresse de l'expéditeur : supprimer.
        rule("Pub", &[(0xE6, vec![words("promo")]), (0x12D, vec![])]),
        // Transfert : signalé, pas exécuté.
        rule("Transfert", &[(0xCB, vec![recipient("camille@b.fr")]), (0x12E, vec![recipient("x@c.fr")])]),
    ]);
    let mut props = Vec::new();
    props.extend(0x6802u16.to_le_bytes());
    props.extend(0x0102u16.to_le_bytes());
    props.extend((rules.len() as u32).to_le_bytes());
    props.extend(&rules);
    c.execute("INSERT INTO fai (folder_id, nid, message_class, subject, props) VALUES (?1, 0, 'IPM.RuleOrganizer', '', ?2)", params![inbox, props])
        .unwrap();
    drop(c);

    let mut mb = Mailbox::open(&dir).unwrap();
    let listed = mb.rules().unwrap();
    assert!(listed.iter().all(|r| r.error.is_none()), "{listed:?}");
    let mut req = RuleRunRequest {
        rule_ids: listed.iter().map(|r| r.id.clone()).collect(),
        folder_id: format!("f{inbox}"),
        subfolders: false,
        filter: RunFilter::All,
        apply: false,
        only: Vec::new(),
        confirm_moves: None,
    };

    // Simulation : m1 s'arrête à « Factures » (pas compté dans « Relances ») ; m2 est exclu par l'exception.
    let sim = mb.run_rules(&req).unwrap();
    let counts: Vec<u32> = sim.rules.iter().map(|r| r.matched).collect();
    assert_eq!((sim.scanned, sim.changed, counts), (5, 3, vec![1, 1, 1, 2]));
    assert_eq!(sim.rules[0].samples, vec!["09/10/2025 · compta@a.fr · Facture 12".to_string()]);
    assert!(sim.rules[3].not_run[0].contains("pas avant le J7"), "{:?}", sim.rules[3]);
    let c = Connection::open(dir.join(thunderlook_store::DB_FILE)).unwrap();
    assert_eq!(where_is(&c, m1), (inbox, false, vec![]));

    // Restriction à un message (règles à l'arrivée) : seul m5 est parcouru, aucune règle ne le concerne.
    req.only = vec![format!("m{m5}")];
    let one = mb.run_rules(&req).unwrap();
    assert_eq!((one.scanned, one.changed), (1, 0));
    req.only.clear();

    // Application.
    req.apply = true;
    let done = mb.run_rules(&req).unwrap();
    assert!(done.errors.is_empty(), "{:?}", done.errors);
    assert_eq!(where_is(&c, m1), (factures, false, vec!["Compta".to_string()]));
    assert_eq!(where_is(&c, m2), (inbox, false, vec![]));
    assert_eq!(where_is(&c, m3), (inbox, true, vec![]));
    assert_eq!(where_is(&c, m4).0, deleted);
    assert_eq!(where_is(&c, m5), (inbox, false, vec![]));
    let copies: i64 = c.query_row("SELECT COUNT(*) FROM message WHERE folder_id = ?1 AND subject = 'Point'", [archives], |r| r.get(0)).unwrap();
    assert_eq!(copies, 1);

    // Rejouer : seule la copie se refait (comme dans Outlook).
    req.apply = false;
    assert_eq!(mb.run_rules(&req).unwrap().changed, 1);
}

/// Sécurité : au-delà de 20 déplacements, rien n'est appliqué sans la confirmation du nombre exact.
#[test]
fn many_moves_need_confirmation() {
    let dir = std::env::temp_dir().join(format!("tl-rule-confirm-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let _f = Fixture { dir: dir.clone() };
    let c = thunderlook_store::open(&dir).unwrap();
    c.execute(
        "INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (1, 'pst', 'x', 'x', 'Courrier', 0, 0)",
        [],
    )
    .unwrap();
    let root = folder(&c, None, "", "root", None);
    let ipm = folder(&c, Some(root), "IPM", "ipm", None);
    let inbox = folder(&c, Some(ipm), "Boîte de réception", "inbox", None);
    let target = folder(&c, Some(inbox), "Factures", "folder", Some(&entry_id(0x8022)));
    for k in 0..21 {
        message(&c, &dir, inbox, "compta@a.fr", &format!("Facture {k}"), "", k);
    }
    let rules = stream(&[rule("Factures", &[(0xCD, vec![words("facture")]), (0x12C, vec![folder_value(&entry_id(0x8022), "Factures")])])]);
    let mut props = Vec::new();
    props.extend(0x6802u16.to_le_bytes());
    props.extend(0x0102u16.to_le_bytes());
    props.extend((rules.len() as u32).to_le_bytes());
    props.extend(&rules);
    c.execute("INSERT INTO fai (folder_id, nid, message_class, subject, props) VALUES (?1, 0, 'IPM.RuleOrganizer', '', ?2)", params![inbox, props])
        .unwrap();
    let count = |f: i64| -> i64 { c.query_row("SELECT COUNT(*) FROM message WHERE folder_id = ?1", [f], |r| r.get(0)).unwrap() };
    let mut mb = Mailbox::open(&dir).unwrap();
    let mut req = RuleRunRequest {
        rule_ids: mb.rules().unwrap().iter().map(|r| r.id.clone()).collect(),
        folder_id: format!("f{inbox}"),
        subfolders: false,
        filter: RunFilter::All,
        apply: true,
        only: Vec::new(),
        confirm_moves: None,
    };
    let r = mb.run_rules(&req).unwrap();
    assert!(r.needs_confirmation && !r.applied && r.moves == 21);
    assert_eq!((count(inbox), count(target)), (21, 0), "rien ne bouge sans confirmation");
    req.confirm_moves = Some(20);
    assert!(mb.run_rules(&req).unwrap().needs_confirmation, "un autre nombre ne vaut pas confirmation");
    req.confirm_moves = Some(21);
    let r = mb.run_rules(&req).unwrap();
    assert!(r.applied && !r.needs_confirmation);
    assert_eq!((count(inbox), count(target)), (0, 21));
}
