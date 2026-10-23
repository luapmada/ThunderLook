//! `NoteForm` ↔ VNOTE 1.1 (fichiers `.vnt`, échange avec le téléphone). Fonctions pures, sans base ni fichier.
//!
//! Le formulaire n'a ni date ni identifiant d'échange : ils passent par [`Meta`]. La couleur n'a pas d'équivalent VNOTE :
//! elle n'est jamais écrite, et à la lecture [`DEFAULT_COLOR`] (jaune, valeur par défaut de la liste des notes) n'est
//! qu'un remplissage : pour une note déjà connue, l'appelant garde la couleur existante.

use thunderlook_model::items::NoteForm;

use crate::textfmt::{self, Prop};

/// Jaune : valeur prise par `notes()` quand la couleur manque.
pub const DEFAULT_COLOR: i32 = 3;

/// Ce qui accompagne le texte d'une note dans le fichier.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meta {
    /// `X-OUTLOOK-UID`, tel que lu ; à l'écriture, l'identifiant du formulaire si absent.
    pub uid: Option<String>,
    /// `DCREATED`, tel quel (`20261023T101500`, `…Z`).
    pub created: Option<String>,
    /// `LAST-MODIFIED`, tel quel.
    pub modified: Option<String>,
}

/// Une note lue. Un enregistrement illisible rend `form == NoteForm::default()` et un avertissement : à ignorer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub form: NoteForm,
    pub meta: Meta,
    pub warnings: Vec<String>,
}

pub fn export(forms: &[NoteForm]) -> String {
    let items: Vec<(NoteForm, Meta)> = forms.iter().map(|f| (f.clone(), Meta::default())).collect();
    export_with_meta(&items)
}

pub fn export_with_meta(items: &[(NoteForm, Meta)]) -> String {
    let mut out = String::new();
    for (f, m) in items {
        out.push_str("BEGIN:VNOTE\r\nVERSION:1.1\r\n");
        out.push_str(&textfmt::qp_line("BODY;ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8:", &f.body));
        out.push_str("\r\n");
        for (name, v) in [("DCREATED", &m.created), ("LAST-MODIFIED", &m.modified)] {
            if let Some(v) = v.as_deref().filter(|v| !v.is_empty()) {
                out.push_str(&format!("{name}:{v}\r\n"));
            }
        }
        if let Some(uid) = m.uid.clone().or_else(|| f.id.clone()).filter(|u| !u.is_empty()) {
            out.push_str(&textfmt::fold(&format!("X-OUTLOOK-UID:{uid}")));
            out.push_str("\r\n");
        }
        out.push_str("END:VNOTE\r\n");
    }
    out
}

