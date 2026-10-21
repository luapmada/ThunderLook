//! Comptes : un compte est une source `imap` avec ses dossiers ; ses paramètres sont dans `account` (schéma v8), son
//! mot de passe dans le gestionnaire d'identifiants de Windows (service « ThunderLook », nom d'utilisateur = adresse).

use std::path::Path;

use thunderlook_model::AccountSummary;
use thunderlook_store::rusqlite::{Connection, OptionalExtension, Row, params};

use crate::{Endpoints, Result, Security, Server};

const KEYRING_SERVICE: &str = "ThunderLook";

fn entry(address: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, &address.to_lowercase()).map_err(|e| format!("gestionnaire d'identifiants : {e}"))
}

pub fn store_password(address: &str, password: &str) -> Result<()> {
    entry(address)?.set_password(password).map_err(|e| format!("gestionnaire d'identifiants : {e}"))
}

pub fn password(address: &str) -> Result<String> {
    entry(address)?.get_password().map_err(|e| format!("mot de passe de {address} introuvable dans le gestionnaire d'identifiants ({e})"))
}

pub fn forget_password(address: &str) -> Result<()> {
    match entry(address)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("gestionnaire d'identifiants : {e}")),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// `account.id` (clé des files `imap_pending` et `outbox`).
    pub id: i64,
    pub source_id: i64,
    pub address: String,
    pub sender_name: String,
    pub endpoints: Endpoints,
    /// Copie des messages envoyés dans « Éléments envoyés » du serveur.
    pub save_sent: bool,
    /// Règles actives appliquées aux nouveaux messages de la Boîte de réception.
    pub rules_on_arrival: bool,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

fn security_name(s: Security) -> &'static str {
    match s {
        Security::Tls => "tls",
        Security::StartTls => "starttls",
    }
}

fn security_of(s: &str) -> thunderlook_store::rusqlite::Result<Security> {
    match s {
        "tls" => Ok(Security::Tls),
        "starttls" => Ok(Security::StartTls),
        other => Err(thunderlook_store::rusqlite::Error::InvalidColumnType(0, format!("sécurité « {other} » non prise en charge"), thunderlook_store::rusqlite::types::Type::Text)),
    }
}

const SELECT: &str = "SELECT id, source_id, name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security,
    save_sent, last_sync, last_error, rules_on_arrival FROM account";

fn row(r: &Row) -> thunderlook_store::rusqlite::Result<(Account, Option<i64>, Option<String>)> {
    let endpoints = Endpoints {
        username: r.get(4)?,
        imap: Server { host: r.get(5)?, port: r.get(6)?, security: security_of(&r.get::<_, String>(7)?)? },
        smtp: Server { host: r.get(8)?, port: r.get(9)?, security: security_of(&r.get::<_, String>(10)?)? },
    };
    let a = Account {
        id: r.get(0)?,
        source_id: r.get(1)?,
        sender_name: r.get(2)?,
        address: r.get(3)?,
        endpoints,
        save_sent: r.get(11)?,
        rules_on_arrival: r.get(14)?,
    };
    Ok((a, r.get(12)?, r.get(13)?))
}

pub fn accounts(conn: &Connection) -> Result<Vec<Account>> {
    let mut stmt = conn.prepare(&format!("{SELECT} ORDER BY id")).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], row).map_err(|e| e.to_string())?;
    rows.map(|r| r.map(|x| x.0).map_err(|e| e.to_string())).collect()
}

/// Compte d'une source.
pub fn account(conn: &Connection, source_id: i64) -> Result<Account> {
    conn.query_row(&format!("{SELECT} WHERE source_id = ?1"), [source_id], row)
        .optional()
        .map_err(|e| e.to_string())?
        .map(|x| x.0)
        .ok_or_else(|| format!("compte introuvable (source {source_id})"))
}

