//! Message complet pour le volet de lecture : en-têtes depuis la base, corps et pièces jointes depuis le `.eml`.
//!
//! Le HTML est livré comme document complet pour un `iframe` isolé (`sandbox` sans scripts). Sa propre
//! politique CSP bloque tout contenu distant, sauf si l'utilisateur l'autorise pour ce message ; les images
//! intégrées (`cid:`) sont remplacées par des URI `data:`.

use std::{collections::HashMap, fs};

use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, Local};
use mail_parser::{Message, MessageParser, MimeHeaders};
use thunderlook_model::{AttachmentInfo, BodyView, FlagState, LastVerb, MessageView, format};
use thunderlook_store::rusqlite::{OptionalExtension, params};

use crate::{Mailbox, Result, err, flag_state, importance, local, message_db_id};

/// Extensions dont l'ouverture directe est refusée (programmes, scripts, raccourcis). L'enregistrement reste possible.
pub const BLOCKED_EXTENSIONS: &[&str] = &[
    "ade", "adp", "app", "application", "appref-ms", "bas", "bat", "chm", "cmd", "com", "cpl", "crt", "csh", "der",
    "diagcab", "dll", "exe", "fxp", "gadget", "hlp", "hta", "htc", "inf", "ins", "isp", "its", "jar", "jnlp", "js",
    "jse", "ksh", "lnk", "mad", "maf", "mag", "mam", "maq", "mar", "mas", "mat", "mau", "mav", "maw", "mcf", "mda",
    "mdb", "mde", "mdt", "mdw", "mdz", "msc", "msh", "msh1", "msh2", "mshxml", "msh1xml", "msh2xml", "msi", "msp",
    "mst", "msu", "ops", "osd", "pcd", "pif", "pl", "plg", "prf", "prg", "printerexport", "ps1", "ps1xml", "ps2",
    "ps2xml", "psc1", "psc2", "psd1", "psdm1", "pst", "py", "pyc", "pyo", "pyw", "pyz", "pyzw", "reg", "scf", "scr",
    "sct", "settingcontent-ms", "shb", "shs", "theme", "tmp", "url", "vb", "vbe", "vbp", "vbs", "vhd", "vhdx", "vsmacros",
    "vsw", "webpnp", "website", "ws", "wsb", "wsc", "wsf", "wsh", "xbap", "xll", "xnk",
];

pub fn is_blocked(name: &str) -> bool {
    let ext = name.rsplit_once('.').map(|(_, e)| e.trim_end_matches(['.', ' ']).to_ascii_lowercase());
    ext.is_some_and(|e| BLOCKED_EXTENSIONS.contains(&e.as_str()))
}

/// Noms de fichiers distincts pour enregistrer plusieurs pièces jointes dans un même dossier :
/// un doublon (sans tenir compte de la casse, comme Windows) devient « nom (2).ext », « nom (3).ext »…
pub fn unique_names(names: &[String]) -> Vec<String> {
    let mut used = std::collections::HashSet::new();
    names
        .iter()
        .map(|name| {
            let (stem, ext) = match name.rsplit_once('.') {
                Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
                _ => (name.as_str(), String::new()),
            };
            let mut candidate = name.clone();
            let mut n = 1;
            while !used.insert(candidate.to_lowercase()) {
                n += 1;
                candidate = format!("{stem} ({n}){ext}");
            }
            candidate
        })
        .collect()
}

/// Pièce jointe extraite du `.eml`.
pub struct AttachmentData {
    pub name: String,
    pub bytes: Vec<u8>,
    pub blocked: bool,
}

pub(crate) struct Row {
    pub(crate) subject: String,
    pub(crate) from_name: String,
    pub(crate) from_addr: String,
    pub(crate) to_line: String,
    pub(crate) cc_line: String,
    pub(crate) received: Option<i64>,
    pub(crate) sent: Option<i64>,
    pub(crate) importance: i64,
    pub(crate) flag_status: i64,
    pub(crate) flag_request: Option<String>,
    pub(crate) flag_due: Option<i64>,
    pub(crate) flag_completed: Option<i64>,
    pub(crate) message_class: String,
    pub(crate) eml_path: String,
    pub(crate) source: i64,
    pub(crate) last_verb: Option<i64>,
    pub(crate) last_verb_at: Option<i64>,
}

