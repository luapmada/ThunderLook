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
    drop(conn);
    let _ = std::fs::remove_dir_all(&dir);
}
