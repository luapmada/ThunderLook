//! Outils texte communs aux formats d'échange (vCard, vCalendar, VNOTE) : lignes repliées, propriétés, quoted-printable,
//! jeux de caractères, échappements. Fonctions pures. Copie identique dans `thunderlook-items` (deux crates sans dépendance commune).

/// Une ligne de contenu : `NOM;PARAM=…:valeur`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prop {
    /// Majuscules, sans préfixe de groupe (`item1.TEL` → `TEL`).
    pub name: String,
    /// Clés en majuscules ; un paramètre nu (vCard 2.1 : `CELL`) devient `TYPE` (`QUOTED-PRINTABLE` → `ENCODING`).
    pub params: Vec<(String, String)>,
    /// Valeur brute, après les deux-points.
    pub value: String,
}

impl Prop {
    pub fn types(&self) -> Vec<String> {
        self.params.iter().filter(|(k, _)| k == "TYPE").map(|(_, v)| v.to_uppercase()).collect()
    }

    pub fn has_type(&self, t: &str) -> bool {
        self.params.iter().any(|(k, v)| k == "TYPE" && v.eq_ignore_ascii_case(t))
    }

    pub fn param(&self, key: &str) -> Option<&str> {
        self.params.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn is_qp(&self) -> bool {
        self.param("ENCODING").is_some_and(|v| v.eq_ignore_ascii_case("QUOTED-PRINTABLE"))
    }
}

/// Lignes logiques : CRLF, LF ou CR ; repliement par espace ou tabulation ; coupures douces du quoted-printable (`=` en
/// fin de ligne d'une propriété QUOTED-PRINTABLE). Les lignes vides hors continuation sont écartées.
pub fn unfold(text: &str) -> Vec<String> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    let mut out: Vec<String> = Vec::new();
    let mut open_qp = false;
    for raw in text.split('\n') {
        if open_qp {
            if let Some(cur) = out.last_mut() {
                cur.pop();
                cur.push_str(raw);
                open_qp = qp_pending(cur);
                continue;
            }
        }
        if raw.starts_with([' ', '\t']) && !out.is_empty() {
            let cur = out.last_mut().expect("non vide");
            cur.push_str(&raw[1..]);
            open_qp = qp_pending(cur);
            continue;
        }
        if raw.trim().is_empty() {
            continue;
        }
        out.push(raw.to_string());
        open_qp = qp_pending(out.last().expect("non vide"));
    }
    out
}

fn qp_pending(line: &str) -> bool {
    let head = line.split(':').next().unwrap_or("").to_uppercase();
    line.ends_with('=') && head.contains("QUOTED-PRINTABLE")
}

/// Découpe une ligne logique ; `None` si elle n'a pas de deux-points ou de nom.
pub fn parse_prop(line: &str) -> Option<Prop> {
    let mut in_quote = false;
    let mut colon = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quote = !in_quote,
            ':' if !in_quote => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let mut parts: Vec<String> = Vec::new();
    let (mut cur, mut q) = (String::new(), false);
    for c in head.chars() {
        match c {
            '"' => {
                q = !q;
                cur.push(c);
            }
            ';' if !q => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    parts.push(cur);
    let raw_name = parts.remove(0);
    let name = raw_name.rsplit('.').next().unwrap_or("").trim().to_uppercase();
    if name.is_empty() {
        return None;
    }
    let mut params = Vec::new();
    for p in parts {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        match p.split_once('=') {
            Some((k, v)) => {
                let k = k.trim().to_uppercase();
                let v = v.trim();
                if k == "TYPE" {
                    for t in v.trim_matches('"').split(',') {
                        params.push((k.clone(), t.trim().to_string()));
                    }
                } else {
                    params.push((k, v.trim_matches('"').to_string()));
                }
            }
            None => {
                let up = p.to_uppercase();
                match up.as_str() {
                    "QUOTED-PRINTABLE" | "BASE64" | "B" | "8BIT" | "7BIT" => params.push(("ENCODING".into(), up)),
                    _ => params.push(("TYPE".into(), p.to_string())),
                }
            }
        }
    }
    Some(Prop { name, params, value: value.to_string() })
}

const CP1252: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ',
    '\u{9d}', 'ž', 'Ÿ',
];

fn cp1252(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| if (0x80..0xA0).contains(&b) { CP1252[(b - 0x80) as usize] } else { b as char }).collect()
}

