//! Conversion d'un message MAPI en `.eml` ([MS-OXCMAIL], sous-ensemble) et en ligne d'index.

use std::{borrow::Cow, rc::Rc};

use mail_builder::{
    MessageBuilder,
    headers::{address::Address, raw::Raw},
    mime::{BodyPart, MimePart},
};
use outlook_pst::{
    ltp::prop_context::PropertyValue,
    messaging::{
        attachment::{AnsiAttachment, Attachment, AttachmentData, UnicodeAttachment},
        message::{AnsiMessage, Message, UnicodeMessage},
    },
    ndb::node_id::NodeId,
};
use thunderlook_mime::{decode, html_to_text, preview_from_text, rtf};

use crate::props::{self, NamedProps, binary, boolean, int, strings, text, time_ms};

/// Propriétés de message utilisées ici ([MS-OXPROPS]).
mod tag {
    pub const IMPORTANCE: u16 = 0x0017;
    pub const MESSAGE_CLASS: u16 = 0x001A;
    pub const SENSITIVITY: u16 = 0x0036;
    pub const SUBJECT: u16 = 0x0037;
    pub const CLIENT_SUBMIT_TIME: u16 = 0x0039;
    pub const SENT_REPRESENTING_NAME: u16 = 0x0042;
    pub const SENT_REPRESENTING_EMAIL: u16 = 0x0065;
    pub const CONVERSATION_TOPIC: u16 = 0x0070;
    pub const CONVERSATION_INDEX: u16 = 0x0071;
    pub const CONVERSATION_INDEX_TRACKING: u16 = 0x3016;
    pub const SENDER_ADDRTYPE: u16 = 0x0C1E;
    pub const SENDER_NAME: u16 = 0x0C1A;
    pub const SENDER_EMAIL: u16 = 0x0C1F;
    pub const DISPLAY_CC: u16 = 0x0E03;
    pub const DISPLAY_TO: u16 = 0x0E04;
    pub const DELIVERY_TIME: u16 = 0x0E06;
    pub const MESSAGE_FLAGS: u16 = 0x0E07;
    pub const MESSAGE_SIZE: u16 = 0x0E08;
    pub const BODY: u16 = 0x1000;
    pub const RTF_COMPRESSED: u16 = 0x1009;
    pub const HTML: u16 = 0x1013;
    pub const INTERNET_MESSAGE_ID: u16 = 0x1035;
    pub const REFERENCES: u16 = 0x1039;
    pub const IN_REPLY_TO: u16 = 0x1042;
    pub const FLAG_STATUS: u16 = 0x1090;
    pub const FLAG_COMPLETE_TIME: u16 = 0x1091;
    pub const CREATION_TIME: u16 = 0x3007;
    pub const LAST_MODIFICATION_TIME: u16 = 0x3008;
    pub const INTERNET_CODEPAGE: u16 = 0x3FDE;
    pub const SENDER_SMTP: u16 = 0x5D01;
    pub const SENT_REPRESENTING_SMTP: u16 = 0x5D02;
    // Destinataires
    pub const RECIPIENT_TYPE: u16 = 0x0C15;
    pub const DISPLAY_NAME: u16 = 0x3001;
    pub const ADDRTYPE: u16 = 0x3002;
    pub const EMAIL_ADDRESS: u16 = 0x3003;
    pub const SMTP_ADDRESS: u16 = 0x39FE;
    // Pièces jointes
    pub const ATTACH_SIZE: u16 = 0x0E20;
    pub const ATTACH_DATA: u16 = 0x3701;
    pub const ATTACH_FILENAME: u16 = 0x3704;
    pub const ATTACH_METHOD: u16 = 0x3705;
    pub const ATTACH_LONG_FILENAME: u16 = 0x3707;
    pub const ATTACH_MIME_TAG: u16 = 0x370E;
    pub const ATTACH_CONTENT_ID: u16 = 0x3712;
    pub const ATTACH_CONTENT_LOCATION: u16 = 0x3713;
    pub const ATTACH_LONG_PATHNAME: u16 = 0x370D;
    pub const ATTACHMENT_HIDDEN: u16 = 0x7FFE;
}

const MSGFLAG_READ: i64 = 0x0001;
/// Profondeur maximale de messages joints imbriqués (protection contre les boucles).
const MAX_DEPTH: usize = 16;

#[derive(Debug)]
pub struct Issue {
    pub severity: &'static str,
    pub kind: &'static str,
    pub detail: String,
}