/// Pour la boîte « Paramètres du compte ».
pub fn summaries(conn: &Connection) -> Result<Vec<AccountSummary>> {
    use chrono::TimeZone;
    let mut stmt = conn.prepare(&format!("{SELECT} ORDER BY id")).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], row).map_err(|e| e.to_string())?;
    rows.map(|r| {
        let (a, last_sync, last_error) = r.map_err(|e| e.to_string())?;
        Ok(AccountSummary {
            id: format!("s{}", a.source_id),
            imap: format!("{}:{}", a.endpoints.imap.host, a.endpoints.imap.port),
            smtp: format!("{}:{}", a.endpoints.smtp.host, a.endpoints.smtp.port),
            address: a.address,
            sender_name: a.sender_name,
            last_sync: last_sync.and_then(|t| chrono::Local.timestamp_millis_opt(t).single()).map(|d| d.format("%d/%m/%Y %H:%M").to_string()),
            last_error,
            rules_on_arrival: a.rules_on_arrival,
        })
    })
    .collect()
}

/// Crée le compte : source, dossiers de base (racine, haut du compte, Boîte d'envoi locale), paramètres, puis mot
/// de passe. La connexion doit avoir été vérifiée avant (`imap_login`, `smtp_check`). Renvoie l'identifiant de la source.
pub fn create(conn: &Connection, address: &str, sender_name: &str, endpoints: &Endpoints, password: &str) -> Result<i64> {
    let address = address.trim();
    let exists = conn
        .query_row("SELECT 1 FROM account WHERE email = ?1 COLLATE NOCASE", [address], |_| Ok(()))
        .optional()
        .map_err(|e| e.to_string())?
        .is_some();
    if exists {
        return Err(format!("le compte {address} existe déjà"));
    }
    let now = now_ms();
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO source (kind, origin_path, work_path, display_name, size, imported_at) VALUES ('imap', ?1, '', ?2, 0, ?3)",
        params![format!("imap:{address}"), address, now],
    )
    .map_err(|e| e.to_string())?;
    let source = tx.last_insert_rowid();
    let folder = |parent: Option<i64>, name: &str, role: &str, position: i64| -> Result<i64> {
        tx.execute(
            "INSERT INTO folder (source_id, parent_id, nid, name, role, container_class, hidden, position) VALUES (?1, ?2, 0, ?3, ?4, 'IPF.Note', 0, ?5)",
            params![source, parent, name, role, position],
        )
        .map_err(|e| e.to_string())?;
        Ok(tx.last_insert_rowid())
    };
    let root = folder(None, "", "root", 0)?;
    let top = folder(Some(root), address, "ipm", 0)?;
    folder(Some(top), "Boîte d'envoi", "outbox", 90)?;
    let (i, s) = (&endpoints.imap, &endpoints.smtp);
    tx.execute(
        "INSERT INTO account (source_id, name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security,
            save_sent, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11)",
        params![
            source,
            sender_name.trim(),
            address,
            endpoints.username,
            i.host,
            i.port,
            security_name(i.security),
            s.host,
            s.port,
            security_name(s.security),
            now
        ],
    )
    .map_err(|e| e.to_string())?;
    store_password(address, password)?;
    tx.commit().map_err(|e| {
        let _ = forget_password(address);
        e.to_string()
    })?;
    Ok(source)
}

/// Retire le compte de ThunderLook (copie locale et mot de passe) ; rien n'est effacé sur le serveur.
pub fn remove(conn: &Connection, data_dir: &Path, source_id: i64) -> Result<()> {
    let a = account(conn, source_id)?;
    thunderlook_store::delete_source(conn, data_dir, source_id).map_err(|e| e.to_string())?;
    forget_password(&a.address)
}

/// Active ou désactive les règles à l'arrivée pour un compte.
pub fn set_rules_on_arrival(conn: &Connection, source_id: i64, on: bool) -> Result<()> {
    let n = conn.execute("UPDATE account SET rules_on_arrival = ?2 WHERE source_id = ?1", params![source_id, on]).map_err(|e| e.to_string())?;
    if n == 0 { Err(format!("compte introuvable (source {source_id})")) } else { Ok(()) }
}

/// Relève réussie (`error` : `None`) ou échouée.
pub fn set_status(conn: &Connection, source_id: i64, error: Option<&str>) -> Result<()> {
    match error {
        None => conn.execute("UPDATE account SET last_sync = ?2, last_error = NULL WHERE source_id = ?1", params![source_id, now_ms()]),
        Some(e) => conn.execute("UPDATE account SET last_error = ?2 WHERE source_id = ?1", params![source_id, e]),
    }
    .map(|_| ())
    .map_err(|e| e.to_string())
}
