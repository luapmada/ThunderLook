//! Configuration automatique d'un compte à partir de l'adresse (lot BJ). Rien n'est envoyé ni enregistré : on ne fait
//! que PROPOSER des serveurs, que l'utilisateur voit et corrige dans « Ajouter un compte » avant la vérification de
//! connexion habituelle.
//!
//! Ordre de recherche (le premier résultat valable l'emporte) :
//! 1. `https://autoconfig.<domaine>/mail/config-v1.1.xml` puis `https://<domaine>/.well-known/autoconfig/mail/config-v1.1.xml`
//!    (format de Mozilla), puis la base de Mozilla `https://autoconfig.thunderbird.net/v1.1/<domaine>` ;
//! 2. enregistrements MX du domaine : le domaine du serveur de courrier (ex. `mx1.hostinger.com` → `hostinger.com`)
//!    est reconnu (Hostinger) ou cherché à son tour dans la base de Mozilla ;
//! 3. enregistrements SRV de la RFC 6186 (`_imaps._tcp`, `_submissions._tcp`, `_imap._tcp`, `_submission._tcp`).
//!
//! Les requêtes DNS partent en UDP vers 1.1.1.1 puis 8.8.8.8 (pas de bibliothèque DNS dans ce crate, et pas de
//! résolveur système accessible de façon portable) : le domaine de l'adresse (jamais la partie locale ni le mot de
//! passe) est donc visible de ces résolveurs, comme il l'est des serveurs d'autoconfiguration de l'étape 1. Seul le
//! HTTPS est utilisé (certificats vérifiés par `tls_connector`) ; une connexion non chiffrée (`socketType` « plain »)
//! n'est jamais proposée.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};

use super::{Endpoints, Result, Security, Server, tls_connector};

/// Résultat de la détection : les serveurs proposés et d'où ils viennent (affiché à l'utilisateur).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub endpoints: Endpoints,
    pub source: String,
}

const TIMEOUT: Duration = Duration::from_secs(6);
const MAX_BODY: usize = 256 * 1024;
const RESOLVERS: [&str; 2] = ["1.1.1.1:53", "8.8.8.8:53"];

/// Cherche les serveurs d'une adresse. Erreur : message à afficher tel quel (saisie manuelle alors).
pub async fn detect(address: &str) -> Result<Detected> {
    let (local, domain) = split_address(address)?;
    let mut tried = Vec::new();

    let direct = [
        (format!("autoconfig.{domain}"), format!("/mail/config-v1.1.xml?emailaddress={}", url_encode(address)), format!("autoconfig.{domain}")),
        (domain.clone(), "/.well-known/autoconfig/mail/config-v1.1.xml".to_string(), format!("{domain} (.well-known)")),
        ("autoconfig.thunderbird.net".to_string(), format!("/v1.1/{domain}"), "base de Mozilla".to_string()),
    ];
    for (host, path, source) in direct {
        match fetch_config(&host, &path, &local, &domain).await {
            Ok(endpoints) => return Ok(Detected { endpoints, source }),
            Err(e) => tried.push(format!("{source} : {e}")),
        }
    }

    // MX : le fournisseur du courrier est souvent différent du domaine de l'adresse (domaine personnel hébergé).
    if let Ok(mut mx) = dns_query(&domain, QType::Mx).await {
        mx.sort();
        for (_, host) in mx.iter().filter_map(|r| r.as_mx()).map(|(p, h)| (p, h.to_string())) {
            let base = base_domain(&host);
            if base == "hostinger.com" {
                return Ok(Detected { endpoints: Endpoints::hostinger(&format!("{local}@{domain}")), source: format!("MX {host} (Hostinger)") });
            }
            if base == domain {
                continue;
            }
            let path = format!("/v1.1/{base}");
            match fetch_config("autoconfig.thunderbird.net", &path, &local, &domain).await {
                Ok(endpoints) => return Ok(Detected { endpoints, source: format!("MX {host} → base de Mozilla ({base})") }),
                Err(e) => tried.push(format!("MX {host} : {e}")),
            }
        }
    }

    // SRV (RFC 6186) : TLS d'abord, STARTTLS ensuite.
    let imap = srv(&domain, &[("_imaps._tcp", Security::Tls), ("_imap._tcp", Security::StartTls)]).await;
    let smtp = srv(&domain, &[("_submissions._tcp", Security::Tls), ("_submission._tcp", Security::StartTls)]).await;
    if let (Some(imap), Some(smtp)) = (imap, smtp) {
        return Ok(Detected { endpoints: Endpoints { imap, smtp, username: address.trim().to_string() }, source: "enregistrements SRV du domaine".into() });
    }
    tried.push("SRV : aucun couple IMAP/SMTP publié".into());

    Err(format!("Aucune configuration trouvée pour {domain} ({}). Saisissez les serveurs à la main.", tried.join(" ; ")))
}

