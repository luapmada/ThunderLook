//! Conversions de contenu sans dépendance au format PST : pages de code Windows, RTF compressé
//! ([MS-OXRTFCP]) et désencapsulation HTML/texte ([MS-OXRTFEX]), aperçu en texte brut.

pub mod rtf;

use encoding_rs::{Encoding, WINDOWS_1252};

/// Encodage d'une page de code Windows (`PidTagInternetCodepage`, `\ansicpg`). Windows-1252 par défaut.
pub fn encoding_for_codepage(codepage: u32) -> &'static Encoding {
    u16::try_from(codepage).ok().and_then(codepage::to_encoding).unwrap_or(WINDOWS_1252)
}

/// Décode des octets dans la page de code donnée. Les octets invalides deviennent U+FFFD.
pub fn decode(bytes: &[u8], codepage: u32) -> String {
    let (text, _, _) = encoding_for_codepage(codepage).decode(bytes);
    text.into_owned()
}

/// Caractères invisibles de mise en forme (trait d'union conditionnel, liants, espace sans chasse),
/// très présents dans les mails publicitaires pour remplir l'aperçu.
fn is_invisible(c: char) -> bool {
    matches!(c, '\u{AD}' | '\u{34F}' | '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}')
}

/// Aperçu d'une ligne : caractères invisibles retirés, espaces normalisés, tronqué à `max` caractères.
pub fn preview_from_text(text: &str, max: usize) -> String {
    let mut out = String::with_capacity(max.min(text.len()));
    let mut count = 0;
    let cleaned: String;
    let text = if text.chars().any(is_invisible) {
        cleaned = text.chars().filter(|c| !is_invisible(*c)).collect();
        &cleaned
    } else {
        text
    };
    for word in text.split_whitespace() {
        if count + 1 >= max && count > 0 {
            break;
        }
        if count > 0 {
            out.push(' ');
            count += 1;
        }
        for c in word.chars() {
            if count >= max {
                return out;
            }
            out.push(c);
            count += 1;
        }
    }
    out
}

/// Texte brut d'un HTML, pour l'aperçu : balises retirées, `head`/`style`/`script` ignorés,
/// entités courantes décodées. Ce n'est pas un rendu.
pub fn html_to_text(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len() / 2);
    let mut i = 0;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Blocs dont le contenu n'est pas du texte affiché.
            let mut skipped = false;
            for tag in ["head", "style", "script", "title", "xml"] {
                let open = &lower[i + 1..];
                if open.starts_with(tag) && open[tag.len()..].starts_with(|c: char| c == '>' || c.is_whitespace()) {
                    let close = format!("</{tag}");
                    i = lower[i..].find(&close).map_or(bytes.len(), |p| i + p + close.len());
                    i = lower[i..].find('>').map_or(bytes.len(), |p| i + p + 1);
                    skipped = true;
                    break;
                }
            }
            if skipped {
                continue;
            }
            if lower[i..].starts_with("<!--") {
                i = lower[i..].find("-->").map_or(bytes.len(), |p| i + p + 3);
                continue;
            }
            out.push(' ');
            i = lower[i..].find('>').map_or(bytes.len(), |p| i + p + 1);
        } else if bytes[i] == b'&' {
            let rest = &html[i..];
            let end = rest.find(';').filter(|&p| p <= 10);
            match end.and_then(|p| decode_entity(&rest[1..p]).map(|c| (c, p))) {
                Some((c, p)) => {
                    out.push(c);
                    i += p + 1;
                }
                None => {
                    out.push('&');
                    i += 1;
                }
            }
        } else {
            let next = html[i..].find(['<', '&']).map_or(html.len(), |p| i + p);
            out.push_str(&html[i..next]);
            i = next;
        }
    }
    out
}

/// Entités HTML d'un texte déjà extrait (aperçus écrits avant le J2 : `&zwnj;`, `&shy;`… restés tels quels).
pub fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        match tail.find(';').filter(|&p| p <= 10).and_then(|p| decode_entity(&tail[1..p]).map(|c| (c, p))) {
            Some((c, p)) => {
                out.push(c);
                rest = &tail[p + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Aperçu stocké, nettoyé pour l'affichage : entités décodées, invisibles retirés.
pub fn clean_preview(preview: &str) -> String {
    if !preview.contains('&') && !preview.chars().any(is_invisible) {
        return preview.to_string();
    }
    preview_from_text(&decode_entities(preview), usize::MAX)
}

fn decode_entity(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match name {
        "nbsp" => ' ',
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "eacute" => 'é',
        "egrave" => 'è',
        "ecirc" => 'ê',
        "agrave" => 'à',
        "acirc" => 'â',
        "ccedil" => 'ç',
        "ocirc" => 'ô',
        "ucirc" => 'û',
        "ugrave" => 'ù',
        "icirc" => 'î',
        "iuml" => 'ï',
        "euml" => 'ë',
        "Eacute" => 'É',
        "rsquo" => '’',
        "lsquo" => '‘',
        "ldquo" => '“',
        "rdquo" => '”',
        "laquo" => '«',
        "raquo" => '»',
        "hellip" => '…',
        "ndash" => '–',
        "mdash" => '—',
        "euro" => '€',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "shy" => '\u{AD}',
        "zwnj" => '\u{200C}',
        "zwj" => '\u{200D}',
        "thinsp" => '\u{2009}',
        "ensp" => '\u{2002}',
        "emsp" => '\u{2003}',
        "deg" => '°',
        "bull" => '•',
        "middot" => '·',
        "dagger" => '†',
        "hearts" => '♥',
        "rarr" => '→',
        "larr" => '←',
        "times" => '×',
        "sect" => '§',
        "acute" => '´',
        "Agrave" => 'À',
        "Acirc" => 'Â',
        "Ccedil" => 'Ç',
        "Egrave" => 'È',
        "Ecirc" => 'Ê',
        "Icirc" => 'Î',
        "Ocirc" => 'Ô',
        "Ucirc" => 'Û',
        "Uuml" => 'Ü',
        "oelig" => 'œ',
        "OElig" => 'Œ',
        "aacute" => 'á',
        "oacute" => 'ó',
        "auml" => 'ä',
        "ouml" => 'ö',
        "uuml" => 'ü',
        "szlig" => 'ß',
        "ntilde" => 'ñ',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_text_and_preview() {
        let html = "<html><head><style>p{}</style></head><body><p>Bonjour&nbsp;Paul,</p><!-- x --><p>&eacute;t&#233; &amp; co</p></body></html>";
        assert_eq!(preview_from_text(&html_to_text(html), 100), "Bonjour Paul, été & co");
        assert_eq!(preview_from_text("a  b\r\n c", 3), "a b");
        assert_eq!(clean_preview("Offre&zwnj; &zwnj;&nbsp;&zwnj; sp&shy;éciale &oelig;uvre &amp; co"), "Offre spéciale œuvre & co");
        assert_eq!(clean_preview("R&D & co"), "R&D & co");
    }

    #[test]
    fn codepages() {
        assert_eq!(decode(b"caf\xe9", 1252), "café");
        assert_eq!(decode("café".as_bytes(), 65001), "café");
        assert_eq!(decode(b"caf\xe9", 28591), "café");
    }
}
