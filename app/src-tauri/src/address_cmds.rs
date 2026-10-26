//! Autocomplétion des destinataires et Carnet d'adresses (lot BC) : lecture seule.

use serde::Serialize;
use tauri::AppHandle;
use thunderlook_contacts::suggest::{self, Kind};

use crate::data_dir;

/// Une proposition d'adresse (`kind` : `contact`, `list` ou `mail`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressSuggestion {
    pub kind: &'static str,
    pub name: String,
    pub address: String,
    pub insert: String,
    pub detail: String,
}

/// Propositions pour le texte tapé (vide : tout le carnet), au plus `limit` (50 par défaut, 500 au plus).
#[tauri::command(async)]
pub fn address_suggest(app: AppHandle, query: String, limit: Option<u32>) -> Result<Vec<AddressSuggestion>, String> {
    let dir = data_dir(&app)?;
    let conn = thunderlook_store::open_read_only(&dir).map_err(|e| e.to_string())?;
    let limit = limit.unwrap_or(50).min(500) as usize;
    Ok(suggest::suggest(&conn, &dir, &query, limit)?
        .into_iter()
        .map(|s| AddressSuggestion {
            kind: match s.kind {
                Kind::Contact => "contact",
                Kind::List => "list",
                Kind::Mail => "mail",
            },
            name: s.name,
            address: s.address,
            insert: s.insert,
            detail: s.detail,
        })
        .collect())
}
