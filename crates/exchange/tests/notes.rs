//! Notes en mode MPE « forcé téléphone → PC » (`NoteSyncMode=2`) : le téléphone est la source, ThunderLook s'aligne.

use thunderlook_exchange::{ChangeKind, ImportOptions, ItemKind, Payload, apply, export_all, import_changes, import_changes_with, last_undo, undo};
use thunderlook_items::vnote::{self, Meta};
use thunderlook_model::items::NoteForm;

mod common;
use common::*;

fn bodies(w: &World) -> Vec<(String, String, i32)> {
    let mut v: Vec<_> = thunderlook_items::notes(&w.conn, &w.dir)
        .unwrap()
        .rows
        .iter()
        .map(|r| thunderlook_items::note_form(&w.conn, &w.dir, r.id[1..].parse().unwrap()).unwrap())
        .map(|f| (f.id.unwrap(), f.body, f.color))
        .collect();
    v.sort();
    v
}

/// Identifiants des deux notes de départ, dans l'ordre de création.
fn ids(w: &World) -> (String, String) {
    let mut v: Vec<i64> = thunderlook_items::notes(&w.conn, &w.dir).unwrap().rows.iter().map(|r| r.id[1..].parse().unwrap()).collect();
    v.sort();
    (format!("n{}", v[0]), format!("n{}", v[1]))
}

fn set_note(w: &mut World, id: &str, body: &str, color: i32) {
    let dir = w.dir.clone();
    thunderlook_items::note_save(&mut w.conn, &dir, &NoteForm { id: Some(id.into()), folder_id: "f3".into(), body: body.into(), color }).unwrap();
}

/// Réécrit `notes.vnt` comme le ferait le téléphone : (identifiant du fichier, texte).
fn phone_file(w: &World, notes: &[(&str, &str)]) {
    let items: Vec<(NoteForm, Meta)> = notes
        .iter()
        .map(|(uid, body)| (NoteForm { body: (*body).into(), ..Default::default() }, Meta { uid: Some((*uid).into()), ..Default::default() }))
        .collect();
    write(w, "notes.vnt", &vnote::export_with_meta(&items));
}

fn forced_world(name: &str) -> World {
    let mut w = world(name);
    fill(&mut w);
    let (a, _) = ids(&w);
    set_note(&mut w, &a, "Courses\npain, lait", 2);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    w
}

fn notes_of(rep: &thunderlook_exchange::ImportReport) -> Vec<(ChangeKind, String)> {
    rep.changes.iter().filter(|c| c.item == ItemKind::Note).map(|c| (c.kind, c.id.clone())).collect()
}

#[test]
fn creation_sur_le_telephone() {
    let mut w = forced_world("n-creation");
    let mut text = read(&w, "notes.vnt");
    text.push_str("BEGIN:VNOTE\r\nVERSION:1.1\r\nBODY;CHARSET=UTF-8:Idée du soir\r\nX-OUTLOOK-UID:mpe-note-1\r\nEND:VNOTE\r\n");
    write(&w, "notes.vnt", &text);
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert_eq!(notes_of(&rep), [(ChangeKind::New, "#2".into())]);
    assert!(rep.conflicts.is_empty() && rep.changes[0].default_selected());
    let done = apply(&mut w.conn, &w.dir, &rep.changes).unwrap();
    assert_eq!((done.applied, done.errors.len()), (1, 0));
    assert_eq!(bodies(&w).len(), 3);
    assert!(bodies(&w).iter().any(|(_, b, _)| b == "Idée du soir"));
    // Relu : plus rien à faire.
    assert!(import_changes(&w.conn, &w.dir, &w.out).unwrap().changes.is_empty());
}

#[test]
fn modification_du_telephone_remplace_sans_conflit_et_garde_la_couleur() {
    let mut w = forced_world("n-modif");
    let (a, b) = ids(&w);
    // Modifiée des deux côtés : en mode bidirectionnel ce serait un conflit.
    set_note(&mut w, &a, "Courses\npain, lait, beurre (PC)", 2);
    phone_file(&w, &[(&a, "Courses\npain, lait, œufs"), (&b, "Code du portail\n1234")]);
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.conflicts.is_empty());
    assert_eq!(notes_of(&rep), [(ChangeKind::Modified, a.clone())]);
    assert!(rep.changes[0].default_selected());
    apply(&mut w.conn, &w.dir, &rep.changes).unwrap();
    let n1 = bodies(&w).into_iter().find(|(id, ..)| *id == a).unwrap();
    assert_eq!((n1.1.as_str(), n1.2), ("Courses\npain, lait, œufs", 2), "texte du téléphone, couleur de ThunderLook");
    assert!(import_changes(&w.conn, &w.dir, &w.out).unwrap().changes.is_empty());
}

