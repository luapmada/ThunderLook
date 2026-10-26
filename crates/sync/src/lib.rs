//! Comptes de ThunderLook (J7) : connexion IMAP et SMTP. Plan : `notes/j7-plan.md`.
//!
//! TLS par rustls, avec les racines de confiance de Mozilla (`webpki-roots`) : aucun certificat n'est accepté sans
//! vérification. Les mots de passe ne passent que par les fonctions de connexion et n'apparaissent dans aucun message
//! d'erreur.

pub mod account;
pub mod autoconfig;
pub mod autoreply;
pub mod fetch;
pub mod push;
pub mod send;

use std::sync::Arc;

use async_imap::Session;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_rustls::{TlsConnector, client::TlsStream, rustls};

pub type Result<T> = std::result::Result<T, String>;
pub type ImapSession = Session<TlsStream<TcpStream>>;

/// Sécurité de la connexion : TLS dès l'ouverture (993, 465) ou STARTTLS (143, 587).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Security {
    Tls,
    StartTls,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub host: String,
    pub port: u16,
    pub security: Security,
}

/// Paramètres de connexion d'un compte (sans le mot de passe).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoints {
    pub imap: Server,
    pub smtp: Server,
    /// Identifiant de connexion (l'adresse, chez Hostinger).
    pub username: String,
}

impl Endpoints {
    /// Serveurs d'un domaine hébergé chez Hostinger (offre Business Email) : IMAP 993 et SMTP 465, en TLS.
    pub fn hostinger(address: &str) -> Self {
        Self {
            imap: Server { host: "imap.hostinger.com".into(), port: 993, security: Security::Tls },
            smtp: Server { host: "smtp.hostinger.com".into(), port: 465, security: Security::Tls },
            username: address.to_string(),
        }
    }
}

fn tls_connector() -> TlsConnector {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

/// Ouvre une session IMAP authentifiée : TLS dès l'ouverture (993), ou STARTTLS (143 : connexion en clair, accueil,
/// commande STARTTLS, puis TLS avant toute identification ; le mot de passe ne passe jamais en clair).
pub async fn imap_login(server: &Server, username: &str, password: &str) -> Result<ImapSession> {
    let tcp = TcpStream::connect((server.host.as_str(), server.port)).await.map_err(|e| format!("{}:{} : {e}", server.host, server.port))?;
    let name = rustls::pki_types::ServerName::try_from(server.host.clone()).map_err(|e| format!("{} : {e}", server.host))?;
    let greeting = |e: std::io::Error| format!("{} : accueil IMAP : {e}", server.host);
    let tls = match server.security {
        Security::Tls => tls_connector().connect(name, tcp).await.map_err(|e| format!("{} : TLS : {e}", server.host))?,
        Security::StartTls => {
            let mut plain = async_imap::Client::new(tcp);
            plain.read_response().await.map_err(greeting)?.ok_or_else(|| format!("{} : connexion fermée avant l'accueil", server.host))?;
            plain
                .run_command_and_check_ok("STARTTLS", None)
                .await
                .map_err(|e| format!("{} : STARTTLS refusé ({e})", server.host))?;
            tls_connector().connect(name, plain.into_inner()).await.map_err(|e| format!("{} : TLS : {e}", server.host))?
        }
    };
    let mut client = async_imap::Client::new(tls);
    // Accueil du serveur (« * OK … ») avant toute commande ; aucun après STARTTLS (RFC 3501 § 6.2.1).
    if server.security == Security::Tls {
        client.read_response().await.map_err(greeting)?.ok_or_else(|| format!("{} : connexion fermée avant l'accueil", server.host))?;
    }
    client.login(username, password).await.map_err(|(e, _)| format!("{} : identification refusée ({e})", server.host))
}

/// Vérifie l'identification SMTP sans rien envoyer.
pub async fn smtp_check(server: &Server, username: &str, password: &str) -> Result<()> {
    use lettre::{AsyncSmtpTransport, Tokio1Executor, transport::smtp::authentication::Credentials};
    let builder = match server.security {
        Security::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&server.host),
        Security::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&server.host),
    }
    .map_err(|e| format!("{} : {e}", server.host))?;
    let transport: AsyncSmtpTransport<Tokio1Executor> = builder.port(server.port).credentials(Credentials::new(username.to_string(), password.to_string())).build();
    match transport.test_connection().await {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!("{} : le serveur SMTP ne répond pas", server.host)),
        Err(e) => Err(format!("{} : SMTP : {e}", server.host)),
    }
}

