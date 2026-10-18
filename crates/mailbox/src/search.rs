//! Recherche (J4) : syntaxe à mots-clés de la référence, en français et en anglais, traduite en requête
//! FTS5 sur `message_fts` et en conditions SQL sur `message`.
//!
//! - Mots libres : préfixe, sans accents ni casse (« fact » trouve « Facturé »). Phrase entre guillemets.
//! - `de:`/`from:`, `à:`/`to:`, `cc:`, `objet:`/`subject:`, `corps:`/`body:`, `pj:`/`attachment:` (nom de pièce jointe).
//! - `lu:oui|non`, `suivi:oui|non|terminé`, `pj:oui|non`, `importance:haute|normale|basse`, `catégorie:nom`,
//!   `adresse:x@y` (expéditeur ou destinataires), `reçu:` (aujourd'hui, hier, cette semaine, semaine dernière,
//!   ce mois, mois dernier, cette année, 2024, 05/2024, 12/05/2024, intervalle `a..b`, `>d`, `<d`),
//!   `taille:>1Mo` / `<500Ko`.
//! - `OU`/`OR` entre deux termes, `-terme` ou `NON`/`NOT` pour exclure.

use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone};
use thunderlook_store::rusqlite::types::Value;

use crate::fold;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Pred {
    Read(bool),
    /// 0 aucun, 1 terminé, 2 marqué.
    Flag(i64),
    Unflagged,
    HasAttachments(bool),
    Importance(i64),
    Category(String),
    /// Bornes en millisecondes, `[début, fin[`.
    Received(Option<i64>, Option<i64>),
    SizeAtLeast(i64),
    SizeBelow(i64),
    Address(String),
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Parsed {
    /// Expression FTS5 (vide : pas de condition plein texte).
    pub fts: String,
    pub preds: Vec<Pred>,
}

#[derive(Debug, PartialEq)]
enum Tok {
    Word(String),
    Phrase(String),
    Key(String, String),
}

fn tokenize(text: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '"' {
            chars.next();
            let phrase: String = chars.by_ref().take_while(|&c| c != '"').collect();
            if !phrase.trim().is_empty() {
                out.push(Tok::Phrase(phrase.trim().to_string()));
            }
            continue;
        }
        let mut word = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                break;
            }
            word.push(c);
            chars.next();
            if c == ':' {
                // Valeur entre guillemets après « clé: ».
                if chars.peek() == Some(&'"') {
                    chars.next();
                    let v: String = chars.by_ref().take_while(|&c| c != '"').collect();
                    word.push_str(&v);
                    word.push('\u{0}');
                }
                break;
            }
        }
        if let Some((k, v)) = word.split_once(':')
            && !k.is_empty()
        {
            let mut v = v.trim_end_matches('\u{0}').to_string();
            if v.is_empty() {
                // « clé: valeur » avec une espace : la valeur est le mot suivant.
                while let Some(&c) = chars.peek() {
                    if !c.is_whitespace() {
                        break;
                    }
                    chars.next();
                }
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    v.push(c);
                    chars.next();
                }
            }
            out.push(Tok::Key(fold(k).replace([' ', '-', '_'], ""), v));
        } else {
            out.push(Tok::Word(word));
        }
    }
    out
}

/// Terme FTS5 entre guillemets (les guillemets internes sont doublés), avec préfixe pour un mot.
fn fts_term(text: &str, prefix: bool) -> String {
    let t = text.replace('"', "\"\"");
    if prefix { format!("\"{t}\"*") } else { format!("\"{t}\"") }
}

fn yes(v: &str) -> Option<bool> {
    match fold(v).as_str() {
        "oui" | "yes" | "vrai" | "true" | "1" => Some(true),
        "non" | "no" | "faux" | "false" | "0" => Some(false),
        _ => None,
    }
}

fn day_ms(d: NaiveDate) -> i64 {
    Local
        .from_local_datetime(&d.and_hms_opt(0, 0, 0).expect("minuit"))
        .earliest()
        .map(|t| t.timestamp_millis())
        .unwrap_or_default()
}

fn month_start(y: i32, m: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(y, m, 1)
}