#[test]
fn modification_cote_thunderlook_ecrasee_et_retrouvable_par_l_annulation() {
    let mut w = forced_world("n-ecrase");
    let (a, _) = ids(&w);
    let before = bodies(&w);
    // Le téléphone n'a rien changé (fichier intact) ; ThunderLook a modifié la note.
    set_note(&mut w, &a, "Courses\nversion PC", 2);
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert_eq!(notes_of(&rep), [(ChangeKind::Modified, a.clone())]);
    apply(&mut w.conn, &w.dir, &rep.changes).unwrap();
    assert_eq!(bodies(&w), before, "la valeur du téléphone est revenue");
    // La valeur écrasée est gardée dans exchange-undo.
    let stamp = last_undo(&w.dir).unwrap();
    let rec = std::fs::read_to_string(w.dir.join("exchange-undo").join(format!("{stamp}.json"))).unwrap();
    assert!(rec.contains("version PC"));
    undo(&mut w.conn, &w.dir, &stamp).unwrap();
    let n1 = bodies(&w).into_iter().find(|(id, ..)| *id == a).unwrap();
    assert_eq!(n1.1, "Courses\nversion PC");
}

#[test]
fn suppression_sur_le_telephone_proposee_decochee_puis_annulable() {
    let mut w = forced_world("n-suppr");
    let (a, b) = ids(&w);
    phone_file(&w, &[(&a, "Courses\npain, lait")]);
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert_eq!(notes_of(&rep), [(ChangeKind::Deleted, b.clone())]);
    assert!(!rep.changes[0].default_selected());
    assert_eq!(bodies(&w).len(), 2, "rien n'est supprimé sans accord");
    apply(&mut w.conn, &w.dir, &rep.changes).unwrap();
    assert_eq!(bodies(&w).len(), 1);
    undo(&mut w.conn, &w.dir, &last_undo(&w.dir).unwrap()).unwrap();
    assert!(bodies(&w).iter().any(|(_, b, _)| b == "Code du portail\n1234"));
}

#[test]
fn reglage_desactive_comportement_bidirectionnel() {
    let mut w = forced_world("n-bidi");
    let (a, b) = ids(&w);
    let off = ImportOptions { notes_from_phone: false };
    // ThunderLook seul a modifié : le fichier intact ne propose rien.
    set_note(&mut w, &a, "Courses\nversion PC", 2);
    assert!(import_changes_with(&w.conn, &w.dir, &w.out, &off).unwrap().changes.is_empty());
    // Modifiée des deux côtés : conflit à part.
    phone_file(&w, &[(&a, "Courses\nversion téléphone"), (&b, "Code du portail\n1234")]);
    let rep = import_changes_with(&w.conn, &w.dir, &w.out, &off).unwrap();
    assert!(notes_of(&rep).is_empty());
    assert_eq!(rep.conflicts.len(), 1);
    // Les autres modules restent bidirectionnels même réglage par défaut (notes forcées).
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.conflicts.is_empty());
}

#[test]
fn corps_long_accents_lignes_vides_et_retours_chariot() {
    let mut w = forced_world("n-corps");
    let (a, b) = ids(&w);
    let long = "Ligne très longue avec accents é è à ù ç œ € ".repeat(40);
    let body = format!("Titre « été »\r\n\r\n\r\n{long}\r\n\r\nFin : 100 % = ok ; a=b");
    phone_file(&w, &[(&a, &body), (&b, "Code du portail\n1234")]);
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    let Payload::Note(f) = &rep.changes[0].payload else { panic!("{:?}", rep.changes) };
    assert_eq!(f.body, body.replace("\r\n", "\n"));
    assert_eq!(f.color, 2);
    apply(&mut w.conn, &w.dir, &rep.changes).unwrap();
    assert!(import_changes(&w.conn, &w.dir, &w.out).unwrap().changes.is_empty());
}

#[test]
fn texte_windows_1252_en_quoted_printable_et_dates() {
    let mut w = forced_world("n-qp");
    let (a, _) = ids(&w);
    let file = &format!("BEGIN:VNOTE\r\nVERSION:1.1\r\nBODY;ENCODING=QUOTED-PRINTABLE;CHARSET=WINDOWS-1252:Rendez-vous =E0 No=EBl=0D=0A=0D=0Aprix 5=80 =\r\nsuite\r\nDCREATED:20261001T080000\r\nLAST-MODIFIED:20261022T101500Z\r\nX-OUTLOOK-UID:{a}\r\nEND:VNOTE\r\n");
    write(&w, "notes.vnt", file);
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    let Payload::Note(f) = &rep.changes[0].payload else { panic!() };
    assert_eq!(f.body, "Rendez-vous à Noël\n\nprix 5€ suite");
    let p = vnote::parse(&file).unwrap();
    assert_eq!((p[0].meta.created.as_deref(), p[0].meta.modified.as_deref()), (Some("20261001T080000"), Some("20261022T101500Z")));
    apply(&mut w.conn, &w.dir, &rep.changes).unwrap();
}
