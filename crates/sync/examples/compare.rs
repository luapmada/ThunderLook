//! Compare, dossier par dossier, le nombre de messages sur le serveur (EXAMINE, lecture seule) et dans ThunderLook.
//! Mot de passe lu dans le gestionnaire d'identifiants (rangé par ThunderLook à l'ajout du compte) ; rien n'est modifié.
//! Usage : `cargo run -p thunderlook-sync --example compare -- <dossier de données> <source>`

use std::path::Path;

use futures::TryStreamExt;
use thunderlook_store::rusqlite::params;
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
        let (mut server_total, mut local_total) = (0u32, 0i64);
        println!("{:<34} {:>8} {:>12}", "dossier (serveur)", "serveur", "ThunderLook");
        for n in &names {
            let name = n.name().to_string();
            let server = match s.examine(&name).await {
                Ok(mb) => mb.exists,
                Err(_) => continue,
            };
            let local: i64 = conn
                .query_row(
                    "SELECT COUNT(m.id) FROM folder f LEFT JOIN message m ON m.folder_id = f.id AND m.imap_uid IS NOT NULL
                     WHERE f.source_id = ?1 AND f.imap_name = ?2",
                    params![source, name],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            server_total += server;
            local_total += local;
            println!("{name:<34} {server:>8} {local:>12}{}", if i64::from(server) == local { "" } else { "   ≠" });
        }
        println!("{:<34} {server_total:>8} {local_total:>12}", "TOTAL");
        s.logout().await.map_err(|e| e.to_string())
    })
}
