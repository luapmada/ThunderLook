//! Écriture des contacts dans la base de ThunderLook : création, modification, suppression. Les propriétés sont écrites
//! dans le sac MAPI de la ligne ([MS-OXOCNTC]) comme Outlook les aurait écrites ; notes et photo vont dans le `.eml`
//! (`eml/<source>/tl/<id>.eml`), la photo existante étant conservée à la modification.

use std::path::Path;

use base64::Engine;
use thunderlook_calendar::{
    bag::{self, Value},
    named::Named,
};
use thunderlook_model::contacts::ContactForm;
use thunderlook_store::rusqlite::{Connection, OptionalExtension, params};

use crate::{Contact, Ids, Result, extras, load};

fn e(x: thunderlook_store::rusqlite::Error) -> String {
    x.to_string()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

fn join(parts: &[&str], sep: &str) -> String {
    parts.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(sep)
}

/// Formulaire d'un contact existant (`p<message>`), avec ses notes.
pub fn form_of(conn: &Connection, data_dir: &Path, id: i64) -> Result<ContactForm> {
    let all = load(conn, None, Some(id))?;
    Ok(form_from(all.contacts.first().ok_or("contact introuvable")?, data_dir))
}

/// Formulaire d'un contact déjà lu (export en série : un seul chargement pour tout le dossier).
pub fn form_from(c: &Contact, data_dir: &Path) -> ContactForm {
    let id = c.id;
    let (notes, photo) = extras(c, data_dir);
    let phone = |label: &str| c.phones.iter().find(|(l, _)| *l == label).map(|(_, v)| v.clone()).unwrap_or_default();
    let email = |i: usize| c.emails.get(i).map(|m| m.address.clone()).unwrap_or_default();
    let (y, m, d) = c.birthday_ms.map(crate::day).unwrap_or((0, 0, 0));
    ContactForm {
        id: Some(format!("p{id}")),
        folder_id: format!("k{}", c.folder_id),
        prefix: c.prefix.clone(),
        given_name: c.given_name.clone(),
        middle_name: c.middle_name.clone(),
        surname: c.surname.clone(),
        suffix: c.suffix.clone(),
        nickname: c.nickname.clone(),
        company: c.company.clone(),
        job_title: c.job_title.clone(),
        department: c.department.clone(),
        email1: email(0),
        email2: email(1),
        email3: email(2),
        mobile: phone("Mobile"),
        business: phone("Bureau"),
        home_phone: phone("Domicile"),
        business_fax: phone("Télécopie bureau"),
        home_fax: phone("Télécopie domicile"),
        business2: phone("Bureau 2"),
        home2: phone("Domicile 2"),
        car_phone: phone("Voiture"),
        pager: phone("Récepteur d'appels"),
        other_phone: phone("Autre"),
        work_street: c.work.street.clone(),
        work_city: c.work.city.clone(),
        work_postal: c.work.postal_code.clone(),
        work_country: c.work.country.clone(),
        home_street: c.home.street.clone(),
        home_city: c.home.city.clone(),
        home_postal: c.home.postal_code.clone(),
        home_country: c.home.country.clone(),
        other_street: c.other.street.clone(),
        other_city: c.other.city.clone(),
        other_postal: c.other.postal_code.clone(),
        other_country: c.other.country.clone(),
        web_page: c.web_page.clone(),
        im_address: c.im_address.clone(),
        spouse: c.spouse.clone(),
        birthday: if y > 0 { format!("{y:04}-{m:02}-{d:02}") } else { String::new() },
        notes: notes.unwrap_or_default(),
        photo: photo.filter(|(_, b)| crate::is_jpeg(b)).map(|(_, b)| crate::photo_uri(&b)),
        categories: c.categories.join(", "),
    }
}

fn named_of(conn: &Connection, source_id: i64) -> Result<Named> {
    let raw: Vec<u8> = conn.query_row("SELECT props FROM source WHERE id = ?1", [source_id], |r| r.get::<_, Option<Vec<u8>>>(0)).map_err(e)?.unwrap_or_default();
    Named::parse(&raw)
}

/// Minuit UTC du jour `AAAA-MM-JJ` en millisecondes Unix (la lecture arrondit à midi : `crate::day`).
fn day_ms(date: &str) -> Result<i64> {
    let d = chrono_free_date(date).ok_or_else(|| format!("date invalide : {date}"))?;
    Ok(d * 86_400_000)
}

/// Jours depuis 1970-01-01 d'une date `AAAA-MM-JJ` (H. Hinnant).
fn chrono_free_date(s: &str) -> Option<i64> {
    let mut p = s.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, d) = (p.next()??, p.next()??, p.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let yy = if m <= 2 { y - 1 } else { y };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

struct Names {
    display: String,
    file_as: String,
}

fn names(f: &ContactForm) -> Names {
    let display = join(&[&f.given_name, &f.surname], " ");
    let display = if display.is_empty() { f.company.trim().to_string() } else { display };
    let file_as = if !f.surname.trim().is_empty() && !f.given_name.trim().is_empty() {
        join(&[&f.surname, &f.given_name], ", ")
    } else {
        display.clone()
    };
    Names { display, file_as }
}

fn text(v: &mut Vec<(u16, Option<Value>)>, id: Option<u16>, s: &str) {
    if let Some(id) = id {
        let s = s.trim();
        v.push((id, if s.is_empty() { None } else { Some(Value::Unicode(s.to_string())) }));
    }
}

fn props_of(named: &Named, f: &ContactForm, n: &Names) -> Result<Vec<(u16, Option<Value>)>> {
    let ids = Ids::new(named);
    let mut v: Vec<(u16, Option<Value>)> = Vec::new();
    text(&mut v, Some(0x3001), &n.display);
    text(&mut v, Some(0x3A45), &f.prefix);
    text(&mut v, Some(0x3A06), &f.given_name);
    text(&mut v, Some(0x3A44), &f.middle_name);
    text(&mut v, Some(0x3A11), &f.surname);
    text(&mut v, Some(0x3A05), &f.suffix);
    text(&mut v, Some(0x3A4F), &f.nickname);
    text(&mut v, Some(0x3A16), &f.company);
    text(&mut v, Some(0x3A17), &f.job_title);
    text(&mut v, Some(0x3A18), &f.department);
    text(&mut v, ids.file_under, &n.file_as);
    // Trois adresses : nom affiché « Nom (adresse) », type SMTP, adresse.
    for (slot, addr) in ids.emails.iter().zip([&f.email1, &f.email2, &f.email3]) {
        let (display, kind, address, original) = *slot;
        let addr = addr.trim();
        let shown = if addr.is_empty() { String::new() } else { format!("{} ({addr})", if n.display.is_empty() { addr } else { &n.display }) };
        text(&mut v, display, &shown);
        text(&mut v, kind, if addr.is_empty() { "" } else { "SMTP" });
        text(&mut v, address, addr);
        text(&mut v, original, "");
    }
    text(&mut v, Some(0x3A1C), &f.mobile);
    text(&mut v, Some(0x3A08), &f.business);
    text(&mut v, Some(0x3A09), &f.home_phone);
    for (id, val) in [
        (0x3A24, &f.business_fax),
        (0x3A25, &f.home_fax),
        (0x3A1B, &f.business2),
        (0x3A2F, &f.home2),
        (0x3A1E, &f.car_phone),
        (0x3A21, &f.pager),
        (0x3A1F, &f.other_phone),
    ] {
        text(&mut v, Some(id), val);
    }
    // La région (3ᵉ propriété de chaque adresse) n'est pas dans le formulaire : on n'y touche pas.
    let w = &ids.work;
    for (id, val) in [(w[0], &f.work_street), (w[1], &f.work_city), (w[3], &f.work_postal), (w[4], &f.work_country)] {
        text(&mut v, id, val);
    }
    let addrs = [
        ([0x3A5D, 0x3A59, 0x3A5B, 0x3A5A], [&f.home_street, &f.home_city, &f.home_postal, &f.home_country]),
        ([0x3A63, 0x3A5F, 0x3A61, 0x3A60], [&f.other_street, &f.other_city, &f.other_postal, &f.other_country]),
    ];
    for (ids4, vals) in addrs {
        for (id, val) in ids4.into_iter().zip(vals) {
            text(&mut v, Some(id), val);
        }
    }
    text(&mut v, Some(0x3A51), &f.web_page);
    text(&mut v, ids.im, &f.im_address);
    text(&mut v, Some(0x3A48), &f.spouse);
    text(&mut v, ids.html, "");
    let cats: Vec<String> = f.categories.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    if let Some(id) = ids.keywords {
        v.push((id, if cats.is_empty() { None } else { Some(Value::Strings(cats)) }));
    }
    v.push((0x3A42, if f.birthday.trim().is_empty() { None } else { Some(Value::time_ms(day_ms(f.birthday.trim())?)) }));
    Ok(v)
}

/// Ce que le formulaire demande pour la photo.
enum PhotoOp {
    Keep,
    Remove,
    Set(Vec<u8>),
}

fn photo_op(f: &ContactForm) -> Result<PhotoOp> {
    match f.photo.as_deref().map(str::trim) {
        None => Ok(PhotoOp::Keep),
        Some("") => Ok(PhotoOp::Remove),
        Some(uri) => crate::photo_bytes(uri).map(PhotoOp::Set),
    }
}

/// `PidLidHasPicture` (named 0x8015) ; absente de la table de noms de la source : rien à écrire.
fn has_picture(named: &Named, set: &mut Vec<(u16, Option<Value>)>, on: bool) {
    if let Some(id) = Ids::new(named).has_picture {
        set.push((id, Some(Value::Bool(on))));
    }
}

fn write_eml(data_dir: &Path, source_id: i64, id: i64, subject: &str, body: &str, photo: Option<&[u8]>) -> Result<String> {
    let rel = format!("eml/{source_id}/tl/{id}.eml");
    let path = thunderlook_store::resolve(data_dir, &rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|x| x.to_string())?;
    }
    let subject = subject.replace(['\r', '\n'], " ");
    let text = match photo {
        None => format!("MIME-Version: 1.0\r\nSubject: {subject}\r\nContent-Type: text/plain; charset=\"utf-8\"\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{body}"),
        Some(jpg) => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(jpg);
            let lines = b64.as_bytes().chunks(76).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join("\r\n");
            format!(
                "MIME-Version: 1.0\r\nSubject: {subject}\r\nContent-Type: multipart/mixed; boundary=\"tl-contact\"\r\n\r\n--tl-contact\r\nContent-Type: text/plain; charset=\"utf-8\"\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{body}\r\n--tl-contact\r\nContent-Type: image/jpeg; name=\"ContactPicture.jpg\"\r\nContent-Disposition: attachment; filename=\"ContactPicture.jpg\"\r\nContent-Transfer-Encoding: base64\r\n\r\n{lines}\r\n--tl-contact--\r\n"
            )
        }
    };
    std::fs::write(&path, text).map_err(|x| x.to_string())?;
    Ok(rel)
}

