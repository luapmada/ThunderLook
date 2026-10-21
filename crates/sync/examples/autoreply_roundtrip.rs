//! Réponse automatique réelle (dossier de données de test) : relève, réponses aux nouveaux messages de la Boîte de
//! réception, puis envoi. L'état (`replied`) est gardé dans `<dossier>/autoreply-test.json` entre deux lancements.
//! Usage : `cargo run -p thunderlook-sync --example autoreply_roundtrip -- <dossier de test> <source>`

use std::path::Path;

use thunderlook_model::AutoReply;
use thunderlook_sync::{account, autoreply, sync_account_blocking};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, source) = (Path::new(&args[0]), args[1].parse::<i64>().map_err(|e| e.to_string())?);
    let state = dir.join("autoreply-test.json");
    let mut cfg: AutoReply = std::fs::read_to_string(&state).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(AutoReply {
        enabled: true,
        text: "Je suis absent jusqu'au 31 octobre. Réponse automatique de ThunderLook (essai du J7).".into(),
        ..Default::default()
    });
    let r = sync_account_blocking(dir, source)?;
    println!("relève : {} nouveau(x), dont {} dans la Boîte de réception", r.new_messages, r.new_inbox_ids.len());
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    let acc = account::account(&conn, source)?;
    let replied = autoreply::queue_replies(&conn, dir, &acc, &r.new_inbox_ids, &cfg)?;
    println!("réponses préparées pour : {replied:?} (déjà servis : {:?})", cfg.replied);
    cfg.replied.extend(replied);
    std::fs::write(&state, serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let r = sync_account_blocking(dir, source)?;
    println!("envoi : {} envoyé(s), erreur {:?}", r.sent, r.send_error);
    Ok(())
}
