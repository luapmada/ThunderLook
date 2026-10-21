//! Noms de dossiers IMAP en UTF-7 modifié (RFC 3501 §5.1.3) : ASCII imprimable tel quel sauf « & » (« &- ») ; le
//! reste en UTF-16BE codé en base64 modifié (« , » au lieu de « / », sans « = »), entre « & » et « - ».

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+,";

/// Nom affiché → nom sur le serveur.
pub fn encode(name: &str) -> String {
    let mut out = String::new();
    let mut pending: Vec<u16> = Vec::new();
    let flush = |pending: &mut Vec<u16>, out: &mut String| {
        if pending.is_empty() {
            return;
        }
        let bytes: Vec<u8> = pending.iter().flat_map(|u| u.to_be_bytes()).collect();
        out.push('&');
        for chunk in bytes.chunks(3) {
            let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            let digits = chunk.len() + 1;
            for k in 0..digits {
                out.push(B64[((n >> (18 - 6 * k)) & 63) as usize] as char);
            }
        }
        out.push('-');
        pending.clear();
    };
    for c in name.chars() {
        if (' '..='~').contains(&c) {
            flush(&mut pending, &mut out);
            if c == '&' { out.push_str("&-") } else { out.push(c) }
        } else {
            let mut buf = [0u16; 2];
            pending.extend_from_slice(c.encode_utf16(&mut buf));
        }
    }
    flush(&mut pending, &mut out);
    out
}

/// Nom sur le serveur → nom affiché ; un nom mal formé est rendu tel quel.
pub fn decode(name: &str) -> String {
    let mut out = String::new();
    let mut rest = name;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('-') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let encoded = &after[..end];
        if encoded.is_empty() {
            out.push('&');
        } else {
            let mut bits: u32 = 0;
            let mut nbits = 0;
            let mut bytes = Vec::new();
            for c in encoded.bytes() {
                let Some(v) = B64.iter().position(|b| *b == c) else { return name.to_string() };
                bits = (bits << 6) | v as u32;
                nbits += 6;
                if nbits >= 8 {
                    nbits -= 8;
                    bytes.push((bits >> nbits) as u8);
                    bits &= (1 << nbits) - 1;
                }
            }
            let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
            out.push_str(&String::from_utf16_lossy(&units));
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_rfc_examples() {
        // Exemples de la RFC 3501 §5.1.3 et noms réels de dossiers.
        assert_eq!(encode("~peter/mail/台北/日本語"), "~peter/mail/&U,BTFw-/&ZeVnLIqe-");
        assert_eq!(decode("~peter/mail/&U,BTFw-/&ZeVnLIqe-"), "~peter/mail/台北/日本語");
        assert_eq!(encode("Accusés"), "Accus&AOk-s");
        assert_eq!(encode("Ebay & Paypal"), "Ebay &- Paypal");
        assert_eq!(encode("Hébergements WEB"), "H&AOk-bergements WEB");
        for n in ["Accusés", "Ebay & Paypal", "Hébergements WEB", "LBC | FB", "Éléments envoyés", "[electromagnetisme.fr]", "€uro ✓"] {
            assert_eq!(decode(&encode(n)), n);
        }
        assert_eq!(decode("INBOX"), "INBOX");
    }
}
