//! `ContactForm` ↔ vCard 2.1 / 3.0 (fichiers `.vcf`, échange avec le téléphone). Fonctions pures, sans base ni fichier.
//!
//! Écriture : 2.1 ou 3.0. Lecture : 2.1, 3.0 et 4.0 (le format exact qu'écrit MPE n'est pas établi). L'identifiant du
//! formulaire part dans `UID` ; à la lecture, `UID` ou à défaut `X-OUTLOOK-UID` va dans [`Parsed::uid`] et le formulaire
//! rendu a `id: None` et `folder_id` vide (à remplir par l'appelant). La photo n'a pas de champ : ignorée, avec avertissement.

use thunderlook_model::contacts::ContactForm;

use crate::textfmt::{self, Prop};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    V21,
    V30,
}

/// Un contact lu. Un enregistrement illisible rend `form == ContactForm::default()` et un avertissement : à ignorer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub form: ContactForm,
    pub uid: Option<String>,
    pub warnings: Vec<String>,
}

// --- Écriture -------------------------------------------------------------------------------------------------------

struct Writer {
    v: Version,
    out: String,
}

impl Writer {
    /// `NOM[;paramètres]:valeur` ; `fields` = champs structurés déjà échappés ou valeur simple (`escaped`).
    fn line(&mut self, name: &str, types: &[&str], value: &str) {
        let multi = value.contains('\n');
        let params = match self.v {
            Version::V21 => types.iter().map(|t| format!(";{t}")).collect::<String>(),
            Version::V30 if types.is_empty() => String::new(),
            Version::V30 => format!(";TYPE={}", types.join(",")),
        };
        let line = match self.v {
            Version::V21 if multi || textfmt::needs_qp(value) || name.len() + params.len() + value.len() > 70 => {
                textfmt::qp_line(&format!("{name}{params};CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:"), value)
            }
            Version::V21 => format!("{name}{params}:{value}"),
            Version::V30 => textfmt::fold(&format!("{name}{params}:{value}")),
        };
        self.out.push_str(&line);
        self.out.push_str("\r\n");
    }

    fn esc(&self, s: &str) -> String {
        match self.v {
            Version::V21 => textfmt::escape(s, false, false),
            Version::V30 => textfmt::escape(s, true, true),
        }
    }
}