async fn fetch_config(host: &str, path: &str, local: &str, domain: &str) -> Result<Endpoints> {
    let body = https_get(host, path).await?;
    let xml = String::from_utf8_lossy(&body);
    parse_client_config(&xml, local, domain)
}

async fn srv(domain: &str, kinds: &[(&str, Security)]) -> Option<Server> {
    for (prefix, security) in kinds {
        if let Ok(records) = dns_query(&format!("{prefix}.{domain}"), QType::Srv).await {
            let mut srvs: Vec<_> = records.iter().filter_map(|r| r.as_srv()).collect();
            srvs.sort_by_key(|(prio, _, _, _)| *prio);
            // Un enregistrement dont la cible est « . » signifie « service absent » (RFC 2782).
            if let Some((_, _, port, target)) = srvs.into_iter().find(|(_, _, port, t)| *port > 0 && !t.is_empty() && valid_host(t)) {
                return Some(Server { host: target.to_string(), port, security: *security });
            }
        }
    }
    None
}

// ───────────────────────── adresse et noms ─────────────────────────

fn split_address(address: &str) -> Result<(String, String)> {
    let a = address.trim();
    match a.rsplit_once('@') {
        Some((local, domain)) if !local.is_empty() && valid_host(&domain.to_ascii_lowercase()) && domain.contains('.') => {
            Ok((local.to_string(), domain.to_ascii_lowercase()))
        }
        _ => Err("Adresse de messagerie invalide.".into()),
    }
}

/// Nom d'hôte DNS plausible : étiquettes de lettres, chiffres et tirets séparées par des points. Sert à ne jamais
/// mettre dans une requête HTTP ou une connexion un nom venu d'un serveur distant sans l'avoir vérifié.
fn valid_host(h: &str) -> bool {
    !h.is_empty()
        && h.len() <= 253
        && h.split('.').all(|l| !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
}

/// Deux dernières étiquettes d'un nom d'hôte (`mx1.hostinger.com` → `hostinger.com`). Approximation : les suffixes à
/// deux niveaux (`co.uk`) donnent un domaine trop court, ce qui échoue simplement dans la base de Mozilla.
fn base_domain(host: &str) -> String {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    labels[labels.len().saturating_sub(2)..].join(".")
}

fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

// ───────────────────────── format de Mozilla ─────────────────────────

/// Extrait IMAP et SMTP d'un `clientConfig` de Mozilla. On préfère « SSL » à « STARTTLS » ; « plain » est refusé.
pub fn parse_client_config(xml: &str, local: &str, domain: &str) -> Result<Endpoints> {
    let imap = pick_server(xml, "incomingServer", "imap", local, domain)?;
    let smtp = pick_server(xml, "outgoingServer", "smtp", local, domain)?;
    // L'identifiant de connexion est celui du serveur entrant ; le SMTP utilise le même (cas général des comptes).
    Ok(Endpoints { imap: imap.0, smtp: smtp.0, username: imap.1 })
}

fn pick_server(xml: &str, tag: &str, kind: &str, local: &str, domain: &str) -> Result<(Server, String)> {
    let mut best: Option<(u8, Server, String)> = None;
    let mut found_kind = false;
    for block in blocks(xml, tag) {
        if attr(block.0, "type").as_deref() != Some(kind) {
            continue;
        }
        found_kind = true;
        let (Some(host), Some(port), Some(socket)) = (text(block.1, "hostname"), text(block.1, "port"), text(block.1, "socketType")) else { continue };
        let host = host.trim().to_ascii_lowercase();
        let (Ok(port), true) = (port.trim().parse::<u16>(), valid_host(&host)) else { continue };
        let (rank, security) = match socket.trim().to_ascii_uppercase().as_str() {
            "SSL" | "TLS" => (2, Security::Tls),
            "STARTTLS" => (1, Security::StartTls),
            _ => continue,
        };
        let user = text(block.1, "username").map(|u| expand(u.trim(), local, domain)).filter(|u| !u.is_empty()).unwrap_or_else(|| format!("{local}@{domain}"));
        if best.as_ref().is_none_or(|(r, _, _)| rank > *r) {
            best = Some((rank, Server { host, port, security }, user));
        }
    }
    best.map(|(_, s, u)| (s, u)).ok_or_else(|| {
        if found_kind { format!("serveur {kind} sans connexion chiffrée exploitable") } else { format!("pas de serveur {kind} dans la configuration") }
    })
}

fn expand(template: &str, local: &str, domain: &str) -> String {
    template.replace("%EMAILADDRESS%", &format!("{local}@{domain}")).replace("%EMAILLOCALPART%", local).replace("%EMAILDOMAIN%", domain)
}

/// Blocs `<tag …>…</tag>` : (attributs de l'ouverture, contenu). Analyse minimale, suffisante pour ce format.
fn blocks<'a>(xml: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i + open.len()..];
        // Évite de confondre `<incomingServer` avec un nom plus long.
        if !after.starts_with(|c: char| c.is_whitespace() || c == '>') {
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        let Some(end) = after[gt..].find(&close) else { break };
        out.push((&after[..gt], &after[gt + 1..gt + end]));
        rest = &after[gt + end + close.len()..];
    }
    out
}