pub fn parse(text: &str) -> Result<Vec<Parsed>, String> {
    let lines = textfmt::unfold(text);
    let mut out = Vec::new();
    let mut cur: Option<Vec<Prop>> = None;
    let mut n = 0;
    for line in &lines {
        let up = line.trim().to_uppercase();
        if up == "BEGIN:VNOTE" {
            if let Some(props) = cur.take() {
                n += 1;
                out.push(record(n, props, true));
            }
            cur = Some(Vec::new());
        } else if up == "END:VNOTE" {
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
        return Err("aucune note VNOTE dans le fichier".into());
    }
    Ok(out)
}

fn record(n: usize, props: Vec<Prop>, truncated: bool) -> Parsed {
    let mut warnings = Vec::new();
    let mut form = NoteForm { color: DEFAULT_COLOR, ..NoteForm::default() };
    let mut meta = Meta::default();
    let mut body: Option<String> = None;
    for p in &props {
        match p.name.as_str() {
            "BODY" => {
                let v = textfmt::decode_value(p, &mut warnings);
                let v = if p.is_qp() { v } else { textfmt::unescape(&v) };
                body = Some(match body {
                    Some(b) => format!("{b}\n{v}"),
                    None => v,
                });
            }
            "X-OUTLOOK-UID" => meta.uid = Some(p.value.trim().to_string()).filter(|s| !s.is_empty()),
            "DCREATED" => meta.created = Some(p.value.trim().to_string()).filter(|s| !s.is_empty()),
            "LAST-MODIFIED" => meta.modified = Some(p.value.trim().to_string()).filter(|s| !s.is_empty()),
            _ => {}
        }
    }
    if let Some(b) = body {
        form.body = b;
    }
    if truncated {
        warnings.push("enregistrement tronqué (END:VNOTE absent)".into());
    }
    if form.body.trim().is_empty() {
        let mut w = vec![format!("note {n} illisible : texte vide")];
        w.extend(warnings.into_iter().map(|x| format!("note {n} : {x}")));
        return Parsed { form: NoteForm::default(), meta: Meta::default(), warnings: w };
    }
    Parsed { form, meta, warnings: warnings.into_iter().map(|x| format!("note {n} : {x}")).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(body: &str) -> NoteForm {
        NoteForm { color: DEFAULT_COLOR, body: body.into(), ..NoteForm::default() }
    }

    #[test]
    fn roundtrip() {
        let forms = vec![
            note("Courses : pain, lait\nŒufs à 3 € ; café = important\n\nFin avec espace "),
            note("Ligne ASCII très longue ".repeat(12).trim_end()),
            note("é"),
        ];
        let out = export(&forms);
        assert!(out.contains("BODY;ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8:"));
        let back: Vec<NoteForm> = parse(&out).unwrap().into_iter().map(|p| p.form).collect();
        assert_eq!(back, forms);
        assert!(out.lines().all(|l| l.len() <= 76));
    }

    #[test]
    fn roundtrip_uid_et_dates() {
        let items = vec![(
            note("Bonjour"),
            Meta { uid: Some("00000000ABCDEF0123".into()), created: Some("20261023T101500".into()), modified: Some("20261023T111500Z".into()) },
        )];
        let p = parse(&export_with_meta(&items)).unwrap();
        assert_eq!(p[0].meta, items[0].1);
        assert!(p[0].warnings.is_empty());
    }

    #[test]
    fn uid_par_defaut_est_l_identifiant() {
        let mut f = note("x");
        f.id = Some("n42".into());
        let p = parse(&export(&[f])).unwrap();
        assert_eq!(p[0].meta.uid.as_deref(), Some("n42"));
        assert_eq!(p[0].form.id, None);
    }

    #[test]
    fn lecture_qp_windows_1252_lignes_repliees() {
        let txt = "BEGIN:VNOTE\nVERSION:1.1\nBODY;ENCODING=QUOTED-PRINTABLE;CHARSET=WINDOWS-1252:Rendez-vous =E0 No=EBl=0D=0Aprix 5=80 =\nsuite coup=E9e\nDCREATED:20261001T080000\nX-OUTLOOK-UID:AB\n CD\nEND:VNOTE\n";
        let p = parse(txt).unwrap();
        assert_eq!(p[0].form.body, "Rendez-vous à Noël\nprix 5€ suite coupée");
        assert_eq!(p[0].meta.uid.as_deref(), Some("ABCD"));
        assert_eq!(p[0].meta.created.as_deref(), Some("20261001T080000"));
        assert_eq!(p[0].form.color, DEFAULT_COLOR);
    }

    #[test]
    fn degrades() {
        // vide, tronqué, puis une bonne note
        let txt = "BEGIN:VNOTE\r\nVERSION:1.1\r\nEND:VNOTE\r\nBEGIN:VNOTE\r\nBODY:Reste\r\nBEGIN:VNOTE\r\nBODY:Bonne\\nnote\r\nEND:VNOTE\r\n";
        let p = parse(txt).unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].form, NoteForm::default());
        assert!(p[0].warnings[0].contains("illisible"));
        assert_eq!(p[1].form.body, "Reste");
        assert!(p[1].warnings[0].contains("tronqué"));
        assert_eq!(p[2].form.body, "Bonne\nnote");
        assert_eq!(parse("").unwrap().len(), 0);
        assert!(parse("n'importe quoi").is_err());
    }

    #[test]
    fn corps_long_lignes_vides_et_retours_chariot() {
        let body = format!("Titre « été »\r\n\r\n\r\n{}\r\n\n\nFin : a=b ; 5 €", "Ligne très longue é è à œ ".repeat(60));
        let items = vec![(note(&body), Meta { uid: Some("U1".into()), created: Some("20261001T080000".into()), modified: Some("20261022T101500Z".into()) })];
        let out = export_with_meta(&items);
        assert!(out.lines().all(|l| l.len() <= 76));
        let p = parse(&out).unwrap();
        assert!(p[0].warnings.is_empty());
        // Les retours chariot sont normalisés en fin de ligne `\n` ; lignes vides et texte restent intacts.
        assert_eq!(p[0].form.body, body.replace('\r', ""));
        assert_eq!(p[0].meta, items[0].1);
    }

    #[test]
    fn charset_sans_qp_et_dates_conservees() {
        let txt = "BEGIN:VNOTE\r\nVERSION:1.1\r\nBODY;CHARSET=UTF-8:Idée\\n\\nsuite\r\nDCREATED:20260101T000000Z\r\nLAST-MODIFIED:20260102T030405Z\r\nEND:VNOTE\r\n";
        let p = parse(txt).unwrap();
        assert_eq!(p[0].form.body, "Idée\n\nsuite");
        assert_eq!(p[0].meta.created.as_deref(), Some("20260101T000000Z"));
        assert_eq!(p[0].meta.modified.as_deref(), Some("20260102T030405Z"));
    }
}
