//! `TaskForm` ↔ VTODO (iCalendar, `.ics`). Fonctions pures, sans base ni fichier.
//!
//! Conventions de `task_save` (crates/items/src/lib.rs) : `status` borné à 0..=4, 2 = terminée (seule valeur que le code
//! teste ; `task_complete` remet 0) ; `importance` bornée à 0..=2. Le sens de 0, 1, 3 et 4 vient de [MS-OXOTASK]
//! (0 à faire, 1 en cours, 3 en attente, 4 reportée), pas du dépôt. 3 et 4 n'ont pas d'équivalent iCalendar : écrits
//! `STATUS:IN-PROCESS` + `X-THUNDERLOOK-STATUS`. Les dates du formulaire sont des jours (`AAAA-MM-JJ`).

use chrono::{Duration, NaiveDate, NaiveDateTime};
use thunderlook_calendar::tz::TimeZone;
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

const MINUTE: &str = "%Y-%m-%dT%H:%M";

/// Rappel du formulaire (heure de Paris) → `TRIGGER;VALUE=DATE-TIME` absolu en UTC. Toujours absolu à l'export : une
/// échéance du formulaire n'a pas d'heure, un déclencheur relatif à `DUE` perdrait l'heure du rappel.
fn trigger_of(reminder: &str) -> Option<String> {
    let local = NaiveDateTime::parse_from_str(reminder.trim(), MINUTE).ok()?;
    Some(format!("TRIGGER;VALUE=DATE-TIME:{}", TimeZone::PARIS.to_utc(local).format("%Y%m%dT%H%M%SZ")))
}

/// Date-heure iCalendar (`20261023`, `20261023T101500`, `20261023T081500Z`) → heure de Paris. Une date seule vaut minuit ;
/// une heure flottante est déjà locale.
fn local_of(v: &str) -> Option<NaiveDateTime> {
    let v = v.trim();
    let utc = v.ends_with(['Z', 'z']);
    let v = v.trim_end_matches(['Z', 'z']);
    let t = match v.split_once(['T', 't']) {
        Some((d, h)) => NaiveDateTime::parse_from_str(&format!("{d}T{h}"), "%Y%m%dT%H%M%S").ok()?,
        None => NaiveDate::parse_from_str(v, "%Y%m%d").ok()?.and_hms_opt(0, 0, 0)?,
    };
    Some(if utc { TimeZone::PARIS.to_local(t) } else { t })
}

/// Durée iCalendar (`-PT15M`, `-P1DT9H`, `P1W`) ; négative = avant l'ancre.
fn duration_of(v: &str) -> Option<Duration> {
    let v = v.trim();
    let (neg, v) = match v.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, v.strip_prefix('+').unwrap_or(v)),
    };
    let v = v.strip_prefix(['P', 'p'])?;
    let (mut total, mut num, mut time, mut any) = (Duration::zero(), String::new(), false, false);
    for c in v.chars() {
        let n = || num.parse::<i64>().ok();
        let unit = match c.to_ascii_uppercase() {
            'T' => {
                time = true;
                continue;
            }
            'W' => Duration::weeks(n()?),
            'D' => Duration::days(n()?),
            'H' if time => Duration::hours(n()?),
            'M' if time => Duration::minutes(n()?),
            'S' if time => Duration::seconds(n()?),
            d if d.is_ascii_digit() => {
                num.push(d);
                continue;
            }
            _ => return None,
        };
        (total, num, any) = (total + unit, String::new(), true);
    }
    (any && num.is_empty()).then_some(if neg { -total } else { total })
}

