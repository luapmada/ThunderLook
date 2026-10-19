//! Fidélité de la réponse (J5) : compare la réponse à tous préparée par ThunderLook à celle qu'Outlook a écrite
//! pour le même message (lecture seule, rien n'est enregistré).
//! Usage : cargo run -p thunderlook-mailbox --example fidelity -- <données> <id original> <id réponse d'Outlook>

use std::{env, fs, path::PathBuf};

use mail_parser::MessageParser;
use thunderlook_mailbox::{Mailbox, parse_addresses};
use thunderlook_model::ComposeKind;

/// Texte du bloc cité « De : … Objet : … », balises retirées et espaces normalisés.
fn quote_block(html: &str) -> String {
    let start = html.find("border-top:solid #E1E1E1").unwrap_or(0);
    let rest = &html[start..];
    let end = rest.find("Objet&nbsp;:").map(|i| i + rest[i..].find("</p>").unwrap_or(0)).unwrap_or(rest.len());
    let mut out = String::new();
    let mut tag = false;
    for c in rest[..end].chars() {
        match c {
            '<' => tag = true,
            '>' => {
                tag = false;
                out.push(' ');
            }
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    let out = thunderlook_mime::decode_entities(&out).replace('\u{a0}', " ");
    let out: String = out.split_whitespace().collect::<Vec<_>>().join(" ");
    out.trim_start_matches(|c: char| c != 'D').to_string()
}

fn main() {
    let data = PathBuf::from(env::args().nth(1).expect("dossier de données"));
    let original = env::args().nth(2).expect("id du message d'origine");
    let reply = env::args().nth(3).expect("id de la réponse d'Outlook");
    let mb = Mailbox::open(&data).expect("ouverture");
    let ours = mb.compose(ComposeKind::ReplyAll, Some(&original)).expect("réponse");

    let conn = thunderlook_store::open_read_only(&data).expect("base");
    let eml: String = conn
        .query_row("SELECT eml_path FROM message WHERE id = ?1", [reply.trim_start_matches('m').parse::<i64>().unwrap()], |r| r.get(0))
        .expect("réponse d'Outlook");
    let bytes = fs::read(thunderlook_store::resolve(&data, &eml)).expect("eml");
    let theirs = MessageParser::default().parse(&bytes).expect("eml lisible");
    let addrs = |a: Option<&mail_parser::Address>| -> Vec<String> {
        a.map(|a| a.iter().filter_map(|x| x.address().map(str::to_ascii_lowercase)).collect()).unwrap_or_default()
    };
    let ours_list = |s: &str| -> Vec<String> { parse_addresses(s).into_iter().map(|m| m.addr.to_ascii_lowercase()).collect() };

    let mut same = true;
    let mut row = |what: &str, a: String, b: String| {
        let ok = a == b;
        same &= ok;
        println!("{} {what}\n    ThunderLook : {a}\n    Outlook     : {b}", if ok { "=" } else { "≠" });
    };
    row("À", format!("{:?}", ours_list(&ours.to)), format!("{:?}", addrs(theirs.to())));
    row("Cc", format!("{:?}", ours_list(&ours.cc)), format!("{:?}", addrs(theirs.cc())));
    // Les .eml importés avant le schéma v6 gardent le marqueur de préfixe du PST dans « Subject ».
    row("Objet", ours.subject.clone(), thunderlook_store::strip_subject_marker(theirs.subject().unwrap_or("")).to_string());
    row("Bloc cité", quote_block(&ours.html), quote_block(&theirs.body_html(0).unwrap_or_default()));
    println!("{}", if same { "identique" } else { "écarts ci-dessus" });
}