pub fn export(forms: &[ContactForm], version: Version) -> String {
    let mut w = Writer { v: version, out: String::new() };
    for f in forms {
        w.out.push_str("BEGIN:VCARD\r\n");
        w.out.push_str(if version == Version::V21 { "VERSION:2.1\r\n" } else { "VERSION:3.0\r\n" });
        let (given, surname) = (w.esc(&f.given_name), w.esc(&f.surname));
        w.line("N", &[], &format!("{surname};{given};;;"));
        let full = [f.given_name.trim(), f.surname.trim()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ");
        let full = if full.is_empty() { f.company.trim().to_string() } else { full };
        w.line("FN", &[], &w.esc(&full));
        if !f.company.is_empty() || !f.department.is_empty() {
            let org = format!("{};{}", w.esc(&f.company), w.esc(&f.department));
            w.line("ORG", &[], org.trim_end_matches(';'));
        }
        if !f.job_title.is_empty() {
            w.line("TITLE", &[], &w.esc(&f.job_title));
        }
        for (i, m) in [&f.email1, &f.email2, &f.email3].into_iter().enumerate().filter(|(_, m)| !m.is_empty()) {
            let pref: &[&str] = if i == 0 { &["PREF", "INTERNET"] } else { &["INTERNET"] };
            w.line("EMAIL", pref, &w.esc(m));
        }
        for (num, types) in [(&f.mobile, &["CELL", "VOICE"][..]), (&f.business, &["WORK", "VOICE"]), (&f.home_phone, &["HOME", "VOICE"])] {
            if !num.is_empty() {
                w.line("TEL", types, &w.esc(num));
            }
        }
        for (t, street, city, postal, country) in [
            ("WORK", &f.work_street, &f.work_city, &f.work_postal, &f.work_country),
            ("HOME", &f.home_street, &f.home_city, &f.home_postal, &f.home_country),
        ] {
            if [street, city, postal, country].iter().any(|s| !s.is_empty()) {
                let v = format!(";;{};{};;{};{}", w.esc(street), w.esc(city), w.esc(postal), w.esc(country));
                w.line("ADR", &[t], &v);
            }
        }
        if !f.web_page.is_empty() {
            w.line("URL", &[], &w.esc(&f.web_page));
        }
        if !f.birthday.is_empty() {
            w.line("BDAY", &[], &f.birthday);
        }
        if !f.notes.is_empty() {
            w.line("NOTE", &[], &w.esc(&f.notes));
        }
        if let Some(id) = f.id.as_deref().filter(|s| !s.is_empty()) {
            w.line("UID", &[], &w.esc(id));
        }
        w.out.push_str("END:VCARD\r\n");
    }
    w.out
}

// --- Lecture --------------------------------------------------------------------------------------------------------

pub fn parse(text: &str) -> Result<Vec<Parsed>, String> {
    let lines = textfmt::unfold(text);
    let mut out = Vec::new();
    let mut cur: Option<Vec<Prop>> = None;
    let mut n = 0;
    for line in &lines {
        let up = line.trim().to_uppercase();
        if up == "BEGIN:VCARD" {
            if let Some(props) = cur.take() {
                n += 1;
                out.push(record(n, props, true));
            }
            cur = Some(Vec::new());
        } else if up == "END:VCARD" {
            if let Some(props) = cur.take() {
                n += 1;
                out.push(record(n, props, false));
            }
        } else if let Some(props) = cur.as_mut() {
            if let Some(p) = textfmt::parse_prop(line) {
                props.push(p);
            }
        }
    }
    if let Some(props) = cur.take() {
        n += 1;
        out.push(record(n, props, true));
    }
    if n == 0 && !text.trim().is_empty() {
        return Err("aucune fiche VCARD dans le fichier".into());
    }
    Ok(out)
}

/// `AAAA-MM-JJ` d'une date vCard (`19800412`, `1980-04-12`, avec ou sans heure) ; `None` sinon.
fn birthday_of(v: &str) -> Option<String> {
    let d = v.trim().split(['T', 't']).next().unwrap_or("");
    let digits: String = d.chars().filter(char::is_ascii_digit).collect();
    if digits.len() != 8 || d.starts_with("--") {
        return None;
    }
    let (m, dd): (u32, u32) = (digits[4..6].parse().ok()?, digits[6..].parse().ok()?);
    ((1..=12).contains(&m) && (1..=31).contains(&dd)).then(|| format!("{}-{}-{}", &digits[..4], &digits[4..6], &digits[6..]))
}

fn field(parts: &[String], i: usize) -> String {
    parts.get(i).map(|s| textfmt::unescape(s).trim().to_string()).unwrap_or_default()
}

fn record(n: usize, props: Vec<Prop>, truncated: bool) -> Parsed {
    let mut w: Vec<String> = Vec::new();
    let mut f = ContactForm::default();
    let (mut uid, mut x_uid) = (None, None);
    let mut emails: Vec<(bool, String)> = Vec::new();
    let (mut fn_name, mut got_n) = (String::new(), false);
    let (mut photo, mut cats, mut urls) = (false, false, 0);
    let mut notes: Vec<String> = Vec::new();
    for p in &props {
        let mut pw = Vec::new();
        let raw = textfmt::decode_value(p, &mut pw);
        w.extend(pw);
        let simple = || textfmt::unescape(&raw).trim().to_string();
        match p.name.as_str() {
            "N" => {
                let parts = textfmt::split_unescaped(&raw, ';');
                (f.surname, f.given_name) = (field(&parts, 0), field(&parts, 1));
                got_n = true;
            }
            "FN" => fn_name = simple(),
            "ORG" => {
                let parts = textfmt::split_unescaped(&raw, ';');
                (f.company, f.department) = (field(&parts, 0), field(&parts, 1));
            }
            "TITLE" => f.job_title = simple(),
            "URL" => {
                urls += 1;
                if urls == 1 {
                    f.web_page = simple();
                } else {
                    w.push(format!("URL en trop ignorée ({})", simple()));
                }
            }
            "NOTE" => notes.push(textfmt::unescape(&raw)),
            "BDAY" => {
                if !raw.trim().is_empty() {
                    match birthday_of(&raw) {
                        Some(d) => f.birthday = d,
                        None => w.push(format!("BDAY illisible ({}), ignoré", raw.trim())),
                    }
                }
            }
            "UID" => uid = Some(simple()).filter(|s| !s.is_empty()),
            "X-OUTLOOK-UID" => x_uid = Some(simple()).filter(|s| !s.is_empty()),
            "EMAIL" => {
                let v = simple();
                if !v.is_empty() {
                    emails.push((p.has_type("PREF"), v.strip_prefix("mailto:").unwrap_or(&v).to_string()));
                }
            }
            "TEL" => {
                let v = simple();
                let v = v.strip_prefix("tel:").unwrap_or(&v).trim().to_string();
                if v.is_empty() {
                    continue;
                }
                let t = p.types();
                let has = |x: &str| t.iter().any(|y| y == x);
                let slot = if has("CELL") {
                    Some(0)
                } else if ["FAX", "PAGER", "CAR", "VIDEO", "MODEM", "ISDN", "BBS", "MSG"].iter().any(|x| has(x)) {
                    None
                } else if has("WORK") {
                    Some(1)
                } else if has("HOME") {
                    Some(2)
                } else {
                    Some(3)
                };
                let slots = [&mut f.mobile, &mut f.business, &mut f.home_phone];
                match slot {
                    None => w.push(format!("téléphone de type {} ignoré ({v})", t.join(","))),
                    Some(3) => match slots.into_iter().find(|s| s.is_empty()) {
                        Some(s) => {
                            *s = v.clone();
                            w.push(format!("téléphone {v} sans type (CELL/WORK/HOME) : rangé dans la première case libre"));
                        }
                        None => w.push(format!("téléphone {v} sans type et sans case libre : ignoré")),
                    },
                    Some(i) => {
                        let s = slots.into_iter().nth(i).expect("case");
                        if s.is_empty() {
                            *s = v;
                        } else {
                            w.push(format!("deuxième téléphone du même type ignoré ({v})"));
                        }
                    }
                }
            }
            "ADR" => {
                let parts = textfmt::split_unescaped(&raw, ';');
                let street = [field(&parts, 1), field(&parts, 2)].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n");
                let a = (street, field(&parts, 3), field(&parts, 5), field(&parts, 6));
                if !field(&parts, 0).is_empty() || !field(&parts, 4).is_empty() {
                    w.push("ADR : boîte postale ou région sans champ, ignorée".into());
                }
                if a.0.is_empty() && a.1.is_empty() && a.2.is_empty() && a.3.is_empty() {
                    continue;
                }
                let t = p.types();
                let work = t.iter().any(|x| x == "WORK");
                let home = t.iter().any(|x| x == "HOME");
                let wf_free = f.work_street.is_empty() && f.work_city.is_empty() && f.work_postal.is_empty() && f.work_country.is_empty();
                let hf_free = f.home_street.is_empty() && f.home_city.is_empty() && f.home_postal.is_empty() && f.home_country.is_empty();
                let to_work = if work {
                    true
                } else if home {
                    false
                } else if hf_free {
                    w.push("ADR sans type (WORK/HOME) : rangée en domicile".into());
                    false
                } else {
                    w.push("ADR sans type (WORK/HOME) : rangée en travail".into());
                    true
                };
                if to_work && wf_free {
                    (f.work_street, f.work_city, f.work_postal, f.work_country) = a;
                } else if !to_work && hf_free {
                    (f.home_street, f.home_city, f.home_postal, f.home_country) = a;
                } else {
                    w.push(format!("deuxième adresse du même type ignorée ({})", a.1));
                }
            }
            "PHOTO" | "LOGO" => photo = true,
            "CATEGORIES" => cats = true,
            _ => {}
        }
    }
    if photo {
        w.push("photo ignorée (le formulaire n'a pas de champ photo)".into());
    }
    if cats {
        w.push("CATEGORIES ignoré (le formulaire n'a pas de champ catégories)".into());
    }
    if !got_n && !fn_name.is_empty() && fn_name != f.company {
        match fn_name.rsplit_once(' ') {
            Some((g, s)) => (f.given_name, f.surname) = (g.trim().to_string(), s.trim().to_string()),
            None => f.surname = fn_name.clone(),
        }
        w.push(format!("N absent : FN « {fn_name} » découpé (dernier mot = nom)"));
    }
    emails.sort_by_key(|(pref, _)| !*pref);
    for (i, (_, m)) in emails.into_iter().enumerate() {
        match i {
            0 => f.email1 = m,
            1 => f.email2 = m,
            2 => f.email3 = m,
            _ => w.push(format!("quatrième adresse électronique ignorée ({m})")),
        }
    }
    f.notes = notes.join("\n").trim_end().to_string();
    if truncated {
        w.push("enregistrement tronqué (END:VCARD absent)".into());
    }
    if f == ContactForm::default() {
        let mut out = vec![format!("fiche {n} illisible : aucun champ utilisable")];
        out.extend(w.into_iter().map(|x| format!("fiche {n} : {x}")));
        return Parsed { form: ContactForm::default(), uid: None, warnings: out };
    }
    Parsed { form: f, uid: uid.or(x_uid), warnings: w.into_iter().map(|x| format!("fiche {n} : {x}")).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> ContactForm {
        ContactForm {
            given_name: "René".into(),
            surname: "Dupont-Léger".into(),
            company: "Société électrique; & Fils, SARL".into(),
            job_title: "Directeur".into(),
            department: "Atelier".into(),
            email1: "rene@example.fr".into(),
            email2: "r.dupont@example.com".into(),
            email3: "autre@example.org".into(),
            mobile: "06 12 34 56 78".into(),
            business: "05 46 00 00 00".into(),
            home_phone: "+33 5 46 11 11 11".into(),
            work_street: "12 rue de l'Église\nBât. B".into(),
            work_city: "Fenioux".into(),
            work_postal: "17350".into(),
            work_country: "France".into(),
            home_street: "3 impasse des Lilas".into(),
            home_city: "Saint-Jean-d'Angély".into(),
            home_postal: "17400".into(),
            home_country: "France".into(),
            web_page: "https://example.fr/a?b=c;d".into(),
            birthday: "1961-04-12".into(),
            notes: "Ligne 1, avec virgule ; point-virgule \\ antislash\nLigne 2 : œuf à 3 €\n\nLigne 4 très longue pour forcer le repliement des lignes de la fiche, au moins une fois.".into(),
            ..ContactForm::default()
        }
    }

    fn back(forms: &[ContactForm], v: Version) -> Vec<ContactForm> {
        let out = export(forms, v);
        if v == Version::V30 {
            assert!(out.split("\r\n").all(|l| l.len() <= 75), "ligne trop longue");
        }
        parse(&out).unwrap().into_iter().map(|p| p.form).collect()
    }

    #[test]
    fn roundtrip_les_deux_versions() {
        let min = ContactForm { surname: "Seul".into(), ..ContactForm::default() };
        let societe = ContactForm { company: "Entreprise".into(), email1: "a@b.fr".into(), ..ContactForm::default() };
        let forms = vec![full(), min, societe];
        for v in [Version::V21, Version::V30] {
            assert_eq!(back(&forms, v), forms, "{v:?}");
        }
    }

    #[test]
    fn uid_roundtrip() {
        let mut f = full();
        f.id = Some("p77".into());
        for v in [Version::V21, Version::V30] {
            let p = parse(&export(&[f.clone()], v)).unwrap();
            assert_eq!(p[0].uid.as_deref(), Some("p77"));
            assert_eq!(p[0].form.id, None);
            assert!(p[0].warnings.is_empty(), "{:?}", p[0].warnings);
        }
    }

    #[test]
    fn vcard_21_qp_accents_replie_notes_multilignes() {
        let txt = "BEGIN:VCARD\r\nVERSION:2.1\r\nN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:Dupont;Ren=C3=A9e;;;\r\n\
FN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:Ren=C3=A9e Dupont\r\n\
ORG;CHARSET=WINDOWS-1252;ENCODING=QUOTED-PRINTABLE:Soci=E9t=E9 =E9lectrique;Atelier\r\n\
TEL;CELL;VOICE:06 12 34 56 78\r\nTEL;WORK:05 46 00 00 00\r\nTEL;HOME;VOICE:05 46 11 11 11\r\nTEL;FAX;WORK:05 46 22 22 22\r\n\
EMAIL;INTERNET;PREF:renee@example.fr\r\nEMAIL;INTERNET:autre@example.fr\r\n\
ADR;WORK;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:;;12 rue de l'=C3=89glise;Fenioux;;17350;France\r\n\
NOTE;ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8:Ligne un=0D=0A=C3=A0 rappeler=0D=0Aet une tr=\r\n\
=C3=A8s longue suite\r\n\
URL:https://example.fr/une/adresse/tres/longue/qui/est/\r\n repliee/sur/deux/lignes\r\n\
PHOTO;ENCODING=BASE64;JPEG:/9j/4AAQSkZJRg==\r\n AAAA\r\n\
UID:abc-1\r\nEND:VCARD\r\n";
        let p = parse(txt).unwrap();
        assert_eq!(p.len(), 1);
        let f = &p[0].form;
        assert_eq!((f.given_name.as_str(), f.surname.as_str()), ("Renée", "Dupont"));
        assert_eq!((f.company.as_str(), f.department.as_str()), ("Société électrique", "Atelier"));
        assert_eq!((f.mobile.as_str(), f.business.as_str(), f.home_phone.as_str()), ("06 12 34 56 78", "05 46 00 00 00", "05 46 11 11 11"));
        assert_eq!((f.email1.as_str(), f.email2.as_str()), ("renee@example.fr", "autre@example.fr"));
        assert_eq!((f.work_street.as_str(), f.work_city.as_str(), f.work_postal.as_str()), ("12 rue de l'Église", "Fenioux", "17350"));
        assert_eq!(f.notes, "Ligne un\nà rappeler\net une très longue suite");
        assert_eq!(f.web_page, "https://example.fr/une/adresse/tres/longue/qui/est/repliee/sur/deux/lignes");
        assert_eq!(p[0].uid.as_deref(), Some("abc-1"));
        assert!(p[0].warnings.iter().any(|w| w.contains("photo")), "{:?}", p[0].warnings);
        assert!(p[0].warnings.iter().any(|w| w.contains("FAX")));
    }

    #[test]
    fn vcard_30_et_40_lf() {
        let v3 = "BEGIN:VCARD\nVERSION:3.0\nN:Martin;Léa;;;\nFN:Léa Martin\nTEL;TYPE=CELL,VOICE:0611\nTEL;TYPE=work:0222\nEMAIL;TYPE=INTERNET:l@m.fr\nNOTE:a\\nb\\, c\\; d\nBDAY:19800412\nX-OUTLOOK-UID:0000AB\nEND:VCARD\n";
        let v4 = "BEGIN:VCARD\nVERSION:4.0\nFN:Léa Martin\nTEL;VALUE=uri;TYPE=\"cell,voice\":tel:+33611\nEMAIL;PREF=1:l@m.fr\nBDAY:1980-04-12\nADR;TYPE=home:;;1 rue X;Ville;;75000;France\nEND:VCARD\n";
        let a = &parse(v3).unwrap()[0];
        assert_eq!((a.form.given_name.as_str(), a.form.mobile.as_str(), a.form.business.as_str()), ("Léa", "0611", "0222"));
        assert_eq!((a.form.notes.as_str(), a.form.birthday.as_str(), a.uid.as_deref()), ("a\nb, c; d", "1980-04-12", Some("0000AB")));
        let b = &parse(v4).unwrap()[0];
        assert_eq!((b.form.given_name.as_str(), b.form.surname.as_str(), b.form.mobile.as_str()), ("Léa", "Martin", "+33611"));
        assert_eq!((b.form.home_city.as_str(), b.form.birthday.as_str()), ("Ville", "1980-04-12"));
        assert!(b.warnings.iter().any(|w| w.contains("N absent")));
    }

    #[test]
    fn qp_utf8_invalide_lu_en_windows_1252() {
        let p = parse("BEGIN:VCARD\nVERSION:2.1\nN;ENCODING=QUOTED-PRINTABLE:Caf=E9;Jo=\n=EB;;;\nEND:VCARD\n").unwrap();
        assert_eq!((p[0].form.surname.as_str(), p[0].form.given_name.as_str()), ("Café", "Joë"));
        assert!(p[0].warnings.iter().any(|w| w.contains("WINDOWS-1252")));
    }

    #[test]
    fn degrades() {
        let txt = "BEGIN:VCARD\r\nVERSION:3.0\r\nEND:VCARD\r\n\
BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Tronquée Un\r\nTEL;TYPE=CELL:0600\r\nBDAY:pas une date\r\nEMAIL:\r\n\
BEGIN:VCARD\r\nN:Bonne;Fiche;;;\r\nTEL:0700\r\nEND:VCARD\r\nligne hors fiche\r\n\
BEGIN:VCARD\r\nN;ENCODING=QUOTED-PRINTABLE:Coup=E\r\nEND:VCARD\r\n";
        let p = parse(txt).unwrap();
        assert_eq!(p.len(), 4);
        assert_eq!(p[0].form, ContactForm::default());
        assert!(p[0].warnings[0].contains("illisible"));
        assert_eq!((p[1].form.surname.as_str(), p[1].form.mobile.as_str(), p[1].form.birthday.as_str()), ("Un", "0600", ""));
        assert!(p[1].warnings.iter().any(|w| w.contains("tronqué")) && p[1].warnings.iter().any(|w| w.contains("BDAY")));
        assert_eq!((p[2].form.surname.as_str(), p[2].form.mobile.as_str()), ("Bonne", "0700"));
        assert!(p[2].warnings.iter().any(|w| w.contains("sans type")));
        assert_eq!(p[3].form.surname, "Coup=E");
        assert_eq!(parse("  \n").unwrap().len(), 0);
        assert!(parse("pas une vcard").is_err());
    }

    #[test]
    fn champs_vides_ne_produisent_rien() {
        let out = export(&[ContactForm { surname: "X".into(), ..ContactForm::default() }], Version::V30);
        for k in ["EMAIL", "TEL", "ADR", "URL", "BDAY", "NOTE", "ORG", "TITLE", "UID"] {
            assert!(!out.contains(&format!("\r\n{k}")), "{k}");
        }
    }
}