/// Octets du quoted-printable ; un `=` mal formé est gardé tel quel.
pub fn qp_bytes(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'=' && i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() {
            let h = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("00");
            out.push(u8::from_str_radix(h, 16).unwrap_or(b'?'));
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

/// Valeur d'une propriété, décodée si elle est en quoted-printable (UTF-8, WINDOWS-1252 ou ISO-8859-1 ; un jeu inconnu
/// ou des octets UTF-8 invalides se lisent en WINDOWS-1252, avec un avertissement). Sans QP, la valeur est rendue telle quelle.
pub fn decode_value(p: &Prop, warnings: &mut Vec<String>) -> String {
    if !p.is_qp() {
        return p.value.clone();
    }
    let bytes = qp_bytes(&p.value);
    let cs = p.param("CHARSET").unwrap_or("UTF-8").to_uppercase();
    let text = match cs.as_str() {
        "UTF-8" | "UTF8" | "US-ASCII" | "ASCII" => match String::from_utf8(bytes.clone()) {
            Ok(s) => s,
            Err(_) => {
                warnings.push(format!("{} : octets non UTF-8 lus en WINDOWS-1252", p.name));
                cp1252(&bytes)
            }
        },
        "WINDOWS-1252" | "CP1252" => cp1252(&bytes),
        "ISO-8859-1" | "LATIN1" | "ISO8859-1" => bytes.iter().map(|&b| b as char).collect(),
        other => {
            warnings.push(format!("{} : jeu de caractères {other} inconnu, lu en UTF-8 ou WINDOWS-1252", p.name));
            String::from_utf8(bytes.clone()).unwrap_or_else(|_| cp1252(&bytes))
        }
    };
    text.replace('\r', "")
}

/// Découpe sur `sep` hors échappement `\x` ; les morceaux restent échappés.
pub fn split_unescaped(s: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut esc = false;
    for c in s.chars() {
        if esc {
            cur.push('\\');
            cur.push(c);
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if c == sep {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    if esc {
        cur.push('\\');
    }
    out.push(cur);
    out
}

/// `\n` `\N` `\,` `\;` `\\` `\:` ; une autre séquence est gardée telle quelle.
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.peek().copied() {
                Some('n' | 'N') => {
                    out.push('\n');
                    it.next();
                }
                Some(x @ (',' | ';' | '\\' | ':')) => {
                    out.push(x);
                    it.next();
                }
                _ => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out.replace('\r', "")
}

/// Échappe `\` et `;` (et `,` si demandé) ; saut de ligne → `\n` si `newline`, sinon gardé (pour le QP).
pub fn escape(s: &str, commas: bool, newline: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str(r"\;"),
            ',' if commas => out.push_str("\\,"),
            '\r' => {}
            '\n' if newline => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out
}

/// Repliement à 75 octets, sans couper un caractère ; continuation = CRLF + espace. Rend la ligne SANS CRLF final.
pub fn fold(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + 8);
    let mut len = 0;
    for c in line.chars() {
        let w = c.len_utf8();
        if len + w > 75 {
            out.push_str("\r\n ");
            len = 1;
        }
        out.push(c);
        len += w;
    }
    out
}

/// `PREFIXE` + valeur en quoted-printable (UTF-8), coupures douces `=` pour tenir en 76 caractères. Le saut de ligne est
/// codé `=0A`. Rend la ligne SANS CRLF final.
pub fn qp_line(prefix: &str, value: &str) -> String {
    let bytes: Vec<u8> = value.replace('\r', "").into_bytes();
    let mut out = String::from(prefix);
    let mut len = prefix.chars().count();
    for (i, &b) in bytes.iter().enumerate() {
        let last = i + 1 == bytes.len();
        let tok = match b {
            b'=' => "=3D".to_string(),
            b' ' | b'\t' if last => format!("={b:02X}"),
            33..=126 | b' ' | b'\t' => (b as char).to_string(),
            _ => format!("={b:02X}"),
        };
        if len + tok.len() > 75 {
            out.push_str("=\r\n");
            len = 0;
        }
        out.push_str(&tok);
        len += tok.len();
    }
    out
}

/// Vrai si la valeur doit passer en quoted-printable dans un format 2.1/1.1.
pub fn needs_qp(value: &str) -> bool {
    value.chars().any(|c| !c.is_ascii() || c.is_ascii_control())
}
