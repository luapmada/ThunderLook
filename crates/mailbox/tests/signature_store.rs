//! Signatures propres à ThunderLook (lot BE) sur un dossier de données synthétique : stockage, validation, choix par compte
//! et par type de message, repli sur le dossier d'Outlook, insertion dans `compose`. Aucune donnée réelle.

use std::{fs, path::PathBuf};

use thunderlook_mailbox::{Mailbox, StoredSignature, signature_for_compose, signature_pick, signatures_load, signatures_menu, signatures_save};
use thunderlook_model::ComposeKind;
use thunderlook_store::rusqlite::{Connection, params};

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> Dir {
    let dir = std::env::temp_dir().join(format!("tl-sig-{tag}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    Dir(dir)
}

fn sig(name: &str, html: &str, new: &[&str], reply: &[&str]) -> StoredSignature {
    StoredSignature {
        name: name.into(),
        html: html.into(),
        for_new: new.iter().map(|s| s.to_string()).collect(),
        for_reply: reply.iter().map(|s| s.to_string()).collect(),
    }
}

fn folder(c: &Connection, name: &str, role: &str) -> i64 {
    c.execute(
        "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position) VALUES (1, NULL, 0, ?1, ?2, NULL, 0, 0)",
        params![name, role],
    )
    .unwrap();
    c.last_insert_rowid()
}

/// Dossier de données avec une Boîte de réception et un message envoyé par `paul@example.fr` (c'est ce qui donne « mes adresses »).
fn mailbox(dir: &PathBuf) -> Mailbox {
    let c = thunderlook_store::open(dir).unwrap();
    c.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (1, 'pst', 'x', 'x', 'Test', 0, 0)", []).unwrap();
    folder(&c, "Boîte de réception", "inbox");
    let sent = folder(&c, "Éléments envoyés", "sent");
    c.execute(
        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, received_at, sent_at,
            message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props,
            internet_message_id, conversation_topic, conversation_index, conversation_id)
         VALUES (?1, 0, 'IPM.Note', 's', 'Paul', 'paul@example.fr', '', '', 1791490440000, 1791490440000, 1, 1, 0, 1, 0, 100, 0, 'text', '', '', x'',
            'sent@example.org', 's', x'', x'')",
        params![sent],
    )
    .unwrap();
    let id = c.last_insert_rowid();
    let rel = thunderlook_store::eml_relative_path(1, id);
    let path = thunderlook_store::resolve(dir, &rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "From: Paul <paul@example.fr>\r\nTo: x@example.org\r\nSubject: s\r\n\r\ncorps\r\n").unwrap();
    c.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, rel]).unwrap();
    drop(c);
    Mailbox::open(dir).unwrap()
}

#[test]
fn store_round_trip_validation_and_choice() {
    let data = temp("data");
    // Dossier d'« Outlook » synthétique : une signature de repli pour paul@example.fr, une autre sans lien avec un compte.
    let outlook = temp("outlook");
    fs::write(outlook.0.join("Paul (paul@example.fr).htm"), "<html><body><p>OUTLOOK-PAUL</p></body></html>").unwrap();
    fs::write(outlook.0.join("Autre.htm"), "<html><body><p>OUTLOOK-AUTRE</p></body></html>").unwrap();
    // SAFETY : un seul test dans ce fichier, la variable est posée avant toute lecture.
    unsafe { std::env::set_var("THUNDERLOOK_SIGNATURES", &outlook.0) };

    // Fichier absent : liste vide, pas d'erreur.
    assert!(signatures_load(&data.0).unwrap().is_empty());

    // Aller-retour ; adresses nettoyées (minuscules, sans doublon ni blanc) ; aucun fichier temporaire ne reste.
    let list = vec![sig("Pro", "<p>PRO</p>", &["Paul@Example.fr", " paul@example.fr "], &[]), sig("Court", "<p>COURT</p>", &[], &["paul@example.fr"])];
    signatures_save(&data.0, &list).unwrap();
    let back = signatures_load(&data.0).unwrap();
    assert_eq!(back[0].for_new, vec!["paul@example.fr"]);
    assert_eq!(back[1].for_reply, vec!["paul@example.fr"]);
    assert!(!data.0.join("signatures.json.tmp").exists());

    // Refus : nom vide, nom en double, deux signatures pour un même compte et un même type. Le fichier reste intact.
    assert!(signatures_save(&data.0, &[sig(" ", "x", &[], &[])]).is_err());
    assert!(signatures_save(&data.0, &[sig("A", "x", &[], &[]), sig("a", "y", &[], &[])]).is_err());
    let clash = signatures_save(&data.0, &[sig("A", "x", &["paul@example.fr"], &[]), sig("B", "y", &["PAUL@example.fr"], &[])]);
    assert!(clash.unwrap_err().contains("paul@example.fr"));
    assert_eq!(signatures_load(&data.0).unwrap(), back);

    // Choix : nouveau → « Pro », réponse et transfert → « Court », autre compte → rien.
    let mine = vec!["paul@example.fr".to_string()];
    assert_eq!(signature_pick(&back, ComposeKind::New, &mine).unwrap().name, "Pro");
    assert_eq!(signature_pick(&back, ComposeKind::Reply, &mine).unwrap().name, "Court");
    assert_eq!(signature_pick(&back, ComposeKind::Forward, &mine).unwrap().name, "Court");
    assert!(signature_pick(&back, ComposeKind::New, &["lou@example.org".to_string()]).is_none());

    // Valeur insérée : l'enregistrée prime ; sans attribution, repli sur Outlook ; sans rien, vide.
    assert_eq!(signature_for_compose(&data.0, ComposeKind::New, &mine), "<p>PRO</p>");
    assert!(signature_for_compose(&data.0, ComposeKind::New, &mine).contains("PRO"));
    let none = temp("none");
    assert!(signature_for_compose(&none.0, ComposeKind::New, &mine).contains("OUTLOOK-PAUL"));
    assert_eq!(signature_for_compose(&none.0, ComposeKind::New, &["lou@example.org".to_string()]), "");

    // Un fichier illisible n'est pas écrasé par le chargement et laisse le repli Outlook jouer.
    fs::write(none.0.join("signatures.json"), "{pas du json").unwrap();
    assert!(signatures_load(&none.0).is_err());
    assert!(signature_for_compose(&none.0, ComposeKind::New, &mine).contains("OUTLOOK-PAUL"));
    assert_eq!(fs::read_to_string(none.0.join("signatures.json")).unwrap(), "{pas du json");

    // Menu : les enregistrées d'abord, puis celles d'Outlook de nom différent.
    let names: Vec<String> = signatures_menu(&data.0).into_iter().map(|s| s.name).collect();
    assert_eq!(names, ["Pro", "Court", "Autre", "Paul (paul@example.fr)"]);

    // Dans `compose` : nouveau message et réponse reçoivent chacun leur signature.
    let mb = mailbox(&data.0);
    let new = mb.compose(ComposeKind::New, None).unwrap();
    assert!(new.html.contains("<p>PRO</p>") && !new.html.contains("COURT"), "{}", new.html);
    // Sans enregistrée pour ce compte, c'est celle d'Outlook qui sert, comme avant le lot.
    signatures_save(&data.0, &[]).unwrap();
    let fallback = mb.compose(ComposeKind::New, None).unwrap();
    assert!(fallback.html.contains("OUTLOOK-PAUL"), "{}", fallback.html);
}
