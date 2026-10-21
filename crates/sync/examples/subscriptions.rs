//! Dossiers abonnés (LSUB) d'un compte ; avec `--subscribe`, abonne aussi les dossiers qui ne le sont pas.
//! Usage : `cargo run -p thunderlook-sync --example subscriptions -- <dossier de données> <source> [--subscribe]`

use std::path::Path;

use futures::TryStreamExt;
use thunderlook_sync::{account, imap_login};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, source) = (Path::new(&args[0]), args[1].parse::<i64>().map_err(|e| e.to_string())?);
    // `--subscribe [nom IMAP…]` : seulement ces dossiers s'ils sont nommés, sinon tous ceux qui ne le sont pas.
    let subscribe = args.iter().any(|a| a == "--subscribe");
    let only: Vec<&String> = args.iter().skip_while(|a| *a != "--subscribe").skip(1).collect();
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    let acc = account::account(&conn, source)?;
    let password = account::password(&acc.address)?;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let mut s = imap_login(&acc.endpoints.imap, &acc.endpoints.username, &password).await?;
        let all: Vec<_> = s.list(Some(""), Some("*")).await.map_err(|e| e.to_string())?.try_collect().await.map_err(|e| e.to_string())?;
        let subscribed: Vec<String> =
            s.lsub(Some(""), Some("*")).await.map_err(|e| e.to_string())?.try_collect::<Vec<_>>().await.map_err(|e| e.to_string())?.iter().map(|n| n.name().to_string()).collect();
        for n in &all {
            let name = n.name().to_string();
            let is = subscribed.contains(&name);
            let noselect = n.attributes().iter().any(|a| matches!(a, async_imap::types::NameAttribute::NoSelect));
            let mut action = "";
            if subscribe && !is && !noselect && (only.is_empty() || only.iter().any(|o| **o == name)) {
                s.subscribe(&name).await.map_err(|e| e.to_string())?;
                action = " → abonné";
            }
            println!("{name:<34} abonné : {}{action}", if is { "oui" } else { "non" });
        }
        s.logout().await.map_err(|e| e.to_string())
    })
}
