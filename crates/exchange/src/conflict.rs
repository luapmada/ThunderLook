//! Conflits d'échange : un élément peut avoir changé dans ThunderLook ET sur le téléphone depuis le dernier export.
//!
//! Le manifeste retient, par élément exporté, l'empreinte (SHA-256) de sa valeur canonique (voir [`fingerprint`]).
//! À l'import, trois empreintes se comparent : celle du manifeste (`exportée`), celle du fichier (`téléphone`) et celle
//! de la base aujourd'hui (`ici`).
//!
//! | téléphone = exportée | ici = exportée | verdict |
//! |---|---|---|
//! | oui | — | [`Verdict::Intact`] : le téléphone n'a rien changé, la valeur de ThunderLook fait foi, rien n'est proposé |
//! | non | oui | [`Verdict::PhoneEdit`] : modification du téléphone, proposée normalement |
//! | non | non | [`Verdict::Conflict`] : les deux ont changé |
//!
//! Sans empreinte pour l'élément (manifeste d'avant ce format, ou élément absent de l'export) : [`Verdict::Conflict`]
//! d'origine [`Origin::Unknown`], par prudence.
//!
//! Un conflit est rendu à part ([`crate::ImportReport::conflicts`]), jamais coché d'office, et [`crate::apply`] ne
//! l'applique que si l'utilisateur a choisi ([`Resolution`]).

use serde::{Deserialize, Serialize};

use crate::sha256;

/// Empreinte de la valeur canonique d'un élément (sa sérialisation JSON, déterministe pour un même type).
pub(crate) fn fingerprint<T: Serialize>(value: &T) -> String {
    sha256::hex(serde_json::to_string(value).unwrap_or_default().as_bytes())
}

/// Pourquoi un changement est classé conflit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    /// Les deux côtés ont changé depuis l'export.
    Both,
    /// La valeur exportée n'est pas connue (ancien manifeste) : impossible de savoir lequel a changé.
    Unknown,
}

/// Choix de l'utilisateur pour un conflit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Resolution {
    /// Garder la valeur de ThunderLook : rien n'est écrit, le prochain export la renverra au téléphone.
    KeepThunderlook,
    /// Garder celle du téléphone : appliquée comme une modification (annulable ; la corbeille interne reste le filet).
    KeepPhone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Intact,
    PhoneEdit,
    Conflict(Origin),
}

/// Classe un élément dont le fichier diffère de la base. `exported` : empreinte du manifeste, si elle existe.
pub(crate) fn classify(exported: Option<&str>, phone: &str, here: &str) -> Verdict {
    match exported {
        None => Verdict::Conflict(Origin::Unknown),
        Some(e) if e == phone => Verdict::Intact,
        Some(e) if e == here => Verdict::PhoneEdit,
        Some(_) => Verdict::Conflict(Origin::Both),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classement() {
        assert_eq!(classify(Some("a"), "a", "b"), Verdict::Intact);
        assert_eq!(classify(Some("a"), "b", "a"), Verdict::PhoneEdit);
        assert_eq!(classify(Some("a"), "b", "c"), Verdict::Conflict(Origin::Both));
        assert_eq!(classify(None, "b", "c"), Verdict::Conflict(Origin::Unknown));
    }

    #[test]
    fn empreinte_stable() {
        assert_eq!(fingerprint(&"x"), fingerprint(&"x"));
        assert_ne!(fingerprint(&"x"), fingerprint(&"y"));
    }
}
