//! Comptes de ThunderLook (J7) : connexion IMAP et SMTP. Plan : `notes/j7-plan.md`.
//!
//! TLS par rustls, avec les racines de confiance de Mozilla (`webpki-roots`) : aucun certificat n'est accepté sans
//! vérification. Les mots de passe ne passent que par les fonctions de connexion et n'apparaissent dans aucun message
//! d'erreur.

pub mod account;

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

/// Ouvre une session IMAP authentifiée (TLS implicite ; STARTTLS n'est pas encore pris en charge).
pub async fn imap_login(server: &Server, username: &str, password: &str) -> Result<ImapSession> {
    if server.security != Security::Tls {
        return Err(format!("{} : seule la connexion TLS (port 993) est prise en charge pour l'instant", server.host));
    }
    let tcp = TcpStream::connect((server.host.as_str(), server.port)).await.map_err(|e| format!("{}:{} : {e}", server.host, server.port))?;
    let name = rustls::pki_types::ServerName::try_from(server.host.clone()).map_err(|e| format!("{} : {e}", server.host))?;
    let tls = tls_connector().connect(name, tcp).await.map_err(|e| format!("{} : TLS : {e}", server.host))?;
    let mut client = async_imap::Client::new(tls);
    // Accueil du serveur (« * OK … ») avant toute commande.
    client
        .read_response()
        .await
        .map_err(|e| format!("{} : accueil IMAP : {e}", server.host))?
        .ok_or_else(|| format!("{} : connexion fermée avant l'accueil", server.host))?;
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
