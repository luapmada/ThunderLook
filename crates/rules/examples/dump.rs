//! Affiche les règles d'un flux (`.rwz` ou `PR_RW_RULES_STREAM` extrait).
//! Usage : `cargo run -p thunderlook-rules --example dump -- <fichier> [--json]`

use thunderlook_rules::{describe, parse};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.iter().find(|a| !a.starts_with("--")).expect("chemin du flux attendu");
    let bytes = std::fs::read(path).expect("lecture du flux");
    let stream = parse(&bytes).unwrap_or_else(|e| panic!("{path} : {e}"));
    if args.iter().any(|a| a == "--json") {
        println!("{}", serde_json::to_string_pretty(&stream).expect("JSON"));
        return;
    }
    let ok = stream.rules.iter().filter(|r| r.error.is_none()).count();
    println!("version {:#x} / {:#x}, {} règles, {ok} décodées", stream.version, stream.version2, stream.rules.len());
    for (k, rule) in stream.rules.iter().enumerate() {
        let state = rule.error.as_deref().map_or("ok".to_string(), |e| format!("ÉCHEC : {e}"));
        println!("[{k}] {:?} active={} {state}", rule.name, rule.active);
        for line in describe(rule) {
            println!("      {line}");
        }
    }
    println!("dossier des modèles {:?}", stream.template_dir);
}
