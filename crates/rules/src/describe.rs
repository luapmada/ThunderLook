//! Description d'une règle en français, ligne à ligne, à partir des libellés d'Outlook (catalogue).

use crate::{Element, Rule, Value, catalog};

fn quoted(list: &[String]) -> String {
    list.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(" ou ")
}

/// Date PT_APPTIME (jours depuis le 30/12/1899) au format JJ/MM/AAAA.
fn apptime(days: f64) -> String {
    // Algorithme civil (Howard Hinnant) à partir du nombre de jours depuis le 1970-01-01.
    let z = days.floor() as i64 - 25_569 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{d:02}/{m:02}/{y}")
}

fn importance(level: u32) -> &'static str {
    match level {
        0 => "faible",
        2 => "haute",
        _ => "normale",
    }
}

fn sensitivity(level: u32) -> &'static str {
    match level {
        1 => "personnel",
        2 => "privé",
        3 => "confidentiel",
        _ => "normal",
    }
}

/// Texte de la valeur d'un élément, à placer à la place de `%s` dans son libellé.
fn value_text(e: &Element) -> String {
    let mut parts: Vec<String> = Vec::new();
    for v in &e.values {
        match v {
            Value::Flags { flags, .. } if e.id == 0x190 => {
                let mut w = Vec::new();
                if flags & 1 != 0 {
                    w.push("après l'arrivée du message");
                }
                if flags & 4 != 0 {
                    w.push("après avoir envoyé le message");
                }
                parts.push(w.join(" ou "));
            }
            Value::Recipients { recipients, .. } => {
                for r in recipients {
                    parts.push(match (&r.name, &r.address) {
                        (Some(n), Some(a)) if n != a => format!("{n} <{a}>"),
                        (_, Some(a)) => a.clone(),
                        (Some(n), None) => n.clone(),
                        (None, None) => "?".into(),
                    });
                }
            }
            Value::Words { text, .. } => parts.push(format!("\"{text}\"")),
            Value::Level { level, .. } => parts.push(if e.id == 0xD3 || e.id == 0x1FF { sensitivity(*level) } else { importance(*level) }.into()),
            Value::Text { text, .. } | Value::Account { name: text, .. } | Value::AddressBook { name: text, .. } => parts.push(text.clone()),
            Value::FlagFor { text, n, .. } if e.id == 0x131 => parts.push(format!("{text} dans {n} jour(s)")),
            Value::FlagFor { text, .. } => parts.push(text.clone()),
            Value::Size { min_kb, max_kb, .. } => parts.push(format!("comprise entre {min_kb} et {max_kb} Ko")),
            Value::Date { test_after, after, test_before, before, .. } => {
                let mut w = Vec::new();
                if *test_after != 0 {
                    w.push(format!("après le {}", apptime(*after)));
                }
                if *test_before != 0 {
                    w.push(format!("avant le {}", apptime(*before)));
                }
                parts.push(w.join(" et "));
            }
            Value::Form { name, .. } => parts.push(name.clone()),
            Value::Folder { name, .. } => parts.push(name.clone()),
            Value::Minutes { minutes, .. } => parts.push(minutes.to_string()),
            Value::ServerReply { subject, .. } => parts.push(format!("\"{subject}\"")),
            Value::FollowUp { text, .. } => parts.push(text.clone()),
            Value::Flags { .. } | Value::Machine { .. } => {}
        }
    }
    if parts.len() > 1 && e.values.iter().all(|v| matches!(v, Value::Words { .. })) {
        let words: Vec<String> = e.values.iter().filter_map(|v| if let Value::Words { text, .. } = v { Some(text.clone()) } else { None }).collect();
        return quoted(&words);
    }
    parts.join(" ou ")
}

/// Ligne d'un élément ; `None` pour l'élément invisible `0x64`.
pub fn describe_element(e: &Element) -> Option<String> {
    if e.id == 0x64 {
        return None;
    }
    let label = catalog(e.id).map(|c| c.2).filter(|l| !l.is_empty()).unwrap_or("(élément inconnu)");
    let label = if label == "(néant)" { "(élément obsolète)" } else { label };
    Some(if label.contains("%s") { label.replace("%s", &value_text(e)) } else { label.to_string() })
}

/// Description complète, dans l'ordre de la règle (moment d'application, conditions, actions, exceptions).
pub fn describe(rule: &Rule) -> Vec<String> {
    let mut out: Vec<String> = rule.elements.iter().filter_map(describe_element).collect();
    if let Some(e) = &rule.error {
        out.push(format!("(lecture incomplète : {e})"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::apptime;

    #[test]
    fn apptime_dates() {
        assert_eq!(apptime(43_466.0), "01/01/2019");
        assert_eq!(apptime(43_465.5), "31/12/2018");
    }
}
