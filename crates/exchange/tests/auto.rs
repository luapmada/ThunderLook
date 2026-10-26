//! Synchronisation automatique (lot AM) : fichiers instables, suppression, conflit, seuil, deux passages identiques,
//! annulation, export conditionnel, journal. Le « téléphone » est simulé en réécrivant les fichiers du dossier d'échange.

use std::time::Duration;

use thunderlook_exchange::{
    export_all, import_changes, undo,
    auto::{self, Outcome, Params, PassReport, Settings, Stability},
};

mod common;
use common::*;

/// Fichiers « posés depuis longtemps » : pas d'attente, pas d'âge minimal.
fn quick() -> Params {
    Params { stable_gap: Duration::from_millis(20), min_age: Duration::ZERO, ..Params::default() }
}

fn pass(w: &mut World, p: &Params) -> PassReport {
    let (dir, out) = (w.dir.clone(), w.out.clone());
    auto::run_pass(&mut w.conn, &dir, &out, p)
}

fn ready(name: &str) -> World {
    let mut w = world(name);
    fill(&mut w);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    w
}

fn counts(w: &World) -> [usize; 4] {
    [
        contacts_of(w).len(),
        thunderlook_items::tasks(&w.conn).unwrap().rows.len(),
        thunderlook_items::notes(&w.conn, &w.dir).unwrap().rows.len(),
        thunderlook_calendar::load(&w.conn).unwrap().appointments.len(),
    ]
}

fn card(given: &str, uid: &str) -> String {
    format!("BEGIN:VCARD\r\nVERSION:2.1\r\nN:Test{uid};{given};;;\r\nTEL;CELL:06000000{uid}\r\nX-OUTLOOK-UID:mpe-{uid}\r\nEND:VCARD\r\n")
}

fn phone_adds_contacts(w: &World, n: usize) {
    let extra: String = (0..n).map(|i| card("Nouveau", &format!("{i:02}"))).collect();
    write(w, "contacts.vcf", &format!("{}{extra}", read(w, "contacts.vcf")));
}

fn phone_edits_mobile(w: &World, from: &str, to: &str) {
    let text = read(w, "contacts.vcf");
    assert!(text.contains(from), "{text}");
    write(w, "contacts.vcf", &text.replace(from, to));
}

fn drop_contact(w: &World, needle: &str) {
    let mut out = String::new();
    let mut block = String::new();
    let mut inside = false;
    for line in read(w, "contacts.vcf").split_inclusive("\r\n") {
        if line.starts_with("BEGIN:VCARD") { inside = true }
        if inside { block.push_str(line) } else { out.push_str(line) }
        if line.starts_with("END:VCARD") {
            inside = false;
            if !block.contains(needle) { out.push_str(&block) }
            block.clear();
        }
    }
    write(w, "contacts.vcf", &out);
}

// --- Fichiers instables ----------------------------------------------------------------------------------------------

#[test]
fn fichier_en_cours_d_ecriture_n_est_pas_lu() {
    let mut w = ready("auto-instable");
    phone_adds_contacts(&w, 1);
    let before = counts(&w);
    // MPE écrit PENDANT l'intervalle entre les deux lectures.
    let path = w.out.join("contacts.vcf");
    let p = Params { stable_gap: Duration::from_millis(300), min_age: Duration::ZERO, ..Params::default() };
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("\r\n");
        std::fs::write(&path, text).unwrap();
    });
    let rep = pass(&mut w, &p);
    writer.join().unwrap();
    assert!(matches!(rep.outcome, Outcome::Skipped(ref s) if s.contains("contacts.vcf")), "{rep:?}");
    assert_eq!((rep.created, rep.modified, rep.undo.clone()), (0, 0, None));
    assert_eq!(counts(&w), before);
    assert!(auto::read_journal(&w.dir).is_empty(), "run_pass seul ne journalise pas");
}

#[test]
fn fichier_ecrit_il_y_a_un_instant_est_attendu() {
    let mut w = ready("auto-recent");
    phone_adds_contacts(&w, 1);
    let p = Params { stable_gap: Duration::from_millis(10), min_age: Duration::from_secs(3600), ..Params::default() };
    assert!(matches!(auto::check_stable(&w.out, &p), Stability::Unstable(_)));
    let rep = pass(&mut w, &p);
    assert!(matches!(rep.outcome, Outcome::Skipped(_)), "{rep:?}");
    assert_eq!(counts(&w)[0], 2);
    // Une fois posé, il est lu.
    assert_eq!(auto::check_stable(&w.out, &quick()), Stability::Stable);
    assert_eq!(pass(&mut w, &quick()).created, 1);
}

