//! Lot BB : fidélité des contacts à l'échange. Un contact qui porte TOUS les champs du formulaire (fax, téléphones
//! supplémentaires, adresse « autre », second prénom, surnom, messagerie, titre, conjoint) fait l'aller-retour
//! base → `.vcf` → téléphone → import sans perte, et un changement fait côté téléphone ne touche que le champ changé.

use thunderlook_exchange::{ChangeKind, Payload, apply, export_all, import_changes};
use thunderlook_model::contacts::ContactForm;

mod common;
use common::*;

fn riche() -> ContactForm {
    ContactForm {
        prefix: "Dr".into(),
        given_name: "Léa".into(),
        middle_name: "Marie".into(),
        surname: "Martin".into(),
        suffix: "Jr".into(),
        nickname: "Lélé".into(),
        company: "ADI".into(),
        job_title: "Directrice".into(),
        department: "Atelier".into(),
        email1: "lea@exemple.fr".into(),
        mobile: "0601020304".into(),
        business: "0546000001".into(),
        home_phone: "0546000002".into(),
        business_fax: "0546000003".into(),
        home_fax: "0546000004".into(),
        business2: "0546000005".into(),
        home2: "0546000006".into(),
        car_phone: "0601000007".into(),
        pager: "0601000008".into(),
        other_phone: "0546000009".into(),
        home_street: "2 rue B".into(),
        home_city: "Niort".into(),
        home_postal: "79000".into(),
        home_country: "France".into(),
        other_street: "3 rue C".into(),
        other_city: "Surgères".into(),
        other_postal: "17700".into(),
        other_country: "France".into(),
        web_page: "https://exemple.fr".into(),
        im_address: "lea@jabber.exemple".into(),
        spouse: "Paul".into(),
        birthday: "1980-04-12".into(),
        notes: "Une note".into(),
        categories: "Clients".into(),
        ..Default::default()
    }
}

fn sans_id(mut f: ContactForm) -> ContactForm {
    f.id = None;
    f.folder_id.clear();
    f
}

fn with_world(name: &str) -> World {
    let mut w = world(name);
    let dir = w.dir.clone();
    thunderlook_contacts::write::create(&mut w.conn, &dir, 1, &riche()).unwrap();
    export_all(&w.conn, &w.dir, &w.out).unwrap();
    w
}

#[test]
fn aucun_champ_perdu_dans_le_va_et_vient() {
    let mut w = with_world("bb-aller-retour");
    assert_eq!(sans_id(contacts_of(&w).remove(0)), riche(), "la base ne garde pas tout");
    let vcf = read(&w, "contacts.vcf");
    for k in ["FAX", "CAR", "PAGER", "NICKNAME", "IMPP", "X-SPOUSE", "ADR;TYPE=OTHER"] {
        assert!(vcf.contains(k), "{k} absent du .vcf");
    }
    // Le téléphone relit et réécrit le fichier, trois fois : rien à proposer, rien de changé.
    for tour in 0..3 {
        write(&w, "contacts.vcf", &read(&w, "contacts.vcf").replace("\r\n", "\n"));
        let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
        assert!(rep.warnings.is_empty(), "tour {tour} : {:?}", rep.warnings);
        assert!(rep.changes.is_empty(), "tour {tour} : {:?}", rep.changes);
        let dir = w.dir.clone();
        apply(&mut w.conn, &dir, &rep.changes).unwrap();
        export_all(&w.conn, &w.dir, &w.out).unwrap();
        assert_eq!(sans_id(contacts_of(&w).remove(0)), riche(), "tour {tour}");
    }
}

#[test]
fn le_telephone_change_un_numero_le_reste_est_intact() {
    let mut w = with_world("bb-modif-telephone");
    let vcf = read(&w, "contacts.vcf").replace("0601020304", "0699999999");
    write(&w, "contacts.vcf", &vcf);
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert_eq!(rep.changes.len(), 1, "{:?}", rep.changes);
    let c = &rep.changes[0];
    assert_eq!(c.kind, ChangeKind::Modified);
    let Payload::Contact(f) = &c.payload else { panic!("pas un contact") };
    assert_eq!(f.mobile, "0699999999");
    let dir = w.dir.clone();
    let done = apply(&mut w.conn, &dir, &rep.changes).unwrap();
    assert!(done.errors.is_empty(), "{:?}", done.errors);
    assert_eq!(sans_id(contacts_of(&w).remove(0)), ContactForm { mobile: "0699999999".into(), ..riche() });
}

#[test]
fn un_telephone_qui_oublie_les_champs_supplementaires_ne_les_efface_pas_en_silence() {
    // Un téléphone qui ne sait pas porter fax / voiture / récepteur d'appels réécrit la fiche sans eux : ThunderLook propose
    // la modification en nommant les champs (rien n'est écrit tant que `apply` n'a pas reçu le changement) ; non appliquée,
    // la base reste intacte.
    let mut w = with_world("bb-champs-absents");
    let pauvre: String = read(&w, "contacts.vcf").lines().filter(|l| !["TEL;TYPE=FAX", "TEL;TYPE=CAR", "TEL;TYPE=PAGER", "TEL;FAX", "TEL;CAR", "TEL;PAGER"].iter().any(|p| l.starts_with(p))).collect::<Vec<_>>().join("\n");
    write(&w, "contacts.vcf", &pauvre);
    let rep = import_changes(&w.conn, &w.dir, &w.out).unwrap();
    assert_eq!(rep.changes.len(), 1, "{:?}", rep.changes);
    let c = &rep.changes[0];
    assert_eq!(c.kind, ChangeKind::Modified);
    for champ in ["businessFax", "homeFax", "carPhone", "pager"] {
        assert!(c.summary.contains(champ), "{champ} absent de « {} »", c.summary);
    }
    let dir = w.dir.clone();
    apply(&mut w.conn, &dir, &[]).unwrap();
    assert_eq!(sans_id(contacts_of(&w).remove(0)), riche());
}