fn preview(body: &str) -> String {
    body.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect()
}

fn check(f: &ContactForm) -> Result<()> {
    if names(f).display.is_empty() && f.email1.trim().is_empty() {
        return Err("Indiquez au moins un nom, une société ou une adresse électronique.".into());
    }
    Ok(())
}

/// Nouveau contact dans le dossier `k<id>` ; rend l'identifiant du message.
pub fn create(conn: &mut Connection, data_dir: &Path, folder_id: i64, f: &ContactForm) -> Result<i64> {
    check(f)?;
    let source_id: i64 = conn.query_row("SELECT source_id FROM folder WHERE id = ?1", [folder_id], |r| r.get(0)).map_err(|_| "dossier de contacts inconnu".to_string())?;
    let named = named_of(conn, source_id)?;
    let n = names(f);
    let photo = match photo_op(f)? {
        PhotoOp::Set(b) => Some(b),
        _ => None,
    };
    let mut set = props_of(&named, f, &n)?;
    set.retain(|(_, v)| v.is_some());
    if photo.is_some() {
        has_picture(&named, &mut set, true);
    }
    if let Some(id) = Ids::new(&named).file_under_id {
        // Classer sous « Nom, Prénom » (comme le choix par défaut d'Outlook).
        set.push((id, Some(Value::Long(0x8017))));
    }
    let props = bag::merge(&[], &set)?;
    let nid: i64 = conn.query_row("SELECT MAX(COALESCE(MAX(nid), 0), 1879048192) + 1 FROM message WHERE folder_id = ?1", [folder_id], |r| r.get(0)).map_err(e)?;
    let now = now_ms();
    let tx = conn.transaction().map_err(e)?;
    tx.execute(
        "INSERT INTO message (folder_id, nid, message_class, subject, from_name, from_addr, to_line, cc_line, created_at, modified_at, received_at,
            message_flags, is_read, flag_status, importance, sensitivity, size, attachment_count, body_kind, preview, eml_path, props)
         VALUES (?1, ?2, 'IPM.Contact', ?3, '', '', '', '', ?4, ?4, ?4, 0, 1, 0, 1, 0, 0, 0, 'text', ?5, '', ?6)",
        params![folder_id, nid, n.file_as, now, preview(&f.notes), props],
    )
    .map_err(e)?;
    let id = tx.last_insert_rowid();
    let eml = write_eml(data_dir, source_id, id, &n.file_as, &f.notes, photo.as_deref())?;
    tx.execute("UPDATE message SET eml_path = ?2 WHERE id = ?1", params![id, eml]).map_err(e)?;
    tx.commit().map_err(e)?;
    Ok(id)
}

