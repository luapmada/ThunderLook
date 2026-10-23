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
    let (notes, _) = extras(c, data_dir);
    let phone = |label: &str| c.phones.iter().find(|(l, _)| *l == label).map(|(_, v)| v.clone()).unwrap_or_default();
    let email = |i: usize| c.emails.get(i).map(|m| m.address.clone()).unwrap_or_default();
    let (y, m, d) = c.birthday_ms.map(crate::day).unwrap_or((0, 0, 0));
    ContactForm {
        id: Some(format!("p{id}")),
        folder_id: format!("k{}", c.folder_id),
        given_name: c.given_name.clone(),
        surname: c.surname.clone(),
        company: c.company.clone(),
        job_title: c.job_title.clone(),
        department: c.department.clone(),
        email1: email(0),
        email2: email(1),
        email3: email(2),
        mobile: phone("Mobile"),
        business: phone("Bureau"),
        home_phone: phone("Domicile"),
        work_street: c.work.street.clone(),
        work_city: c.work.city.clone(),
        work_postal: c.work.postal_code.clone(),
        work_country: c.work.country.clone(),
        home_street: c.home.street.clone(),
        home_city: c.home.city.clone(),
        home_postal: c.home.postal_code.clone(),
        home_country: c.home.country.clone(),
        web_page: c.web_page.clone(),
        birthday: if y > 0 { format!("{y:04}-{m:02}-{d:02}") } else { String::new() },
        notes: notes.unwrap_or_default(),
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
    text(&mut v, Some(0x3A06), &f.given_name);
    text(&mut v, Some(0x3A11), &f.surname);
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
    for (id, val) in ids.work.iter().zip([&f.work_street, &f.work_city, &String::new(), &f.work_postal, &f.work_country]) {
        text(&mut v, *id, val);
    }
    for (id, val) in [0x3A5D, 0x3A59, 0x3A5C, 0x3A5B, 0x3A5A].into_iter().zip([&f.home_street, &f.home_city, &String::new(), &f.home_postal, &f.home_country]) {
        text(&mut v, Some(id), val);
    }
    text(&mut v, Some(0x3A51), &f.web_page);
    text(&mut v, ids.html, "");
    v.push((0x3A42, if f.birthday.trim().is_empty() { None } else { Some(Value::time_ms(day_ms(f.birthday.trim())?)) }));
    Ok(v)
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
    let mut set = props_of(&named, f, &n)?;
    set.retain(|(_, v)| v.is_some());
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
    let eml = write_eml(data_dir, source_id, id, &n.file_as, &f.notes, None)?;
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
    let set = props_of(&named, f, &n)?;
    let props = bag::merge(&old, &set)?;
    let all = load(conn, None, Some(message_id))?;
    let photo = all.contacts.first().and_then(|c| extras(c, data_dir).1).map(|(_, bytes)| bytes);
    let eml = write_eml(data_dir, source_id, message_id, &n.file_as, &f.notes, photo.as_deref())?;
    conn.execute(
        "UPDATE message SET subject = ?2, modified_at = ?3, preview = ?4, eml_path = ?5, props = ?6, indexed = 0 WHERE id = ?1",
        params![message_id, n.file_as, now_ms(), preview(&f.notes), eml, props],
    )
    .map_err(e)?;
    conn.execute("DELETE FROM message_fts WHERE rowid = ?1", [message_id]).map_err(e)?;
    Ok(())
}

/// Suppression d'un contact.
pub fn delete(conn: &mut Connection, message_id: i64) -> Result<()> {
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
}
