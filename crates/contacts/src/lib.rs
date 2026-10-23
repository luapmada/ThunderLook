//! Contacts (J10) : éléments `IPM.Contact` importés, dont le sac MAPI complet est conservé dans `message.props`.
//! Comme le calendrier (option A), Outlook fait foi : lecture seule ici. Propriétés d'après [MS-OXOCNTC].

pub mod view;

use std::collections::HashMap;

use thunderlook_calendar::{
    bag::Bag,
    named::{Named, PS_PUBLIC_STRINGS},
};
use thunderlook_store::rusqlite::Connection;

pub type Result<T> = std::result::Result<T, String>;

pub const PSETID_ADDRESS: &str = "00062004-0000-0000-C000-000000000046";

/// Adresse postale (professionnelle, domicile ou autre).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Address {
    pub street: String,
    pub city: String,
    pub state: String,
    pub postal_code: String,
    pub country: String,
}

impl Address {
    pub fn is_empty(&self) -> bool {
        [&self.street, &self.city, &self.state, &self.postal_code, &self.country].iter().all(|s| s.is_empty())
    }

    /// Présentation d'Outlook en France : rue, puis « code ville », puis région et pays.
    pub fn lines(&self) -> Vec<String> {
        let mut out: Vec<String> = self.street.lines().map(str::to_string).collect();
        let city = [self.postal_code.as_str(), self.city.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ");
        for l in [city, self.state.clone(), self.country.clone()] {
            if !l.is_empty() {
                out.push(l);
            }
        }
        out
    }
}

/// Adresse de messagerie (Courrier 1 à 3 d'Outlook).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Email {
    pub address: String,
    /// Nom affiché pour cette adresse (« Jean Dupont (jean@exemple.fr) » chez Outlook).
    pub display: String,
}