#[test]
fn dossier_sans_aucun_fichier_rien_n_est_lu() {
    let mut w = world("auto-dossier-vide");
    fill(&mut w);
    std::fs::create_dir_all(&w.out).unwrap();
    write(&w, "notes-perso.txt", "pas un fichier du téléphone");
    let rep = pass(&mut w, &quick());
    assert!(matches!(rep.outcome, Outcome::Skipped(ref s) if s.contains("aucun fichier")), "{rep:?}");
    assert_eq!(counts(&w)[0], 2);
}

// --- Appliqué automatiquement : un seul côté -------------------------------------------------------------------------

#[test]
fn creation_et_modification_du_telephone_sont_appliquees_deux_passages_sans_doublon() {
    let mut w = ready("auto-deux-passages");
    let before = counts(&w);
    phone_adds_contacts(&w, 1);
    phone_edits_mobile(&w, "0612345678", "0688888888");

    let first = pass(&mut w, &quick());
    assert_eq!(first.outcome, Outcome::Done, "{first:?}");
    assert_eq!((first.created, first.modified), (1, 1), "{first:?}");
    assert_eq!(first.aside.total(), 0);
    assert_eq!(counts(&w)[0], before[0] + 1);
    let snap = contacts_of(&w);

    // Même dossier, deuxième passage : rien à faire, aucun doublon.
    let second = pass(&mut w, &quick());
    assert_eq!(second.outcome, Outcome::Done, "{second:?}");
    assert_eq!((second.created, second.modified, second.undo.clone()), (0, 0, None), "{second:?}");
    assert_eq!(counts(&w)[0], before[0] + 1);
    assert_eq!(contacts_of(&w), snap);
    // Le fichier du téléphone ne propose plus rien.
    assert!(import_changes(&w.conn, &w.dir, &w.out).unwrap().changes.is_empty());
}

// --- Mis de côté -----------------------------------------------------------------------------------------------------

#[test]
fn suppression_du_telephone_est_comptee_pas_appliquee() {
    let mut w = ready("auto-suppression");
    let before = counts(&w);
    drop_contact(&w, "Durand");
    let rep = pass(&mut w, &quick());
    assert_eq!(rep.outcome, Outcome::Done, "{rep:?}");
    assert_eq!((rep.aside.deletions, rep.created, rep.modified), (1, 0, 0), "{rep:?}");
    assert_eq!(counts(&w), before);
    // Toujours là au passage suivant : le compteur reste visible tant que rien n'est tranché.
    assert_eq!(pass(&mut w, &quick()).aside.deletions, 1);
    assert_eq!(counts(&w), before);
}

#[test]
fn conflit_est_compte_pas_applique() {
    let mut w = ready("auto-conflit");
    let (dir, id) = (w.dir.clone(), contacts_of(&w).into_iter().find(|f| f.surname == "Martin").unwrap().id.unwrap());
    let n: i64 = id[1..].parse().unwrap();
    let mut f = thunderlook_contacts::write::form_of(&w.conn, &dir, n).unwrap();
    f.mobile = "0700000000".into();
    thunderlook_contacts::write::update(&mut w.conn, &dir, n, &f).unwrap();
    phone_edits_mobile(&w, "0612345678", "0688888888");
    let rep = pass(&mut w, &quick());
    assert_eq!(rep.outcome, Outcome::Done, "{rep:?}");
    assert_eq!((rep.aside.conflicts, rep.created, rep.modified), (1, 0, 0), "{rep:?}");
    assert_eq!(thunderlook_contacts::write::form_of(&w.conn, &dir, n).unwrap().mobile, "0700000000");
    assert_eq!(rep.undo, None);
}