impl Issue {
    fn degraded(kind: &'static str, detail: impl Into<String>) -> Self {
        Self { severity: "degraded", kind, detail: detail.into() }
    }
}

pub struct AttachmentMeta {
    pub filename: String,
    pub mime_type: String,
    pub size: i64,
    pub method: i64,
    pub content_id: Option<String>,
    pub inline: bool,
    pub hidden: bool,
    pub props: Vec<u8>,
    pub embedded: bool,
}

pub struct Converted {
    pub eml: Vec<u8>,
    pub message_class: String,
    pub subject: String,
    pub from_name: String,
    pub from_addr: String,
    pub to_line: String,
    pub cc_line: String,
    pub sent_at: Option<i64>,
    pub received_at: Option<i64>,
    pub created_at: Option<i64>,
    pub modified_at: Option<i64>,
    pub message_flags: i64,
    pub is_read: bool,
    pub flag_status: i64,
    pub flag_request: Option<String>,
    pub flag_due: Option<i64>,
    pub flag_completed: Option<i64>,
    pub importance: i64,
    pub sensitivity: i64,
    pub size: i64,
    pub body_kind: &'static str,
    pub preview: String,
    pub internet_message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub conversation_topic: Option<String>,
    pub conversation_index: Option<Vec<u8>>,
    /// `PidTagConversationId` calculé ([`thunderlook_store::conversation_id`]).
    pub conversation_id: Option<[u8; 16]>,
    pub categories: Vec<String>,
    pub attachments: Vec<AttachmentMeta>,
    pub props: Vec<u8>,
    pub issues: Vec<Issue>,
}

struct Recipient {
    kind: i64,
    name: String,
    addr: String,
}

fn recipients(msg: &dyn Message) -> Vec<Recipient> {
    let Some(table) = msg.recipient_table() else { return Vec::new() };
    let context = table.context();
    let cols = context.columns();
    let index = |id: u16| cols.iter().position(|c| c.prop_id() == id);
    let (i_type, i_name, i_smtp, i_email, i_atype) = (
        index(tag::RECIPIENT_TYPE),
        index(tag::DISPLAY_NAME),
        index(tag::SMTP_ADDRESS),
        index(tag::EMAIL_ADDRESS),
        index(tag::ADDRTYPE),
    );
    let mut out = Vec::new();
    for row in table.rows_matrix() {
        let Ok(values) = row.columns(context) else { continue };
        let read = |i: Option<usize>| -> Option<PropertyValue> {
            let i = i?;
            let value = values.get(i)?.as_ref()?;
            table.read_column(value, cols[i].prop_type()).ok()
        };
        let kind = int(read(i_type).as_ref()).unwrap_or(1);
        let name = text(read(i_name).as_ref()).unwrap_or_default();
        let atype = text(read(i_atype).as_ref()).unwrap_or_default();
        let addr = text(read(i_smtp).as_ref())
            .or_else(|| text(read(i_email).as_ref()).filter(|_| atype.eq_ignore_ascii_case("SMTP")))
            .unwrap_or_default();
        out.push(Recipient { kind, name, addr });
    }
    out
}

fn address_list(list: &[&Recipient]) -> Address<'static> {
    Address::new_list(
        list.iter()
            .map(|r| {
                let name = (!r.name.is_empty() && r.name != r.addr).then(|| r.name.clone());
                Address::new_address(name, r.addr.clone())
            })
            .collect(),
    )
}

fn open_attachment(msg: &Rc<dyn Message>, sub_node: NodeId) -> std::io::Result<Rc<dyn Attachment>> {
    let any = msg.clone().into_any();
    match any.downcast::<UnicodeMessage>() {
        Ok(m) => Ok(UnicodeAttachment::read(m, sub_node, None)?),
        Err(any) => match any.downcast::<AnsiMessage>() {
            Ok(m) => Ok(AnsiAttachment::read(m, sub_node, None)?),
            Err(_) => Err(std::io::Error::other("type de message inattendu")),
        },
    }
}

/// Types MIME courants d'après l'extension, quand `PidTagAttachMimeTag` manque.
fn guess_mime(filename: &str) -> &'static str {
    let ext = filename.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "txt" => "text/plain",
        "htm" | "html" => "text/html",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "zip" => "application/zip",
        "eml" => "message/rfc822",
        "ics" => "text/calendar",
        "vcf" => "text/vcard",
        "rtf" => "application/rtf",
        _ => "application/octet-stream",
    }
}