fn attr(attrs: &str, name: &str) -> Option<String> {
    let i = attrs.find(&format!("{name}="))? + name.len() + 1;
    let q = attrs[i..].chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let v = &attrs[i + 1..];
    Some(v[..v.find(q)?].to_string())
}

fn text<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let i = block.find(&open)? + open.len();
    let j = block[i..].find(&format!("</{tag}>"))?;
    Some(&block[i..i + j])
}

// ───────────────────────── HTTPS minimal ─────────────────────────

async fn https_get(host: &str, path: &str) -> Result<Vec<u8>> {
    if !valid_host(host) {
        return Err("nom d'hôte invalide".into());
    }
    let work = async {
        let tcp = TcpStream::connect((host, 443)).await.map_err(|e| format!("connexion : {e}"))?;
        let name = tokio_rustls::rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|e| e.to_string())?;
        let mut tls = tls_connector().connect(name, tcp).await.map_err(|e| format!("TLS : {e}"))?;
        let req = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: ThunderLook\r\nAccept: text/xml, application/xml\r\nConnection: close\r\n\r\n");
        tls.write_all(req.as_bytes()).await.map_err(|e| e.to_string())?;
        let mut raw = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = tls.read(&mut buf).await.unwrap_or(0);
            if n == 0 || raw.len() + n > MAX_BODY {
                break;
            }
            raw.extend_from_slice(&buf[..n]);
        }
        parse_http(&raw)
    };
    tokio::time::timeout(TIMEOUT, work).await.map_err(|_| "délai dépassé".to_string())?
}

/// Corps d'une réponse HTTP/1.1 « 200 » (corps simple ou `chunked`). Les redirections ne sont pas suivies.
fn parse_http(raw: &[u8]) -> Result<Vec<u8>> {
    let sep = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("réponse HTTP incomplète")?;
    let head = String::from_utf8_lossy(&raw[..sep]).to_string();
    let status = head.lines().next().unwrap_or_default();
    if status.split_whitespace().nth(1) != Some("200") {
        return Err(format!("réponse « {} »", status.trim()));
    }
    let body = &raw[sep + 4..];
    if head.lines().any(|l| l.to_ascii_lowercase().replace(' ', "").starts_with("transfer-encoding:chunked")) {
        return dechunk(body);
    }
    Ok(body.to_vec())
}

fn dechunk(mut body: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let eol = body.windows(2).position(|w| w == b"\r\n").ok_or("corps « chunked » tronqué")?;
        let size_txt = String::from_utf8_lossy(&body[..eol]);
        let size = usize::from_str_radix(size_txt.split(';').next().unwrap_or("").trim(), 16).map_err(|_| "taille de bloc invalide")?;
        body = &body[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        if body.len() < size {
            // Réponse coupée par la limite de taille : on garde ce qui est lisible, l'analyse dira si c'est exploitable.
            out.extend_from_slice(body);
            return Ok(out);
        }
        out.extend_from_slice(&body[..size]);
        body = body.get(size + 2..).unwrap_or_default();
    }
}