/// Barre d'information d'Outlook 2021 sur un message répondu ou transféré.
pub(crate) fn last_verb_label(verb: LastVerb, at: Option<DateTime<Local>>) -> String {
    let what = if verb.is_forward() { "Vous avez transféré ce message" } else { "Vous avez répondu à ce message" };
    match at {
        Some(d) => format!("{what} le {}.", d.format("%d/%m/%Y %H:%M")),
        None => format!("{what}."),
    }
}

fn item_kind(class: &str) -> Option<String> {
    let c = class.to_ascii_lowercase();
    let label = if c.starts_with("ipm.note") || c == "ipm" || c.is_empty() {
        return None;
    } else if c.starts_with("ipm.schedule.meeting") {
        "Demande de réunion"
    } else if c.starts_with("ipm.appointment") {
        "Rendez-vous"
    } else if c.starts_with("ipm.contact") {
        "Contact"
    } else if c.starts_with("ipm.distlist") {
        "Liste de distribution"
    } else if c.starts_with("ipm.task") {
        "Tâche"
    } else if c.starts_with("ipm.stickynote") {
        "Note"
    } else if c.starts_with("report.") {
        "Rapport de remise"
    } else {
        return Some(class.to_string());
    };
    Some(label.to_string())
}

/// Nom affiché d'une pièce jointe ; un message joint sans nom prend son objet.
pub(crate) fn part_name(part: &mail_parser::MessagePart, index: usize) -> String {
    if let Some(n) = part.attachment_name().map(str::trim).filter(|n| !n.is_empty()) {
        return n.to_string();
    }
    if let Some(m) = part.message() {
        let subject = m.subject().unwrap_or("").trim();
        let base = if subject.is_empty() { "Message joint" } else { subject };
        let safe: String = base.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c }).collect();
        return format!("{safe}.eml");
    }
    format!("pièce jointe {}", index + 1)
}

pub(crate) fn content_id(part: &mail_parser::MessagePart) -> Option<String> {
    part.content_id().map(|c| c.trim().trim_start_matches('<').trim_end_matches('>').to_string())
}

/// Variante `text/rtf` sans nom écrite par l'import à côté du corps : ce n'est pas une pièce jointe.
fn is_rtf_body(part: &mail_parser::MessagePart) -> bool {
    part.attachment_name().is_none()
        && part.content_type().is_some_and(|c| c.ctype() == "text" && c.subtype() == Some("rtf"))
}

pub(crate) fn mime_of(part: &mail_parser::MessagePart) -> String {
    part.content_type()
        .map(|c| match c.subtype() {
            Some(s) => format!("{}/{}", c.ctype(), s),
            None => c.ctype().to_string(),
        })
        .unwrap_or_else(|| "application/octet-stream".into())
}

