//! Recherche en lecture seule (EXAMINE) d'un texte dans l'objet, dans tous les dossiers d'un compte.
//! Usage : `cargo run -p thunderlook-sync --example find -- <dossier de données> <source> <texte>…`

use std::path::Path;

use futures::TryStreamExt;
use thunderlook_sync::{account, imap_login};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, source) = (Path::new(&args[0]), args[1].parse::<i64>().map_err(|e| e.to_string())?);
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    let acc = account::account(&conn, source)?;
    let password = account::password(&acc.address)?;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let mut s = imap_login(&acc.endpoints.imap, &acc.endpoints.username, &password).await?;
        let names: Vec<_> = s.list(Some(""), Some("*")).await.map_err(|e| e.to_string())?.try_collect().await.map_err(|e| e.to_string())?;
        for n in &names {
            let name = n.name().to_string();
            let Ok(mb) = s.examine(&name).await else { continue };
            let mut line = format!("{name} ({} messages) :", mb.exists);
            for text in &args[2..] {
                let found = s.uid_search(format!("SUBJECT \"{text}\"")).await.map_err(|e| e.to_string())?;
                line.push_str(&format!(" « {text} » {}", found.len()));
            }
            println!("{line}");
        }
        s.logout().await.map_err(|e| e.to_string())
    })
}
