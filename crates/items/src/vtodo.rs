//! `TaskForm` ↔ VTODO (iCalendar, `.ics`). Fonctions pures, sans base ni fichier.
//!
//! Conventions de `task_save` (crates/items/src/lib.rs) : `status` borné à 0..=4, 2 = terminée (seule valeur que le code
//! teste ; `task_complete` remet 0) ; `importance` bornée à 0..=2. Le sens de 0, 1, 3 et 4 vient de [MS-OXOTASK]
//! (0 à faire, 1 en cours, 3 en attente, 4 reportée), pas du dépôt. 3 et 4 n'ont pas d'équivalent iCalendar : écrits
//! `STATUS:IN-PROCESS` + `X-THUNDERLOOK-STATUS`. Les dates du formulaire sont des jours (`AAAA-MM-JJ`).

use thunderlook_model::items::TaskForm;

use crate::textfmt::{self, Prop};

/// Une tâche lue. Un enregistrement illisible rend `form == TaskForm::default()` et un avertissement : à ignorer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub form: TaskForm,
    pub uid: Option<String>,
    pub warnings: Vec<String>,
}

fn ical_date(day: &str) -> Option<String> {
    let d: String = day.chars().filter(char::is_ascii_digit).collect();
    (d.len() == 8).then_some(d)
}

const FREQS: [(&str, &str); 4] = [("daily", "DAILY"), ("weekly", "WEEKLY"), ("monthly", "MONTHLY"), ("yearly", "YEARLY")];

/// Périodicité : `RRULE` pour une série fixe ; « recommencer après la fin » n'a pas d'équivalent iCalendar, écrit
/// `X-THUNDERLOOK-REGEN:<DAILY|…>;<N>`. Une périodicité « personnalisée » n'est pas exportée.
fn push_repeat(f: &TaskForm, push: &mut impl FnMut(String)) {
    let every = f.repeat_every.max(1);
    if let Some(freq) = f.repeat.strip_prefix(crate::trecur::REGEN).and_then(|r| FREQS.iter().find(|(k, _)| *k == r)) {
        push(format!("X-THUNDERLOOK-REGEN:{};{every}", freq.1));
    } else if let Some((_, freq)) = FREQS.iter().find(|(k, _)| *k == f.repeat) {
        let end = match f.repeat_end.as_str() {
            "count" => format!(";COUNT={}", f.repeat_count.max(1)),
            "until" => ical_date(&f.repeat_until).map(|d| format!(";UNTIL={d}")).unwrap_or_default(),
            _ => String::new(),
        };
        push(format!("RRULE:FREQ={freq};INTERVAL={every}{end}"));
    }
}

/// `RRULE` ou `X-THUNDERLOOK-REGEN` → champs de périodicité du formulaire.
fn read_repeat(f: &mut TaskForm, name: &str, v: &str, w: &mut Vec<String>) {
    let freq = |t: &str| FREQS.iter().find(|(_, k)| k.eq_ignore_ascii_case(t.trim())).map(|(k, _)| *k);
    if name == "X-THUNDERLOOK-REGEN" {
        let (kind, n) = v.split_once(';').unwrap_or((v, "1"));
        match freq(kind) {
            Some(k) => (f.repeat, f.repeat_every, f.repeat_end) = (format!("{}{k}", crate::trecur::REGEN), n.trim().parse().unwrap_or(1), "never".into()),
            None => w.push(format!("X-THUNDERLOOK-REGEN illisible ({}), ignoré", v.trim())),
        }
        return;
    }
    let mut found = None;
    let (mut every, mut end, mut count, mut until) = (1, "never", 0, String::new());
    for part in v.split(';') {
        let Some((k, x)) = part.split_once('=') else { continue };
        match k.trim().to_uppercase().as_str() {
            "FREQ" => found = freq(x),
            "INTERVAL" => every = x.trim().parse().unwrap_or(1),
            "COUNT" => (end, count) = ("count", x.trim().parse().unwrap_or(1)),
            "UNTIL" => match day_of(x) {
                Some(d) => (end, until) = ("until", d),
                None => w.push(format!("RRULE UNTIL illisible ({}), série sans fin", x.trim())),
            },
            "BYDAY" | "BYMONTHDAY" | "BYMONTH" | "BYSETPOS" | "BYYEARDAY" | "BYWEEKNO" => w.push(format!("RRULE {k} non pris en charge : ignoré")),
            _ => {}
        }
    }
    match found {
        Some(k) => (f.repeat, f.repeat_every, f.repeat_end, f.repeat_count, f.repeat_until) = (k.into(), every, end.into(), count, until),
        None => w.push(format!("RRULE non prise en charge ({}), ignorée", v.trim())),
    }
}