/// Intervalle de jours `[début, fin[` désigné par une valeur de date.
fn date_range(v: &str, today: NaiveDate) -> Option<(Option<NaiveDate>, Option<NaiveDate>)> {
    let f = fold(v).replace([' ', '\''], "");
    let monday = today - Duration::days(i64::from(today.weekday().num_days_from_monday()));
    let this_month = month_start(today.year(), today.month())?;
    let prev_month = this_month.checked_sub_months(chrono::Months::new(1))?;
    let r = match f.as_str() {
        "aujourdhui" | "today" => (Some(today), Some(today + Duration::days(1))),
        "hier" | "yesterday" => (Some(today - Duration::days(1)), Some(today)),
        "cettesemaine" | "thisweek" => (Some(monday), Some(monday + Duration::days(7))),
        "semainederniere" | "lastweek" => (Some(monday - Duration::days(7)), Some(monday)),
        "cemois" | "thismonth" => (Some(this_month), this_month.checked_add_months(chrono::Months::new(1))),
        "moisdernier" | "lastmonth" => (Some(prev_month), Some(this_month)),
        "cetteannee" | "thisyear" => (NaiveDate::from_ymd_opt(today.year(), 1, 1), NaiveDate::from_ymd_opt(today.year() + 1, 1, 1)),
        _ => {
            if let Some((a, b)) = f.split_once("..") {
                let a = date_range(a, today)?.0;
                let b = date_range(b, today)?.1;
                return Some((a, b));
            }
            for (op, open_end) in [(">=", true), (">", true), ("<=", false), ("<", false)] {
                if let Some(rest) = f.strip_prefix(op) {
                    let (start, end) = date_range(rest, today)?;
                    return Some(match (op, open_end) {
                        (">=", _) => (start, None),
                        (">", _) => (end, None),
                        ("<=", _) => (None, end),
                        _ => (None, start),
                    });
                }
            }
            let parts: Vec<&str> = f.split(['/', '-', '.']).collect();
            match parts.as_slice() {
                [y] if y.len() == 4 => {
                    let y: i32 = y.parse().ok()?;
                    (NaiveDate::from_ymd_opt(y, 1, 1), NaiveDate::from_ymd_opt(y + 1, 1, 1))
                }
                [m, y] if y.len() == 4 => {
                    let start = month_start(y.parse().ok()?, m.parse().ok()?)?;
                    (Some(start), start.checked_add_months(chrono::Months::new(1)))
                }
                [d, m, y] => {
                    let mut y: i32 = y.parse().ok()?;
                    if y < 100 {
                        y += 2000;
                    }
                    let day = NaiveDate::from_ymd_opt(y, m.parse().ok()?, d.parse().ok()?)?;
                    (Some(day), Some(day + Duration::days(1)))
                }
                _ => return None,
            }
        }
    };
    Some(r)
}

fn size_bytes(v: &str) -> Option<i64> {
    let f = fold(v).replace(' ', "");
    let (num, mult) = if let Some(n) = f.strip_suffix("go").or_else(|| f.strip_suffix("gb")) {
        (n, 1i64 << 30)
    } else if let Some(n) = f.strip_suffix("mo").or_else(|| f.strip_suffix("mb")) {
        (n, 1 << 20)
    } else if let Some(n) = f.strip_suffix("ko").or_else(|| f.strip_suffix("kb")) {
        (n, 1 << 10)
    } else {
        (f.strip_suffix('o').unwrap_or(&f), 1)
    };
    let n: f64 = num.replace(',', ".").parse().ok()?;
    Some((n * mult as f64) as i64)
}

/// Mots de date en deux parties tapés sans guillemets (« reçu:cette semaine »).
fn joins_date(first: &str, next: &str) -> bool {
    matches!(fold(first).as_str(), "cette" | "ce" | "semaine" | "mois")
        && matches!(fold(next).as_str(), "semaine" | "mois" | "annee" | "derniere" | "dernier")
}

