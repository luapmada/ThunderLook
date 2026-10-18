//! Vérification indépendante d'un dossier de données après import : chaque `.eml` est relu par
//! mail-parser (autre bibliothèque que celle qui l'a écrit) et comparé à la base.
//! Usage : cargo run --release --example verify_store -- <dossier-de-données>

use std::{collections::BTreeMap, env, fs, path::PathBuf};

use mail_parser::{MessageParser, MimeHeaders};
use thunderlook_store::{resolve, rusqlite::params};

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn main() {
    let data = PathBuf::from(env::args().nth(1).expect("dossier de données"));
    let conn = thunderlook_store::open(&data).expect("base");
    let mut stmt = conn
        .prepare("SELECT m.id, m.subject, m.body_kind, m.eml_path, (SELECT COUNT(*) FROM attachment a WHERE a.message_id = m.id) FROM message m")
        .unwrap();
    let rows = stmt
        .query_map(params![], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?))
        })
        .unwrap();
    let parser = MessageParser::default();
    let mut checked = 0u64;
    let mut problems: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for row in rows {
        let (id, subject, body_kind, rel, attachments) = row.unwrap();
        checked += 1;
        let mut flag = |kind: &'static str, detail: String| problems.entry(kind).or_default().push(format!("#{id} {detail}"));
        let bytes = match fs::read(resolve(&data, &rel)) {
            Ok(b) => b,
            Err(e) => {
                flag("fichier absent", e.to_string());
                continue;
            }
        };
        let Some(msg) = parser.parse(&bytes) else {
            flag("eml illisible", rel);
            continue;
        };
        if norm(msg.subject().unwrap_or("")) != norm(&subject) {
            flag("sujet différent", format!("{:?} ≠ {:?}", msg.subject(), subject));
        }
        // mail-parser range la variante text/rtf d'un corps RTF natif parmi les pièces jointes.
        let parsed = msg
            .attachments()
            .filter(|a| {
                a.attachment_name().is_some()
                    || !a.content_type().is_some_and(|c| c.ctype() == "text" && c.subtype() == Some("rtf"))
            })
            .count() as i64;
        if parsed != attachments {
            let names: Vec<String> = msg.attachments().map(|a| a.attachment_name().unwrap_or("?").to_string()).collect();
            flag("pièces jointes", format!("eml {parsed} ≠ base {attachments} {names:?}"));
        }
        let ok = match body_kind.as_str() {
            "html" | "rtf-html" => msg.body_html(0).is_some(),
            "text" | "rtf-text" | "rtf" => msg.body_text(0).is_some(),
            _ => true,
        };
        if !ok {
            flag("corps manquant", body_kind);
        }
    }
    println!("{checked} messages relus");
    if problems.is_empty() {
        println!("aucun écart");
    }
    for (kind, list) in &problems {
        println!("{kind} : {}", list.len());
        for p in list.iter().take(5) {
            println!("   {p}");
        }
    }
}
