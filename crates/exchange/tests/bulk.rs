//! Lot AD : banc de charge de l'échange. 2 000 contacts synthétiques (aucune donnée réelle) : export → import → ré-export,
//! puis trois tours de modifications alternées côté « téléphone » (fichier réécrit) et côté ThunderLook (base).
//! 0 doublon, 0 perte ; les durées sont mesurées et affichées (`cargo test -p thunderlook-exchange --test bulk -- --nocapture`).

use std::{collections::HashSet, time::{Duration, Instant}};

use thunderlook_exchange::{ChangeKind, ItemKind, apply, export_all, import_changes};
use thunderlook_model::contacts::ContactForm;

mod common;
use common::*;

const N: usize = 2000;
/// Seuil d'échec très large (build de débogage, machine de CI) : il attrape une dérive quadratique, pas un à-peu-près.
const LIMIT: Duration = Duration::from_secs(120);

fn mobile(i: usize, generation: usize) -> String {
    format!("06{generation}{i:07}")
}

fn form(i: usize, generation: usize) -> ContactForm {
    ContactForm {
        given_name: format!("Prénom{i}"),
        surname: format!("ZZ-TEST Nom{i:04}"),
        email1: format!("zz-test{i}@exemple.invalid"),
        mobile: mobile(i, generation),
        categories: "Test".into(),
        ..Default::default()
    }
}

/// Ce que MPE fait en réécrivant le fichier : fins de ligne `\n`.
fn rewrite_like_phone(w: &World) {
    write(w, "contacts.vcf", &read(w, "contacts.vcf").replace("\r\n", "\n"));
}

fn timed<T>(label: &str, total: &mut Duration, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let r = f();
    let d = t.elapsed();
    *total += d;
    println!("bulk: {label:<40} {d:>9.2?}");
    r
}

/// Importe tout ce qui est coché d'office, puis ré-exporte ; rend le nombre de changements appliqués.
fn sync(w: &mut World) -> usize {
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert!(rep.conflicts.is_empty(), "{} conflits", rep.conflicts.len());
    let accepted: Vec<_> = rep.changes.iter().filter(|c| c.default_selected()).cloned().collect();
    let dir = w.dir.clone();
    let done = apply(&mut w.conn, &dir, &accepted).unwrap();
    assert!(done.errors.is_empty(), "{:?}", done.errors);
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    accepted.len()
}

/// 0 doublon, 0 perte : exactement N contacts, chacun une fois, avec le téléphone attendu.
fn assert_state(w: &World, generation: &[usize]) {
    let got = contacts_of(w);
    assert_eq!(got.len(), N, "perte ou doublon : {} contacts", got.len());
    let names: HashSet<_> = got.iter().map(|f| f.surname.clone()).collect();
    assert_eq!(names.len(), N, "doublon de nom");
    let ids: HashSet<_> = got.iter().filter_map(|f| f.id.clone()).collect();
    assert_eq!(ids.len(), N, "doublon d'identifiant");
    for f in &got {
        let i: usize = f.surname.rsplit("Nom").next().unwrap().parse().unwrap();
        assert_eq!(f.mobile, mobile(i, generation[i]), "contact {i}");
        assert_eq!((f.given_name.as_str(), f.categories.as_str()), (format!("Prénom{i}").as_str(), "Test"), "contact {i}");
    }
    let vcf = read(w, "contacts.vcf");
    assert_eq!(vcf.matches("BEGIN:VCARD").count(), N, "fichier : nombre de fiches");
}

fn seeded(name: &str) -> World {
    let mut w = world(name);
    let dir = w.dir.clone();
    for i in 0..N {
        thunderlook_contacts::write::create(&mut w.conn, &dir, 1, &form(i, 0)).unwrap();
    }
    w
}