// ───────────────────────── DNS (MX, SRV) ─────────────────────────

#[derive(Clone, Copy)]
enum QType {
    Mx = 15,
    Srv = 33,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Record {
    /// (priorité, serveur)
    Mx(u16, String),
    /// (priorité, poids, port, cible)
    Srv(u16, u16, u16, String),
}

impl Record {
    fn as_mx(&self) -> Option<(u16, &str)> {
        match self {
            Record::Mx(p, h) => Some((*p, h)),
            _ => None,
        }
    }
    fn as_srv(&self) -> Option<(u16, u16, u16, &str)> {
        match self {
            Record::Srv(p, w, port, t) => Some((*p, *w, *port, t)),
            _ => None,
        }
    }
}

async fn dns_query(name: &str, qtype: QType) -> Result<Vec<Record>> {
    let id = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos()) & 0xffff) as u16;
    let packet = build_query(id, name, qtype as u16)?;
    let mut last = "aucun résolveur joignable".to_string();
    for resolver in RESOLVERS {
        let attempt = async {
            let sock = UdpSocket::bind("0.0.0.0:0").await.map_err(|e| e.to_string())?;
            sock.send_to(&packet, resolver).await.map_err(|e| e.to_string())?;
            let mut buf = [0u8; 1500];
            let (n, _) = sock.recv_from(&mut buf).await.map_err(|e| e.to_string())?;
            parse_response(&buf[..n], id, qtype as u16)
        };
        match tokio::time::timeout(Duration::from_secs(3), attempt).await {
            Ok(Ok(r)) => return Ok(r),
            Ok(Err(e)) => last = e,
            Err(_) => last = format!("{resolver} : délai dépassé"),
        }
    }
    Err(last)
}

fn build_query(id: u16, name: &str, qtype: u16) -> Result<Vec<u8>> {
    let mut p = Vec::with_capacity(32);
    p.extend_from_slice(&id.to_be_bytes());
    p.extend_from_slice(&[0x01, 0x00]); // récursion souhaitée
    p.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]); // 1 question
    for label in name.trim_end_matches('.').split('.') {
        // Les étiquettes SRV commencent par « _ », que `valid_host` refuse : on vérifie le reste.
        let bare = label.trim_start_matches('_');
        if bare.is_empty() || label.len() > 63 || !bare.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err(format!("nom DNS invalide : {name}"));
        }
        p.push(label.len() as u8);
        p.extend_from_slice(label.as_bytes());
    }
    p.push(0);
    p.extend_from_slice(&qtype.to_be_bytes());
    p.extend_from_slice(&[0, 1]); // classe IN
    Ok(p)
}

fn parse_response(p: &[u8], id: u16, qtype: u16) -> Result<Vec<Record>> {
    let u16_at = |i: usize| p.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]])).ok_or_else(|| "réponse DNS tronquée".to_string());
    if u16_at(0)? != id {
        return Err("réponse DNS d'une autre requête".into());
    }
    let flags = u16_at(2)?;
    if flags & 0x8000 == 0 {
        return Err("ce n'est pas une réponse DNS".into());
    }
    if flags & 0x0200 != 0 {
        return Err("réponse DNS tronquée (UDP)".into());
    }
    match flags & 0xf {
        0 => {}
        3 => return Ok(Vec::new()), // NXDOMAIN : pas d'enregistrement
        c => return Err(format!("erreur DNS {c}")),
    }
    let (qd, an) = (u16_at(4)? as usize, u16_at(6)? as usize);
    let mut pos = 12;
    for _ in 0..qd {
        pos = skip_name(p, pos)? + 4;
    }
    let mut out = Vec::new();
    for _ in 0..an {
        pos = skip_name(p, pos)?;
        let (typ, rdlen) = (u16_at(pos)?, u16_at(pos + 8)? as usize);
        let rdata = pos + 10;
        pos = rdata + rdlen;
        if pos > p.len() {
            return Err("réponse DNS tronquée".into());
        }
        if typ != qtype {
            continue;
        }
        match typ {
            15 => out.push(Record::Mx(u16_at(rdata)?, read_name(p, rdata + 2)?.0)),
            33 => out.push(Record::Srv(u16_at(rdata)?, u16_at(rdata + 2)?, u16_at(rdata + 4)?, read_name(p, rdata + 6)?.0)),
            _ => {}
        }
    }
    Ok(out)
}

