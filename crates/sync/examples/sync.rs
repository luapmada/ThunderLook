//! Relève du compte de test dans un dossier de données de test (jamais `data/store`) : crée le compte s'il manque
//! (`data/test-account.json`), relève deux fois et affiche les dossiers. Le second passage ne doit rien changer.
//! Usage : `cargo run -p thunderlook-sync --example sync -- <dossier de données de test> [--account <fichier>] [--remove]`

use std::path::Path;

use thunderlook_sync::{Endpoints, account, imap_login, smtp_check, sync_account_blocking};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = Path::new(args.first().ok_or("dossier de données de test attendu")?);
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    // Fichier de compte : `data/test-account.json` par défaut, ou celui donné par `--account <fichier>`.
    let default = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/test-account.json").to_string();
    let path = args.iter().position(|a| a == "--account").and_then(|i| args.get(i + 1)).cloned().unwrap_or(default);
    let acc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let address = acc["address"].as_str().unwrap_or_default();
    let source = match account::accounts(&conn)?.into_iter().find(|a| a.address.eq_ignore_ascii_case(address)) {
        Some(a) if args.iter().any(|x| x == "--remove") => {
            account::remove(&conn, dir, a.source_id)?;
            println!("compte retiré du dossier de test (rien n'est effacé sur le serveur)");
            return Ok(());
        }
        Some(a) => {
            // Outil de test : le mot de passe de test est remis dans le gestionnaire d'identifiants (un retrait du
            // compte ailleurs, par ui_check par exemple, l'en a ôté : l'entrée est propre à l'adresse).
            account::store_password(address, acc["password"].as_str().unwrap_or_default())?;
            a.source_id
        }
        None => {
            let password = acc["password"].as_str().unwrap_or_default();
            let ep = Endpoints::hostinger(address);
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
            rt.block_on(async {
                smtp_check(&ep.smtp, &ep.username, password).await?;
                imap_login(&ep.imap, &ep.username, password).await?.logout().await.map_err(|e| e.to_string())
            })?;
            let id = account::create(&conn, address, "ThunderLook test", &ep, password)?;
            println!("compte créé (source {id})");
            id
        }
    };
    for pass in 1..=2 {
        let r = sync_account_blocking(dir, source)?;
        println!(
            "relève {pass} : {} dossiers, {} nouveaux, {} drapeaux mis à jour, {} retirés, {} ms",
            r.folders, r.new_messages, r.updated_flags, r.removed, r.elapsed_ms
        );
    }
    let mut stmt = conn
        .prepare(
            "SELECT f.name, f.role, f.imap_name, f.imap_uidvalidity, f.imap_uidnext, COUNT(m.id), SUM(m.is_read = 0)
             FROM folder f LEFT JOIN message m ON m.folder_id = f.id WHERE f.source_id = ?1 AND f.role NOT IN ('root', 'ipm')
             GROUP BY f.id ORDER BY f.position",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([source], |r| {
            Ok(format!(
                "  {:<22} {:<8} {:<14} uidvalidity {:?} uidnext {:?} : {} messages, {} non lus",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?.unwrap_or_else(|| "(local)".into()),
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, Option<i64>>(6)?.unwrap_or(0)
            ))
        })
        .map_err(|e| e.to_string())?;
    for r in rows {
        println!("{}", r.map_err(|e| e.to_string())?);
    }
    let mut stmt = conn
        .prepare("SELECT m.subject, m.from_addr, m.is_read, m.imap_uid, m.eml_path FROM message m JOIN folder f ON f.id = m.folder_id WHERE f.source_id = ?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([source], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, bool>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, String>(4)?)))
        .map_err(|e| e.to_string())?;
    for r in rows {
        let (subject, from, read, uid, eml) = r.map_err(|e| e.to_string())?;
        let ok = thunderlook_store::resolve(dir, &eml).is_file();
        println!("  uid {uid:?} {} « {subject} » de {from} ; .eml {}", if read { "lu" } else { "non lu" }, if ok { "présent" } else { "ABSENT" });
    }
    Ok(())
}
