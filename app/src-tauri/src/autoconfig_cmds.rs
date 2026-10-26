//! Configuration automatique d'un compte (lot BJ) : propose des serveurs à partir de l'adresse. Lecture seule : rien
//! n'est enregistré, la vérification de la connexion reste celle d'`add_account`.

use thunderlook_sync::autoconfig::Detected;

/// Cherche les serveurs IMAP/SMTP d'une adresse (fichiers d'autoconfiguration, MX, SRV).
#[tauri::command(async)]
pub fn detect_account(address: String) -> Result<Detected, String> {
    // Fil à part : la commande tourne déjà sur le runtime de Tauri.
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        rt.block_on(thunderlook_sync::autoconfig::detect(&address))
    })
    .join()
    .map_err(|_| "recherche interrompue".to_string())?
}