/// Recherche ASCII insensible à la casse.
pub(crate) fn find_ci(hay: &str, needle: &str, from: usize) -> Option<usize> {
    let h = hay.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    (from..=h.len() - n.len()).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

/// Remplace les références `cid:` connues par des URI `data:`.
pub(crate) fn inline_cids(html: &str, cids: &HashMap<String, String>) -> (String, Vec<String>) {
    let mut out = String::with_capacity(html.len());
    let mut used = Vec::new();
    let mut pos = 0;
    while let Some(i) = find_ci(html, "cid:", pos) {
        out.push_str(&html[pos..i]);
        let rest = &html[i + 4..];
        let end = rest.find(|c: char| matches!(c, '"' | '\'' | ')' | ' ' | '>' | '\t' | '\r' | '\n')).unwrap_or(rest.len());
        let id = &rest[..end];
        match cids.get(id).or_else(|| cids.get(&id.to_ascii_lowercase())) {
            Some(uri) => {
                out.push_str(uri);
                used.push(id.to_ascii_lowercase());
            }
            None => out.push_str(&html[i..i + 4 + end]),
        }
        pos = i + 4 + end;
    }
    out.push_str(&html[pos..]);
    (out, used)
}

/// Présence de ressources distantes chargées automatiquement (images, styles, polices, fonds).
pub(crate) fn has_remote(html: &str) -> bool {
    const PATTERNS: [&str; 12] = [
        "src=\"http", "src='http", "src=http", "url(http", "url(\"http", "url('http", "background=\"http",
        "background='http", "srcset=\"http", "@import", "<link", "src=\"//",
    ];
    PATTERNS.iter().any(|p| find_ci(html, p, 0).is_some())
}

/// Neutralise `<meta http-equiv="refresh">` (redirection automatique).
fn strip_refresh(html: &str) -> String {
    let mut out = html.to_string();
    while let Some(i) = find_ci(&out, "http-equiv=\"refresh\"", 0).or_else(|| find_ci(&out, "http-equiv=refresh", 0)) {
        out.replace_range(i..i + "http-equiv".len(), "data-blocked");
    }
    out
}

/// Document complet pour l'`iframe` du volet de lecture.
pub(crate) fn srcdoc(html: &str, allow_remote: bool) -> String {
    let remote = if allow_remote { " https: http:" } else { "" };
    let csp = format!(
        "default-src 'none'; img-src data:{remote}; style-src 'unsafe-inline'{remote}; font-src data:{remote}; media-src data:"
    );
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"{csp}\">\
         <meta name=\"referrer\" content=\"no-referrer\">\
         <style>html{{background:#fff;color:#000;color-scheme:light}}\
         body{{margin:12px 16px;font-family:'Segoe UI',Inter,Arial,sans-serif;font-size:14px;line-height:1.4;overflow-wrap:break-word}}\
         img{{max-width:100%}}pre{{white-space:pre-wrap}}</style></head><body>{}</body></html>",
        strip_refresh(html)
    )
}

impl Mailbox {
    pub(crate) fn row(&self, id: i64) -> Result<Row> {
        self.conn()
            .query_row(
                "SELECT m.subject, m.from_name, m.from_addr, m.to_line, m.cc_line, m.received_at, m.sent_at, m.importance,
                        m.flag_status, m.flag_request, m.flag_due, m.flag_completed, m.message_class, m.eml_path, f.source_id,
                        m.last_verb, m.last_verb_at
                 FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1",
                params![id],
                |r| {
                    Ok(Row {
                        subject: r.get(0)?,
                        from_name: r.get(1)?,
                        from_addr: r.get(2)?,
                        to_line: r.get(3)?,
                        cc_line: r.get(4)?,
                        received: r.get(5)?,
                        sent: r.get(6)?,
                        importance: r.get(7)?,
                        flag_status: r.get(8)?,
                        flag_request: r.get(9)?,
                        flag_due: r.get(10)?,
                        flag_completed: r.get(11)?,
                        message_class: r.get(12)?,
                        eml_path: r.get(13)?,
                        source: r.get(14)?,
                        last_verb: r.get(15)?,
                        last_verb_at: r.get(16)?,
                    })
                },
            )
            .optional()
            .map_err(err)?
            .ok_or_else(|| format!("message introuvable : m{id}"))
    }

    pub(crate) fn eml(&self, row: &Row) -> Result<Vec<u8>> {
        let path = thunderlook_store::resolve(self.data_dir(), &row.eml_path);
        fs::read(&path).map_err(|e| format!("{} : {e}", path.display()))
    }