pub(crate) fn parse(text: &str, today: NaiveDate) -> Parsed {
    let toks = tokenize(text);
    let mut fts: Vec<String> = Vec::new();
    let mut preds = Vec::new();
    let mut pending_or = false;
    let mut pending_not = false;
    let mut i = 0;
    let push = |term: String, fts: &mut Vec<String>, or: &mut bool, not: &mut bool| {
        if *not {
            if !fts.is_empty() {
                fts.push(format!("NOT {term}"));
            }
            *not = false;
        } else if *or && !fts.is_empty() {
            let last = fts.pop().unwrap_or_default();
            fts.push(format!("({last} OR {term})"));
        } else {
            fts.push(term);
        }
        *or = false;
    };
    while i < toks.len() {
        match &toks[i] {
            Tok::Word(w) if matches!(w.as_str(), "OU" | "OR") => pending_or = true,
            Tok::Word(w) if matches!(w.as_str(), "NON" | "NOT" | "SAUF") => pending_not = true,
            Tok::Word(w) => {
                let (neg, w) = match w.strip_prefix('-') {
                    Some(rest) if !rest.is_empty() => (true, rest),
                    _ => (false, w.as_str()),
                };
                if neg {
                    pending_not = true;
                }
                let w = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '@' && c != '.');
                if !w.is_empty() {
                    push(fts_term(w, true), &mut fts, &mut pending_or, &mut pending_not);
                }
            }
            Tok::Phrase(p) => push(fts_term(p, false), &mut fts, &mut pending_or, &mut pending_not),
            Tok::Key(k, v) => {
                let mut v = v.clone();
                let column = match k.as_str() {
                    "de" | "from" | "expediteur" => Some("sender"),
                    "a" | "to" | "pour" | "destinataire" | "cc" => Some("recipients"),
                    "objet" | "subject" | "sujet" => Some("subject"),
                    "corps" | "body" | "message" => Some("body"),
                    "pj" | "piecejointe" | "piecesjointes" | "attachment" | "attachments" if yes(&v).is_none() => Some("attachments"),
                    _ => None,
                };
                if let Some(col) = column {
                    if !v.is_empty() {
                        let prefix = !v.contains(' ');
                        push(format!("{col} : {}", fts_term(&v, prefix)), &mut fts, &mut pending_or, &mut pending_not);
                    }
                    i += 1;
                    continue;
                }
                match k.as_str() {
                    "lu" | "read" => {
                        if let Some(b) = yes(&v) {
                            preds.push(Pred::Read(b));
                        }
                    }
                    "suivi" | "flag" | "indicateur" | "flagged" => match fold(&v).as_str() {
                        "termine" | "complete" | "completed" => preds.push(Pred::Flag(1)),
                        other => match yes(other) {
                            Some(true) => preds.push(Pred::Flag(2)),
                            Some(false) => preds.push(Pred::Unflagged),
                            None => {}
                        },
                    },
                    "pj" | "piecejointe" | "piecesjointes" | "attachment" | "attachments" | "hasattachments" | "hasattachment" => {
                        if let Some(b) = yes(&v) {
                            preds.push(Pred::HasAttachments(b));
                        }
                    }
                    "importance" => match fold(&v).as_str() {
                        "haute" | "high" | "elevee" => preds.push(Pred::Importance(2)),
                        "normale" | "normal" => preds.push(Pred::Importance(1)),
                        "basse" | "low" | "faible" => preds.push(Pred::Importance(0)),
                        _ => {}
                    },
                    "categorie" | "category" | "categories" => {
                        if !v.is_empty() {
                            preds.push(Pred::Category(v.clone()));
                        }
                    }
                    "adresse" | "address" | "participant" | "participants" => {
                        if !v.is_empty() {
                            preds.push(Pred::Address(v.clone()));
                        }
                    }
                    "recu" | "received" | "date" | "envoye" | "sent" => {
                        if let Some(Tok::Word(next)) = toks.get(i + 1)
                            && joins_date(&v, next)
                        {
                            v = format!("{v}{next}");
                            i += 1;
                        }
                        if let Some((a, b)) = date_range(&v, today) {
                            preds.push(Pred::Received(a.map(day_ms), b.map(day_ms)));
                        }
                    }
                    "taille" | "size" => {
                        let f = v.trim();
                        if let Some(n) = f.strip_prefix(">=").or_else(|| f.strip_prefix('>')).and_then(size_bytes) {
                            preds.push(Pred::SizeAtLeast(n));
                        } else if let Some(n) = f.strip_prefix("<=").or_else(|| f.strip_prefix('<')).and_then(size_bytes) {
                            preds.push(Pred::SizeBelow(n));
                        }
                    }
                    // Clé inconnue : le mot entier est cherché tel quel.
                    _ => push(fts_term(&format!("{k} {v}"), false), &mut fts, &mut pending_or, &mut pending_not),
                }
            }
        }
        i += 1;
    }
    Parsed { fts: fts.join(" "), preds }
}

