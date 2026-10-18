//! RTF des messages : décompression ([MS-OXRTFCP], crate `compressed-rtf`) et désencapsulation
//! ([MS-OXRTFEX]) du HTML (`\fromhtml1`) ou du texte (`\fromtext`) qu'Outlook y a enveloppé.
//!
//! Pour un RTF « natif » (ni l'un ni l'autre), on n'extrait que le texte, pour l'aperçu ;
//! le RTF lui-même est conservé tel quel par l'appelant.

use crate::encoding_for_codepage;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RtfBody {
    /// HTML d'origine, reconstitué depuis les groupes `\*\htmltag` et le texte hors `\htmlrtf`.
    Html(String),
    /// Texte d'origine (`\fromtext`).
    Text(String),
    /// RTF natif : seul le texte visible est extrait.
    Rtf { text: String },
}

/// Décompresse `PidTagRtfCompressed` en RTF (chaîne ASCII étendue, octet par caractère).
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 16 {
        return Err("RTF compressé tronqué (moins de 16 octets)".into());
    }
    compressed_rtf::decompress_rtf(data)
        .map(|s| s.chars().map(|c| c as u32 as u8).collect())
        .map_err(|e| e.to_string())
}

/// Désencapsule un RTF décompressé.
pub fn decode(rtf: &[u8]) -> RtfBody {
    // Les marqueurs se trouvent dans l'en-tête, avant la table des polices.
    let head = &rtf[..rtf.len().min(1024)];
    let mode = if contains(head, b"\\fromhtml") {
        Mode::Html
    } else if contains(head, b"\\fromtext") {
        Mode::Text
    } else {
        Mode::Native
    };
    let out = Parser::new(rtf, mode).run();
    match mode {
        Mode::Html => RtfBody::Html(out),
        Mode::Text => RtfBody::Text(out),
        Mode::Native => RtfBody::Rtf { text: out },
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Html,
    Text,
    Native,
}

#[derive(Clone, Copy, Default)]
struct GroupState {
    /// Groupe ignoré (destination non affichée : fonttbl, colortbl, `\*` inconnu…).
    skip: bool,
    /// Dans un `\*\htmltag` ou `\*\mhtmltag` : contenu HTML d'origine.
    html_tag: bool,
    /// `\htmlrtf` actif : texte RTF ajouté par l'encapsulation, à ignorer.
    htmlrtf: bool,
    /// Nombre de caractères de repli après `\uN` (`\ucN`).
    uc: u32,
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
    mode: Mode,
    out: String,
    /// Octets `\'hh` en attente de décodage dans la page de code du document.
    pending: Vec<u8>,
    codepage: u32,
    stack: Vec<GroupState>,
    state: GroupState,
    /// Numéro du dernier `\mhtmltag` : le `\htmltag` de même numéro qui suit est redondant.
    last_mhtml: Option<i32>,
    /// Caractères de repli restant à sauter après un `\uN`.
    skip_chars: u32,
}

const SKIPPED_DESTINATIONS: &[&[u8]] = &[
    b"fonttbl", b"colortbl", b"stylesheet", b"info", b"pict", b"object", b"header", b"footer",
    b"headerl", b"headerr", b"footerl", b"footerr", b"listtable", b"listoverridetable",
    b"revtbl", b"rsidtbl", b"generator", b"xmlnstbl", b"themedata", b"colorschememapping",
    b"latentstyles", b"datastore", b"filetbl", b"pgdsctbl", b"mmathPr",
];

impl<'a> Parser<'a> {
    fn new(src: &'a [u8], mode: Mode) -> Self {
        Self {
            src,
            pos: 0,
            mode,
            out: String::with_capacity(src.len() / 2),
            pending: Vec::new(),
            codepage: 1252,
            stack: Vec::new(),
            state: GroupState { uc: 1, ..Default::default() },
            last_mhtml: None,
            skip_chars: 0,
        }
    }

    /// Le texte courant doit-il être émis ?
    fn emitting(&self) -> bool {
        if self.state.skip {
            return false;
        }
        match self.mode {
            Mode::Html => self.state.html_tag || !self.state.htmlrtf,
            Mode::Text | Mode::Native => true,
        }
    }

    fn flush(&mut self) {
        if !self.pending.is_empty() {
            let (text, _, _) = encoding_for_codepage(self.codepage).decode(&self.pending);
            self.out.push_str(&text);
            self.pending.clear();
        }
    }

    fn emit_byte(&mut self, b: u8) {
        if self.skip_chars > 0 {
            self.skip_chars -= 1;
            return;
        }
        if self.emitting() {
            self.pending.push(b);
        }
    }

    fn emit_str(&mut self, s: &str) {
        if self.emitting() {
            self.flush();
            self.out.push_str(s);
        }
    }

    fn run(mut self) -> String {
        while self.pos < self.src.len() {
            let b = self.src[self.pos];
            self.pos += 1;
            match b {
                b'{' => {
                    self.stack.push(self.state);
                    self.skip_chars = 0;
                }
                b'}' => {
                    self.flush();
                    if let Some(s) = self.stack.pop() {
                        self.state = s;
                    }
                    self.skip_chars = 0;
                }
                b'\\' => self.control(),
                b'\r' | b'\n' => {}
                _ => self.emit_byte(b),
            }
        }
        self.flush();
        self.out
    }

    /// Lit un mot de contrôle ou un symbole après `\`.
    fn control(&mut self) {
        let Some(&c) = self.src.get(self.pos) else { return };
        if !c.is_ascii_alphabetic() {
            self.pos += 1;
            match c {
                b'\'' => {
                    let hex = self.src.get(self.pos..self.pos + 2).unwrap_or_default();
                    self.pos += hex.len();
                    if let Ok(v) = u8::from_str_radix(std::str::from_utf8(hex).unwrap_or(""), 16) {
                        self.emit_byte(v);
                    }
                }
                b'*' => {
                    // Destination facultative : ignorée sauf htmltag/mhtmltag, traités au mot suivant.
                    let word = self.peek_word();
                    if word != b"htmltag" && word != b"mhtmltag" {
                        self.state.skip = true;
                    }
                }
                b'\\' | b'{' | b'}' => self.emit_byte(c),
                b'~' => self.emit_str("\u{a0}"),
                b'_' => self.emit_str("\u{2011}"),
                b'\r' | b'\n' => self.emit_str("\r\n"),
                _ => {}
            }
            return;
        }
        let start = self.pos;
        while self.src.get(self.pos).is_some_and(u8::is_ascii_alphabetic) {
            self.pos += 1;
        }
        let word = &self.src[start..self.pos];
        let num_start = self.pos;
        if self.src.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        while self.src.get(self.pos).is_some_and(u8::is_ascii_digit) {
            self.pos += 1;
        }
        let param: Option<i32> = std::str::from_utf8(&self.src[num_start..self.pos]).ok().and_then(|s| s.parse().ok());
        if self.src.get(self.pos) == Some(&b' ') {
            self.pos += 1;
        }
        self.word(word, param);
    }

    fn peek_word(&self) -> &'a [u8] {
        let mut p = self.pos;
        if self.src.get(p) == Some(&b'\\') {
            p += 1;
        }
        let start = p;
        while self.src.get(p).is_some_and(u8::is_ascii_alphabetic) {
            p += 1;
        }
        &self.src[start..p]
    }

    fn word(&mut self, word: &[u8], param: Option<i32>) {
        match word {
            b"ansicpg" => {
                if let Some(cp) = param.and_then(|p| u32::try_from(p).ok()) {
                    self.flush();
                    self.codepage = cp;
                }
            }
            b"htmltag" => {
                if self.mode == Mode::Html {
                    if param.is_some() && param == self.last_mhtml {
                        self.state.skip = true;
                    } else {
                        self.state.html_tag = true;
                    }
                    self.last_mhtml = None;
                }
            }
            b"mhtmltag" => {
                if self.mode == Mode::Html {
                    self.state.html_tag = true;
                    self.last_mhtml = param;
                }
            }
            b"htmlrtf" => self.state.htmlrtf = param != Some(0),
            b"uc" => self.state.uc = param.and_then(|p| u32::try_from(p).ok()).unwrap_or(1),
            b"u" => {
                if let Some(p) = param {
                    let code = if p < 0 { (p + 65536) as u32 } else { p as u32 };
                    if let Some(ch) = char::from_u32(code) {
                        let mut buf = [0; 4];
                        self.emit_str(ch.encode_utf8(&mut buf));
                    }
                    self.skip_chars = self.state.uc;
                }
            }
            b"par" | b"line" => self.emit_str("\r\n"),
            b"tab" => self.emit_str("\t"),
            b"emdash" => self.emit_str("—"),
            b"endash" => self.emit_str("–"),
            b"lquote" => self.emit_str("‘"),
            b"rquote" => self.emit_str("’"),
            b"ldblquote" => self.emit_str("“"),
            b"rdblquote" => self.emit_str("”"),
            b"bullet" => self.emit_str("•"),
            w if SKIPPED_DESTINATIONS.contains(&w) => self.state.skip = true,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fromhtml_round_trip() {
        // Exemple réduit dans l'esprit de [MS-OXRTFEX] §3.
        let rtf = br"{\rtf1\ansi\ansicpg1252\fromhtml1 \deff0{\fonttbl{\f0\fswiss Arial;}}
{\*\htmltag19 <html>}{\*\htmltag34 <head>}{\*\htmltag41 </head>}
{\*\htmltag50 <body>}\htmlrtf {\htmlrtf0 {\*\htmltag64 <p>}caf\'e9 \{ok\}\htmlrtf\par\htmlrtf0 {\*\htmltag72 </p>}
\htmlrtf }\htmlrtf0 {\*\htmltag58 </body>}{\*\htmltag27 </html>}}";
        let RtfBody::Html(html) = decode(rtf) else { panic!("mode HTML attendu") };
        assert_eq!(html, "<html><head></head><body><p>café {ok}</p></body></html>");
    }

    #[test]
    fn fromtext_and_unicode() {
        let rtf = br"{\rtf1\ansi\ansicpg1252\fromtext \deff0{\fonttbl{\f0 Courier;}}
Bonjour\par \u8364?20 \'e0 vous}";
        assert_eq!(decode(rtf), RtfBody::Text("Bonjour\r\n€20 à vous".into()));
    }

    #[test]
    fn native_text_only() {
        let rtf = br"{\rtf1\ansi{\fonttbl{\f0 Arial;}}{\colortbl;\red0\green0\blue0;}{\*\generator x;}\f0 Texte \b gras\b0.\par}";
        assert_eq!(decode(rtf), RtfBody::Rtf { text: "Texte gras.\r\n".into() });
    }
}