pub fn export(forms: &[TaskForm]) -> String {
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//ThunderLook//vtodo//FR\r\n");
    let mut push = |s: String| {
        out.push_str(&textfmt::fold(&s));
        out.push_str("\r\n");
    };
    for f in forms {
        push("BEGIN:VTODO".into());
        if let Some(id) = f.id.as_deref().filter(|s| !s.is_empty()) {
            push(format!("UID:{}", textfmt::escape(id, true, true)));
        }
        push(format!("SUMMARY:{}", textfmt::escape(&f.subject, true, true)));
        for (name, v) in [("DTSTART", &f.start), ("DUE", &f.due)] {
            if let Some(d) = ical_date(v) {
                push(format!("{name};VALUE=DATE:{d}"));
            }
        }
        let status = f.status.clamp(0, 4);
        push(format!(
            "STATUS:{}",
            match status {
                0 => "NEEDS-ACTION",
                2 => "COMPLETED",
                _ => "IN-PROCESS",
            }
        ));
        if status == 3 || status == 4 {
            push(format!("X-THUNDERLOOK-STATUS:{status}"));
        }
        push_repeat(f, &mut push);
        push(format!("PERCENT-COMPLETE:{}", if status == 2 { 100 } else { 0 }));
        push(format!("PRIORITY:{}", match f.importance.clamp(0, 2) { 2 => 1, 0 => 9, _ => 5 }));
        if !f.body.is_empty() {
            push(format!("DESCRIPTION:{}", textfmt::escape(&f.body, true, true)));
        }
        let cats: Vec<String> = f.categories.split(',').map(str::trim).filter(|s| !s.is_empty()).map(|s| textfmt::escape(s, true, true)).collect();
        if !cats.is_empty() {
            push(format!("CATEGORIES:{}", cats.join(",")));
        }
        push("END:VTODO".into());
    }
    out.push_str("END:VCALENDAR\r\n");
    out
}

pub fn parse(text: &str) -> Result<Vec<Parsed>, String> {
    let lines = textfmt::unfold(text);
    let mut out = Vec::new();
    let mut cur: Option<Vec<Prop>> = None;
    let mut n = 0;
    let mut other = 0;
    // profondeur dans un composant imbriqué (VALARM) ou étranger (VEVENT, VJOURNAL…) du VTODO
    let mut skip: Vec<String> = Vec::new();
    for line in &lines {
        let up = line.trim().to_uppercase();
        if let Some(name) = up.strip_prefix("BEGIN:") {
            if name == "VTODO" && skip.is_empty() {
                if let Some(props) = cur.take() {
                    n += 1;
                    out.push(record(n, props, true));
                }
                cur = Some(Vec::new());
            } else if name != "VCALENDAR" {
                if cur.is_none() && skip.is_empty() {
                    other += 1;
                }
                skip.push(name.to_string());
            }
        } else if let Some(name) = up.strip_prefix("END:") {
            if skip.last().is_some_and(|s| s == name) {
                skip.pop();
            } else if name == "VTODO" && skip.is_empty() {
                if let Some(props) = cur.take() {
                    n += 1;
                    out.push(record(n, props, false));
                }
            }
        } else if skip.is_empty() {
            if let (Some(props), Some(p)) = (cur.as_mut(), textfmt::parse_prop(line)) {
                props.push(p);
            }
        }
    }
    if let Some(props) = cur.take() {
        n += 1;
        out.push(record(n, props, true));
    }
    if n == 0 && !text.trim().is_empty() {
        return Err(if other > 0 { "aucune tâche VTODO dans le fichier (autres composants seulement)".into() } else { "aucune tâche VTODO dans le fichier".into() });
    }
    Ok(out)
}

/// `AAAA-MM-JJ` d'une date iCalendar (`20261023`, `20261023T101500Z`, `2026-10-23`) ; l'heure est abandonnée.
fn day_of(v: &str) -> Option<String> {
    let d = v.trim().split(['T', 't']).next().unwrap_or("");
    let digits: String = d.chars().filter(char::is_ascii_digit).collect();
    if digits.len() != 8 {
        return None;
    }
    let (y, m, dd) = (&digits[..4], &digits[4..6], &digits[6..]);
    let (mi, di): (u32, u32) = (m.parse().ok()?, dd.parse().ok()?);
    ((1..=12).contains(&mi) && (1..=31).contains(&di)).then(|| format!("{y}-{m}-{dd}"))
}

