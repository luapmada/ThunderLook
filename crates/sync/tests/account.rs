//! Comptes (J7) : création, lecture, suppression ; le mot de passe est dans le gestionnaire d'identifiants de Windows
//! et n'apparaît nulle part dans la base. Adresse fictive (`.invalid`), aucune connexion réseau.

use thunderlook_sync::{Endpoints, account};

#[test]
fn account_lifecycle_keeps_password_out_of_the_database() {
    let dir = std::env::temp_dir().join(format!("tl-account-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let conn = thunderlook_store::open(&dir).unwrap();
    let address = format!("test-{}@thunderlook.invalid", std::process::id());
    let secret = format!("Secret-{}-x9!", std::process::id());
    let ep = Endpoints::hostinger(&address);

    let id = account::create(&conn, &address, "Test", &ep, &secret).unwrap();
    assert!(account::create(&conn, &address.to_uppercase(), "Test", &ep, &secret).is_err(), "doublon refusé");
    assert_eq!(account::password(&address).unwrap(), secret);
    let a = account::account(&conn, id).unwrap();
    assert_eq!((a.address.as_str(), a.endpoints.imap.host.as_str()), (address.as_str(), "imap.hostinger.com"));
    let s = account::summaries(&conn).unwrap();
    assert_eq!((s[0].id.clone(), s[0].smtp.clone()), (format!("s{id}"), "smtp.hostinger.com:465".to_string()));
    // Règles à l'arrivée : désactivées par défaut.
    assert!(!a.rules_on_arrival && !s[0].rules_on_arrival);
    account::set_rules_on_arrival(&conn, id, true).unwrap();
    assert!(account::account(&conn, id).unwrap().rules_on_arrival);
    // Section du volet : un dossier « ipm » avec la Boîte d'envoi locale.
    let outbox: String = conn
        .query_row("SELECT f.name FROM folder f JOIN folder p ON p.id = f.parent_id WHERE p.role = 'ipm' AND f.role = 'outbox' AND f.source_id = ?1", [id], |r| r.get(0))
        .unwrap();
    assert_eq!(outbox, "Boîte d'envoi");

    // Le mot de passe n'est écrit nulle part dans le dossier de données.
    drop(conn);
    for e in std::fs::read_dir(&dir).unwrap().flatten() {
        if e.path().is_file() {
            let bytes = std::fs::read(e.path()).unwrap();
            assert!(!bytes.windows(secret.len()).any(|w| w == secret.as_bytes()), "mot de passe trouvé dans {:?}", e.path());
        }
    }

    let conn = thunderlook_store::open(&dir).unwrap();
    account::remove(&conn, &dir, id).unwrap();
    assert!(account::accounts(&conn).unwrap().is_empty());
    assert!(account::password(&address).is_err(), "mot de passe retiré du gestionnaire d'identifiants");
    assert_eq!(account::password_users(&address).unwrap(), None, "liste des utilisateurs retirée aussi");
    drop(conn);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Point ouvert du J7 : le même compte dans deux dossiers de données ; le retirer de l'un garde le mot de passe pour
/// l'autre. Un mot de passe enregistré avant la liste des utilisateurs n'est jamais effacé.
#[test]
fn password_is_shared_between_data_dirs() {
    let base = std::env::temp_dir().join(format!("tl-account-shared-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (a, b) = (base.join("Données"), base.join("Essai"));
    let (ca, cb) = (thunderlook_store::open(&a).unwrap(), thunderlook_store::open(&b).unwrap());
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let address = format!("partage-{}-{nanos}@thunderlook.invalid", std::process::id());
    // Restes éventuels d'un essai interrompu (mêmes noms) : retirés d'abord.
    for user in [address.to_lowercase(), format!("{}#dossiers", address.to_lowercase())] {
        let _ = keyring::Entry::new("ThunderLook", &user).unwrap().delete_credential();
    }
    let ep = Endpoints::hostinger(&address);
    let ia = account::create(&ca, &address, "A", &ep, "secret-a").unwrap();
    let ib = account::create(&cb, &address, "B", &ep, "secret-a").unwrap();
    let users = account::password_users(&address).unwrap().unwrap();
    assert_eq!(users, vec![account::data_dir_key(&a), account::data_dir_key(&b)]);
    // Même dossier écrit autrement (casse) : même clé.
    assert_eq!(account::data_dir_key(&base.join("DONNÉES")), account::data_dir_key(&a));

    account::remove(&ca, &a, ia).unwrap();
    assert_eq!(account::password(&address).unwrap(), "secret-a", "encore utilisé par l'autre dossier");
    assert_eq!(account::password_users(&address).unwrap().unwrap(), vec![account::data_dir_key(&b)]);
    account::remove(&cb, &b, ib).unwrap();
    assert!(account::password(&address).is_err());
    assert_eq!(account::password_users(&address).unwrap(), None);

    // Mot de passe antérieur (v0.9.0 : entrée par adresse, sans liste) : ses utilisateurs sont inconnus.
    let legacy = keyring::Entry::new("ThunderLook", &address.to_lowercase()).unwrap();
    legacy.set_password("ancien").unwrap();
    let ia = account::create(&ca, &address, "A", &ep, "ancien").unwrap();
    assert_eq!(account::password_users(&address).unwrap().unwrap(), vec![account::LEGACY_USER.to_string(), account::data_dir_key(&a)]);
    account::remove(&ca, &a, ia).unwrap();
    assert_eq!(account::password(&address).unwrap(), "ancien", "jamais effacé sans savoir qui s'en sert");
    // Nettoyage du gestionnaire d'identifiants.
    let _ = legacy.delete_credential();
    let _ = keyring::Entry::new("ThunderLook", &format!("{}#dossiers", address.to_lowercase())).unwrap().delete_credential();
    drop((ca, cb));
    let _ = std::fs::remove_dir_all(&base);
}

/// IMAP en STARTTLS (port 143) : paramètres gardés et relus tels quels, affichés comme tels.
#[test]
fn starttls_endpoints_round_trip() {
    use thunderlook_sync::Security;
    let dir = std::env::temp_dir().join(format!("tl-account-starttls-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let conn = thunderlook_store::open(&dir).unwrap();
    let address = format!("starttls-{}@thunderlook.invalid", std::process::id());
    let mut ep = Endpoints::hostinger(&address);
    (ep.imap.port, ep.imap.security, ep.smtp.port, ep.smtp.security) = (143, Security::StartTls, 587, Security::StartTls);
    let id = account::create(&conn, &address, "Test", &ep, "x").unwrap();
    assert_eq!(account::account(&conn, id).unwrap().endpoints, ep);
    let s = &account::summaries(&conn).unwrap()[0];
    assert_eq!((s.imap.as_str(), s.smtp.as_str()), ("imap.hostinger.com:143 (STARTTLS)", "smtp.hostinger.com:587 (STARTTLS)"));
    account::remove(&conn, &dir, id).unwrap();
    drop(conn);
    let _ = std::fs::remove_dir_all(&dir);
}
