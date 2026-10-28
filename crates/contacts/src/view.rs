//! Ce que l'interface affiche : dossiers de contacts, liste triée comme Outlook (par « Classer sous », sans tenir
//! compte de la casse ni des accents) et fiche par sections.

use thunderlook_model::contacts::{ContactCard, ContactField, ContactFieldKind, ContactFolder, ContactList, ContactRow};
use thunderlook_store::rusqlite::Connection;

use crate::{Address, Contact, Result};

/// Dossiers qui contiennent des contacts, Contacts du premier fichier de données d'abord.
pub fn folders(conn: &Connection) -> Result<Vec<ContactFolder>> {
    let e = |x: thunderlook_store::rusqlite::Error| x.to_string();
    let mut stmt = conn
        .prepare(
            "SELECT f.id, f.name, s.display_name, COUNT(*) FROM message m JOIN folder f ON f.id = m.folder_id JOIN source s ON s.id = f.source_id
             WHERE m.message_class = 'IPM.Contact' OR m.message_class LIKE 'IPM.Contact.%'
             GROUP BY f.id ORDER BY s.id, f.name <> 'Contacts', f.name",
        )
        .map_err(e)?;
    stmt.query_map([], |r| Ok(ContactFolder { id: format!("k{}", r.get::<_, i64>(0)?), name: r.get(1)?, section: r.get(2)?, count: r.get(3)? }))
        .map_err(e)?
        .collect::<std::result::Result<_, _>>()
        .map_err(e)
}

/// Clé de tri : minuscules sans accents (« Élodie » avec les E).
pub fn sort_key(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            c => c,
        })
        .collect()
}

/// Initiales de la pastille : prénom et nom, sinon les deux premiers mots du libellé.
pub fn initials(c: &Contact) -> String {
    let first = |s: &str| s.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
    if !c.given_name.is_empty() && !c.surname.is_empty() {
        return format!("{}{}", first(&c.given_name), first(&c.surname));
    }
    c.title().split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).take(2).map(first).collect()
}

fn row(c: &Contact) -> ContactRow {
    let title = c.title();
    let letter = sort_key(&title).chars().find(|c| !c.is_whitespace()).filter(char::is_ascii_lowercase).map_or("#".into(), |c| c.to_ascii_uppercase().to_string());
    let subtitle = if !c.company.is_empty() && c.company != title {
        c.company.clone()
    } else {
        c.emails.first().map(|e| e.address.clone()).unwrap_or_default()
    };
    let mut search: Vec<String> = vec![c.company.clone()];
    search.extend(c.emails.iter().map(|e| e.address.clone()));
    for (_, v) in &c.phones {
        search.push(v.clone());
        search.push(v.chars().filter(char::is_ascii_digit).collect());
    }
    let search = search.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n").to_lowercase();
    let phone = ["Mobile", "Bureau", "Domicile"].iter().find_map(|l| c.phones.iter().find(|(x, v)| x == l && !v.is_empty())).or(c.phones.first()).map(|(_, v)| v.clone()).unwrap_or_default();
    ContactRow {
        id: format!("p{}", c.id),
        title,
        subtitle,
        initials: initials(c),
        letter,
        search,
        company: c.company.clone(),
        email: c.emails.first().map(|e| e.address.clone()).unwrap_or_default(),
        phone,
        ..Default::default()
    }
}

/// Listes de distribution d'un dossier, en lignes `d<message>` (membres dans `members`), triées par nom.
pub fn dist_rows(conn: &Connection, folder_id: i64) -> Result<Vec<ContactRow>> {
    Ok(crate::distlist::find(conn, Some(folder_id))?
        .into_iter()
        .map(|f| {
            let n = f.list.members.len();
            let unresolved = if f.unresolved > 0 { format!(" ({} non résolu{})", f.unresolved, if f.unresolved > 1 { "s" } else { "" }) } else { String::new() };
            ContactRow {
                title: f.list.name.clone(),
                subtitle: format!("{n} membre{}{unresolved}", if n > 1 { "s" } else { "" }),
                initials: f.list.name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_default(),
                search: f.list.members.iter().flat_map(|m| [m.name.as_str(), m.email.as_str()]).collect::<Vec<_>>().join("\n").to_lowercase(),
                id: f.list.id,
                kind: thunderlook_model::contacts::ContactRowKind::DistList,
                members: f.list.members,
                ..Default::default()
            }
        })
        .collect())
}