#[test]
fn serie_refusee_est_comptee() {
    let mut w = ready("auto-serie");
    let ics = read(&w, "calendrier.ics");
    let bad = "BEGIN:VEVENT\r\nUID:mpe-serie-1\r\nSUMMARY:Toutes les heures\r\nDTSTART:20261106T180000Z\r\nDTEND:20261106T190000Z\r\nRRULE:FREQ=HOURLY;COUNT=5\r\nEND:VEVENT\r\nEND:VCALENDAR";
    write(&w, "calendrier.ics", &ics.replace("END:VCALENDAR", bad));
    let before = counts(&w);
    let rep = pass(&mut w, &quick());
    assert_eq!(rep.outcome, Outcome::Done, "{rep:?}");
    assert_eq!(rep.aside.series, 1, "{rep:?}");
    assert_eq!(counts(&w), before);
}

// --- Seuil de sécurité -----------------------------------------------------------------------------------------------

#[test]
fn seuil_depasse_rien_n_est_applique() {
    let mut w = ready("auto-seuil");
    let before = counts(&w);
    // 4 créations sur 2 contacts : plus de 5 % et au-delà du plancher (3) → bloqué, y compris le reste.
    phone_adds_contacts(&w, 4);
    write(&w, "notes.vnt", &read(&w, "notes.vnt").replace("Courses", "Courses du jour"));
    let rep = pass(&mut w, &quick());
    assert!(matches!(rep.outcome, Outcome::Blocked(ref s) if s.contains("contacts")), "{rep:?}");
    assert_eq!((rep.created, rep.modified, rep.undo.clone()), (0, 0, None));
    assert_eq!(counts(&w), before);
    assert!(!thunderlook_items::notes(&w.conn, &w.dir).unwrap().rows.iter().any(|r| format!("{r:?}").contains("du jour")));
}

#[test]
fn plafond_absolu_meme_sur_un_gros_module() {
    let mut w = ready("auto-plafond");
    let before = counts(&w);
    let p = Params { threshold_pct: 1000.0, threshold_max: 2, threshold_floor: 0, ..quick() };
    phone_adds_contacts(&w, 3);
    let rep = pass(&mut w, &p);
    assert!(matches!(rep.outcome, Outcome::Blocked(_)), "{rep:?}");
    assert_eq!(counts(&w), before);
    // Sous le plafond (et sous le pourcentage) : appliqué.
    let ok = Params { threshold_pct: 1000.0, threshold_max: 3, threshold_floor: 0, ..quick() };
    let rep = pass(&mut w, &ok);
    assert_eq!((rep.outcome.clone(), rep.created), (Outcome::Done, 3), "{rep:?}");
}

#[test]
fn petit_changement_sous_le_plancher_passe() {
    let mut w = ready("auto-plancher");
    phone_adds_contacts(&w, 1); // 1 sur 2 = 50 % mais sous le plancher
    assert_eq!(pass(&mut w, &quick()).created, 1);
}

// --- Annulation ------------------------------------------------------------------------------------------------------

#[test]
fn instantane_avant_application_permet_d_annuler() {
    let mut w = ready("auto-annuler");
    let before = counts(&w);
    let snapshot = contacts_of(&w);
    phone_adds_contacts(&w, 1);
    phone_edits_mobile(&w, "0612345678", "0688888888");
    let rep = pass(&mut w, &quick());
    assert_eq!((rep.created, rep.modified), (1, 1), "{rep:?}");
    let stamp = rep.undo.clone().expect("instantané consigné");
    assert_eq!(auto::read_state(&w.dir).last, None, "l'état n'est écrit que par record");
    assert!(w.dir.join("exchange-undo").join(format!("{stamp}.json")).is_file());
    assert_eq!(thunderlook_exchange::last_undo(&w.dir), Some(stamp.clone()));

    let dir = w.dir.clone();
    assert_eq!(undo(&mut w.conn, &dir, &stamp).unwrap(), 2);
    assert_eq!(counts(&w), before);
    let sorted = |mut v: Vec<thunderlook_model::contacts::ContactForm>| {
        v.sort_by(|a, b| a.surname.cmp(&b.surname));
        v.into_iter().map(|f| (f.given_name, f.surname, f.mobile)).collect::<Vec<_>>()
    };
    assert_eq!(sorted(contacts_of(&w)), sorted(snapshot));
}

// --- Export après import ---------------------------------------------------------------------------------------------