fn record(n: usize, props: Vec<Prop>, truncated: bool) -> Parsed {
    let mut w: Vec<String> = Vec::new();
    let mut f = TaskForm::default();
    let mut uid = None;
    let (mut status, mut x_status, mut percent): (Option<i32>, Option<i32>, Option<i32>) = (None, None, None);
    let mut desc: Option<String> = None;
    let mut cats: Vec<String> = Vec::new();
    let mut time_dropped = false;
    for p in &props {
        let mut pw = Vec::new();
        let v = textfmt::decode_value(p, &mut pw);
        w.extend(pw);
        match p.name.as_str() {
            "UID" => uid = Some(textfmt::unescape(&v).trim().to_string()).filter(|s| !s.is_empty()),
            "SUMMARY" => f.subject = textfmt::unescape(&v),
            "DESCRIPTION" => {
                let t = textfmt::unescape(&v);
                desc = Some(match desc {
                    Some(d) => format!("{d}\n{t}"),
                    None => t,
                });
            }
            "DTSTART" | "DUE" => {
                let has_time = v.contains(['T', 't']);
                match day_of(&v) {
                    Some(d) => {
                        if has_time {
                            time_dropped = true;
                        }
                        if p.name == "DTSTART" { f.start = d } else { f.due = d }
                    }
                    None => w.push(format!("{} illisible ({}), ignoré", p.name, v.trim())),
                }
            }
            "STATUS" => match v.trim().to_uppercase().as_str() {
                "NEEDS-ACTION" => status = Some(0),
                "IN-PROCESS" => status = Some(1),
                "COMPLETED" => status = Some(2),
                "CANCELLED" => {
                    status = Some(4);
                    w.push("STATUS:CANCELLED lu comme « reportée » (4) : aucun équivalent établi".into());
                }
                other => w.push(format!("STATUS inconnu ({other}), ignoré")),
            },
            "X-THUNDERLOOK-STATUS" => x_status = v.trim().parse::<i32>().ok().filter(|s| (0..=4).contains(s)),
            "PERCENT-COMPLETE" => percent = v.trim().parse::<i32>().ok(),
            "PRIORITY" => match v.trim().parse::<i32>() {
                Ok(0) => f.importance = 1,
                Ok(1..=4) => f.importance = 2,
                Ok(5) => f.importance = 1,
                Ok(6..=9) => f.importance = 0,
                _ => w.push(format!("PRIORITY illisible ({}), normale", v.trim())),
            },
            "RRULE" | "X-THUNDERLOOK-REGEN" => read_repeat(&mut f, &p.name, &v, &mut w),
            "CATEGORIES" => {
                for c in textfmt::split_unescaped(&v, ',') {
                    let c = textfmt::unescape(&c).trim().to_string();
                    if !c.is_empty() {
                        cats.push(c);
                    }
                }
            }
            _ => {}
        }
    }
    f.importance = if props.iter().any(|p| p.name == "PRIORITY") { f.importance } else { 1 };
    f.status = match (status, percent) {
        (Some(1), _) if matches!(x_status, Some(3 | 4)) => x_status.unwrap_or(1),
        (Some(s), _) => s,
        (None, Some(100)) => 2,
        (None, Some(1..=99)) => 1,
        _ => 0,
    };
    if status == Some(2) && percent.is_some_and(|p| p < 100) {
        w.push("STATUS:COMPLETED avec PERCENT-COMPLETE < 100 : terminée".into());
    }
    f.body = desc.unwrap_or_default();
    f.categories = cats.join(", ");
    if time_dropped {
        w.push("heure de DTSTART/DUE abandonnée (le formulaire ne garde que le jour)".into());
    }
    if truncated {
        w.push("enregistrement tronqué (END:VTODO absent)".into());
    }
    if f.subject.trim().is_empty() {
        let mut out = vec![format!("tâche {n} illisible : objet vide")];
        out.extend(w.into_iter().map(|x| format!("tâche {n} : {x}")));
        return Parsed { form: TaskForm::default(), uid: None, warnings: out };
    }
    Parsed { form: f, uid, warnings: w.into_iter().map(|x| format!("tâche {n} : {x}")).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(subject: &str) -> TaskForm {
        TaskForm { subject: subject.into(), importance: 1, ..TaskForm::default() }
    }

    #[test]
    fn roundtrip_tous_les_statuts_et_importances() {
        let mut forms = Vec::new();
        for status in 0..=4 {
            for importance in 0..=2 {
                forms.push(TaskForm {
                    subject: format!("Tâche {status}/{importance} ; avec, virgule"),
                    start: "2026-10-01".into(),
                    due: "2026-10-31".into(),
                    status,
                    importance,
                    body: "Ligne 1\nLigne 2 avec \\ et é, ;\nCe texte est assez long pour être replié sur plusieurs lignes de 75 octets au moins, oui.".into(),
                    categories: "Pro, Perso".into(),
                    ..TaskForm::default()
                });
            }
        }
        forms.push(task("Minimale"));
        let out = export(&forms);
        assert!(out.lines().all(|l| l.len() <= 75));
        let back: Vec<TaskForm> = parse(&out).unwrap().into_iter().map(|p| p.form).collect();
        assert_eq!(back, forms);
    }

    #[test]
    fn periodicite_aller_retour() {
        let base = |repeat: &str, end: &str| TaskForm {
            subject: "Série".into(),
            importance: 1,
            start: "2026-10-23".into(),
            repeat: repeat.into(),
            repeat_every: 2,
            repeat_end: end.into(),
            repeat_count: if end == "count" { 4 } else { 0 },
            repeat_until: if end == "until" { "2027-01-31".into() } else { String::new() },
            ..TaskForm::default()
        };
        let forms = vec![base("weekly", "count"), base("monthly", "until"), base("daily", "never"), base("regen_yearly", "never")];
        let out = export(&forms);
        assert!(out.contains("RRULE:FREQ=WEEKLY;INTERVAL=2;COUNT=4") && out.contains("UNTIL=20270131") && out.contains("X-THUNDERLOOK-REGEN:YEARLY;2"));
        assert_eq!(out.matches("RRULE").count(), 3);
        let back: Vec<TaskForm> = parse(&out).unwrap().into_iter().map(|p| p.form).collect();
        assert_eq!(back, forms);
        let ext = parse("BEGIN:VTODO\nSUMMARY:x\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE\nEND:VTODO").unwrap();
        assert_eq!(ext[0].form.repeat, "weekly");
        assert!(ext[0].warnings.iter().any(|w| w.contains("BYDAY")));
    }

    #[test]
    fn uid_est_l_identifiant() {
        let mut f = task("x");
        f.id = Some("t12".into());
        let p = parse(&export(&[f])).unwrap();
        assert_eq!(p[0].uid.as_deref(), Some("t12"));
        assert_eq!(p[0].form.id, None);
    }

    #[test]
    fn lecture_externe() {
        let txt = "BEGIN:VCALENDAR\nVERSION:2.0\nBEGIN:VEVENT\nSUMMARY:RDV\nEND:VEVENT\nBEGIN:VTODO\nUID:abc\nSUMMARY:Appeler\n Paul\nDTSTART:20261001T080000Z\nDUE;VALUE=DATE:20261015\nPERCENT-COMPLETE:40\nPRIORITY:2\nBEGIN:VALARM\nACTION:DISPLAY\nSUMMARY:alarme\nEND:VALARM\nCATEGORIES:A\\,B,C\nEND:VTODO\nEND:VCALENDAR\n";
        let p = parse(txt).unwrap();
        assert_eq!(p.len(), 1);
        let f = &p[0].form;
        assert_eq!((f.subject.as_str(), f.start.as_str(), f.due.as_str()), ("AppelerPaul", "2026-10-01", "2026-10-15"));
        assert_eq!((f.status, f.importance, f.categories.as_str()), (1, 2, "A,B, C"));
        assert!(p[0].warnings.iter().any(|w| w.contains("heure")));
    }

    #[test]
    fn degrades() {
        let txt = "BEGIN:VCALENDAR\r\nBEGIN:VTODO\r\nUID:1\r\nEND:VTODO\r\nBEGIN:VTODO\r\nSUMMARY:Tronquée\r\nDUE:n'importe quoi\r\nBEGIN:VTODO\r\nSUMMARY:Bonne\r\nSTATUS:COMPLETED\r\nEND:VTODO\r\n";
        let p = parse(txt).unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].form, TaskForm::default());
        assert!(p[0].warnings[0].contains("illisible"));
        assert_eq!(p[1].form.subject, "Tronquée");
        assert_eq!(p[1].form.due, "");
        assert!(p[1].warnings.iter().any(|w| w.contains("tronqué")));
        assert_eq!((p[2].form.subject.as_str(), p[2].form.status), ("Bonne", 2));
        assert_eq!(parse("").unwrap().len(), 0);
        assert!(parse("BEGIN:VCALENDAR\nBEGIN:VEVENT\nEND:VEVENT\nEND:VCALENDAR").is_err());
    }
}
