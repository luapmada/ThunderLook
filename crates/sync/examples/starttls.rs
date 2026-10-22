//! Point ouvert du J7 : connexion IMAP en STARTTLS (port 143) et, pour comparaison, en TLS (993) ; SMTP en STARTTLS
//! (587). Compte de test seulement ; mot de passe lu dans le gestionnaire d'identifiants (exemple `sync` d'abord).
//! Usage : `cargo run -p thunderlook-sync --example starttls -- <adresse du compte de test> [<serveur IMAP> <serveur SMTP>]`

use std::time::Instant;

use thunderlook_sync::{Security, Server, account, imap_login, smtp_check};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let address = args.first().ok_or("adresse du compte de test attendue")?;
    if !address.starts_with("thunderlook-test") {
        return Err("compte de test seulement (thunderlook-test@…)".into());
    }
    let imap_host = args.get(1).cloned().unwrap_or_else(|| "imap.hostinger.com".into());
    let smtp_host = args.get(2).cloned().unwrap_or_else(|| "smtp.hostinger.com".into());
    let password = account::password(address)?;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    rt.block_on(async {
        for (port, security) in [(143, Security::StartTls), (993, Security::Tls)] {
            let server = Server { host: imap_host.clone(), port, security };
            let t = Instant::now();
            let mut s = imap_login(&server, address, &password).await?;
            let tls = matches!(s.get_ref().get_ref().1.protocol_version(), Some(_));
            let inbox = s.examine("INBOX").await.map_err(|e| e.to_string())?;
            println!(
                "IMAP {port} {security:?} : identifié en {:.2} s, TLS {} ({:?}), INBOX {} messages",
                t.elapsed().as_secs_f32(),
                if tls { "actif" } else { "ABSENT" },
                s.get_ref().get_ref().1.protocol_version(),
                inbox.exists
            );
            let _ = s.logout().await;
        }
        // Refus attendu : un mot de passe faux, en STARTTLS aussi, ne passe pas (et le vrai n'est jamais envoyé en clair).
        let server = Server { host: imap_host.clone(), port: 143, security: Security::StartTls };
        match imap_login(&server, address, "mot-de-passe-faux").await {
            Ok(_) => println!("IMAP 143 : mot de passe faux ACCEPTÉ (anormal)"),
            Err(e) => println!("IMAP 143 : mot de passe faux refusé : {e}"),
        }
        let smtp = Server { host: smtp_host.clone(), port: 587, security: Security::StartTls };
        smtp_check(&smtp, address, &password).await?;
        println!("SMTP 587 StartTls : identification acceptée");
        Ok(())
    })
}
