//! Commandes Tauri du lot BI : aperçu des pièces jointes (image, texte, PDF) pour le volet de lecture.
//! Lecture seule : rien n'est écrit, ni dans Outlook ni sur le disque.

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{Store, with_mailbox};

/// Au-delà, pas d'aperçu (le contenu transite par l'IPC en base64) : « Ouvrir » ou « Enregistrer sous » restent possibles.
const MAX_BINARY: usize = 12 * 1024 * 1024;
/// Texte : on n'affiche que le début.
const MAX_TEXT: usize = 200 * 1024;

/// Résultat d'aperçu ; `kind` vaut `image`, `pdf`, `text` ou `none` (alors `note` explique pourquoi).
#[derive(Serialize, Debug, Default, PartialEq)]
pub struct AttachPreview {
    pub kind: String,
    pub mime: String,
    pub name: String,
    /// `data:<mime>;base64,…` pour `image` et `pdf`.
    pub data_uri: String,
    /// Début du contenu pour `text`.
    pub text: String,
    pub truncated: bool,
    pub note: String,
}

fn ext(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

/// Type d'image affichable par `<img>` d'après l'extension, ou `None`.
fn image_mime(name: &str) -> Option<&'static str> {
    Some(match ext(name).as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" | "jpe" | "jfif" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        _ => return None,
    })
}

fn is_text(name: &str) -> bool {
    matches!(
        ext(name).as_str(),
        "txt" | "log" | "md" | "csv" | "tsv" | "json" | "xml" | "html" | "htm" | "css" | "js" | "ini" | "cfg" | "conf" | "yml"
            | "yaml" | "ics" | "vcf" | "eml" | "rs" | "py" | "sh" | "bat" | "sql" | "toml"
    )
}

fn none(name: &str, note: impl Into<String>) -> AttachPreview {
    AttachPreview { kind: "none".into(), name: name.into(), note: note.into(), ..Default::default() }
}

/// Décide de l'aperçu d'après le nom et les octets. Fonction pure, testée.
pub fn build_preview(name: &str, bytes: &[u8], blocked: bool) -> AttachPreview {
    if blocked {
        // Même un aperçu de texte d'un exécutable n'a pas d'intérêt ; et on reste cohérent avec « Ouvrir ».
        return none(name, "Type de fichier bloqué : pas d'aperçu. Utilisez « Enregistrer sous ».");
    }
    if let Some(mime) = image_mime(name) {
        if bytes.len() > MAX_BINARY {
            return none(name, "Image trop volumineuse pour l'aperçu.");
        }
        return AttachPreview {
            kind: "image".into(),
            mime: mime.into(),
            name: name.into(),
            data_uri: format!("data:{mime};base64,{}", STANDARD.encode(bytes)),
            ..Default::default()
        };
    }
    if ext(name) == "pdf" {
        if bytes.len() > MAX_BINARY {
            return none(name, "PDF trop volumineux pour l'aperçu.");
        }
        if !bytes.starts_with(b"%PDF") {
            return none(name, "Ce fichier n'est pas un PDF valide.");
        }
        return AttachPreview {
            kind: "pdf".into(),
            mime: "application/pdf".into(),
            name: name.into(),
            data_uri: format!("data:application/pdf;base64,{}", STANDARD.encode(bytes)),
            ..Default::default()
        };
    }
    if is_text(name) {
        let truncated = bytes.len() > MAX_TEXT;
        let mut head = &bytes[..bytes.len().min(MAX_TEXT)];
        if head.contains(&0) {
            return none(name, "Contenu binaire : pas d'aperçu texte.");
        }
        // Évite de couper un caractère UTF-8 en deux à la limite.
        if truncated {
            while !head.is_empty() && std::str::from_utf8(head).is_err() && head.len() + 4 > MAX_TEXT {
                head = &head[..head.len() - 1];
            }
        }
        return AttachPreview {
            kind: "text".into(),
            mime: "text/plain".into(),
            name: name.into(),
            text: String::from_utf8_lossy(head).into_owned(),
            truncated,
            ..Default::default()
        };
    }
    none(name, "Aucun aperçu pour ce type de fichier. Utilisez « Ouvrir » ou « Enregistrer sous ».")
}

/// Aperçu d'une pièce jointe, désignée comme pour `open_attachment`.
#[tauri::command(async)]
pub fn attachment_preview(app: AppHandle, store: State<Store>, id: String, index: u32) -> Result<AttachPreview, String> {
    let att = with_mailbox(&app, &store, |mb| mb.attachment(&id, index))?;
    Ok(build_preview(&att.name, &att.bytes, att.blocked))
}

/// Pièce jointe prête pour le glisser vers l'Explorateur (`DownloadURL` de Chromium/WebView2).
#[derive(Serialize, Debug, PartialEq)]
pub struct AttachDrag {
    pub name: String,
    pub mime: String,
    pub data_uri: String,
}

/// Contenu complet en `data:` URI, quel que soit le type ; `None` au-delà de la limite (pas de glisser possible).
pub fn build_drag(name: &str, bytes: &[u8]) -> Option<AttachDrag> {
    if bytes.len() > MAX_BINARY {
        return None;
    }
    let mime = match (image_mime(name), ext(name).as_str()) {
        (Some(m), _) => m,
        (None, "pdf") => "application/pdf",
        _ => "application/octet-stream",
    };
    // Le format `DownloadURL` est « mime:nom:url » : les « : » du nom le casseraient.
    let name = name.replace(':', "_");
    Some(AttachDrag { data_uri: format!("data:{mime};base64,{}", STANDARD.encode(bytes)), mime: mime.into(), name })
}

#[tauri::command(async)]
pub fn attachment_drag(app: AppHandle, store: State<Store>, id: String, index: u32) -> Result<Option<AttachDrag>, String> {
    let att = with_mailbox(&app, &store, |mb| mb.attachment(&id, index))?;
    Ok(build_drag(&att.name, &att.bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_devient_data_uri() {
        let p = build_preview("Photo.JPG", &[1, 2, 3], false);
        assert_eq!(p.kind, "image");
        assert_eq!(p.mime, "image/jpeg");
        assert!(p.data_uri.starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn pdf_verifie_l_en_tete() {
        assert_eq!(build_preview("a.pdf", b"%PDF-1.7 ...", false).kind, "pdf");
        assert_eq!(build_preview("a.pdf", b"MZ....", false).kind, "none");
    }

    #[test]
    fn texte_tronque_et_binaire_refuse() {
        let long = vec![b'a'; MAX_TEXT + 10];
        let p = build_preview("n.txt", &long, false);
        assert!(p.truncated);
        assert_eq!(p.text.len(), MAX_TEXT);
        assert_eq!(build_preview("n.txt", &[b'a', 0, b'b'], false).kind, "none");
    }

    #[test]
    fn glisser_nom_sans_deux_points() {
        let d = build_drag("a:b.docx", b"x").unwrap();
        assert_eq!(d.name, "a_b.docx");
        assert_eq!(d.mime, "application/octet-stream");
        assert!(build_drag("a.bin", &vec![0; MAX_BINARY + 1]).is_none());
    }

    #[test]
    fn bloque_inconnu_et_trop_gros() {
        assert_eq!(build_preview("virus.txt", b"x", true).kind, "none");
        assert_eq!(build_preview("a.xyz", b"x", false).kind, "none");
        assert_eq!(build_preview("g.png", &vec![0; MAX_BINARY + 1], false).kind, "none");
    }
}
