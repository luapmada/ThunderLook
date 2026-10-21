//! Dates de réception (INTERNALDATE) sur le serveur et date d'en-tête, pour les premiers messages d'un dossier.
//! Usage : `cargo run -p thunderlook-sync --example dates -- <dossier de données> <source> <nom IMAP> <nombre>`

use std::path::Path;

use futures::TryStreamExt;
use thunderlook_sync::{account, imap_login};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, source, mailbox, n) = (Path::new(&args[0]), args[1].parse::<i64>().map_err(|e| e.to_string())?, &args[2], &args[3]);
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    let acc = account::account(&conn, source)?;
    let password = account::password(&acc.address)?;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let mut s = imap_login(&acc.endpoints.imap, &acc.endpoints.username, &password).await?;
        s.examine(mailbox).await.map_err(|e| e.to_string())?;
        let fetches: Vec<_> = s
            .fetch(format!("1:{n}"), "(INTERNALDATE BODY.PEEK[HEADER.FIELDS (DATE SUBJECT)])")
            .await
            .map_err(|e| e.to_string())?
            .try_collect()
            .await
            .map_err(|e| e.to_string())?;
        for f in &fetches {
            let head = f.header().map(|h| String::from_utf8_lossy(h).split_whitespace().collect::<Vec<_>>().join(" ")).unwrap_or_default();
            println!("serveur {:?} | {}", f.internal_date().map(|d| d.to_rfc2822()), head.chars().take(110).collect::<String>());
        }
        s.logout().await.map_err(|e| e.to_string())
    })
}