/// Rappel (`AAAA-MM-JJTHH:MM`, Paris) d'un `VALARM` ; `Err` : motif d'abandon. `start`/`due` : valeurs brutes du VTODO.
fn alarm_reminder(alarm: &[Prop], start: Option<&str>, due: Option<&str>) -> Result<String, String> {
    let get = |n: &str| alarm.iter().find(|p| p.name == n);
    let trigger = get("TRIGGER").ok_or("VALARM sans TRIGGER, ignoré")?;
    let param = |k: &str| trigger.params.iter().find(|(n, _)| n == k).map(|(_, v)| v.trim().to_uppercase());
    let at = if param("VALUE").as_deref() == Some("DATE-TIME") {
        local_of(&trigger.value).ok_or_else(|| format!("TRIGGER illisible ({}), ignoré", trigger.value.trim()))?
    } else {
        let d = duration_of(&trigger.value).ok_or_else(|| format!("TRIGGER illisible ({}), ignoré", trigger.value.trim()))?;
        let (anchor, name) = if param("RELATED").as_deref() == Some("START") { (start, "DTSTART") } else { (due, "DUE") };
        let base = anchor.and_then(local_of).ok_or_else(|| format!("TRIGGER relatif à {name} mais {name} absent ou illisible, ignoré"))?;
        base + d
    };
    Ok(at.format(MINUTE).to_string())
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
        if let Some(t) = f.reminder.as_deref().and_then(trigger_of) {
            push("BEGIN:VALARM".into());
            push("ACTION:DISPLAY".into());
            push(format!("DESCRIPTION:{}", textfmt::escape(&f.subject, true, true)));
            push(t);
            push("END:VALARM".into());
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
    let mut alarms: Vec<Vec<Prop>> = Vec::new();
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
                    out.push(record(n, props, std::mem::take(&mut alarms), true));
                }
                cur = Some(Vec::new());
            } else if name == "VALARM" && skip.is_empty() && cur.is_some() {
                alarms.push(Vec::new());
                skip.push(name.to_string());
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
                    out.push(record(n, props, std::mem::take(&mut alarms), false));
                }
            }
        } else if skip.is_empty() {
            if let (Some(props), Some(p)) = (cur.as_mut(), textfmt::parse_prop(line)) {
                props.push(p);
            }
        } else if skip == ["VALARM"] && cur.is_some() {
            if let (Some(a), Some(p)) = (alarms.last_mut(), textfmt::parse_prop(line)) {
                a.push(p);
            }
        }
    }
    if let Some(props) = cur.take() {
        n += 1;
        out.push(record(n, props, std::mem::take(&mut alarms), true));
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

fn record(n: usize, props: Vec<Prop>, alarms: Vec<Vec<Prop>>, truncated: bool) -> Parsed {
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
    // Un VALARM absent laisse `reminder` à `None` : l'appelant ne doit pas y lire un effacement (le format ne dit pas si
    // l'application du téléphone sait écrire des rappels de tâche).
    let raw = |name: &str| props.iter().find(|p| p.name == name).map(|p| textfmt::decode_value(p, &mut Vec::new()));
    let (raw_start, raw_due) = (raw("DTSTART"), raw("DUE"));
    let mut found = alarms.iter().map(|a| alarm_reminder(a, raw_start.as_deref(), raw_due.as_deref()));
    for r in found.by_ref() {
        match r {
            Ok(t) => {
                f.reminder = Some(t);
                break;
            }
            Err(m) => w.push(m),
        }
    }
    if f.reminder.is_some() && alarms.len() > 1 {
        w.push("plusieurs VALARM : seul le premier lisible est gardé (un rappel par tâche)".into());
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

    fn with_reminder(due: &str, reminder: Option<&str>) -> TaskForm {
        TaskForm { due: due.into(), reminder: reminder.map(Into::into), ..task("Rappel") }
    }

    #[test]
    fn rappel_aller_retour() {
        // absolu (heure d'hiver depuis le 25/10/2026 : UTC+1), à une autre date que l'échéance, sans échéance, sans rappel
        let forms = vec![
            with_reminder("2026-10-30", Some("2026-10-29T09:30")),
            with_reminder("2026-12-15", Some("2026-12-14T08:00")),
            with_reminder("", Some("2026-11-02T18:45")),
            with_reminder("2026-10-30", None),
        ];
        let out = export(&forms);
        assert!(out.contains("TRIGGER;VALUE=DATE-TIME:20261029T083000Z") && out.contains("TRIGGER;VALUE=DATE-TIME:20261214T070000Z"));
        assert_eq!(out.matches("BEGIN:VALARM").count(), 3);
        assert!(out.lines().all(|l| l.len() <= 75));
        let back: Vec<TaskForm> = parse(&out).unwrap().into_iter().map(|p| p.form).collect();
        assert_eq!(back, forms);
    }

    #[test]
    fn rappel_relatif_du_telephone() {
        let todo = |extra: &str, alarm: &str| format!("BEGIN:VTODO\nSUMMARY:x\n{extra}\nBEGIN:VALARM\nACTION:DISPLAY\n{alarm}\nEND:VALARM\nEND:VTODO");
        let read = |t: String| parse(&t).unwrap().remove(0);
        // relatif à DUE (jour entier = minuit de Paris), à DTSTART, avec heure UTC
        assert_eq!(read(todo("DUE;VALUE=DATE:20261030", "TRIGGER:-PT15H")).form.reminder.as_deref(), Some("2026-10-29T09:00"));
        assert_eq!(read(todo("DTSTART:20261030T080000", "TRIGGER;RELATED=START:-PT15M")).form.reminder.as_deref(), Some("2026-10-30T07:45"));
        assert_eq!(read(todo("DUE:20261030T100000Z", "TRIGGER:-PT30M")).form.reminder.as_deref(), Some("2026-10-30T10:30"));
        // sans ancre, illisible, sans TRIGGER : pas de rappel, avec avertissement
        for (extra, alarm) in [("", "TRIGGER:-PT15M"), ("DUE;VALUE=DATE:20261030", "TRIGGER:bof"), ("", "X-FOO:1")] {
            let p = read(todo(extra, alarm));
            assert_eq!(p.form.reminder, None);
            assert_eq!(p.warnings.len(), 1, "{:?}", p.warnings);
        }
        // pas de VALARM : `None` (jamais lu comme un effacement par ce module)
        assert_eq!(read("BEGIN:VTODO\nSUMMARY:x\nEND:VTODO".into()).form.reminder, None);
        // deux alarmes : la première lisible, avertissement
        let two = "BEGIN:VTODO\nSUMMARY:x\nDUE;VALUE=DATE:20261030\nBEGIN:VALARM\nTRIGGER:-PT1H\nEND:VALARM\nBEGIN:VALARM\nTRIGGER:-PT2H\nEND:VALARM\nEND:VTODO";
        let p = read(two.into());
        assert_eq!(p.form.reminder.as_deref(), Some("2026-10-29T23:00"));
        assert!(p.warnings.iter().any(|w| w.contains("plusieurs")));
    }

    #[test]
    fn rappel_periodique_decale_comme_l_echeance() {
        // `spawn_next` décale début, échéance et rappel du même nombre de jours : l'écart rappel/échéance lu depuis un
        // déclencheur relatif reste donc celui du fichier pour l'occurrence suivante.
        let t = "BEGIN:VTODO\nSUMMARY:Série\nDTSTART;VALUE=DATE:20261023\nDUE;VALUE=DATE:20261030\nRRULE:FREQ=WEEKLY;INTERVAL=1\nBEGIN:VALARM\nTRIGGER:-PT15H\nEND:VALARM\nEND:VTODO";
        let f = parse(t).unwrap().remove(0).form;
        assert_eq!((f.repeat.as_str(), f.reminder.as_deref()), ("weekly", Some("2026-10-29T09:00")));
        let back = parse(&export(std::slice::from_ref(&f))).unwrap().remove(0).form;
        assert_eq!(back, f);
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
