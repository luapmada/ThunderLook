//! Sonde du compte de test : identification IMAP et SMTP, dossiers et état de la Boîte de réception.
//! Usage : `cargo run -p thunderlook-sync --example probe` (lit `data/test-account.json`, non versionné).

use futures::TryStreamExt;
use thunderlook_sync::{Endpoints, imap_login, smtp_check};

#[tokio::main]
async fn main() -> Result<(), String> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/test-account.json");
    let acc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let (address, password) = (acc["address"].as_str().unwrap_or_default(), acc["password"].as_str().unwrap_or_default());
    let ep = Endpoints::hostinger(address);

    smtp_check(&ep.smtp, &ep.username, password).await?;
    println!("SMTP {}:{} : identification OK", ep.smtp.host, ep.smtp.port);

    let mut s = imap_login(&ep.imap, &ep.username, password).await?;
    println!("IMAP {}:{} : identification OK", ep.imap.host, ep.imap.port);
    let caps = s.capabilities().await.map_err(|e| e.to_string())?;
    let mut list: Vec<String> = caps.iter().map(|c| format!("{c:?}")).collect();
    list.sort();
    println!("capacités : {}", list.join(" "));
    let names: Vec<_> = s.list(Some(""), Some("*")).await.map_err(|e| e.to_string())?.try_collect().await.map_err(|e| e.to_string())?;
    for n in &names {
        println!("dossier {:?} (séparateur {:?}) {:?}", n.name(), n.delimiter(), n.attributes());
    }
    let mb = s.examine("INBOX").await.map_err(|e| e.to_string())?;
    println!("INBOX : {} messages, UIDVALIDITY {:?}, UIDNEXT {:?}", mb.exists, mb.uid_validity, mb.uid_next);
    s.logout().await.map_err(|e| e.to_string())?;
    Ok(())
}