#[derive(Clone, Debug, Default)]
pub struct Contact {
    pub id: i64,
    pub source_id: i64,
    pub folder_id: i64,
    /// Nom complet (`PidTagDisplayName`).
    pub display_name: String,
    /// Classer sous (`PidLidFileUnder`) : ordre et en-tête des fiches d'Outlook.
    pub file_as: String,
    pub prefix: String,
    pub given_name: String,
    pub middle_name: String,
    pub surname: String,
    pub suffix: String,
    pub nickname: String,
    pub company: String,
    pub job_title: String,
    pub department: String,
    pub office: String,
    pub emails: Vec<Email>,
    /// Téléphones renseignés, dans l'ordre des libellés d'Outlook.
    pub phones: Vec<(&'static str, String)>,
    pub work: Address,
    pub home: Address,
    pub other: Address,
    /// Adresse postale choisie (`PidLidPostalAddressId`) : 1 domicile, 2 bureau, 3 autre.
    pub mailing: i64,
    pub web_page: String,
    pub im_address: String,
    /// Dates en millisecondes Unix (minuit UTC du jour pour les contacts d'Outlook).
    pub birthday_ms: Option<i64>,
    pub anniversary_ms: Option<i64>,
    pub spouse: String,
    pub assistant: String,
    pub manager: String,
    pub categories: Vec<String>,
    pub has_picture: bool,
    /// `.eml` de l'élément (notes et photo).
    pub eml_path: Option<String>,
}

impl Contact {
    /// Libellé de la fiche dans la liste : « Classer sous » (sur une ligne), sinon nom complet, société, adresse.
    pub fn title(&self) -> String {
        [self.file_as.replace("\r\n", " "), self.display_name.clone(), self.company.clone()]
            .into_iter()
            .find(|s| !s.trim().is_empty())
            .or_else(|| self.emails.first().map(|e| e.address.clone()))
            .unwrap_or_default()
    }
}

/// Téléphones : (libellé d'Outlook, propriété fixe), dans l'ordre de la carte de visite.
const PHONES: [(&str, u16); 19] = [
    ("Bureau", 0x3A08),
    ("Bureau 2", 0x3A1B),
    ("Assistant(e)", 0x3A2E),
    ("Rappel", 0x3A02),
    ("Voiture", 0x3A1E),
    ("Société", 0x3A57),
    ("Domicile", 0x3A09),
    ("Domicile 2", 0x3A2F),
    ("Télécopie domicile", 0x3A25),
    ("RNIS", 0x3A2D),
    ("Mobile", 0x3A1C),
    ("Autre", 0x3A1F),
    ("Autre télécopie", 0x3A23),
    ("Récepteur d'appels", 0x3A21),
    ("Principal", 0x3A1A),
    ("Radio", 0x3A1D),
    ("Télex", 0x3A2C),
    ("TTY/TDD", 0x3A4B),
    ("Télécopie bureau", 0x3A24),
];

/// Propriétés que lit ce crate (fixes et nommées de la source) : celles qu'une synchronisation avec Outlook remplace,
/// une propriété absente d'Outlook étant effacée du contact.
pub fn tracked_ids(named: &Named) -> Vec<u16> {
    let ids = Ids::new(named);
    let mut v: Vec<u16> = vec![
        0x3001, 0x3A45, 0x3A06, 0x3A44, 0x3A11, 0x3A05, 0x3A4F, 0x3A16, 0x3A17, 0x3A18, 0x3A19, 0x3A5D, 0x3A59, 0x3A5C, 0x3A5B, 0x3A5A, 0x3A63, 0x3A5F, 0x3A62,
        0x3A61, 0x3A60, 0x3A51, 0x3A42, 0x3A41,
    ];
    v.extend(PHONES.iter().map(|&(_, id)| id));
    v.extend([ids.file_under, ids.file_under_id, ids.postal_id, ids.html, ids.im, ids.has_picture, ids.keywords].into_iter().flatten());
    v.extend(ids.emails.iter().flat_map(|e| [e.0, e.1, e.2, e.3]).flatten());
    v.extend(ids.work.iter().flatten());
    v
}

/// Identifiants locaux des propriétés nommées, pour une source.
struct Ids {
    file_under: Option<u16>,
    file_under_id: Option<u16>,
    /// (nom affiché, type d'adresse, adresse, nom d'origine) des Courriers 1 à 3.
    emails: [(Option<u16>, Option<u16>, Option<u16>, Option<u16>); 3],
    work: [Option<u16>; 5],
    postal_id: Option<u16>,
    html: Option<u16>,
    im: Option<u16>,
    has_picture: Option<u16>,
    keywords: Option<u16>,
}

impl Ids {
    fn new(n: &Named) -> Self {
        let a = |lid| n.id(PSETID_ADDRESS, lid);
        let email = |base: u32| (a(base), a(base + 2), a(base + 3), a(base + 4));
        Self {
            file_under: a(0x8005),
            file_under_id: a(0x8006),
            emails: [email(0x8080), email(0x8090), email(0x80A0)],
            work: [a(0x8045), a(0x8046), a(0x8047), a(0x8048), a(0x8049)],
            postal_id: a(0x8022),
            html: a(0x802B),
            im: a(0x8062),
            has_picture: a(0x8015),
            keywords: n.name(PS_PUBLIC_STRINGS, "Keywords"),
        }
    }
}

fn text(bag: &Bag, id: Option<u16>) -> String {
    id.and_then(|i| bag.text(i)).unwrap_or_default()
}

fn address(bag: &Bag, ids: [Option<u16>; 5]) -> Address {
    Address {
        street: text(bag, ids[0]).replace("\r\n", "\n"),
        city: text(bag, ids[1]),
        state: text(bag, ids[2]),
        postal_code: text(bag, ids[3]),
        country: text(bag, ids[4]),
    }
}

/// « Classer sous » d'après `PidLidFileUnderId` ([MS-OXOCNTC] 2.2.1.1.12), quand la valeur enregistrée manque.
/// Les séparateurs autour d'un champ absent tombent. `None` : valeur saisie à la main (0xFFFFFFFF), absente ou inconnue.
fn file_under(code: i64, c: &Contact) -> Option<String> {
    let join = |parts: &[&str], sep: &str| parts.iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(sep);
    let gm = join(&[&c.given_name, &c.middle_name], " ");
    let comma = join(&[&c.surname, &gm], ", ");
    let space = join(&[&c.surname, &gm], " ");
    // Nom accolé au prénom (formats destinés aux langues d'Asie de l'Est).
    let glued = format!("{}{}", c.surname, gm);
    let lines = |a: &str, b: &str| join(&[a, b], "\r\n");
    Some(match code {
        0 => String::new(),
        0x3001 => c.display_name.clone(),
        0x3A06 => c.given_name.clone(),
        0x3A11 => c.surname.clone(),
        0x3A16 => c.company.clone(),
        0x8017 => comma,
        0x8018 => lines(&c.company, &comma),
        0x8019 => lines(&comma, &c.company),
        0x8030 => glued,
        0x8031 => space,
        0x8032 => lines(&c.company, &glued),
        0x8033 => lines(&c.company, &space),
        0x8034 => lines(&glued, &c.company),
        0x8035 => lines(&space, &c.company),
        0x8036 => join(&[&space, &c.suffix], " "),
        0x8037 => join(&[&gm, &c.surname, &c.suffix], " "),
        0x8038 => join(&[&glued, &c.suffix], " "),
        _ => return None,
    })
}

fn read(bag: &Bag, ids: &Ids, row: &Row) -> Contact {
    let mut c = read_fields(bag, ids, row);
    // Outlook affiche la valeur enregistrée (2 137 fiches sur 2 138 comparées par COM le 22/10) ; le code ne sert
    // qu'à la reconstituer quand elle manque.
    if c.file_as.is_empty() {
        if let Some(f) = ids.file_under_id.and_then(|i| bag.int(i)).and_then(|code| file_under(code & 0xFFFF_FFFF, &c)) {
            c.file_as = f;
        }
    }
    c
}

fn read_fields(bag: &Bag, ids: &Ids, row: &Row) -> Contact {
    let fixed = |id: u16| bag.text(id).unwrap_or_default();
    let emails = ids
        .emails
        .iter()
        .filter_map(|&(display, kind, addr, original)| {
            let address = text(bag, addr);
            let display = text(bag, display);
            // Adresse Exchange (X.500) : Outlook montre l'adresse SMTP gardée dans le nom d'origine, s'il en a une.
            let address = if text(bag, kind).eq_ignore_ascii_case("EX") {
                let original = text(bag, original);
                if original.contains('@') { original } else { address }
            } else {
                address
            };
            (!address.is_empty()).then_some(Email { address, display })
        })
        .collect();
    Contact {
        id: row.id,
        source_id: row.source_id,
        folder_id: row.folder_id,
        display_name: fixed(0x3001),
        file_as: text(bag, ids.file_under),
        prefix: fixed(0x3A45),
        given_name: fixed(0x3A06),
        middle_name: fixed(0x3A44),
        surname: fixed(0x3A11),
        suffix: fixed(0x3A05),
        nickname: fixed(0x3A4F),
        company: fixed(0x3A16),
        job_title: fixed(0x3A17),
        department: fixed(0x3A18),
        office: fixed(0x3A19),
        emails,
        phones: PHONES.iter().filter_map(|&(label, id)| bag.text(id).map(|v| (label, v))).collect(),
        work: address(bag, ids.work),
        home: address(bag, [Some(0x3A5D), Some(0x3A59), Some(0x3A5C), Some(0x3A5B), Some(0x3A5A)]),
        other: address(bag, [Some(0x3A63), Some(0x3A5F), Some(0x3A62), Some(0x3A61), Some(0x3A60)]),
        mailing: ids.postal_id.and_then(|i| bag.int(i)).unwrap_or(0),
        web_page: { let w = text(bag, ids.html); if w.is_empty() { fixed(0x3A51) } else { w } },
        im_address: text(bag, ids.im),
        birthday_ms: bag.time_ms(0x3A42),
        anniversary_ms: bag.time_ms(0x3A41),
        spouse: fixed(0x3A48),
        assistant: fixed(0x3A30),
        manager: fixed(0x3A4E),
        categories: ids.keywords.and_then(|i| bag.strings(i)).unwrap_or_default(),
        has_picture: ids.has_picture.and_then(|i| bag.bool(i)).unwrap_or(false),
        eml_path: Some(row.eml_path.clone()).filter(|p| !p.is_empty()),
    }
}

struct Row {
    id: i64,
    source_id: i64,
    folder_id: i64,
    eml_path: String,
}

/// Un contact illisible, et pourquoi.
#[derive(Clone, Debug)]
pub struct Issue {
    pub id: i64,
    pub subject: String,
    pub detail: String,
}

pub struct Contacts {
    pub contacts: Vec<Contact>,
    pub issues: Vec<Issue>,
}

/// Contacts de la base (`IPM.Contact` et classes dérivées) : tous, ceux d'un dossier, ou un seul.
pub fn load(conn: &Connection, folder: Option<i64>, id: Option<i64>) -> Result<Contacts> {
    let e = |x: thunderlook_store::rusqlite::Error| x.to_string();
    let mut named: HashMap<i64, Ids> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT id, props FROM source WHERE props IS NOT NULL").map_err(e)?;
        let mut rows = stmt.query([]).map_err(e)?;
        while let Some(r) = rows.next().map_err(e)? {
            let raw: Vec<u8> = r.get(1).map_err(e)?;
            named.insert(r.get(0).map_err(e)?, Ids::new(&Named::parse(&raw)?));
        }
    }
    let mut stmt = conn
        .prepare(
            "SELECT m.id, f.source_id, m.folder_id, m.subject, m.props, m.eml_path
             FROM message m JOIN folder f ON f.id = m.folder_id
             WHERE (m.message_class = 'IPM.Contact' OR m.message_class LIKE 'IPM.Contact.%') AND (?1 IS NULL OR m.folder_id = ?1) AND (?2 IS NULL OR m.id = ?2)",
        )
        .map_err(e)?;
    let mut rows = stmt.query([folder, id]).map_err(e)?;
    let (mut contacts, mut issues) = (Vec::new(), Vec::new());
    let none = Ids::new(&Named::default());
    while let Some(r) = rows.next().map_err(e)? {
        let row = Row { id: r.get(0).map_err(e)?, source_id: r.get(1).map_err(e)?, folder_id: r.get(2).map_err(e)?, eml_path: r.get(5).map_err(e)? };
        let subject: String = r.get(3).map_err(e)?;
        let props: Option<Vec<u8>> = r.get(4).map_err(e)?;
        let Some(props) = props else {
            issues.push(Issue { id: row.id, subject, detail: "sac MAPI absent".into() });
            continue;
        };
        match Bag::parse(&props) {
            Ok(bag) => {
                contacts.push(read(&bag, named.get(&row.source_id).unwrap_or(&none), &row));
            }
            Err(detail) => issues.push(Issue { id: row.id, subject, detail }),
        }
    }
    Ok(Contacts { contacts, issues })
}