    pub fn message_view(&self, id: &str, allow_remote: bool) -> Result<MessageView> {
        let db_id = message_db_id(id)?;
        let row = self.row(db_id)?;
        let bytes = self.eml(&row)?;
        let parsed = MessageParser::default().parse(&bytes).ok_or("fichier .eml illisible")?;

        let mut categories = Vec::new();
        {
            let mut stmt = self
                .conn()
                .prepare_cached("SELECT name FROM message_category WHERE message_id = ?1 ORDER BY name")
                .map_err(err)?;
            let rows = stmt.query_map(params![db_id], |r| r.get::<_, String>(0)).map_err(err)?;
            for name in rows {
                categories.push(name.map_err(err)?);
            }
        }
        let categories = self.chips(row.source, categories);

        let (body, remote_blocked, used_cids) = body_view(&parsed, allow_remote);
        let attachments = attachment_parts(&parsed)
            .enumerate()
            .filter(|(_, p)| !content_id(p).is_some_and(|c| used_cids.contains(&c.to_ascii_lowercase())))
            .map(|(i, p)| {
                let name = part_name(p, i);
                AttachmentInfo {
                    index: i as u32,
                    size_label: format::size(p.contents().len() as u64),
                    message: p.is_message(),
                    blocked: is_blocked(&name),
                    name,
                }
            })
            .collect();

        let date = local(row.received.or(row.sent));
        let flag = flag_state(row.flag_status);
        let flag_label = match flag {
            FlagState::Flagged => {
                let what = row.flag_request.clone().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "Assurer un suivi".into());
                Some(match local(row.flag_due) {
                    Some(d) => format!("{what}. Échéance : {}.", format::long_day(&d)),
                    None => format!("{what}."),
                })
            }
            FlagState::Done => Some(match local(row.flag_completed) {
                Some(d) => format!("Indicateur terminé le {}.", format::long_day(&d)),
                None => "Indicateur terminé.".into(),
            }),
            FlagState::None => None,
        };

        Ok(MessageView {
            id: id.to_string(),
            subject: row.subject.clone(),
            from: if row.from_name.trim().is_empty() { row.from_addr.clone() } else { row.from_name.clone() },
            from_address: row.from_addr.clone(),
            to: row.to_line.clone(),
            cc: row.cc_line.clone(),
            date_label: date.map(|d| format::full_date(&d)).unwrap_or_default(),
            importance: importance(row.importance),
            flag,
            flag_label,
            categories,
            attachments,
            body,
            remote_blocked,
            item_kind: item_kind(&row.message_class),
            last_verb: row.last_verb.and_then(LastVerb::from_code),
            last_verb_label: row.last_verb.and_then(LastVerb::from_code).map(|v| last_verb_label(v, local(row.last_verb_at))),
        })
    }

    /// Contenu d'une pièce jointe, désignée par l'index donné dans [`MessageView::attachments`].
    pub fn attachment(&self, id: &str, index: u32) -> Result<AttachmentData> {
        let row = self.row(message_db_id(id)?)?;
        let bytes = self.eml(&row)?;
        let parsed = MessageParser::default().parse(&bytes).ok_or("fichier .eml illisible")?;
        let part = attachment_parts(&parsed).nth(index as usize).ok_or("pièce jointe introuvable")?;
        let name = part_name(part, index as usize);
        // Message joint : on rend le message lui-même (.eml), pas son seul corps.
        let data = match part.message() {
            Some(m) => m.raw_message().to_vec(),
            None => part.contents().to_vec(),
        };
        Ok(AttachmentData { blocked: is_blocked(&name), name, bytes: data })
    }

    /// Date de référence du message (réception, sinon envoi), pour les outils.
    pub fn message_date(&self, id: &str) -> Result<Option<DateTime<Local>>> {
        let row = self.row(message_db_id(id)?)?;
        Ok(local(row.received.or(row.sent)))
    }
}

/// Pièces jointes au sens de l'interface, dans l'ordre du `.eml` (index stable entre deux lectures).
pub(crate) fn attachment_parts<'a>(m: &'a Message<'a>) -> impl Iterator<Item = &'a mail_parser::MessagePart<'a>> {
    m.attachments().filter(|p| !is_rtf_body(p))
}