/// Conditions SQL (sur l'alias `m`) et leurs paramètres.
pub(crate) fn predicates_sql(preds: &[Pred]) -> (String, Vec<Value>) {
    let mut sql = String::new();
    let mut params: Vec<Value> = Vec::new();
    for p in preds {
        match p {
            Pred::Read(b) => sql.push_str(if *b { " AND m.is_read = 1" } else { " AND m.is_read = 0" }),
            // Valeurs écrites en clair : SQLite ne choisit un index partiel (`message_flagged`) que si la
            // condition de la requête reprend littéralement celle de l'index.
            Pred::Flag(s) => sql.push_str(&format!(" AND m.flag_status = {s}")),
            Pred::Unflagged => sql.push_str(" AND m.flag_status = 0"),
            Pred::HasAttachments(b) => sql.push_str(if *b {
                " AND EXISTS (SELECT 1 FROM attachment a WHERE a.message_id = m.id AND a.inline = 0 AND a.hidden = 0)"
            } else {
                " AND NOT EXISTS (SELECT 1 FROM attachment a WHERE a.message_id = m.id AND a.inline = 0 AND a.hidden = 0)"
            }),
            Pred::Importance(i) => sql.push_str(&format!(" AND m.importance = {i}")),
            Pred::Category(c) => {
                sql.push_str(" AND EXISTS (SELECT 1 FROM message_category c WHERE c.message_id = m.id AND c.name LIKE ? ESCAPE '\\')");
                params.push(Value::Text(format!("{}%", c.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"))));
            }
            Pred::Received(a, b) => {
                if let Some(a) = a {
                    sql.push_str(" AND m.received_at >= ?");
                    params.push(Value::Integer(*a));
                }
                if let Some(b) = b {
                    sql.push_str(" AND m.received_at < ?");
                    params.push(Value::Integer(*b));
                }
            }
            Pred::SizeAtLeast(n) => {
                sql.push_str(" AND m.size >= ?");
                params.push(Value::Integer(*n));
            }
            Pred::SizeBelow(n) => {
                sql.push_str(" AND m.size < ?");
                params.push(Value::Integer(*n));
            }
            Pred::Address(a) => {
                // Union plutôt que OU : chaque branche garde son index (adresse, plein texte).
                sql.push_str(
                    " AND m.id IN (SELECT id FROM message WHERE from_addr = ? COLLATE NOCASE
                                   UNION SELECT rowid FROM message_fts WHERE message_fts MATCH ?)",
                );
                params.push(Value::Text(a.clone()));
                params.push(Value::Text(format!("recipients : {}", fts_term(a, false))));
            }
        }
    }
    (sql, params)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Parsed {
        parse(text, NaiveDate::from_ymd_opt(2026, 10, 19).unwrap())
    }

    #[test]
    fn free_text_and_keywords() {
        assert_eq!(p("fact devis").fts, "\"fact\"* \"devis\"*");
        assert_eq!(p("\"bon de commande\"").fts, "\"bon de commande\"");
        assert_eq!(p("de:camille objet:\"facture 2026\"").fts, "sender : \"camille\"* subject : \"facture 2026\"");
        assert_eq!(p("de: camille").fts, "sender : \"camille\"*");
        assert_eq!(p("facture OU devis").fts, "(\"facture\"* OR \"devis\"*)");
        assert_eq!(p("facture -relance").fts, "\"facture\"* NOT \"relance\"*");
        assert_eq!(p("À:paul@adam-declic.fr").fts, "recipients : \"paul@adam-declic.fr\"*");
        let q = p("lu:non pj:oui suivi:oui importance:haute catégorie:ADI");
        assert_eq!(q.fts, "");
        assert_eq!(q.preds, vec![Pred::Read(false), Pred::HasAttachments(true), Pred::Flag(2), Pred::Importance(2), Pred::Category("ADI".into())]);
        assert_eq!(p("pj:facture").fts, "attachments : \"facture\"*");
        assert_eq!(p("taille:>1Mo").preds, vec![Pred::SizeAtLeast(1 << 20)]);
    }

    #[test]
    fn dates() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap(); // lundi
        let d = |y, m, dd| Some(NaiveDate::from_ymd_opt(y, m, dd).unwrap());
        assert_eq!(date_range("hier", today), Some((d(2026, 10, 18), d(2026, 10, 19))));
        assert_eq!(date_range("semaine dernière", today), Some((d(2026, 10, 12), d(2026, 10, 19))));
        assert_eq!(date_range("05/2024", today), Some((d(2024, 5, 1), d(2024, 6, 1))));
        assert_eq!(date_range("12/05/2024..14/05/2024", today), Some((d(2024, 5, 12), d(2024, 5, 15))));
        assert_eq!(date_range(">31/12/2025", today), Some((d(2026, 1, 1), None)));
        assert_eq!(date_range("<2020", today), Some((None, d(2020, 1, 1))));
        assert_eq!(p("reçu:cette semaine facture").preds.len(), 1);
        assert_eq!(p("reçu:cette semaine facture").fts, "\"facture\"*");
    }
}