/// Mention de la source d'un dossier : lecture seule, fichier et date de l'import.
pub fn source_note(conn: &Connection, folder: i64) -> Result<Option<String>> {
    let e = |x: thunderlook_store::rusqlite::Error| x.to_string();
    let row: Option<(String, String, i64)> = conn
        .query_row(
            "SELECT s.kind, s.origin_path, s.imported_at FROM folder f JOIN source s ON s.id = f.source_id WHERE f.id = ?1",
            [folder],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map(Some)
        .or_else(|x| if matches!(x, thunderlook_store::rusqlite::Error::QueryReturnedNoRows) { Ok(None) } else { Err(x) })
        .map_err(e)?;
    Ok(row.filter(|(kind, _, _)| kind == "pst").map(|(_, path, at)| {
        let name = std::path::Path::new(&path).file_name().map_or(path.clone(), |n| n.to_string_lossy().into_owned());
        let (y, m, d) = day(at);
        format!("Lecture seule — Outlook fait foi. Source : « {name} », lu le {d:02}/{m:02}/{y}.")
    }))
}

/// Jour (année, mois, jour) d'un anniversaire : Outlook l'enregistre à minuit local converti en UTC (veille à 22 h ou
/// 23 h) ou à midi UTC selon les versions ; arrondir à midi donne le bon jour dans tous les cas.
pub fn day(ms: i64) -> (i64, u32, u32) {
    let days = (ms + 43_200_000).div_euclid(86_400_000);
    // Jours depuis 1970-01-01 → date civile (algorithme de H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Notes (texte du `.eml`) et photo (pièce jointe `ContactPicture.jpg` : type MIME et octets) d'un contact.
pub fn extras(c: &Contact, data_dir: &std::path::Path) -> (Option<String>, Option<(String, Vec<u8>)>) {
    use mail_parser::MimeHeaders;
    let Some(bytes) = c.eml_path.as_deref().and_then(|p| std::fs::read(thunderlook_store::resolve(data_dir, p)).ok()) else { return (None, None) };
    let Some(msg) = mail_parser::MessageParser::default().parse(&bytes) else { return (None, None) };
    let notes = msg.body_text(0).map(|t| t.into_owned()).filter(|t| !t.trim().is_empty());
    let photo = msg.attachments().find(|a| a.attachment_name().is_some_and(|n| n.eq_ignore_ascii_case("ContactPicture.jpg"))).map(|a| {
        let mime = a.content_type().map(|ct| format!("{}/{}", ct.ctype(), ct.subtype().unwrap_or("jpeg"))).unwrap_or_else(|| "image/jpeg".into());
        (mime, a.contents().to_vec())
    });
    (notes, photo)
}
