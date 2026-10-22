//! Calendrier commun : synchronisation depuis Outlook (ouvert). `--dry-run` : rien n'est écrit ; le bilan donne les
//! écarts entre Outlook et la base, par propriété.
//! Usage : `cargo run -p thunderlook-outlook --example calendar_sync -- <dossier de données> [--dry-run]`

fn main() -> Result<(), String> {
    let dir = std::env::args().nth(1).ok_or("dossier de données manquant")?;
    let dry = std::env::args().any(|a| a == "--dry-run");
    let r = thunderlook_outlook::calendar::sync(std::path::Path::new(&dir), dry)?;
    println!("{}", serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?);
    Ok(())
}
