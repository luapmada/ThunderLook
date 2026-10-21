//! Renomme ou supprime un dossier sur le serveur d'un compte, en refusant de supprimer un dossier qui contient des
//! messages (ménage du 21/10 : dossiers de règles inutiles, nom avec séparateur).
//! Usage : `cargo run -p thunderlook-sync --example folder_fix -- <dossier de données> <source> status|delete|rename <nom IMAP> [nouveau nom IMAP]`

use std::path::Path;

use thunderlook_sync::{account, imap_login};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, source) = (Path::new(&args[0]), args[1].parse::<i64>().map_err(|e| e.to_string())?);
    let (op, name) = (args[2].as_str(), args[3].as_str());
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    let acc = account::account(&conn, source)?;
    let password = account::password(&acc.address)?;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let mut s = imap_login(&acc.endpoints.imap, &acc.endpoints.username, &password).await?;
        // Un dossier \Noselect (parent implicite) n'a pas de messages : EXAMINE échoue, on le traite comme vide.
        let count = match s.examine(name).await {
            Ok(m) => m.exists,
            Err(_) => 0,
        };
        println!("{name} : {count} message(s)");
        match op {
            "status" => {}
            "delete" if count > 0 => return Err(format!("{name} n'est pas vide : suppression refusée")),
            "delete" => {
                s.close().await.ok();
                let _ = s.unsubscribe(name).await;
                s.delete(name).await.map_err(|e| format!("DELETE {name} : {e}"))?;
                println!("{name} supprimé");
            }
            "rename" => {
                let to = args.get(4).ok_or("nouveau nom manquant")?;
                s.close().await.ok();
                let _ = s.unsubscribe(name).await;
                s.rename(name, to).await.map_err(|e| format!("RENAME {name} : {e}"))?;
                s.subscribe(to).await.map_err(|e| format!("SUBSCRIBE {to} : {e}"))?;
                println!("{name} → {to} (abonné)");
            }
            _ => return Err(format!("opération inconnue : {op}")),
        }
        s.logout().await.map_err(|e| e.to_string())
    })
}
