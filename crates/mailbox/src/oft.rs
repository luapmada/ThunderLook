//! Modèles Outlook `.oft` (J5) : fichier composé OLE au format `.msg` ([MS-OXMSG]). On en lit l'objet, les
//! destinataires, le corps (HTML, sinon RTF désencapsulé, sinon texte) et les pièces jointes, pour en faire un
//! nouveau message. Le fichier n'est jamais modifié.

use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use thunderlook_model::{ComposeKind, Draft, DraftAttachment, DraftAttachmentSource, format};
use thunderlook_mime::rtf::{self, RtfBody};

use crate::{
    Result,
    compose::{MailAddress, format_address, split_html},
};

/// Flux d'une propriété : `__substg1.0_<id><type>`.
fn stream_name(id: u16, ty: u16) -> String {
    format!("__substg1.0_{id:04X}{ty:04X}")
}

struct Msg {
    file: cfb::CompoundFile<std::fs::File>,
}

impl Msg {
    fn bytes(&mut self, storage: &str, id: u16, ty: u16) -> Option<Vec<u8>> {
        let path = format!("{storage}/{}", stream_name(id, ty));
        let mut s = self.file.open_stream(&path).ok()?;
        let mut out = Vec::new();
        s.read_to_end(&mut out).ok()?;
        Some(out)
    }

    /// Chaîne Unicode (`001F`), sinon ANSI (`001E`, page de code Windows-1252).
    fn text(&mut self, storage: &str, id: u16) -> Option<String> {
        if let Some(b) = self.bytes(storage, id, 0x001F) {
            let units: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            return Some(String::from_utf16_lossy(&units).trim_end_matches('\0').to_string());
        }
        self.bytes(storage, id, 0x001E).map(|b| thunderlook_mime::decode(&b, 1252).trim_end_matches('\0').to_string())
    }

    /// Propriété fixe 32 bits lue dans `__properties_version1.0` (en-tête de 8 octets pour un sous-stockage,
    /// 32 octets à la racine, puis des entrées de 16 octets : balise, drapeaux, valeur).
    fn int32(&mut self, storage: &str, id: u16, root: bool) -> Option<u32> {
        let path = format!("{storage}/__properties_version1.0");
        let mut s = self.file.open_stream(&path).ok()?;
        let mut b = Vec::new();
        s.read_to_end(&mut b).ok()?;
        let start = if root { 32 } else { 8 };
        b.get(start..)?.chunks_exact(16).find_map(|e| {
            let tag = u32::from_le_bytes([e[0], e[1], e[2], e[3]]);
            ((tag >> 16) as u16 == id && tag & 0xFFFF == 0x0003).then(|| u32::from_le_bytes([e[8], e[9], e[10], e[11]]))
        })
    }

    fn storages(&self, prefix: &str) -> Vec<String> {
        let mut v: Vec<String> = self
            .file
            .read_root_storage()
            .filter(|e| e.is_storage() && e.name().starts_with(prefix))
            .map(|e| format!("/{}", e.name()))
            .collect();
        v.sort();
        v
    }
}

/// Lit un modèle `.oft` (ou un `.msg`) et en fait un nouveau message. Les pièces jointes sont copiées dans
/// `scratch` (un dossier temporaire propre à l'appel) pour être jointes à l'enregistrement du brouillon.
pub fn draft_from_oft(path: &Path, scratch: &Path) -> Result<Draft> {
    let file = cfb::open(path).map_err(|e| format!("{} : {e}", path.display()))?;
    let mut m = Msg { file };
    let subject = m.text("", 0x0037).unwrap_or_default();
    let subject = thunderlook_store::strip_subject_marker(&subject).to_string();

    // Destinataires : PidTagRecipientType 1 = À, 2 = Cc, 3 = Cci.
    let (mut to, mut cc, mut bcc) = (Vec::new(), Vec::new(), Vec::new());
    for r in m.storages("__recip_version1.0_") {
        let addr = m.text(&r, 0x39FE).or_else(|| m.text(&r, 0x3003)).unwrap_or_default();
        if !addr.contains('@') {
            continue;
        }
        let name = m.text(&r, 0x3001).filter(|n| !n.is_empty() && *n != addr);
        let who = format_address(&MailAddress { name, addr });
        match m.int32(&r, 0x0C15, false).unwrap_or(1) {
            2 => cc.push(who),
            3 => bcc.push(who),
            _ => to.push(who),
        }
    }

    // Pièces jointes : celles référencées par `cid:` deviennent des images intégrées, les autres des fichiers.
    let mut cids: HashMap<String, String> = HashMap::new();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (i, a) in m.storages("__attach_version1.0_").into_iter().enumerate() {
        let Some(data) = m.bytes(&a, 0x3701, 0x0102) else { continue };
        let name = m.text(&a, 0x3707).or_else(|| m.text(&a, 0x3704)).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("pièce jointe {}", i + 1));
        match m.text(&a, 0x3712).filter(|c| !c.is_empty()) {
            Some(cid) => {
                let mime = m.text(&a, 0x370E).unwrap_or_else(|| "application/octet-stream".into());
                cids.insert(cid.to_ascii_lowercase(), format!("data:{mime};base64,{}", STANDARD.encode(&data)));
                // Gardé aussi comme fichier si le corps ne l'utilise pas (vérifié plus bas).
                files.push((format!("\u{0}{cid}\u{0}{name}"), data));
            }
            None => files.push((name, data)),
        }
    }

    // Corps : HTML (PidTagHtml, binaire), sinon RTF compressé désencapsulé, sinon texte.
    let codepage = m.int32("", 0x3FDE, true).unwrap_or(65001);
    let (mut html, styles) = if let Some(h) = m.bytes("", 0x1013, 0x0102) {
        split_html(&thunderlook_mime::decode(&h, codepage))
    } else if let Some(r) = m.bytes("", 0x1009, 0x0102).and_then(|c| rtf::decompress(&c).ok()) {
        match rtf::decode(&r) {
            RtfBody::Html(h) => split_html(&h),
            RtfBody::Text(t) | RtfBody::Rtf { text: t } => (crate::compose::text_to_html_pub(&t), String::new()),
        }
    } else {
        (m.text("", 0x1000).map(|t| crate::compose::text_to_html_pub(&t)).unwrap_or_default(), String::new())
    };
    let (inlined, used) = crate::message::inline_cids(&html, &cids);
    html = inlined;

    std::fs::create_dir_all(scratch).map_err(|e| format!("{} : {e}", scratch.display()))?;
    let mut attachments = Vec::new();
    for (key, data) in files {
        let name = match key.strip_prefix('\u{0}').and_then(|k| k.split_once('\u{0}')) {
            Some((cid, _)) if used.contains(&cid.to_ascii_lowercase()) => continue,
            Some((_, name)) => name.to_string(),
            None => key,
        };
        let safe: String = name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '_' } else { c }).collect();
        let target: PathBuf = scratch.join(format!("{}-{safe}", attachments.len() + 1));
        std::fs::write(&target, &data).map_err(|e| format!("{} : {e}", target.display()))?;
        attachments.push(DraftAttachment {
            name,
            size_label: format::size(data.len() as u64),
            source: DraftAttachmentSource::File { path: target.to_string_lossy().into_owned() },
        });
    }
    Ok(Draft {
        id: None,
        kind: ComposeKind::New,
        source_id: None,
        to: to.join("; "),
        cc: cc.join("; "),
        bcc: bcc.join("; "),
        subject,
        html,
        styles,
        attachments,
        from_account: None,
    })
}
