//! Rédaction (J5) : nouveau message, réponse, réponse à tous, transfert, signatures d'Outlook, brouillons.
//!
//! La citation reprend ce qu'écrit Outlook 2021 en français (relevé sur un message envoyé de la copie) :
//! objet préfixé de « RE: » ou « TR: », filet gris, puis « De : », « Envoyé : », « À : », « Cc : »,
//! « Objet : » en gras, et le message d'origine sans retrait. Un brouillon est un `.eml` du dossier
//! Brouillons de la source, marqué `X-Unsent: 1`, avec sa ligne dans la base (non envoyé, lu).

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::Local;
use mail_builder::{
    MessageBuilder,
    headers::{address::Address, raw::Raw},
    mime::{BodyPart, MimePart},
};
use mail_parser::{Addr, Message, MessageParser};
use thunderlook_model::{ComposeKind, Draft, DraftAttachment, DraftAttachmentSource, Signature, format};
use thunderlook_store::rusqlite::{OptionalExtension, params};

use crate::{
    Mailbox, Result, err, local,
    message::{attachment_parts, content_id, find_ci, inline_cids, mime_of, part_name},
    message_db_id, normalized_subject,
};

/// `MSGFLAG_READ | MSGFLAG_UNSENT` ([MS-OXCMSG] `PidTagMessageFlags`).
const DRAFT_FLAGS: i64 = 0x1 | 0x8;

/// Une adresse : nom affiché (facultatif) et adresse électronique.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailAddress {
    pub name: Option<String>,
    pub addr: String,
}

