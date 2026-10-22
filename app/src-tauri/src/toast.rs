//! Notifications Windows de ThunderLook, avec sa propre identité (AUMID) et une activation par lien `thunderlook:` :
//! un clic, sur la notification à l'écran ou dans le Centre de notifications, fait lancer par Windows
//! `thunderlook.exe thunderlook:open?source=3&message=123` ; l'instance ouverte reçoit ce lien (greffon
//! single-instance) et ouvre le message. Le greffon de notification de Tauri ne convient pas : hors installation, il
//! affiche les notifications sous l'identité de PowerShell, et le clic dans le Centre de notifications ne revient pas.

use std::path::Path;

use windows::{
    Data::Xml::Dom::XmlDocument,
    UI::Notifications::{ToastNotification, ToastNotificationManager},
    Win32::System::Registry::{HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey, RegCreateKeyExW, RegSetValueExW},
    core::{HSTRING, PCWSTR},
};

pub const AUMID: &str = "fr.local.thunderlook";
pub const SCHEME: &str = "thunderlook";

/// Écrit une valeur texte (nom vide : valeur par défaut) sous `HKCU\<path>`.
fn set_value(path: &str, name: &str, value: &str) -> Result<(), String> {
    let path = HSTRING::from(path);
    let mut key = HKEY::default();
    unsafe {
        RegCreateKeyExW(HKEY_CURRENT_USER, &path, None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut key, None)
            .ok()
            .map_err(|e| format!("registre {path} : {e}"))?;
        let data: Vec<u8> = value.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
        let name = HSTRING::from(name);
        let r = RegSetValueExW(key, &name, None, REG_SZ, Some(&data)).ok().map_err(|e| format!("registre {path} : {e}"));
        let _ = RegCloseKey(key);
        r
    }
}

/// Identité des notifications (nom, icône) et lien `thunderlook:` vers l'exécutable en cours, dans
/// `HKCU\Software\Classes` (rien hors du profil de l'utilisateur). Refait à chaque démarrage : l'exécutable peut avoir
/// changé de place.
pub fn register(icon: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let aumid = format!("Software\\Classes\\AppUserModelId\\{AUMID}");
    set_value(&aumid, "DisplayName", "ThunderLook")?;
    set_value(&aumid, "IconUri", &icon.display().to_string())?;
    let scheme = format!("Software\\Classes\\{SCHEME}");
    set_value(&scheme, "", "URL:ThunderLook")?;
    set_value(&scheme, "URL Protocol", "")?;
    set_value(&format!("{scheme}\\DefaultIcon"), "", &format!("\"{}\",0", exe.display()))?;
    set_value(&format!("{scheme}\\shell\\open\\command"), "", &format!("\"{}\" \"%1\"", exe.display()))
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Lien d'ouverture d'une notification de nouveaux messages.
pub fn open_uri(source: i64, message: Option<i64>) -> String {
    match message {
        Some(m) => format!("{SCHEME}:open?source={source}&message={m}"),
        None => format!("{SCHEME}:open?source={source}"),
    }
}

/// Lit un lien `thunderlook:open?source=…[&message=…]` : (source, message).
pub fn parse_uri(arg: &str) -> Option<(i64, Option<i64>)> {
    let query = arg.strip_prefix(SCHEME)?.strip_prefix(':')?.trim_end_matches('/').strip_prefix("open?")?;
    let get = |k: &str| query.split('&').find_map(|p| p.strip_prefix(k)?.strip_prefix('=')).and_then(|v| v.parse().ok());
    Some((get("source")?, get("message")))
}

/// Affiche une notification (titre puis lignes) ; le clic lance `launch`.
pub fn show(title: &str, lines: &[&str], launch: &str) -> Result<(), String> {
    let texts: String = std::iter::once(title).chain(lines.iter().copied()).take(3).map(|t| format!("<text>{}</text>", escape(t))).collect();
    let xml = format!(
        r#"<toast activationType="protocol" launch="{}"><visual><binding template="ToastGeneric">{texts}</binding></visual></toast>"#,
        escape(launch)
    );
    let e = |x: windows::core::Error| format!("notification Windows : {x}");
    let doc = XmlDocument::new().map_err(e)?;
    doc.LoadXml(&HSTRING::from(xml)).map_err(e)?;
    let toast = ToastNotification::CreateToastNotification(&doc).map_err(e)?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID)).map_err(e)?.Show(&toast).map_err(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_round_trip() {
        assert_eq!(parse_uri(&open_uri(3, Some(42))), Some((3, Some(42))));
        assert_eq!(parse_uri(&open_uri(4, None)), Some((4, None)));
        assert_eq!(parse_uri("thunderlook:open?source=5/"), Some((5, None)));
        assert_eq!(parse_uri("thunderlook:autre"), None);
        assert_eq!(parse_uri("C:\\chemin"), None);
    }
}