/// Corps à afficher, contenu distant bloqué ou non, identifiants `cid:` utilisés dans le HTML.
fn body_view(m: &Message, allow_remote: bool) -> (BodyView, bool, Vec<String>) {
    let html_part = m.html_part(0).filter(|p| p.is_text_html());
    if let Some(html) = html_part.and_then(|p| p.text_contents()) {
        let mut cids = HashMap::new();
        for p in m.attachments() {
            if let Some(cid) = content_id(p) {
                let uri = format!("data:{};base64,{}", mime_of(p), STANDARD.encode(p.contents()));
                cids.insert(cid.to_ascii_lowercase(), uri.clone());
                cids.insert(cid, uri);
            }
        }
        let (html, used) = inline_cids(html, &cids);
        let remote = has_remote(&html);
        return (BodyView::Html { srcdoc: srcdoc(&html, allow_remote) }, remote && !allow_remote, used);
    }
    match m.body_text(0) {
        Some(t) if !t.trim().is_empty() => (BodyView::Text { text: t.into_owned() }, false, Vec::new()),
        _ => (BodyView::Empty, false, Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EML: &str = "From: Camille <c@example.org>\r\nTo: p@example.org\r\nSubject: Test\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"M\"\r\n\r\n--M\r\nContent-Type: multipart/related; boundary=\"R\"\r\n\r\n\
--R\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Bonjour <img src=\"cid:logo@x\"> <img src=\"https://t.example.org/p.gif\"></p>\r\n\
--R\r\nContent-Type: image/png\r\nContent-ID: <logo@x>\r\nContent-Transfer-Encoding: base64\r\n\r\niVBORw0KGgo=\r\n--R--\r\n\
--M\r\nContent-Type: application/pdf; name=\"facture.pdf\"\r\nContent-Disposition: attachment; filename=\"facture.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQK\r\n\
--M\r\nContent-Type: application/octet-stream; name=\"setup.exe\"\r\nContent-Disposition: attachment; filename=\"setup.exe\"\r\n\r\nMZ\r\n--M--\r\n";

    #[test]
    fn html_body_with_cid_and_remote() {
        let m = MessageParser::default().parse(EML.as_bytes()).unwrap();
        let (body, blocked, used) = body_view(&m, false);
        let BodyView::Html { srcdoc } = body else { panic!("corps HTML attendu") };
        assert!(srcdoc.contains("data:image/png;base64,iVBORw0KGgo="));
        assert!(!srcdoc.contains("cid:logo"));
        assert!(srcdoc.contains("default-src 'none'; img-src data:;"));
        assert!(blocked);
        assert_eq!(used, vec!["logo@x".to_string()]);
        let names: Vec<String> = attachment_parts(&m).enumerate().map(|(i, p)| part_name(p, i)).collect();
        assert_eq!(names, ["pièce jointe 1", "facture.pdf", "setup.exe"]);
        let (_, blocked, _) = body_view(&m, true);
        assert!(!blocked);
    }

    #[test]
    fn helpers() {
        assert!(is_blocked("Setup.EXE"));
        assert!(is_blocked("photo.jpg.exe"));
        assert!(!is_blocked("facture.pdf"));
        assert!(!is_blocked("sans-extension"));
        assert_eq!(strip_refresh("<meta http-equiv=\"Refresh\" content=\"0;url=x\">"), "<meta data-blocked=\"Refresh\" content=\"0;url=x\">");
        assert!(!has_remote("<img src=\"data:image/png;base64,xx\">"));
        assert!(has_remote("<td background='http://x/y.gif'>"));
        assert_eq!(item_kind("IPM.Note.SMIME"), None);
        assert_eq!(item_kind("IPM.Contact").as_deref(), Some("Contact"));
    }

    #[test]
    fn duplicate_names_get_a_counter() {
        let names: Vec<String> = ["a.pdf", "A.PDF", "b", "b", "a.pdf", ".bashrc"].iter().map(|s| s.to_string()).collect();
        assert_eq!(super::unique_names(&names), ["a.pdf", "A (2).PDF", "b", "b (2)", "a (3).pdf", ".bashrc"]);
    }
}