fn skip_name(p: &[u8], mut pos: usize) -> Result<usize> {
    loop {
        let len = *p.get(pos).ok_or("nom DNS tronqué")?;
        match len {
            0 => return Ok(pos + 1),
            l if l & 0xc0 == 0xc0 => return Ok(pos + 2),
            l => pos += 1 + l as usize,
        }
    }
}

/// Nom DNS à partir de `pos`, pointeurs de compression compris (nombre de sauts borné contre les boucles).
fn read_name(p: &[u8], mut pos: usize) -> Result<(String, usize)> {
    let mut labels = Vec::new();
    let mut end = None;
    for _ in 0..64 {
        let len = *p.get(pos).ok_or("nom DNS tronqué")? as usize;
        if len == 0 {
            return Ok((labels.join("."), end.unwrap_or(pos + 1)));
        }
        if len & 0xc0 == 0xc0 {
            let lo = *p.get(pos + 1).ok_or("nom DNS tronqué")? as usize;
            end.get_or_insert(pos + 2);
            pos = ((len & 0x3f) << 8) | lo;
            continue;
        }
        let label = p.get(pos + 1..pos + 1 + len).ok_or("nom DNS tronqué")?;
        labels.push(String::from_utf8_lossy(label).to_ascii_lowercase());
        pos += 1 + len;
    }
    Err("nom DNS en boucle".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0"?>
<clientConfig version="1.1">
  <emailProvider id="example.fr">
    <incomingServer type="pop3"><hostname>pop.example.fr</hostname><port>995</port><socketType>SSL</socketType><username>%EMAILADDRESS%</username></incomingServer>
    <incomingServer type="imap">
      <hostname>imap.example.fr</hostname><port>143</port><socketType>STARTTLS</socketType><username>%EMAILLOCALPART%</username>
    </incomingServer>
    <incomingServer type="imap">
      <hostname>IMAP.example.fr</hostname><port>993</port><socketType>SSL</socketType><username>%EMAILLOCALPART%</username>
    </incomingServer>
    <outgoingServer type="smtp"><hostname>smtp.example.fr</hostname><port>25</port><socketType>plain</socketType></outgoingServer>
    <outgoingServer type="smtp"><hostname>smtp.example.fr</hostname><port>587</port><socketType>STARTTLS</socketType></outgoingServer>
  </emailProvider>
</clientConfig>"#;

    #[test]
    fn mozilla_prefers_ssl_refuses_plain_expands_username() {
        let e = parse_client_config(XML, "paul", "example.fr").unwrap();
        assert_eq!(e.imap, Server { host: "imap.example.fr".into(), port: 993, security: Security::Tls });
        assert_eq!(e.smtp, Server { host: "smtp.example.fr".into(), port: 587, security: Security::StartTls });
        assert_eq!(e.username, "paul");
    }

    #[test]
    fn mozilla_without_encryption_or_server_is_an_error() {
        let plain = r#"<incomingServer type="imap"><hostname>a.b</hostname><port>143</port><socketType>plain</socketType></incomingServer>
            <outgoingServer type="smtp"><hostname>a.b</hostname><port>587</port><socketType>STARTTLS</socketType></outgoingServer>"#;
        assert!(parse_client_config(plain, "x", "a.b").unwrap_err().contains("sans connexion chiffrée"));
        assert!(parse_client_config("<clientConfig/>", "x", "a.b").unwrap_err().contains("pas de serveur imap"));
    }

    #[test]
    fn mozilla_rejects_hostile_hostname() {
        let bad = r#"<incomingServer type="imap"><hostname>a.b/../x y</hostname><port>993</port><socketType>SSL</socketType></incomingServer>
            <outgoingServer type="smtp"><hostname>s.b</hostname><port>465</port><socketType>SSL</socketType></outgoingServer>"#;
        assert!(parse_client_config(bad, "x", "a.b").is_err());
    }

    #[test]
    fn username_defaults_to_full_address() {
        let x = r#"<incomingServer type="imap"><hostname>i.b</hostname><port>993</port><socketType>SSL</socketType></incomingServer>
            <outgoingServer type="smtp"><hostname>s.b</hostname><port>465</port><socketType>SSL</socketType></outgoingServer>"#;
        assert_eq!(parse_client_config(x, "paul", "b.fr").unwrap().username, "paul@b.fr");
    }

    #[test]
    fn address_and_names() {
        assert_eq!(split_address(" Paul@Exemple.FR ").unwrap(), ("Paul".into(), "exemple.fr".into()));
        assert!(split_address("pas-une-adresse").is_err());
        assert!(split_address("a@localhost").is_err());
        assert!(split_address("a@b c.fr").is_err());
        assert_eq!(base_domain("mx1.hostinger.com."), "hostinger.com");
        assert_eq!(url_encode("a+b@c.fr"), "a%2Bb%40c.fr");
        assert!(valid_host("imap.example.fr") && !valid_host("-x.fr") && !valid_host("a..fr") && !valid_host(""));
    }

    #[test]
    fn http_plain_and_chunked() {
        assert_eq!(parse_http(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").unwrap(), b"ok");
        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nabcd\r\n3;x=1\r\nefg\r\n0\r\n\r\n";
        assert_eq!(parse_http(chunked).unwrap(), b"abcdefg");
        assert!(parse_http(b"HTTP/1.1 404 Not Found\r\n\r\n").unwrap_err().contains("404"));
        assert!(parse_http(b"HTTP/1.1 301 Moved\r\nLocation: http://x\r\n\r\n").is_err());
    }

    #[test]
    fn dns_query_layout() {
        let q = build_query(0x1234, "_imaps._tcp.example.fr", 33).unwrap();
        assert_eq!(&q[..4], &[0x12, 0x34, 0x01, 0x00]);
        assert_eq!(&q[12..20], b"\x06_imaps\x04");
        assert_eq!(&q[q.len() - 4..], &[0, 33, 0, 1]);
        assert!(build_query(1, "a b.fr", 15).is_err());
    }

    /// Réponse MX avec nom compressé (pointeur vers la question).
    #[test]
    fn dns_mx_with_compression() {
        let mut p = vec![0xab, 0xcd, 0x81, 0x80, 0, 1, 0, 2, 0, 0, 0, 0];
        p.extend_from_slice(b"\x07example\x02fr\x00\x00\x0f\x00\x01"); // question à l'offset 12
        for (prio, host) in [(20u16, &b"\x03mx2\xc0\x0c"[..]), (10, &b"\x03mx1\xc0\x0c"[..])] {
            p.extend_from_slice(&[0xc0, 0x0c, 0, 15, 0, 1, 0, 0, 1, 0]);
            p.extend_from_slice(&((host.len() + 2) as u16).to_be_bytes());
            p.extend_from_slice(&prio.to_be_bytes());
            p.extend_from_slice(host);
        }
        let mut r = parse_response(&p, 0xabcd, 15).unwrap();
        r.sort();
        assert_eq!(r, vec![Record::Mx(10, "mx1.example.fr".into()), Record::Mx(20, "mx2.example.fr".into())]);
        assert_eq!(r[0].as_mx(), Some((10, "mx1.example.fr")));
    }

    #[test]
    fn dns_srv_nxdomain_truncated_and_loop() {
        let mut p = vec![0, 7, 0x81, 0x80, 0, 1, 0, 1, 0, 0, 0, 0];
        p.extend_from_slice(b"\x06_imaps\x04_tcp\x01a\x02fr\x00\x00\x21\x00\x01");
        p.extend_from_slice(&[0xc0, 0x0c, 0, 33, 0, 1, 0, 0, 0, 60, 0, 0]);
        let rdata: &[u8] = b"\x00\x01\x00\x05\x03\xe1\x04mail\x01a\x02fr\x00";
        let at = p.len() - 2;
        p[at..at + 2].copy_from_slice(&(rdata.len() as u16).to_be_bytes());
        p.extend_from_slice(rdata);
        assert_eq!(parse_response(&p, 7, 33).unwrap(), vec![Record::Srv(1, 5, 993, "mail.a.fr".into())]);
        assert!(parse_response(&p, 8, 33).is_err()); // mauvais identifiant
        assert!(parse_response(&p[..p.len() - 5], 7, 33).is_err()); // tronquée
        let nx = [0, 7, 0x81, 0x83, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(parse_response(&nx, 7, 33).unwrap().is_empty());
        // pointeur qui boucle sur lui-même
        assert!(read_name(&[0xc0, 0x00], 0).is_err());
    }
}