/// Analyse une liste saisie : « Nom <adr>; adr2, "Nom, Prénom" <adr3> ». Les entrées sans `@` sont ignorées.
pub fn parse_addresses(s: &str) -> Vec<MailAddress> {
    let mut out = Vec::new();
    let (mut cur, mut quoted, mut angle) = (String::new(), false, false);
    let mut flush = |cur: &mut String| {
        let item = cur.trim();
        if !item.is_empty() {
            let (name, addr) = match (item.rfind('<'), item.rfind('>')) {
                (Some(a), Some(b)) if a < b => (item[..a].trim().trim_matches('"').trim(), item[a + 1..b].trim()),
                _ => ("", item.trim_matches(['"', '\''])),
            };
            if addr.contains('@') {
                out.push(MailAddress { name: (!name.is_empty() && name != addr).then(|| name.to_string()), addr: addr.to_string() });
            }
        }
        cur.clear();
    };
    for c in s.chars() {
        match c {
            '"' if !angle => quoted = !quoted,
            '<' if !quoted => angle = true,
            '>' if !quoted => angle = false,
            ';' | ',' if !quoted && !angle => {
                flush(&mut cur);
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    flush(&mut cur);
    out
}

/// « Nom <adr> » ; le nom est mis entre guillemets s'il contient une virgule ou un point-virgule.
pub fn format_address(m: &MailAddress) -> String {
    match &m.name {
        Some(n) if n.contains([',', ';', '<', '>']) => format!("\"{}\" <{}>", n.replace('"', ""), m.addr),
        Some(n) => format!("{n} <{}>", m.addr),
        None => m.addr.clone(),
    }
}

fn format_list(list: &[MailAddress]) -> String {
    list.iter().map(format_address).collect::<Vec<_>>().join("; ")
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Adresses d'un en-tête analysé (`From`, `To`…), groupes compris.
fn header_addresses(addr: Option<&mail_parser::Address>) -> Vec<MailAddress> {
    let mut out = Vec::new();
    let mut push = |a: &Addr| {
        if let Some(address) = a.address().filter(|x| x.contains('@')) {
            out.push(MailAddress { name: a.name().map(str::trim).filter(|n| !n.is_empty()).map(String::from), addr: address.to_string() });
        }
    };
    match addr {
        Some(mail_parser::Address::List(list)) => list.iter().for_each(&mut push),
        Some(mail_parser::Address::Group(groups)) => groups.iter().flat_map(|g| g.addresses.iter()).for_each(&mut push),
        None => {}
    }
    out
}

/// Ligne du bloc cité : « Nom <adr>; Nom2 <adr2> », échappée pour le HTML.
fn quote_list(list: &[MailAddress]) -> String {
    escape(&list.iter().map(|m| match &m.name { Some(n) => format!("{n} <{}>", m.addr), None => m.addr.clone() }).collect::<Vec<_>>().join("; "))
}

/// Contenu de `<body>` et feuilles de style `<style>` d'un document HTML (tout le document s'il n'a pas de `<body>`).
pub(crate) fn split_html(html: &str) -> (String, String) {
    let mut styles = String::new();
    let mut pos = 0;
    while let Some(a) = find_ci(html, "<style", pos) {
        let Some(open_end) = html[a..].find('>').map(|i| a + i + 1) else { break };
        let Some(close) = find_ci(html, "</style>", open_end) else { break };
        styles.push_str(&html[open_end..close]);
        styles.push('\n');
        pos = close + "</style>".len();
    }
    let body = match find_ci(html, "<body", 0).and_then(|b| html[b..].find('>').map(|i| b + i + 1)) {
        Some(start) => {
            let end = find_ci(html, "</body>", start).unwrap_or(html.len());
            html[start..end].to_string()
        }
        None => html.to_string(),
    };
    (body, styles)
}

/// Texte brut mis en HTML (paragraphes et sauts de ligne).
pub(crate) fn text_to_html_pub(text: &str) -> String {
    text_to_html(text)
}

fn text_to_html(text: &str) -> String {
    escape(text).replace("\r\n", "\n").split('\n').map(|l| if l.is_empty() { "<br>".to_string() } else { format!("{l}<br>") }).collect()
}

/// Préfixe d'objet, comme Outlook 2021 en français (relevé sur la copie : une réponse à « Re: x » donne
/// « RE: x » ; les préfixes intérieurs restent, d'où « RE: TR: x » et « TR: Fw: x ») :
/// - réponse : un premier préfixe « re: » (toute casse, espace facultative) devient « RE: », sinon il est ajouté ;
/// - transfert : « TR: » est toujours ajouté.
fn prefixed_subject(kind: ComposeKind, subject: &str) -> String {
    let s = subject.trim();
    match kind {
        ComposeKind::Reply | ComposeKind::ReplyAll => {
            let rest = s
                .get(..2)
                .filter(|p| p.eq_ignore_ascii_case("re"))
                .and_then(|_| s[2..].trim_start_matches(' ').strip_prefix(':'))
                .map(str::trim_start);
            format!("RE: {}", rest.unwrap_or(s))
        }
        ComposeKind::Forward => format!("TR: {s}"),
        ComposeKind::New => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_prefixes() {
        let re = |s| prefixed_subject(ComposeKind::Reply, s);
        assert_eq!(re("vos réponses"), "RE: vos réponses");
        assert_eq!(re("Re: vos réponses SAGE>ACD"), "RE: vos réponses SAGE>ACD");
        assert_eq!(re("RE : point"), "RE: point");
        assert_eq!(re("TR: devis"), "RE: TR: devis");
        assert_eq!(re("Relance"), "RE: Relance");
        assert_eq!(prefixed_subject(ComposeKind::Forward, "Fw: x"), "TR: Fw: x");
    }
}

/// 16 octets imprévisibles d'un appel à l'autre (identifiants de message et de conversation).
fn unique_bytes() -> [u8; 16] {
    use md5::{Digest, Md5};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut h = Md5::new();
    h.update(std::process::id().to_le_bytes());
    h.update(COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    h.update(Local::now().timestamp_nanos_opt().unwrap_or_default().to_le_bytes());
    h.update(format!("{:p}", &h).as_bytes());
    h.finalize().into()
}

/// FILETIME (centaines de ns depuis 1601) de maintenant.
fn filetime_now() -> u64 {
    let unix_100ns = Local::now().timestamp_nanos_opt().unwrap_or_default() as u64 / 100;
    unix_100ns + 116_444_736_000_000_000
}

/// `PidTagConversationIndex` ([MS-OXOMSG] §2.2.1.3) : nouvel en-tête (premier message), ou en-tête du parent
/// suivi d'un bloc enfant de 5 octets (écart de temps sur 31 bits, puis 8 bits imprévisibles).
fn conversation_index(parent: Option<&[u8]>) -> Vec<u8> {
    let now = filetime_now();
    match parent.filter(|p| p.len() >= 22 && p[0] == 0x01) {
        Some(p) => {
            // En-tête : 5 octets de poids fort du FILETIME (les 3 octets de poids faible sont à zéro).
            let mut ft = [0u8; 8];
            ft[3..8].copy_from_slice(&p[1..6]);
            let header_time = u64::from_be_bytes(ft);
            let delta = now.saturating_sub(header_time);
            // DC = 0 : bits 18 à 48 de l'écart.
            let code = ((delta >> 18) as u32) & 0x7FFF_FFFF;
            let mut out = p.to_vec();
            out.extend_from_slice(&code.to_be_bytes());
            out.push(unique_bytes()[0]);
            out
        }
        None => {
            let mut out = vec![0x01];
            out.extend_from_slice(&now.to_be_bytes()[0..5]);
            out.extend_from_slice(&unique_bytes());
            out
        }
    }
}

/// Dossier des signatures d'Outlook : `THUNDERLOOK_SIGNATURES`, sinon `%APPDATA%\Microsoft\Signatures`.
fn signatures_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("THUNDERLOOK_SIGNATURES") {
        return Some(PathBuf::from(d));
    }
    std::env::var_os("APPDATA").map(|a| Path::new(&a).join("Microsoft").join("Signatures"))
}

/// Décodage d'un `.htm` de Word d'après son `charset` (windows-1252 par défaut, comme Outlook l'écrit).
fn decode_htm(bytes: &[u8]) -> String {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_ascii_lowercase();
    let label = head.split("charset=").nth(1).map(|r| r.trim_start_matches(['"', '\'']).chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect::<String>());
    let enc = label.as_deref().and_then(|l| encoding_rs::Encoding::for_label(l.as_bytes())).unwrap_or(encoding_rs::WINDOWS_1252);
    enc.decode(bytes).0.into_owned()
}

/// Décodage des `%XX` d'un chemin relatif (Word encode les noms de dossiers d'images).
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn image_mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    }
}

/// Images locales d'une signature (`src="Nom_fichiers/image001.png"`) remplacées par des URI `data:`.
/// Une image introuvable garde son chemin (elle s'affichera cassée, rien n'est perdu en silence).
fn embed_local_images(html: &str, base: &Path) -> String {
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;
    while let Some(i) = find_ci(html, "src=\"", pos) {
        let start = i + 5;
        out.push_str(&html[pos..start]);
        let end = html[start..].find('"').map_or(html.len(), |n| start + n);
        let src = &html[start..end];
        let local = !src.contains(':') && !src.starts_with("//");
        match local.then(|| base.join(percent_decode(src).replace('/', std::path::MAIN_SEPARATOR_STR))).and_then(|p| fs::read(&p).ok().map(|b| (p, b))) {
            Some((p, bytes)) => out.push_str(&format!("data:{};base64,{}", image_mime(&p), STANDARD.encode(bytes))),
            None => out.push_str(src),
        }
        pos = end;
    }
    out.push_str(&html[pos..]);
    out
}

/// Signatures `.htm` d'Outlook, triées par nom.
pub fn signatures() -> Vec<Signature> {
    let Some(dir) = signatures_dir() else { return Vec::new() };
    let Ok(entries) = fs::read_dir(&dir) else { return Vec::new() };
    let mut out: Vec<Signature> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("htm")))
        .filter_map(|p| {
            let name = p.file_stem()?.to_string_lossy().into_owned();
            let (body, _) = split_html(&decode_htm(&fs::read(&p).ok()?));
            Some(Signature { name, html: embed_local_images(&body, &dir) })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Signature proposée par défaut : celle dont le nom contient « (adresse) » d'une des adresses du compte.
/// Aucune n'est enregistrée comme signature par défaut dans le registre de la référence (relevé le 2026-10-19).
pub fn default_signature(all: &[Signature], mine: &[String]) -> Option<Signature> {
    all.iter().find(|s| mine.iter().any(|a| s.name.to_ascii_lowercase().contains(&format!("({})", a.to_ascii_lowercase())))).cloned()
}
/// Message d'origine d'une réponse ou d'un transfert, lu dans la base et son `.eml`.
struct Original {
    id: i64,
    source: i64,
    subject: String,
    sent: Option<i64>,
    from: Vec<MailAddress>,
    reply_to: Vec<MailAddress>,
    to: Vec<MailAddress>,
    cc: Vec<MailAddress>,
    body: String,
    styles: String,
    /// Pièces jointes au sens de l'interface (index du volet de lecture), hors images intégrées au corps.
    attachments: Vec<DraftAttachment>,
}

impl Mailbox {
    /// Adresses du compte d'une source : expéditeurs de ses dossiers « Éléments envoyés », du plus fréquent au moins.
    pub fn my_addresses(&self, source: i64) -> Result<Vec<String>> {
        if let Some((_, addr)) = &self.send_as {
            return Ok(vec![addr.to_ascii_lowercase()]);
        }
        let account: Option<String> =
            self.conn().query_row("SELECT lower(email) FROM account WHERE source_id = ?1", [source], |r| r.get(0)).optional().map_err(err)?;
        if let Some(a) = account {
            return Ok(vec![a]);
        }
        let mut stmt = self
            .conn()
            .prepare_cached(
                "SELECT lower(m.from_addr), COUNT(*) n FROM message m JOIN folder f ON f.id = m.folder_id
                 WHERE f.source_id = ?1 AND f.role = 'sent' AND m.from_addr LIKE '%@%' GROUP BY 1 ORDER BY n DESC LIMIT 20",
            )
            .map_err(err)?;
        stmt.query_map([source], |r| r.get(0)).map_err(err)?.collect::<std::result::Result<_, _>>().map_err(err)
    }

    /// Nom affiché le plus fréquent pour l'adresse principale du compte.
    fn my_name(&self, source: i64, addr: &str) -> Result<Option<String>> {
        if let Some((name, _)) = &self.send_as {
            return Ok(Some(name.clone()).filter(|n| !n.is_empty()));
        }
        let account: Option<String> = self
            .conn()
            .query_row("SELECT name FROM account WHERE source_id = ?1 AND name <> ''", [source], |r| r.get(0))
            .optional()
            .map_err(err)?;
        if account.is_some() {
            return Ok(account);
        }
        self.conn()
            .query_row(
                "SELECT m.from_name FROM message m JOIN folder f ON f.id = m.folder_id
                 WHERE f.source_id = ?1 AND f.role = 'sent' AND lower(m.from_addr) = ?2 AND m.from_name <> ''
                 GROUP BY m.from_name ORDER BY COUNT(*) DESC LIMIT 1",
                params![source, addr.to_ascii_lowercase()],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)
    }

    /// Source par défaut d'un nouveau message : celle de la première Boîte de réception.
    fn default_source(&self) -> Result<i64> {
        self.conn()
            .query_row("SELECT source_id FROM folder WHERE role = 'inbox' ORDER BY source_id, id LIMIT 1", [], |r| r.get(0))
            .optional()
            .map_err(err)?
            .ok_or_else(|| "aucun fichier de données ouvert".to_string())
    }

    fn original(&self, id: &str) -> Result<Original> {
        let db_id = message_db_id(id)?;
        let row = self.row(db_id)?;
        let bytes = self.eml(&row)?;
        let parsed: Message = MessageParser::default().parse(&bytes).ok_or("fichier .eml illisible")?;
        // Corps : HTML (images intégrées en data:), sinon texte mis en HTML.
        let mut cids = HashMap::new();
        for p in parsed.attachments() {
            if let Some(cid) = content_id(p) {
                cids.insert(cid.to_ascii_lowercase(), format!("data:{};base64,{}", mime_of(p), STANDARD.encode(p.contents())));
            }
        }
        let (body, styles, used) = match parsed.html_part(0).filter(|p| p.is_text_html()).and_then(|p| p.text_contents()) {
            Some(html) => {
                let (inlined, used) = inline_cids(html, &cids);
                let (body, styles) = split_html(&inlined);
                (body, styles, used)
            }
            None => (parsed.body_text(0).map(|t| text_to_html(&t)).unwrap_or_default(), String::new(), Vec::new()),
        };
        let attachments = attachment_parts(&parsed)
            .enumerate()
            .filter(|(_, p)| !content_id(p).is_some_and(|c| used.contains(&c.to_ascii_lowercase())))
            .map(|(i, p)| DraftAttachment {
                name: part_name(p, i),
                size_label: format::size(p.contents().len() as u64),
                source: DraftAttachmentSource::Message { id: id.to_string(), index: i as u32 },
            })
            .collect();
        let mut from = header_addresses(parsed.from());
        if from.is_empty() && row.from_addr.contains('@') {
            from.push(MailAddress { name: (!row.from_name.is_empty()).then(|| row.from_name.clone()), addr: row.from_addr.clone() });
        }
        Ok(Original {
            id: db_id,
            source: row.source,
            subject: row.subject.clone(),
            sent: row.sent.or(row.received),
            from,
            reply_to: header_addresses(parsed.reply_to()),
            to: header_addresses(parsed.to()),
            cc: header_addresses(parsed.cc()),
            body,
            styles,
            attachments,
        })
    }

    /// Prépare un message : destinataires, objet, signature par défaut et, pour une réponse ou un transfert,
    /// bloc cité et message d'origine. Rien n'est enregistré.
    pub fn compose(&self, kind: ComposeKind, source_id: Option<&str>) -> Result<Draft> {
        let original = match (kind, source_id) {
            (ComposeKind::New, _) | (_, None) => None,
            (_, Some(id)) => Some(self.original(id)?),
        };
        let source = match &original {
            Some(o) => o.source,
            None => self.default_source()?,
        };
        let mine = self.my_addresses(source)?;
        let is_mine = |m: &MailAddress| mine.iter().any(|a| a.eq_ignore_ascii_case(&m.addr));
        let signature = default_signature(&signatures(), &mine).map(|s| s.html).unwrap_or_default();
        // Ligne vide où écrire, puis la signature, comme Outlook.
        let mut html = format!("<p class=MsoNormal><o:p>&nbsp;</o:p></p>{signature}");
        let mut draft = Draft { kind, source_id: source_id.map(String::from), ..Draft::default() };
        let Some(o) = original else {
            draft.html = html;
            return Ok(draft);
        };
        let reply_target = if o.reply_to.is_empty() { o.from.clone() } else { o.reply_to.clone() };
        let mut seen: HashSet<String> = HashSet::new();
        let mut keep = |list: Vec<MailAddress>| -> Vec<MailAddress> {
            list.into_iter().filter(|m| !is_mine(m) && seen.insert(m.addr.to_ascii_lowercase())).collect()
        };
        match kind {
            ComposeKind::Reply => {
                // Réponse à son propre message (Éléments envoyés) : vers ses destinataires, comme la référence.
                let to = keep(reply_target.clone());
                draft.to = format_list(if to.is_empty() { &o.to } else { &to });
            }
            ComposeKind::ReplyAll => {
                let mut to = keep(reply_target.clone());
                to.extend(keep(o.to.clone()));
                draft.to = format_list(&to);
                draft.cc = format_list(&keep(o.cc.clone()));
            }
            _ => {}
        }
        draft.subject = prefixed_subject(kind, &o.subject);
        if kind == ComposeKind::Forward {
            draft.attachments = o.attachments.clone();
        }
        let sent = local(o.sent).map(|d| format::quote_date(&d)).unwrap_or_default();
        let mut block = format!(
            "<div style='border:none;border-top:solid #E1E1E1 1.0pt;padding:3.0pt 0cm 0cm 0cm'><p class=MsoNormal>\
             <b>De&nbsp;:</b> {}<br><b>Envoyé&nbsp;:</b> {sent}<br><b>À&nbsp;:</b> {}<br>",
            quote_list(&o.from),
            quote_list(&o.to)
        );
        if !o.cc.is_empty() {
            block.push_str(&format!("<b>Cc&nbsp;:</b> {}<br>", quote_list(&o.cc)));
        }
        block.push_str(&format!("<b>Objet&nbsp;:</b> {}</p></div><p class=MsoNormal><o:p>&nbsp;</o:p></p>", escape(&o.subject)));
        html.push_str(&format!("<div>{block}</div>{}", o.body));
        draft.html = html;
        draft.styles = o.styles;
        let _ = o.id;
        Ok(draft)
    }
}

/// Images `data:` du corps remplacées par des références `cid:` : (corps, [(cid, type, octets)]).
fn extract_data_images(html: &str) -> (String, Vec<(String, String, Vec<u8>)>) {
    let mut out = String::with_capacity(html.len());
    let mut images = Vec::new();
    let mut pos = 0;
    while let Some(i) = find_ci(html, "data:image/", pos) {
        out.push_str(&html[pos..i]);
        let rest = &html[i + 5..];
        let end = rest.find(|c: char| matches!(c, '"' | '\'' | ')' | ' ' | '>')).unwrap_or(rest.len());
        let uri = &rest[..end];
        match uri.split_once(";base64,").and_then(|(mime, data)| STANDARD.decode(data.trim()).ok().map(|b| (mime.to_string(), b))) {
            Some((mime, bytes)) => {
                let cid = format!("image{:03}.{}@thunderlook", images.len() + 1, hex(&unique_bytes()[..4]));
                out.push_str(&format!("cid:{cid}"));
                images.push((cid, mime, bytes));
            }
            None => out.push_str(&html[i..i + 5 + end]),
        }
        pos = i + 5 + end;
    }
    out.push_str(&html[pos..]);
    (out, images)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Type MIME d'une pièce jointe d'après son extension (à défaut : `application/octet-stream`).
fn file_mime(name: &str) -> &'static str {
    match name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).as_deref() {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("txt") => "text/plain",
        Some("htm" | "html") => "text/html",
        Some("eml") => "message/rfc822",
        Some("zip") => "application/zip",
        Some("doc") => "application/msword",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("xls") => "application/vnd.ms-excel",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Some("odt") => "application/vnd.oasis.opendocument.text",
        Some("ods") => "application/vnd.oasis.opendocument.spreadsheet",
        _ => "application/octet-stream",
    }
}

fn to_address(list: &[MailAddress]) -> Address<'static> {
    Address::new_list(list.iter().map(|m| Address::new_address(m.name.clone(), m.addr.clone())).collect())
}

/// Ligne de destinataires de la liste des messages : noms affichés, sinon adresses.
fn display_line(list: &[MailAddress]) -> String {
    list.iter().map(|m| m.name.clone().unwrap_or_else(|| m.addr.clone())).collect::<Vec<_>>().join("; ")
}

fn angle(id: &str) -> String {
    format!("<{}>", bare(id))
}

fn bare(id: &str) -> String {
    id.trim().trim_start_matches('<').trim_end_matches('>').to_string()
}

/// Fil de discussion d'un brouillon : `In-Reply-To`, `References`, sujet, index et identifiant de conversation.
struct Threading {
    in_reply_to: Option<String>,
    references: Option<String>,
    topic: String,
    index: Vec<u8>,
    conversation_id: Vec<u8>,
}

/// Ligne existante d'un brouillon rouvert.
struct ExistingDraft {
    id: i64,
    source: i64,
    folder: i64,
    eml_path: String,
    message_id: Option<String>,
    topic: Option<String>,
    index: Option<Vec<u8>>,
    conversation_id: Option<Vec<u8>>,
    in_reply_to: Option<String>,
}

impl Mailbox {
    fn references_of(&self, eml_path: &str) -> Option<String> {
        let bytes = fs::read(thunderlook_store::resolve(self.data_dir(), eml_path)).ok()?;
        let m = MessageParser::default().parse(&bytes)?;
        m.header_raw("References").map(|r| r.trim().to_string()).filter(|r| !r.is_empty())
    }

    /// Fil d'une réponse ou d'un transfert de `source_id` ([MS-OXOMSG] : index enfant, même conversation),
    /// ou d'une nouvelle conversation.
    fn threading_for(&self, source_id: Option<&str>, subject: &str) -> Result<Threading> {
        let Some(id) = source_id else {
            let index = conversation_index(None);
            return Ok(Threading {
                in_reply_to: None,
                references: None,
                topic: normalized_subject(subject).to_string(),
                conversation_id: index[6..22].to_vec(),
                index,
            });
        };
        let (mid, topic, index, conv, eml): (Option<String>, Option<String>, Option<Vec<u8>>, Option<Vec<u8>>, String) = self
            .conn()
            .query_row(
                "SELECT internet_message_id, conversation_topic, conversation_index, conversation_id, eml_path FROM message WHERE id = ?1",
                [message_db_id(id)?],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .map_err(err)?;
        let mid = mid.filter(|m| !m.trim().is_empty()).map(|m| angle(&m));
        let references = match (self.references_of(&eml), &mid) {
            (Some(r), Some(m)) => Some(format!("{r} {m}")),
            (None, Some(m)) => Some(m.clone()),
            (r, None) => r,
        };
        let index = conversation_index(index.as_deref());
        let conversation_id = conv.filter(|c| c.len() == 16).unwrap_or_else(|| index[6..22].to_vec());
        let topic = topic.unwrap_or_else(|| normalized_subject(subject).to_string());
        Ok(Threading { in_reply_to: mid, references, topic, index, conversation_id })
    }

    fn existing_draft(&self, id: &str) -> Result<ExistingDraft> {
        self.conn()
            .query_row(
                "SELECT m.id, f.source_id, m.folder_id, m.eml_path, m.internet_message_id, m.conversation_topic,
                        m.conversation_index, m.conversation_id, m.in_reply_to
                 FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1",
                [message_db_id(id)?],
                |r| {
                    Ok(ExistingDraft {
                        id: r.get(0)?,
                        source: r.get(1)?,
                        folder: r.get(2)?,
                        eml_path: r.get(3)?,
                        message_id: r.get(4)?,
                        topic: r.get(5)?,
                        index: r.get(6)?,
                        conversation_id: r.get(7)?,
                        in_reply_to: r.get(8)?,
                    })
                },
            )
            .map_err(err)
    }

    /// « Envoyer » (J7) : le brouillon est enregistré au nom du compte d'envoi, puis placé dans la Boîte d'envoi de ce
    /// compte et dans la file `outbox` ; `thunderlook-sync` le transmet. Compte d'envoi : celui de la source du
    /// brouillon, sinon le premier compte. Renvoie l'identifiant du message.
    pub fn send_draft(&mut self, d: &Draft) -> Result<String> {
        if parse_addresses(&d.to).is_empty() && parse_addresses(&d.cc).is_empty() && parse_addresses(&d.bcc).is_empty() {
            return Err("Ajoutez au moins un destinataire.".into());
        }
        let source = match (&d.id, &d.source_id) {
            (Some(id), _) => self.row(message_db_id(id)?)?.source,
            (None, Some(s)) => self.row(message_db_id(s)?)?.source,
            (None, None) => self.default_source()?,
        };
        let account: Option<(i64, i64, String, String)> = self
            .conn()
            .query_row(
                "SELECT id, source_id, name, email FROM account ORDER BY source_id = ?1 DESC, id LIMIT 1",
                [source],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(err)?;
        let (account_id, account_source, name, email) =
            account.ok_or("Aucun compte de messagerie : ajoutez-en un (Envoyer/Recevoir › Paramètres du compte).")?;
        self.send_as = Some((name, email));
        let saved = self.save_draft(d);
        self.send_as = None;
        let saved = saved?;
        let id = message_db_id(saved.id.as_deref().ok_or("brouillon non enregistré")?)?;
        // Message définitif : sans la marque de brouillon.
        let row = self.row(id)?;
        let path = thunderlook_store::resolve(self.data_dir(), &row.eml_path);
        let bytes = fs::read(&path).map_err(|e| format!("{} : {e}", path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        if let Some(pos) = text.find("X-Unsent: 1\r\n") {
            let mut out = bytes[..pos].to_vec();
            out.extend_from_slice(&bytes[pos + "X-Unsent: 1\r\n".len()..]);
            fs::write(&path, out).map_err(|e| format!("{} : {e}", path.display()))?;
        }
        let outbox: i64 = self
            .conn()
            .query_row("SELECT id FROM folder WHERE source_id = ?1 AND role = 'outbox' ORDER BY id LIMIT 1", [account_source], |r| r.get(0))
            .map_err(err)?;
        let now = Local::now().timestamp_millis();
        self.write_folders(&[(id, outbox)])?;
        self.conn()
            .execute("UPDATE message SET is_read = 1, received_at = ?2, sent_at = ?2 WHERE id = ?1", params![id, now])
            .map_err(err)?;
        self.conn()
            .execute("INSERT OR REPLACE INTO outbox (message_id, account_id, queued_at) VALUES (?1, ?2, ?3)", params![id, account_id, now])
            .map_err(err)?;
        Ok(format!("m{id}"))
    }

    /// Enregistre le brouillon dans le dossier Brouillons de sa source (création ou mise à jour) et le renvoie
    /// avec son identifiant. Le fil de discussion d'un brouillon déjà enregistré est conservé.
    pub fn save_draft(&mut self, d: &Draft) -> Result<Draft> {
        let now = Local::now().timestamp_millis();
        let existing = d.id.as_deref().map(|id| self.existing_draft(id)).transpose()?;
        let source = match (&existing, &d.source_id) {
            (Some(e), _) => e.source,
            (None, Some(s)) => self.row(message_db_id(s)?)?.source,
            (None, None) => self.default_source()?,
        };
        let folder: i64 = match &existing {
            Some(e) => e.folder,
            None => self
                .conn()
                .query_row("SELECT id FROM folder WHERE source_id = ?1 AND role = 'drafts' ORDER BY hidden, id LIMIT 1", [source], |r| r.get(0))
                .optional()
                .map_err(err)?
                .ok_or("pas de dossier Brouillons dans ce fichier de données")?,
        };
        let threading = match &existing {
            Some(e) => Threading {
                in_reply_to: e.in_reply_to.as_deref().map(angle),
                references: self.references_of(&e.eml_path),
                topic: e.topic.clone().unwrap_or_else(|| normalized_subject(&d.subject).to_string()),
                index: e.index.clone().unwrap_or_else(|| conversation_index(None)),
                conversation_id: e.conversation_id.clone().unwrap_or_default(),
            },
            None => self.threading_for(d.source_id.as_deref(), &d.subject)?,
        };
        // Pièces jointes lues avant toute écriture : un brouillon rouvert reprend les siennes.
        let mut files: Vec<(String, String, Vec<u8>)> = Vec::new();
        for a in &d.attachments {
            match &a.source {
                DraftAttachmentSource::Message { id, index } => {
                    let data = self.attachment(id, *index)?;
                    files.push((data.name.clone(), file_mime(&data.name).to_string(), data.bytes));
                }
                DraftAttachmentSource::File { path } => {
                    let bytes = fs::read(path).map_err(|e| format!("{path} : {e}"))?;
                    files.push((a.name.clone(), file_mime(&a.name).to_string(), bytes));
                }
            }
        }
        let (body, images) = extract_data_images(&d.html);
        let html_doc = format!(
            "<html><head><meta http-equiv=Content-Type content=\"text/html; charset=utf-8\"><style>{}</style></head><body lang=FR>{body}</body></html>",
            d.styles
        );
        let text = thunderlook_mime::html_to_text(&html_doc);
        let mine = self.my_addresses(source)?;
        let from_addr = mine.first().cloned().unwrap_or_default();
        let from_name = self.my_name(source, &from_addr)?.unwrap_or_default();
        let (to, cc, bcc) = (parse_addresses(&d.to), parse_addresses(&d.cc), parse_addresses(&d.bcc));
        let message_id = match existing.as_ref().and_then(|e| e.message_id.clone()) {
            Some(m) => bare(&m),
            None => format!("{}@thunderlook.local", hex(&unique_bytes())),
        };

        // Arborescence MIME : mixed[ related[ alternative[texte, html], images ], pièces jointes ].
        let mut body_part = MimePart::new(
            "multipart/alternative",
            BodyPart::Multipart(vec![
                MimePart::new("text/plain", BodyPart::Text(Cow::Owned(text.clone()))),
                MimePart::new("text/html", BodyPart::Text(Cow::Owned(html_doc))),
            ]),
        );
        if !images.is_empty() {
            let mut parts = vec![body_part];
            for (cid, mime, bytes) in &images {
                parts.push(MimePart::new(mime.clone(), BodyPart::Binary(Cow::Owned(bytes.clone()))).inline().cid(cid.clone()));
            }
            body_part = MimePart::new("multipart/related", BodyPart::Multipart(parts));
        }
        if !files.is_empty() {
            let mut parts = vec![body_part];
            for (name, mime, bytes) in &files {
                parts.push(MimePart::new(mime.clone(), BodyPart::Binary(Cow::Owned(bytes.clone()))).attachment(name.clone()));
            }
            body_part = MimePart::new("multipart/mixed", BodyPart::Multipart(parts));
        }
        let mut builder = MessageBuilder::new()
            .from(Address::new_address((!from_name.is_empty()).then(|| from_name.clone()), from_addr.clone()))
            .subject(d.subject.clone())
            .date(now / 1000)
            .message_id(message_id.clone())
            .header("X-Unsent", Raw::new("1"));
        if !to.is_empty() {
            builder = builder.to(to_address(&to));
        }
        if !cc.is_empty() {
            builder = builder.cc(to_address(&cc));
        }
        if !bcc.is_empty() {
            builder = builder.bcc(to_address(&bcc));
        }
        if let Some(r) = &threading.in_reply_to {
            builder = builder.in_reply_to(bare(r));
        }
        if let Some(r) = &threading.references {
            builder = builder.header("References", Raw::new(r.clone()));
        }
        let eml = builder.body(body_part).write_to_vec().map_err(err)?;
        let preview = thunderlook_mime::preview_from_text(&text, 255);
        let in_reply_to = threading.in_reply_to.as_deref().map(bare);
        let attachment_count = files.len() as i64;

        let tx = self.conn().unchecked_transaction().map_err(err)?;
        let (id, rel) = match &existing {
            Some(e) => {
                tx.execute(
                    "UPDATE message SET subject = ?2, from_name = ?3, from_addr = ?4, to_line = ?5, cc_line = ?6, received_at = ?7,
                        modified_at = ?7, size = ?8, attachment_count = ?9, preview = ?10, indexed = 0 WHERE id = ?1",
                    params![e.id, d.subject, from_name, from_addr, display_line(&to), display_line(&cc), now, eml.len() as i64, attachment_count, preview],
                )
                .map_err(err)?;
                tx.execute("DELETE FROM attachment WHERE message_id = ?1", [e.id]).map_err(err)?;
                tx.execute("DELETE FROM message_fts WHERE rowid = ?1", [e.id]).map_err(err)?;
                (e.id, e.eml_path.clone())
            }
            None => {
                tx.execute(
                    "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, received_at,
                        created_at, modified_at, message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count,
                        body_kind, preview, internet_message_id, in_reply_to, conversation_topic, conversation_index, eml_path, props,
                        conversation_id)
                     VALUES (?1, 0, 'IPM.Note', ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?7, ?8, 1, 0, 1, 0, ?9, ?10, 'html', ?11, ?12, ?13, ?14, ?15,
                        ?16, ?17, ?18)",
                    params![
                        folder,
                        d.subject,
                        from_name,
                        from_addr,
                        display_line(&to),
                        display_line(&cc),
                        now,
                        DRAFT_FLAGS,
                        eml.len() as i64,
                        attachment_count,
                        preview,
                        message_id,
                        in_reply_to,
                        threading.topic,
                        threading.index,
                        "",
                        Vec::<u8>::new(),
                        threading.conversation_id
                    ],
                )
                .map_err(err)?;
                let id = tx.last_insert_rowid();
                let rel = thunderlook_store::eml_relative_path(source, id);
                tx.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, rel]).map_err(err)?;
                (id, rel)
            }
        };
        crate::remote::queue(&tx, crate::remote::Change::Rewritten { id })?;
        let mut position = 0i64;
        for (cid, mime, bytes) in &images {
            tx.execute(
                "INSERT INTO attachment (message_id, position, filename, mime_type, size, method, content_id, inline, hidden, props)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, ?3, 1, 0, ?6)",
                params![id, position, cid, mime, bytes.len() as i64, Vec::<u8>::new()],
            )
            .map_err(err)?;
            position += 1;
        }
        for (name, mime, bytes) in &files {
            tx.execute(
                "INSERT INTO attachment (message_id, position, filename, mime_type, size, method, content_id, inline, hidden, props)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, NULL, 0, 0, ?6)",
                params![id, position, name, mime, bytes.len() as i64, Vec::<u8>::new()],
            )
            .map_err(err)?;
            position += 1;
        }
        // Écriture dans un fichier voisin puis renommage : jamais de brouillon à moitié écrit.
        let path = thunderlook_store::resolve(self.data_dir(), &rel);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{} : {e}", dir.display()))?;
        }
        let tmp = path.with_extension("eml.tmp");
        fs::write(&tmp, &eml).map_err(|e| format!("{} : {e}", tmp.display()))?;
        fs::rename(&tmp, &path).map_err(|e| format!("{} : {e}", path.display()))?;
        tx.commit().map_err(err)?;

        // Le brouillon enregistré porte désormais ses propres pièces jointes.
        let mut saved = d.clone();
        saved.id = Some(format!("m{id}"));
        saved.attachments = files
            .iter()
            .enumerate()
            .map(|(i, (name, _, bytes))| DraftAttachment {
                name: name.clone(),
                size_label: format::size(bytes.len() as u64),
                source: DraftAttachmentSource::Message { id: format!("m{id}"), index: (images.len() + i) as u32 },
            })
            .collect();
        Ok(saved)
    }

    /// Rouvre un brouillon enregistré pour le modifier.
    pub fn open_draft(&self, id: &str) -> Result<Draft> {
        let row = self.row(message_db_id(id)?)?;
        let bytes = self.eml(&row)?;
        let parsed = MessageParser::default().parse(&bytes).ok_or("fichier .eml illisible")?;
        let mut cids = HashMap::new();
        for p in parsed.attachments() {
            if let Some(cid) = content_id(p) {
                cids.insert(cid.to_ascii_lowercase(), format!("data:{};base64,{}", mime_of(p), STANDARD.encode(p.contents())));
            }
        }
        let (html, styles, used) = match parsed.html_part(0).filter(|p| p.is_text_html()).and_then(|p| p.text_contents()) {
            Some(h) => {
                let (inlined, used) = inline_cids(h, &cids);
                let (body, styles) = split_html(&inlined);
                (body, styles, used)
            }
            None => (parsed.body_text(0).map(|t| text_to_html(&t)).unwrap_or_default(), String::new(), Vec::new()),
        };
        let attachments = attachment_parts(&parsed)
            .enumerate()
            .filter(|(_, p)| !content_id(p).is_some_and(|c| used.contains(&c.to_ascii_lowercase())))
            .map(|(i, p)| DraftAttachment {
                name: part_name(p, i),
                size_label: format::size(p.contents().len() as u64),
                source: DraftAttachmentSource::Message { id: id.to_string(), index: i as u32 },
            })
            .collect();
        Ok(Draft {
            id: Some(id.to_string()),
            kind: ComposeKind::New,
            source_id: None,
            to: format_list(&header_addresses(parsed.to())),
            cc: format_list(&header_addresses(parsed.cc())),
            bcc: format_list(&header_addresses(parsed.bcc())),
            subject: row.subject.clone(),
            html,
            styles,
            attachments,
        })
    }

    /// Le message est-il un brouillon non envoyé (`MSGFLAG_UNSENT`) ?
    pub fn is_draft(&self, id: &str) -> Result<bool> {
        let flags: i64 = self
            .conn()
            .query_row("SELECT message_flags FROM message WHERE id = ?1", [message_db_id(id)?], |r| r.get(0))
            .map_err(err)?;
        Ok(flags & 0x8 != 0)
    }
}