/// Modification d'un contact (`p<message>` → identifiant). Le reste de ses propriétés et sa photo sont conservés.
pub fn update(conn: &mut Connection, data_dir: &Path, message_id: i64, f: &ContactForm) -> Result<()> {
    check(f)?;
    let row: Option<(i64, Vec<u8>)> = conn
        .query_row("SELECT f.source_id, m.props FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1", [message_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .map_err(e)?;
    let (source_id, old) = row.ok_or("contact introuvable")?;
    let named = named_of(conn, source_id)?;
    let n = names(f);
    let mut set = props_of(&named, f, &n)?;
    let photo = match photo_op(f)? {
        PhotoOp::Set(b) => {
            has_picture(&named, &mut set, true);
            Some(b)
        }
        // Retrait : valeur de `PidLidHasPicture` après suppression non établie (REA-LIMITES.md) ; on écrit « faux ».
        PhotoOp::Remove => {
            has_picture(&named, &mut set, false);
            None
        }
        PhotoOp::Keep => {
            let all = load(conn, None, Some(message_id))?;
            all.contacts.first().and_then(|c| extras(c, data_dir).1).map(|(_, bytes)| bytes)
        }
    };
    let props = bag::merge(&old, &set)?;
    let eml = write_eml(data_dir, source_id, message_id, &n.file_as, &f.notes, photo.as_deref())?;
    conn.execute(
        "UPDATE message SET subject = ?2, modified_at = ?3, preview = ?4, eml_path = ?5, props = ?6, indexed = 0 WHERE id = ?1",
        params![message_id, n.file_as, now_ms(), preview(&f.notes), eml, props],
    )
    .map_err(e)?;
    conn.execute("DELETE FROM message_fts WHERE rowid = ?1", [message_id]).map_err(e)?;
    Ok(())
}

/// Suppression d'un contact ou d'une liste de distribution (annulable : copie dans la corbeille interne).
pub fn delete(conn: &mut Connection, message_id: i64) -> Result<()> {
    thunderlook_safety::snapshot_conn(conn, message_id)?;
    conn.execute("DELETE FROM message_fts WHERE rowid = ?1", [message_id]).map_err(e)?;
    conn.execute("DELETE FROM message WHERE id = ?1", [message_id]).map_err(e)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_outlook() {
        let f = ContactForm { given_name: "Cathie".into(), surname: "Vandé".into(), ..Default::default() };
        let n = names(&f);
        assert_eq!((n.display.as_str(), n.file_as.as_str()), ("Cathie Vandé", "Vandé, Cathie"));
        let f = ContactForm { company: "ADI".into(), ..Default::default() };
        assert_eq!(names(&f).file_as, "ADI");
    }

    #[test]
    fn birthday_round_trip() {
        assert_eq!(day_ms("1970-01-02").unwrap(), 86_400_000);
        assert_eq!(crate::day(day_ms("1985-04-23").unwrap()), (1985, 4, 23));
    }

    /// Base de test : une source dont la table de noms a `PidLidHasPicture` et `Keywords`, un dossier de contacts.
    fn fixture(name: &str) -> (std::path::PathBuf, Connection) {
        let dir = std::env::temp_dir().join(format!("tl-contacts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let conn = thunderlook_store::open(&dir).unwrap();
        let mut names = Vec::new();
        names.extend_from_slice(&0x8000u16.to_le_bytes());
        names.extend_from_slice(crate::PSETID_ADDRESS.as_bytes());
        names.push(0);
        names.extend_from_slice(&0x8015u32.to_le_bytes());
        names.extend_from_slice(&0x8001u16.to_le_bytes());
        names.extend_from_slice(thunderlook_calendar::named::PS_PUBLIC_STRINGS.as_bytes());
        names.push(1);
        names.extend_from_slice(&8u32.to_le_bytes());
        names.extend_from_slice(b"Keywords");
        // Messagerie instantanée, adresse professionnelle (0x8045 à 0x8049) et trois courriels.
        let mut lids = vec![0x8062u32, 0x8045, 0x8046, 0x8047, 0x8048, 0x8049];
        lids.extend([0x8080u32, 0x8090, 0x80A0].into_iter().flat_map(|b| [b, b + 2, b + 3, b + 4]));
        for (i, lid) in lids.into_iter().enumerate() {
            names.extend_from_slice(&(0x8002u16 + i as u16).to_le_bytes());
            names.extend_from_slice(crate::PSETID_ADDRESS.as_bytes());
            names.push(0);
            names.extend_from_slice(&lid.to_le_bytes());
        }
        conn.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at, props) VALUES (1, 'tl', '', '', 't', 0, 0, ?1)", [names]).unwrap();
        conn.execute("INSERT INTO folder (id, source_id, nid, name, role, hidden, position) VALUES (1, 1, 1, 'Contacts', 'folder', 0, 0)", []).unwrap();
        (dir, conn)
    }

    fn jpeg(seed: u8) -> String {
        crate::photo_uri(&(0..64u8).fold(vec![0xFF, 0xD8, 0xFF, 0xE0], |mut v, i| {
            v.push(i.wrapping_mul(seed));
            v
        }))
    }

    #[test]
    fn photo_et_categories_aller_retour() {
        let (dir, mut conn) = fixture("photo");
        let f = ContactForm { given_name: "Léa".into(), surname: "Martin".into(), notes: "Une note".into(), photo: Some(jpeg(3)), categories: "Clients, VIP,, ".into(), ..Default::default() };
        let id = create(&mut conn, &dir, 1, &f).unwrap();
        let c = load(&conn, None, Some(id)).unwrap().contacts.remove(0);
        assert!(c.has_picture);
        assert_eq!(c.categories, ["Clients", "VIP"]);
        let back = form_of(&conn, &dir, id).unwrap();
        assert_eq!((back.photo.as_deref(), back.categories.as_str(), back.notes.as_str()), (f.photo.as_deref(), "Clients, VIP", "Une note"));

        // Modification sans photo dans le formulaire : conservée ; les notes changent, la photo reste.
        let edit = ContactForm { photo: None, notes: "Autre note".into(), categories: "VIP".into(), ..back.clone() };
        update(&mut conn, &dir, id, &edit).unwrap();
        let again = form_of(&conn, &dir, id).unwrap();
        assert_eq!((again.photo.as_deref(), again.notes.as_str(), again.categories.as_str()), (f.photo.as_deref(), "Autre note", "VIP"));

        // Remplacement.
        update(&mut conn, &dir, id, &ContactForm { photo: Some(jpeg(5)), ..again.clone() }).unwrap();
        assert_eq!(form_of(&conn, &dir, id).unwrap().photo, Some(jpeg(5)));

        // Retrait (`Some("")`), catégories vidées.
        update(&mut conn, &dir, id, &ContactForm { photo: Some(String::new()), categories: String::new(), ..again }).unwrap();
        let c = load(&conn, None, Some(id)).unwrap().contacts.remove(0);
        assert!(!c.has_picture && c.categories.is_empty());
        let gone = form_of(&conn, &dir, id).unwrap();
        assert_eq!((gone.photo, gone.notes.as_str()), (None, "Autre note"));

        // Pas un JPEG : refusé, rien n'est écrit.
        let bad = ContactForm { surname: "X".into(), photo: Some("data:image/png;base64,iVBORw0KGgo=".into()), ..Default::default() };
        assert!(create(&mut conn, &dir, 1, &bad).is_err());
    }

    /// Formulaire où chaque champ porte une valeur distincte (aucun champ n'est laissé vide).
    fn tout_rempli() -> ContactForm {
        ContactForm {
            prefix: "Dr".into(),
            given_name: "Léa".into(),
            middle_name: "Marie".into(),
            surname: "Martin".into(),
            suffix: "Jr".into(),
            nickname: "Lélé".into(),
            company: "ADI".into(),
            job_title: "Directrice".into(),
            department: "Atelier".into(),
            email1: "lea@x.fr".into(),
            email2: "lea2@x.fr".into(),
            email3: "lea3@x.fr".into(),
            mobile: "0601".into(),
            business: "0502".into(),
            home_phone: "0503".into(),
            business_fax: "0504".into(),
            home_fax: "0505".into(),
            business2: "0506".into(),
            home2: "0507".into(),
            car_phone: "0608".into(),
            pager: "0609".into(),
            other_phone: "0510".into(),
            work_street: "1 rue A".into(),
            work_city: "Fenioux".into(),
            work_postal: "17350".into(),
            work_country: "France".into(),
            home_street: "2 rue B".into(),
            home_city: "Niort".into(),
            home_postal: "79000".into(),
            home_country: "France".into(),
            other_street: "3 rue C".into(),
            other_city: "Surgères".into(),
            other_postal: "17700".into(),
            other_country: "France".into(),
            web_page: "https://x.fr".into(),
            im_address: "lea@jabber.x".into(),
            spouse: "Paul".into(),
            birthday: "1980-04-12".into(),
            notes: "Une note".into(),
            categories: "Clients".into(),
            ..Default::default()
        }
    }

    #[test]
    fn tous_les_champs_survivent_a_la_base_puis_au_vcf() {
        let (dir, mut conn) = fixture("champs");
        let f = ContactForm { photo: Some(jpeg(3)), ..tout_rempli() };
        let id = create(&mut conn, &dir, 1, &f).unwrap();
        let back = form_of(&conn, &dir, id).unwrap();
        assert_eq!(ContactForm { id: None, folder_id: String::new(), ..back.clone() }, f);

        // Base → .vcf → base : aucun champ perdu, dans les deux versions.
        for v in [crate::vcard::Version::V21, crate::vcard::Version::V30] {
            let parsed = crate::vcard::parse(&crate::vcard::export(&[back.clone()], v)).unwrap();
            assert!(parsed[0].warnings.is_empty(), "{v:?} : {:?}", parsed[0].warnings);
            assert_eq!(parsed[0].form, ContactForm { id: None, folder_id: String::new(), ..back.clone() }, "{v:?}");
        }
    }

    #[test]
    fn la_modification_ne_touche_pas_ce_que_le_formulaire_ne_porte_pas() {
        let (dir, mut conn) = fixture("region");
        let id = create(&mut conn, &dir, 1, &tout_rempli()).unwrap();
        // Une région déposée par Outlook (propriété « État » de l'adresse « autre » : 0x3A62), absente du formulaire.
        let old: Vec<u8> = conn.query_row("SELECT props FROM message WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        let withstate = bag::merge(&old, &[(0x3A62, Some(Value::Unicode("Charente-Maritime".into())))]).unwrap();
        conn.execute("UPDATE message SET props = ?2 WHERE id = ?1", params![id, withstate]).unwrap();

        let mut f = form_of(&conn, &dir, id).unwrap();
        f.other_city = "Rochefort".into();
        f.pager = String::new();
        update(&mut conn, &dir, id, &f).unwrap();
        let c = load(&conn, None, Some(id)).unwrap().contacts.remove(0);
        assert_eq!((c.other.state.as_str(), c.other.city.as_str()), ("Charente-Maritime", "Rochefort"));
        let g = form_of(&conn, &dir, id).unwrap();
        assert_eq!((g.pager.as_str(), g.car_phone.as_str(), g.business_fax.as_str()), ("", "0608", "0504"));
    }

    #[test]
    fn photo_dans_l_export_vcard() {
        let (dir, mut conn) = fixture("vcf");
        let f = ContactForm { surname: "Dupont".into(), photo: Some(jpeg(7)), categories: "A, B".into(), ..Default::default() };
        let id = create(&mut conn, &dir, 1, &f).unwrap();
        let all = load(&conn, Some(1), None).unwrap();
        let forms: Vec<_> = all.contacts.iter().map(|c| form_from(c, &dir)).collect();
        let vcf = crate::vcard::export(&forms, crate::vcard::Version::V30);
        let parsed = crate::vcard::parse(&vcf).unwrap();
        assert_eq!((parsed[0].form.photo.as_deref(), parsed[0].form.categories.as_str()), (f.photo.as_deref(), "A, B"));
        // Import : le contact recréé a sa photo.
        let id2 = create(&mut conn, &dir, 1, &ContactForm { id: None, folder_id: String::new(), ..parsed[0].form.clone() }).unwrap();
        assert_ne!(id, id2);
        assert_eq!(form_of(&conn, &dir, id2).unwrap().photo, f.photo);
    }

    #[test]
    fn suppression_puis_annulation_rend_le_meme_formulaire() {
        let (dir, mut conn) = fixture("annuler");
        let f = ContactForm { given_name: "Léa".into(), surname: "Martin".into(), notes: "Une note".into(), photo: Some(jpeg(3)), categories: "Clients, VIP".into(), ..Default::default() };
        let id = create(&mut conn, &dir, 1, &f).unwrap();
        let before = form_of(&conn, &dir, id).unwrap();

        delete(&mut conn, id).unwrap();
        assert!(form_of(&conn, &dir, id).is_err());
        let msg = thunderlook_safety::undo(&mut conn, &dir).unwrap();
        assert_eq!(msg, "Annulé : suppression du contact « Martin, Léa »");
        assert_eq!(form_of(&conn, &dir, id).unwrap(), before);

        // Modification annulée : l'ancien formulaire revient, photo comprise.
        thunderlook_safety::snapshot_edit(&conn, &dir, id).unwrap();
        update(&mut conn, &dir, id, &ContactForm { surname: "Durand".into(), photo: Some(jpeg(5)), notes: "Autre".into(), ..before.clone() }).unwrap();
        assert_eq!(form_of(&conn, &dir, id).unwrap().surname, "Durand");
        thunderlook_safety::undo(&mut conn, &dir).unwrap();
        assert_eq!(form_of(&conn, &dir, id).unwrap(), before);
    }
}