/// Liste d'un dossier, triée.
pub fn list(contacts: &crate::Contacts, folder_id: i64) -> ContactList {
    let mut rows: Vec<(String, ContactRow)> =
        contacts.contacts.iter().filter(|c| c.folder_id == folder_id).map(|c| (sort_key(&c.title()), row(c))).collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.title.cmp(&b.1.title)));
    ContactList {
        folder_id: format!("k{folder_id}"),
        rows: rows.into_iter().map(|(_, r)| r).collect(),
        issues: contacts.issues.iter().map(|i| format!("« {} » : {}", i.subject, i.detail)).collect(),
        source_note: None,
    }
}

fn field(label: &str, value: &str, kind: ContactFieldKind) -> Option<ContactField> {
    (!value.trim().is_empty()).then(|| ContactField { label: label.into(), value: value.trim().into(), kind })
}

fn address(label: &str, a: &Address) -> Option<ContactField> {
    field(label, &a.lines().join("\n"), ContactFieldKind::Text)
}

fn date(ms: Option<i64>) -> String {
    ms.map(|ms| {
        let (y, m, d) = crate::day(ms);
        format!("{d:02}/{m:02}/{y}")
    })
    .unwrap_or_default()
}

/// Fiche : en-tête et sections non vides, dans l'ordre de la fiche « Personnes » d'Outlook.
pub fn card(c: &Contact, notes: Option<String>, photo: Option<String>) -> ContactCard {
    use ContactFieldKind::*;
    let full_name = [c.prefix.as_str(), &c.given_name, &c.middle_name, &c.surname, &c.suffix]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    let job = [c.job_title.as_str(), &c.department, &c.company].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(", ");
    let mut contact: Vec<ContactField> = c
        .emails
        .iter()
        .enumerate()
        .filter_map(|(i, e)| field(&if c.emails.len() > 1 { format!("Courrier {}", i + 1) } else { "Courrier".into() }, &e.address, Email))
        .collect();
    contact.extend(c.phones.iter().filter_map(|(l, v)| field(l, v, Phone)));
    contact.extend(field("Messagerie instantanée", &c.im_address, Text));
    contact.extend(field("Page web", &c.web_page, Web));
    let addresses: Vec<ContactField> =
        [address("Bureau", &c.work), address("Domicile", &c.home), address("Autre", &c.other)].into_iter().flatten().collect();
    let dates: Vec<ContactField> =
        [field("Anniversaire", &date(c.birthday_ms), Text), field("Anniversaire de mariage", &date(c.anniversary_ms), Text)].into_iter().flatten().collect();
    let others: Vec<ContactField> = [
        field("Surnom", &c.nickname, Text),
        field("Bureau (lieu)", &c.office, Text),
        field("Responsable", &c.manager, Text),
        field("Assistant(e)", &c.assistant, Text),
        field("Conjoint(e)", &c.spouse, Text),
    ]
    .into_iter()
    .flatten()
    .collect();
    let sections = [("Contact", contact), ("Adresses", addresses), ("Dates", dates), ("Autres informations", others)]
        .into_iter()
        .filter(|(_, f)| !f.is_empty())
        .map(|(t, f)| (t.to_string(), f))
        .collect();
    ContactCard {
        id: format!("p{}", c.id),
        title: c.title(),
        full_name: [full_name, c.display_name.clone(), c.title()].into_iter().find(|s| !s.trim().is_empty()).unwrap_or_default(),
        job,
        initials: initials(c),
        photo,
        sections,
        categories: c.categories.clone(),
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Email;

    #[test]
    fn la_recherche_couvre_societe_courriel_telephone() {
        let c = Contact {
            id: 3,
            surname: "Martin".into(),
            company: "Affûtage Charentes".into(),
            emails: vec![Email { address: "Lea@Exemple.fr".into(), display: String::new() }],
            phones: vec![("Mobile", "06 12 34 56 78".into())],
            ..Default::default()
        };
        let r = row(&c);
        for needle in ["affûtage", "lea@exemple", "06 12 34", "0612345678"] {
            assert!(r.search.contains(needle), "{needle} dans {:?}", r.search);
        }
    }
}