/// Envoi de la Boîte d'envoi, report des changements locaux, puis relève complète d'un compte, hors de tout runtime asynchrone (fil dédié) : ouvre sa propre connexion à la base,
/// lit le mot de passe dans le gestionnaire d'identifiants, puis note le résultat dans `account`.
pub fn sync_account_blocking(data_dir: &std::path::Path, source_id: i64) -> Result<fetch::SyncReport> {
    let conn = thunderlook_store::open(data_dir).map_err(|e| e.to_string())?;
    let account = account::account(&conn, source_id)?;
    let password = account::password(&account.address)?;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    let result = rt.block_on(async {
        let mut s = imap_login(&account.endpoints.imap, &account.endpoints.username, &password).await?;
        // Envois, puis changements locaux (dont la copie des envoyés), puis relève, qui ne doit pas les écraser.
        let (sent, send_error) = send::send_outbox(&conn, data_dir, &account, &password).await?;
        let (_, push_error) = push::push(&mut s, &conn, data_dir, &account).await?;
        let r = fetch::sync(&mut s, &conn, data_dir, &account).await.map(|mut r| {
            r.push_error = push_error;
            r.sent = sent;
            r.send_error = send_error;
            r
        });
        let _ = s.logout().await;
        r
    });
    account::set_status(&conn, source_id, result.as_ref().err().map(String::as_str))?;
    result
}

/// Arrêt d'une surveillance IDLE depuis un autre fil (compte retiré, « Travailler hors connexion ») : immédiat, même
/// au milieu d'une attente de 10 min.
#[derive(Default)]
pub struct WatchStop {
    stopped: std::sync::atomic::AtomicBool,
    /// Interrupteur de l'attente IDLE en cours (`StopSource` d'async-imap) : le lâcher termine l'attente.
    interrupt: std::sync::Mutex<Option<Box<dyn std::any::Any + Send>>>,
}

impl WatchStop {
    pub fn stop(&self) {
        self.stopped.store(true, std::sync::atomic::Ordering::SeqCst);
        let pending = self.interrupt.lock().map(|mut i| i.take()).unwrap_or(None);
        drop(pending);
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Garde l'interrupteur de l'attente qui commence ; s'il est déjà trop tard, l'attente est interrompue tout de suite.
    fn arm(&self, interrupt: Box<dyn std::any::Any + Send>) {
        if let Ok(mut slot) = self.interrupt.lock() {
            *slot = Some(interrupt);
        }
        if self.is_stopped() {
            self.stop();
        }
    }

    fn disarm(&self) {
        let pending = self.interrupt.lock().map(|mut i| i.take()).unwrap_or(None);
        drop(pending);
    }
}

/// Surveille la Boîte de réception du compte (IMAP IDLE) et appelle `on_new` à chaque changement signalé par le
/// serveur, et `on_error` à chaque coupure (sans secret). Rend la main quand `stop` passe à vrai (hors connexion),
/// quand `cancel` est arrêté (compte retiré : l'attente en cours est interrompue), ou quand le compte n'existe plus.
/// Les coupures sont reprises après une minute ; l'attente est relancée toutes les 10 min (le serveur peut couper un
/// IDLE trop long).
pub fn watch_inbox_blocking(
    data_dir: &std::path::Path,
    source_id: i64,
    stop: &std::sync::atomic::AtomicBool,
    cancel: &WatchStop,
    on_new: impl Fn(),
    on_error: impl Fn(&str),
) {
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    let stop = || stop.load(Ordering::SeqCst) || cancel.is_stopped();
    let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
    while !stop() {
        let account = match thunderlook_store::open(data_dir).map_err(|e| e.to_string()).and_then(|c| account::account(&c, source_id)) {
            Ok(a) => a,
            Err(_) => return,
        };
        let session: Result<()> = rt.block_on(async {
            let password = account::password(&account.address)?;
            let mut s = imap_login(&account.endpoints.imap, &account.endpoints.username, &password).await?;
            s.select("INBOX").await.map_err(|e| e.to_string())?;
            loop {
                if stop() {
                    let _ = s.logout().await;
                    return Ok(());
                }
                let mut idle = s.idle();
                idle.init().await.map_err(|e| e.to_string())?;
                let (wait, interrupt) = idle.wait_with_timeout(Duration::from_secs(10 * 60));
                cancel.arm(Box::new(interrupt));
                let response = wait.await.map_err(|e| e.to_string());
                cancel.disarm();
                let response = response?;
                s = idle.done().await.map_err(|e| e.to_string())?;
                if matches!(response, async_imap::extensions::idle::IdleResponse::NewData(_)) {
                    on_new();
                }
            }
        });
        match session {
            Ok(()) => return,
            Err(e) => on_error(&e),
        }
        for _ in 0..60 {
            if stop() {
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::{AtomicU32, Ordering}};

    /// Tient lieu de l'interrupteur d'async-imap : le lâcher interrompt l'attente.
    struct Probe(Arc<AtomicU32>);

    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Compte retiré pendant une attente IDLE : l'attente est interrompue tout de suite, et une attente qui commence
    /// après l'arrêt l'est aussi (pas de fenêtre de 10 min entre la vérification et l'attente).
    #[test]
    fn watch_stop_interrupts_the_current_and_late_waits() {
        let dropped = Arc::new(AtomicU32::new(0));
        let w = WatchStop::default();
        w.arm(Box::new(Probe(dropped.clone())));
        assert_eq!(dropped.load(Ordering::SeqCst), 0);
        w.stop();
        assert!(w.is_stopped());
        assert_eq!(dropped.load(Ordering::SeqCst), 1, "attente en cours interrompue");
        w.arm(Box::new(Probe(dropped.clone())));
        assert_eq!(dropped.load(Ordering::SeqCst), 2, "attente commencée trop tard : interrompue aussitôt");
    }
}
