//! Contacts (J10) : dossiers, liste et fiche calculés par le cœur, que l'interface affiche tels quels. Lecture seule
//! (Outlook fait foi, comme pour le calendrier).

use serde::{Deserialize, Serialize};

/// Dossier de contacts (« Mes contacts » d'Outlook).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactFolder {
    /// `k<dossier>`.
    pub id: String,
    pub name: String,
    /// Fichier de données (nom affiché de la source).
    pub section: String,
    pub count: u32,
}

/// Ligne de la liste des contacts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactRow {
    /// `p<message>`.
    pub id: String,
    pub title: String,
    /// Société, sinon première adresse électronique.
    pub subtitle: String,
    pub initials: String,
    /// Lettre de l'index (A à Z, « # » pour le reste).
    pub letter: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactList {
    pub folder_id: String,
    pub rows: Vec<ContactRow>,
    /// Contacts illisibles (avec la raison).
    pub issues: Vec<String>,
    /// Mention de la source (lecture seule, date de la copie du PST).
    pub source_note: Option<String>,
}

/// Une ligne de la fiche : libellé et valeur (« Mobile », « 06 … »).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactField {
    pub label: String,
    pub value: String,
    /// Adresse électronique (lien « Nouveau message ») ou page web (lien externe).
    pub kind: ContactFieldKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContactFieldKind {
    #[default]
    Text,
    Email,
    Phone,
    Web,
}

/// Fiche d'un contact, par sections (Contact, Adresses, Dates, Autres).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactCard {
    pub id: String,
    pub title: String,
    pub full_name: String,
    /// Fonction, service et société sur une ligne.
    pub job: String,
    pub initials: String,
    /// Photo en `data:image/…;base64`, si le contact en a une.
    pub photo: Option<String>,
    pub sections: Vec<(String, Vec<ContactField>)>,
    pub categories: Vec<String>,
    pub notes: Option<String>,
}

/// Contact saisi dans la boîte « Contact » (création ou modification).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ContactForm {
    /// `p<message>` ; absent pour un nouveau contact.
    pub id: Option<String>,
    /// Dossier `k<id>`.
    pub folder_id: String,
    pub given_name: String,
    pub surname: String,
    pub company: String,
    pub job_title: String,
    pub department: String,
    pub email1: String,
    pub email2: String,
    pub email3: String,
    pub mobile: String,
    pub business: String,
    pub home_phone: String,
    pub work_street: String,
    pub work_city: String,
    pub work_postal: String,
    pub work_country: String,
    pub home_street: String,
    pub home_city: String,
    pub home_postal: String,
    pub home_country: String,
    pub web_page: String,
    /// `AAAA-MM-JJ`, vide si inconnu.
    pub birthday: String,
    pub notes: String,
}