/// Retire les chevrons d'un identifiant de message (mail-builder les remet).
fn bare_id(id: &str) -> String {
    id.trim().trim_start_matches('<').trim_end_matches('>').to_string()
}

pub fn convert(msg: &Rc<dyn Message>, named: &NamedProps) -> Converted {
    convert_at(msg, named, 0)
}

fn convert_at(msg: &Rc<dyn Message>, named: &NamedProps, depth: usize) -> Converted {
    let p = msg.properties();
    let get = |id: u16| p.get(id);
    let mut issues = Vec::new();

    let message_class = text(get(tag::MESSAGE_CLASS)).unwrap_or_else(|| "IPM.Note".into());
    let subject = text(get(tag::SUBJECT)).map(|s| thunderlook_store::strip_subject_marker(&s).to_string()).unwrap_or_default();
    let sender_type = text(get(tag::SENDER_ADDRTYPE)).unwrap_or_default();
    let from_name = text(get(tag::SENT_REPRESENTING_NAME)).or_else(|| text(get(tag::SENDER_NAME))).unwrap_or_default();
    let from_addr = text(get(tag::SENT_REPRESENTING_SMTP))
        .or_else(|| text(get(tag::SENDER_SMTP)))
        .or_else(|| text(get(tag::SENT_REPRESENTING_EMAIL)).filter(|a| a.contains('@')))
        .or_else(|| text(get(tag::SENDER_EMAIL)).filter(|_| !sender_type.eq_ignore_ascii_case("EX")))
        .unwrap_or_default();
    let sent_at = time_ms(get(tag::CLIENT_SUBMIT_TIME));
    let created_at = time_ms(get(tag::CREATION_TIME));
    let received_at = time_ms(get(tag::DELIVERY_TIME)).or(sent_at).or(created_at);
    let message_flags = int(get(tag::MESSAGE_FLAGS)).unwrap_or(0);
    let categories = named.keywords.map(|id| strings(get(id))).unwrap_or_default();

    // Corps.
    let codepage = int(get(tag::INTERNET_CODEPAGE)).and_then(|c| u32::try_from(c).ok()).unwrap_or(65001);
    let mut html = match get(tag::HTML) {
        Some(PropertyValue::Binary(b)) => Some(decode(b.buffer(), codepage)),
        other => text(other),
    };
    let mut plain = text(get(tag::BODY));
    let mut rtf_native: Option<Vec<u8>> = None;
    let mut body_kind = match (&html, &plain) {
        (Some(_), _) => "html",
        (None, Some(_)) => "text",
        (None, None) => "none",
    };
    if html.is_none() {
        if let Some(data) = binary(get(tag::RTF_COMPRESSED)) {
            match rtf::decompress(data) {
                Ok(raw) => match rtf::decode(&raw) {
                    rtf::RtfBody::Html(h) => {
                        html = Some(h);
                        body_kind = "rtf-html";
                    }
                    rtf::RtfBody::Text(t) => {
                        if plain.is_none() {
                            plain = Some(t);
                        }
                        body_kind = "rtf-text";
                    }
                    rtf::RtfBody::Rtf { text: t } => {
                        if plain.is_none() {
                            plain = Some(t);
                        }
                        rtf_native = Some(raw);
                        body_kind = "rtf";
                    }
                },
                Err(e) => issues.push(Issue::degraded("rtf", format!("RTF compressé illisible : {e}"))),
            }
        }
    }
    let preview_src = match (&plain, &html) {
        (Some(t), _) => Cow::Borrowed(t.as_str()),
        (None, Some(h)) => Cow::Owned(html_to_text(h)),
        _ => Cow::Borrowed(""),
    };
    let preview = preview_from_text(&preview_src, 255);

    // Pièces jointes.
    let mut attachments = Vec::new();
    let mut inline_parts = Vec::new();
    let mut file_parts = Vec::new();
    if let Some(table) = msg.attachment_table() {
        for (position, row) in table.rows_matrix().enumerate() {
            let sub_node = NodeId::from(u32::from(row.id()));
            let att = match open_attachment(msg, sub_node) {
                Ok(a) => a,
                Err(e) => {
                    issues.push(Issue { severity: "lost", kind: "attachment-open", detail: format!("pièce jointe n° {position} : {e}") });
                    continue;
                }
            };
            let ap = att.properties();
            let method = int(ap.get(tag::ATTACH_METHOD)).unwrap_or(1);
            let filename = text(ap.get(tag::ATTACH_LONG_FILENAME))
                .or_else(|| text(ap.get(tag::ATTACH_FILENAME)))
                .or_else(|| text(ap.get(tag::DISPLAY_NAME)))
                .unwrap_or_else(|| format!("piece-jointe-{}", position + 1));
            let content_id = text(ap.get(tag::ATTACH_CONTENT_ID)).map(|c| bare_id(&c));
            let hidden = boolean(ap.get(tag::ATTACHMENT_HIDDEN));
            let mut mime_type =
                text(ap.get(tag::ATTACH_MIME_TAG)).unwrap_or_else(|| guess_mime(&filename).to_string());
            let mut embedded = false;
            let data: Option<Vec<u8>> = match att.data() {
                Some(AttachmentData::Binary(b)) => Some(b.buffer().to_vec()),
                Some(AttachmentData::Message(inner)) if depth < MAX_DEPTH => {
                    let sub = convert_at(inner, named, depth + 1);
                    for i in sub.issues {
                        issues.push(Issue { detail: format!("message joint « {filename} » : {}", i.detail), ..i });
                    }
                    embedded = true;
                    mime_type = "message/rfc822".into();
                    Some(sub.eml)
                }
                Some(AttachmentData::Message(_)) => {
                    issues.push(Issue { severity: "lost", kind: "attachment-depth", detail: format!("« {filename} » : imbrication > {MAX_DEPTH}") });
                    None
                }
                None => None,
            };
            let size = int(ap.get(tag::ATTACH_SIZE)).unwrap_or(0);
            let inline = content_id.as_ref().is_some_and(|cid| {
                html.as_deref().is_some_and(|h| h.contains(&format!("cid:{cid}")))
            });
            let skip: &[u16] = if embedded { &[] } else { &[tag::ATTACH_DATA] };
            attachments.push(AttachmentMeta {
                filename: filename.clone(),
                mime_type: mime_type.clone(),
                size: data.as_ref().map_or(size, |d| d.len() as i64),
                method,
                content_id: content_id.clone(),
                inline,
                hidden,
                props: props::propbag(ap.iter(), skip),
                embedded,
            });
            let Some(data) = data else {
                // Par référence : le fichier n'a jamais été dans le PST.
                if matches!(method, 2 | 4 | 7) {
                    let path = text(ap.get(tag::ATTACH_LONG_PATHNAME)).unwrap_or_default();
                    issues.push(Issue::degraded("attachment-reference", format!("« {filename} » : lien vers {path}")));
                } else {
                    issues.push(Issue { severity: "lost", kind: "attachment-data", detail: format!("« {filename} » : données absentes (méthode {method})") });
                }
                continue;
            };
            // RFC 2046 §5.2.1 : message/rfc822 n'admet que 7bit, 8bit ou binary (jamais base64).
            let encoding = embedded.then(|| if data.is_ascii() { "7bit" } else { "8bit" });
            let mut part = MimePart::new(mime_type, BodyPart::Binary(Cow::Owned(data)));
            if let Some(encoding) = encoding {
                part = part.transfer_encoding(encoding);
            }
            if let Some(loc) = text(ap.get(tag::ATTACH_CONTENT_LOCATION)) {
                part = part.location(loc);
            }
            if inline {
                inline_parts.push(part.inline().cid(content_id.unwrap_or_default()));
            } else {
                let name = if embedded && !filename.to_ascii_lowercase().ends_with(".eml") {
                    format!("{filename}.eml")
                } else {
                    filename
                };
                file_parts.push(part.attachment(name));
            }
        }
    }

    // Arborescence MIME : mixed[ related[ alternative[texte, html], images ], pièces jointes ].
    let mut body_parts = Vec::new();
    if let Some(t) = &plain {
        body_parts.push(MimePart::new("text/plain", BodyPart::Text(Cow::Owned(t.clone()))));
    }
    if let Some(h) = &html {
        body_parts.push(MimePart::new("text/html", BodyPart::Text(Cow::Owned(h.clone()))));
    }
    if let Some(r) = rtf_native {
        body_parts.push(MimePart::new("text/rtf", BodyPart::Binary(Cow::Owned(r))));
    }
    let mut body = match body_parts.len() {
        0 => MimePart::new("text/plain", BodyPart::Text(Cow::Borrowed(""))),
        1 => body_parts.pop().unwrap(),
        _ => MimePart::new("multipart/alternative", BodyPart::Multipart(body_parts)),
    };
    if !inline_parts.is_empty() {
        inline_parts.insert(0, body);
        body = MimePart::new("multipart/related", BodyPart::Multipart(inline_parts));
    }
    if !file_parts.is_empty() {
        file_parts.insert(0, body);
        body = MimePart::new("multipart/mixed", BodyPart::Multipart(file_parts));
    }

    // En-têtes.
    let rcpts = recipients(msg.as_ref());
    let by_kind = |k: i64| rcpts.iter().filter(|r| r.kind & 0x0F == k).collect::<Vec<_>>();
    let (to, cc, bcc) = (by_kind(1), by_kind(2), by_kind(3));
    let internet_message_id = text(get(tag::INTERNET_MESSAGE_ID));
    let in_reply_to = text(get(tag::IN_REPLY_TO));
    let importance = int(get(tag::IMPORTANCE)).unwrap_or(1);
    let sensitivity = int(get(tag::SENSITIVITY)).unwrap_or(0);
    let mut builder = MessageBuilder::new()
        .from(Address::new_address((!from_name.is_empty()).then(|| from_name.clone()), from_addr.clone()))
        .subject(subject.clone());
    if !to.is_empty() {
        builder = builder.to(address_list(&to));
    }
    if !cc.is_empty() {
        builder = builder.cc(address_list(&cc));
    }
    if !bcc.is_empty() {
        builder = builder.bcc(address_list(&bcc));
    }
    if let Some(date) = sent_at.or(received_at) {
        builder = builder.date(date / 1000);
    }
    if let Some(id) = &internet_message_id {
        builder = builder.message_id(bare_id(id));
    }
    if let Some(id) = &in_reply_to {
        builder = builder.in_reply_to(bare_id(id));
    }
    if let Some(refs) = text(get(tag::REFERENCES)) {
        builder = builder.header("References", Raw::new(refs));
    }
    if importance != 1 {
        builder = builder.header("Importance", Raw::new(if importance == 2 { "high" } else { "low" }));
    }
    if !categories.is_empty() {
        builder = builder.header("Keywords", mail_builder::headers::text::Text::new(categories.join(", ")));
    }
    if !message_class.eq_ignore_ascii_case("IPM.Note") {
        builder = builder.header("X-ThunderLook-Message-Class", Raw::new(message_class.clone()));
    }
    let eml = builder.body(body).write_to_vec().unwrap_or_default();

    // Sac brut : tout sauf les corps effectivement exportés dans le .eml.
    let mut skip = Vec::new();
    if plain.is_some() && p.get(tag::BODY).is_some() {
        skip.push(tag::BODY);
    }
    if body_kind == "html" {
        skip.push(tag::HTML);
    }

    Converted {
        eml,
        subject,
        from_name,
        from_addr,
        to_line: text(get(tag::DISPLAY_TO)).unwrap_or_default(),
        cc_line: text(get(tag::DISPLAY_CC)).unwrap_or_default(),
        sent_at,
        received_at,
        created_at,
        modified_at: time_ms(get(tag::LAST_MODIFICATION_TIME)),
        message_flags,
        is_read: message_flags & MSGFLAG_READ != 0,
        flag_status: int(get(tag::FLAG_STATUS)).unwrap_or(0),
        flag_request: named.flag_request.and_then(|id| text(get(id))),
        flag_due: named.task_due_date.and_then(|id| time_ms(get(id))),
        flag_completed: time_ms(get(tag::FLAG_COMPLETE_TIME)),
        importance,
        sensitivity,
        size: int(get(tag::MESSAGE_SIZE)).unwrap_or(0),
        body_kind,
        preview,
        internet_message_id,
        in_reply_to,
        conversation_topic: text(get(tag::CONVERSATION_TOPIC)),
        conversation_index: binary(get(tag::CONVERSATION_INDEX)).map(<[u8]>::to_vec),
        conversation_id: thunderlook_store::conversation_id(
            binary(get(tag::CONVERSATION_INDEX)),
            boolean(get(tag::CONVERSATION_INDEX_TRACKING)),
            text(get(tag::CONVERSATION_TOPIC)).as_deref(),
        ),
        categories,
        attachments,
        props: props::propbag(p.iter(), &skip),
        message_class,
        issues,
    }
}