#[test]
fn export_apres_import_seulement_si_le_telephone_n_a_rien_touche() {
    // Téléphone intact + modification locale : exporté, le fichier reçoit la modification.
    let mut w = ready("auto-export-oui");
    let (dir, id) = (w.dir.clone(), contacts_of(&w).into_iter().find(|f| f.surname == "Martin").unwrap().id.unwrap());
    let n: i64 = id[1..].parse().unwrap();
    let mut f = thunderlook_contacts::write::form_of(&w.conn, &dir, n).unwrap();
    f.mobile = "0700000000".into();
    thunderlook_contacts::write::update(&mut w.conn, &dir, n, &f).unwrap();
    let rep = pass(&mut w, &quick());
    assert_eq!((rep.exported, rep.outcome.clone()), (Some(true), Outcome::Done), "{rep:?}");
    assert!(read(&w, "contacts.vcf").contains("0700000000"));
    assert!(thunderlook_exchange::modified_files(&w.out).is_empty());

    // Téléphone qui a modifié un fichier : importé, mais PAS exporté (le fichier garde ce que MPE a écrit).
    let mut w = ready("auto-export-non");
    phone_adds_contacts(&w, 1);
    let written = read(&w, "contacts.vcf");
    let rep = pass(&mut w, &quick());
    assert_eq!((rep.exported, rep.created), (Some(false), 1), "{rep:?}");
    assert!(rep.export_note.contains("téléphone"));
    assert_eq!(read(&w, "contacts.vcf"), written);

    // Export désactivé : ni fait ni refusé.
    let mut w = ready("auto-export-off");
    let rep = pass(&mut w, &Params { auto_export: false, ..quick() });
    assert_eq!(rep.exported, None);
}

// --- Journal, état, réglages -----------------------------------------------------------------------------------------

#[test]
fn journal_et_etat_conservent_les_passages() {
    let mut w = ready("auto-journal");
    phone_adds_contacts(&w, 1);
    drop_contact(&w, "Durand");
    let (dir, out) = (w.dir.clone(), w.out.clone());
    let (first, st) = auto::run_and_record(&mut w.conn, &dir, &out, &quick());
    let st = st.unwrap();
    assert_eq!(st.last.as_ref(), Some(&first));
    assert_eq!(st.last_read.as_ref().map(|r| r.aside.deletions), Some(1));
    // Un passage ignoré n'efface pas les compteurs du dernier passage lu.
    let (skipped, st) = auto::run_and_record(&mut w.conn, &dir, &out, &Params { min_age: Duration::from_secs(3600), ..quick() });
    assert!(matches!(skipped.outcome, Outcome::Skipped(_)));
    let st = st.unwrap();
    assert!(matches!(st.last.clone().unwrap().outcome, Outcome::Skipped(_)));
    assert_eq!(st.last_read.clone().unwrap().aside.deletions, 1);
    assert_eq!(auto::read_journal(&dir).len(), 2);
    assert_eq!(auto::read_state(&dir), st);
}

#[test]
fn intervalle_borne_de_5_a_120_minutes_defaut_15() {
    assert_eq!(Settings::default().minutes, 15);
    assert!(Settings::default().enabled, "la relecture automatique est active dès qu'un dossier est choisi");
    let w = world("auto-reglages");
    for (asked, kept) in [(0, 5), (3, 5), (5, 5), (60, 60), (120, 120), (500, 120)] {
        let st = auto::set_settings(&w.dir, Settings { enabled: true, minutes: asked, auto_export: true }).unwrap();
        assert_eq!(st.settings.minutes, kept);
        assert_eq!(auto::read_state(&w.dir).settings.minutes, kept);
    }
}

#[test]
fn annulation_inscrite_au_journal_et_retire_l_instantane() {
    let mut w = ready("auto-annuler-etat");
    phone_adds_contacts(&w, 1);
    drop_contact(&w, "Durand");
    let (dir, out) = (w.dir.clone(), w.out.clone());
    let (rep, _) = auto::run_and_record(&mut w.conn, &dir, &out, &quick());
    let stamp = rep.undo.clone().unwrap_or_else(|| "aucun".into());
    // Une création seule consigne bien un instantané.
    assert!(rep.undo.is_some(), "{rep:?}");
    undo(&mut w.conn, &dir, &stamp).unwrap();
    let st = auto::mark_undone(&dir, &stamp, "1 élément rétabli").unwrap();
    assert_eq!(st.last.unwrap().undo, None);
    assert_eq!(st.last_read.unwrap().aside.deletions, 1, "les compteurs restent affichés");
    assert!(matches!(auto::read_journal(&dir)[0].outcome, Outcome::Skipped(ref s) if s.contains("annulation")));
}