#[test]
fn deux_mille_contacts_export_import_reexport_sans_perte_ni_doublon() {
    let mut total = Duration::ZERO;
    let mut w = timed("création de 2 000 contacts", &mut total, || seeded("bulk-aller-retour"));
    timed("export", &mut total, || export_all(&w.conn, &w.dir, &w.out).unwrap());
    let first = read(&w, "contacts.vcf");
    let rep = timed("import (fichier inchangé)", &mut total, || import_changes(&w.conn, &w.dir, &w.out).unwrap());
    assert!(rep.changes.is_empty() && rep.conflicts.is_empty(), "{} changements", rep.changes.len());
    assert_eq!(rep.unchanged, N);
    timed("ré-export", &mut total, || export_all(&w.conn, &w.dir, &w.out).unwrap());
    assert_eq!(read(&w, "contacts.vcf"), first, "le ré-export n'est pas identique à l'export");

    // Le téléphone réécrit le fichier tel quel (fins de ligne) : toujours rien à proposer.
    rewrite_like_phone(&w);
    assert_eq!(timed("import (fichier réécrit)", &mut total, || sync(&mut w)), 0);
    assert_state(&w, &vec![0; N]);
    println!("bulk: total mesuré {total:.2?}");
    assert!(total < LIMIT, "{total:?}");
}

#[test]
fn trois_tours_de_modifications_alternees_telephone_et_thunderlook() {
    let mut total = Duration::ZERO;
    let mut w = timed("création de 2 000 contacts", &mut total, || seeded("bulk-tours"));
    timed("export initial", &mut total, || export_all(&w.conn, &w.dir, &w.out).unwrap());
    let mut generation = vec![0usize; N];

    for round in 1..=3usize {
        // Côté téléphone : un contact sur 7 change de numéro dans contacts.vcf.
        let phone: Vec<usize> = (0..N).filter(|i| i % 7 == round).collect();
        let mut vcf = read(&w, "contacts.vcf").replace("\r\n", "\n");
        for &i in &phone {
            let (old, new) = (format!("TEL;TYPE=CELL,VOICE:{}\n", mobile(i, generation[i])), format!("TEL;TYPE=CELL,VOICE:{}\n", mobile(i, round)));
            assert!(vcf.contains(&old), "contact {i} introuvable dans le fichier");
            vcf = vcf.replace(&old, &new);
            generation[i] = round;
        }
        write(&w, "contacts.vcf", &vcf);
        let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
        let mut kinds: Vec<_> = rep.changes.iter().map(|c| (c.kind, c.item)).collect();
        kinds.dedup();
        assert_eq!((rep.changes.len(), kinds.as_slice()), (phone.len(), &[(ChangeKind::Modified, ItemKind::Contact)][..]), "tour {round}, téléphone");
        let n = timed(&format!("tour {round} : import du téléphone"), &mut total, || sync(&mut w));
        assert_eq!(n, phone.len());
        assert_state(&w, &generation);
        assert!(import_changes(&w.conn, &w.dir, &w.out).unwrap().changes.is_empty(), "tour {round} : rien à relire après l'export");

        // Côté ThunderLook : un contact sur 11 change de numéro dans la base, puis export ; le téléphone resynchronise.
        let local: HashSet<usize> = (0..N).filter(|i| i % 11 == round).collect();
        let dir = w.dir.clone();
        timed(&format!("tour {round} : modifications ThunderLook"), &mut total, || {
            for f in contacts_of(&w) {
                let i: usize = f.surname.rsplit("Nom").next().unwrap().parse().unwrap();
                if local.contains(&i) {
                    let id: i64 = f.id.as_deref().unwrap()[1..].parse().unwrap();
                    generation[i] = round + 10;
                    let edited = ContactForm { mobile: mobile(i, round + 10), ..f };
                    thunderlook_contacts::write::update(&mut w.conn, &dir, id, &edited).unwrap();
                }
            }
        });
        timed(&format!("tour {round} : export ThunderLook"), &mut total, || export_all(&w.conn, &w.dir, &w.out).unwrap());
        rewrite_like_phone(&w);
        let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
        assert!(rep.changes.is_empty() && rep.conflicts.is_empty(), "tour {round} : le téléphone n'a rien changé, {} changements", rep.changes.len());
        assert_state(&w, &generation);
    }
    println!("bulk: total mesuré {total:.2?}");
    assert!(total < LIMIT, "{total:?}");
}
