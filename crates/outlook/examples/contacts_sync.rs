//! Contacts communs : synchronisation depuis Outlook (ouvert).
//! Usage : `cargo run -p thunderlook-outlook --example contacts_sync -- <dossier de données>`

fn main() -> Result<(), String> {
    let dir = std::env::args().nth(1).ok_or("dossier de données manquant")?;
    let r = thunderlook_outlook::contacts::sync(std::path::Path::new(&dir))?;
    println!("{}", serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?);
    Ok(())
}
