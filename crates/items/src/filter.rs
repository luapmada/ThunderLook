//! Filtres et tris purs des listes de tâches et de notes (testés ; l'interface les appelle par commande).

use chrono::{Datelike, Duration, NaiveDate};
use thunderlook_model::items::{NoteQuery, NoteRow, TaskQuery, TaskRow};

/// Tâches de la vue demandée, triées. `today` : jour courant.
pub fn filter_tasks(rows: Vec<TaskRow>, q: &TaskQuery, today: NaiveDate) -> Vec<TaskRow> {
    let day = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    let t = day(today);
    let monday = today - Duration::days(i64::from(today.weekday().num_days_from_monday()));
    let (w0, w1) = (day(monday), day(monday + Duration::days(6)));
    let text = q.text.trim().to_lowercase();
    let mut out: Vec<TaskRow> = rows
        .into_iter()
        .filter(|r| match q.view.as_str() {
            "today" => !r.complete && r.due == t,
            "overdue" => !r.complete && !r.due.is_empty() && r.due < t,
            "week" => !r.complete && !r.due.is_empty() && r.due >= w0 && r.due <= w1,
            "done" => r.complete,
            "folder" => r.folder == q.folder,
            _ => true,
        })
        .filter(|r| text.is_empty() || r.title.to_lowercase().contains(&text))
        .collect();
    let due_key = |r: &TaskRow| (r.due.is_empty(), r.due.clone());
    match q.sort.as_str() {
        "importance" => out.sort_by_key(|r| (r.complete, std::cmp::Reverse(r.importance), due_key(r), r.title.to_lowercase())),
        "title" => out.sort_by_key(|r| (r.complete, r.title.to_lowercase())),
        _ => out.sort_by_key(|r| (r.complete, due_key(r), r.title.to_lowercase())),
    }
    out
}

/// Notes dont le titre ou le contenu contient le texte cherché ; "alpha" : par titre, sinon l'ordre reçu (récentes d'abord).
pub fn filter_notes(mut rows: Vec<NoteRow>, q: &NoteQuery) -> Vec<NoteRow> {
    let text = q.text.trim().to_lowercase();
    if !text.is_empty() {
        rows.retain(|r| r.title.to_lowercase().contains(&text) || r.text.to_lowercase().contains(&text));
    }
    if q.sort == "alpha" {
        rows.sort_by_key(|r| r.title.to_lowercase());
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(title: &str, due: &str, done: bool, imp: i32, folder: &str) -> TaskRow {
        TaskRow { title: title.into(), due: due.into(), complete: done, importance: imp, folder: folder.into(), ..Default::default() }
    }

    fn sample() -> Vec<TaskRow> {
        vec![
            row("Zèbre", "2026-10-23", false, 0, "f1"),
            row("Ancienne", "2026-10-10", false, 2, "f1"),
            row("Dimanche", "2026-10-25", false, 1, "f2"),
            row("Lundi suivant", "2026-10-26", false, 1, "f2"),
            row("Sans date", "", false, 2, "f2"),
            row("Faite", "2026-10-01", true, 1, "f1"),
        ]
    }

    fn titles(v: &[TaskRow]) -> Vec<&str> {
        v.iter().map(|r| r.title.as_str()).collect()
    }

    fn q(view: &str, sort: &str) -> TaskQuery {
        TaskQuery { view: view.into(), sort: sort.into(), ..Default::default() }
    }

    const TODAY: (i32, u32, u32) = (2026, 10, 23); // vendredi

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(TODAY.0, TODAY.1, TODAY.2).unwrap()
    }

    #[test]
    fn views() {
        let go = |v: &str| titles(&filter_tasks(sample(), &q(v, ""), today())).join("|");
        assert_eq!(go("today"), "Zèbre");
        assert_eq!(go("overdue"), "Ancienne");
        assert_eq!(go("week"), "Zèbre|Dimanche");
        assert_eq!(go("done"), "Faite");
        assert_eq!(go("all"), "Ancienne|Zèbre|Dimanche|Lundi suivant|Sans date|Faite");
    }

    #[test]
    fn folder_view_and_text() {
        let f = TaskQuery { view: "folder".into(), folder: "f2".into(), ..Default::default() };
        assert_eq!(titles(&filter_tasks(sample(), &f, today())), ["Dimanche", "Lundi suivant", "Sans date"]);
        let t = TaskQuery { text: " LUNDI ".into(), ..Default::default() };
        assert_eq!(titles(&filter_tasks(sample(), &t, today())), ["Lundi suivant"]);
    }

    #[test]
    fn sorts() {
        let go = |s: &str| titles(&filter_tasks(sample(), &q("all", s), today())).join("|");
        assert_eq!(go("importance"), "Ancienne|Sans date|Dimanche|Lundi suivant|Zèbre|Faite");
        assert_eq!(go("title"), "Ancienne|Dimanche|Lundi suivant|Sans date|Zèbre|Faite");
    }

    #[test]
    fn notes_search_and_sort() {
        let n = |t: &str, x: &str| NoteRow { title: t.into(), text: x.into(), ..Default::default() };
        let rows = vec![n("banane", "fruit jaune"), n("Achats", "pain, lait"), n("zzz", "BANANE flambée")];
        let hit = filter_notes(rows.clone(), &NoteQuery { text: "banane".into(), sort: "".into() });
        assert_eq!(hit.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), ["banane", "zzz"]);
        let alpha = filter_notes(rows, &NoteQuery { text: "".into(), sort: "alpha".into() });
        assert_eq!(alpha.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), ["Achats", "banane", "zzz"]);
    }
}
