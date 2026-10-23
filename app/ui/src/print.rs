//! Impression (calendrier, contacts, tâches, notes) : la feuille à imprimer est construite dans `#print-root`
//! (masqué à l'écran, seul visible sous `@media print`, voir `styles/print.css`), puis `window.print()`.
//! « Enregistrer en PDF » : imprimante PDF de Windows.

use std::cell::RefCell;

use leptos::{prelude::*, task::spawn_local};
use thunderlook_model::{
    calendar::{CalendarDay, CalendarEvent, CalendarView, CalendarViewKind, WEEKDAYS_SHORT},
    contacts::{ContactCard, ContactFieldKind, ContactRowKind},
    items::{NoteList, TaskList},
};

use crate::{
    ipc,
    state::{Module, Ui},
};

thread_local! {
    /// Listes affichées à l'écran (vue, tri et recherche appliqués), mémorisées par les modules Tâches et Notes.
    static TASKS: RefCell<Option<TaskList>> = const { RefCell::new(None) };
    static NOTES: RefCell<Option<NoteList>> = const { RefCell::new(None) };
}

pub fn remember_tasks(list: &TaskList) {
    TASKS.with(|t| *t.borrow_mut() = Some(list.clone()));
}

pub fn remember_notes(list: &NoteList) {
    NOTES.with(|t| *t.borrow_mut() = Some(list.clone()));
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// `AAAA-MM-JJ` → `JJ/MM/AAAA` (autre forme : telle quelle).
fn date_fr(d: &str) -> String {
    match d.split('-').collect::<Vec<_>>().as_slice() {
        [y, m, d] if y.len() == 4 => format!("{d}/{m}/{y}"),
        _ => d.to_string(),
    }
}

/// Dépose la feuille, règle le sens de la page, imprime.
fn print_sheet(html: &str, landscape: bool) {
    let doc = document();
    let root = match doc.get_element_by_id("print-root") {
        Some(r) => r,
        None => {
            let Ok(r) = doc.create_element("div") else { return };
            r.set_id("print-root");
            if let Some(b) = doc.body() {
                let _ = b.append_child(&r);
            }
            r
        }
    };
    root.set_inner_html(html);
    let style = match doc.get_element_by_id("print-page") {
        Some(s) => s,
        None => {
            let Ok(s) = doc.create_element("style") else { return };
            s.set_id("print-page");
            if let Some(b) = doc.body() {
                let _ = b.append_child(&s);
            }
            s
        }
    };
    style.set_text_content(Some(&format!("@page {{ size: A4 {}; margin: 12mm; }}", if landscape { "landscape" } else { "portrait" })));
    // Laisse le navigateur appliquer la feuille avant l'aperçu.
    request_animation_frame(|| {
        let _ = window().print();
    });
}

/// Commande « Imprimer » du ruban : `kind` vaut « page » (vue courante), « card » ou « list » (Contacts).
pub fn run(ui: Ui, kind: &str) {
    match ui.module.get_untracked() {
        Module::Calendar => match ui.cal_view.get_untracked() {
            Some(v) => {
                let landscape = matches!(v.kind, CalendarViewKind::Week | CalendarViewKind::WorkWeek | CalendarViewKind::Month);
                print_sheet(&calendar_html(&v), landscape);
            }
            None => ui.error.set(Some("Calendrier pas encore chargé : rien à imprimer.".into())),
        },
        Module::Contacts if kind == "list" => contacts_list(ui),
        Module::Contacts => match ui.contact_card.get_untracked() {
            Some(c) => print_sheet(&card_html(&c), false),
            None => ui.error.set(Some("Sélectionnez un contact à imprimer.".into())),
        },
        Module::Tasks => match TASKS.with(|t| t.borrow().clone()) {
            Some(l) => print_sheet(&tasks_html(&l), false),
            None => ui.error.set(Some("Liste des tâches pas encore chargée.".into())),
        },
        Module::Notes => match NOTES.with(|t| t.borrow().clone()) {
            Some(l) => print_sheet(&notes_html(&l), false),
            None => ui.error.set(Some("Liste des notes pas encore chargée.".into())),
        },
        Module::Mail => ui.error.set(Some("L'impression du courrier n'est pas encore disponible.".into())),
    }
}

// ---------- Calendrier ----------

fn event_text(e: &CalendarEvent) -> (String, String) {
    // Rendez-vous privé : l'objet n'est pas imprimé.
    let subject = if e.private { "Rendez-vous privé".to_string() } else { e.subject.clone() };
    let place = e.location.as_deref().filter(|_| !e.private).map(|l| format!(" ({})", esc(l))).unwrap_or_default();
    (esc(&subject), place)
}

fn event_line(e: &CalendarEvent) -> String {
    let (subject, place) = event_text(e);
    let when = if e.when.is_empty() { String::new() } else { format!("<span class=\"when\">{}</span> ", esc(&e.when)) };
    format!("<div class=\"ev\">{when}<strong>{subject}</strong>{place}</div>")
}

/// Rendez-vous d'un jour : toute la journée d'abord, puis par heure de début.
fn day_events(d: &CalendarDay) -> String {
    let mut timed: Vec<_> = d.timed.iter().collect();
    timed.sort_by_key(|t| (t.start_min, t.end_min));
    d.all_day.iter().map(event_line).chain(timed.into_iter().map(|t| event_line(&t.event))).collect()
}

fn calendar_html(v: &CalendarView) -> String {
    let title = format!("<h1>Calendrier — {}</h1>", esc(&v.title));
    match v.kind {
        CalendarViewKind::Month => {
            let head: String = WEEKDAYS_SHORT.iter().map(|d| format!("<th>{d}</th>")).collect();
            let rows: String = v
                .days
                .chunks(7)
                .map(|week| {
                    let cells: String = week
                        .iter()
                        .map(|d| {
                            let class = if d.in_month { "" } else { " class=\"out\"" };
                            format!("<td{class}><div class=\"dn\">{}</div>{}</td>", esc(&d.label), day_events(d))
                        })
                        .collect();
                    format!("<tr>{cells}</tr>")
                })
                .collect();
            format!("{title}<table class=\"cal month\"><thead><tr>{head}</tr></thead><tbody>{rows}</tbody></table>")
        }
        CalendarViewKind::Day => {
            let Some(d) = v.days.first() else { return title };
            let (mut first, mut last) = (7u32, 19u32);
            for t in &d.timed {
                first = first.min(t.start_min / 60);
                last = last.max(t.end_min.saturating_sub(1) / 60).min(23);
            }
            let hours: String = (first..=last)
                .map(|h| {
                    let here: String = d.timed.iter().filter(|t| t.start_min / 60 == h).map(|t| event_line(&t.event)).collect();
                    format!("<tr><th>{h:02}:00</th><td>{here}</td></tr>")
                })
                .collect();
            let all_day: String = d.all_day.iter().map(event_line).collect();
            let all_day = if all_day.is_empty() { String::new() } else { format!("<div class=\"allday\">{all_day}</div>") };
            let list = day_events(d);
            let list = if list.is_empty() { "<p>Aucun rendez-vous.</p>".to_string() } else { list };
            format!("{title}{all_day}<table class=\"cal day\"><tbody>{hours}</tbody></table><h2>Rendez-vous du jour</h2><div class=\"agenda\">{list}</div>")
        }
        _ => {
            let head: String = v.days.iter().map(|d| format!("<th>{}</th>", esc(&d.label))).collect();
            let cells: String = v.days.iter().map(|d| format!("<td>{}</td>", day_events(d))).collect();
            format!("{title}<table class=\"cal week\"><thead><tr>{head}</tr></thead><tbody><tr>{cells}</tr></tbody></table>")
        }
    }
}

// ---------- Contacts ----------

fn card_html(c: &ContactCard) -> String {
    let photo = c.photo.as_deref().map(|p| format!("<img class=\"photo\" src=\"{}\" alt=\"\" />", esc(p))).unwrap_or_default();
    let job = if c.job.is_empty() { String::new() } else { format!("<p>{}</p>", esc(&c.job)) };
    let cats = if c.categories.is_empty() { String::new() } else { format!("<p>Catégories : {}</p>", esc(&c.categories.join(", "))) };
    let sections: String = c
        .sections
        .iter()
        .map(|(title, fields)| {
            let dl: String = fields.iter().map(|f| format!("<dt>{}</dt><dd>{}</dd>", esc(&f.label), esc(&f.value))).collect();
            format!("<section class=\"keep\"><h2>{}</h2><dl>{dl}</dl></section>", esc(title))
        })
        .collect();
    let notes = c.notes.as_deref().map(|n| format!("<section class=\"keep\"><h2>Notes</h2><p class=\"pre\">{}</p></section>", esc(n))).unwrap_or_default();
    format!("<article class=\"card keep\">{photo}<h1>{}</h1>{job}{cats}{sections}{notes}</article>", esc(&c.title))
}

fn contact_row_html(title: &str, company: &str, phones: &str, emails: &str) -> String {
    format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>", esc(title), esc(company), esc(phones), esc(emails))
}

/// Liste du dossier affiché (filtre de la liste appliqué) : une fiche lue par contact, la barre d'état indique l'avancement.
fn contacts_list(ui: Ui) {
    let rows = crate::contacts::visible(ui);
    if rows.is_empty() {
        ui.error.set(Some("Aucun contact à imprimer.".into()));
        return;
    }
    let folder = ui
        .contact_folders
        .with_untracked(|f| f.iter().find(|f| f.id == ui.contact_folder.get_untracked()).map(|f| f.name.clone()))
        .unwrap_or_else(|| "Contacts".into());
    spawn_local(async move {
        let total = rows.len();
        let mut body = String::new();
        for (i, r) in rows.iter().enumerate() {
            ui.status.set(Some(format!("Préparation de l'impression : {} / {total}", i + 1)));
            if r.kind == ContactRowKind::DistList {
                let members = r.members.iter().map(|m| if m.email.is_empty() { m.name.clone() } else { m.email.clone() }).collect::<Vec<_>>().join(", ");
                body.push_str(&contact_row_html(&r.title, "Liste de distribution", "", &members));
                continue;
            }
            match ipc::contact_card(&r.id).await {
                Ok(c) => {
                    let fields = || c.sections.iter().flat_map(|(_, f)| f.iter());
                    let phones = fields().filter(|f| f.kind == ContactFieldKind::Phone).map(|f| format!("{} : {}", f.label, f.value)).collect::<Vec<_>>().join(" ; ");
                    let emails = fields().filter(|f| f.kind == ContactFieldKind::Email).map(|f| f.value.clone()).collect::<Vec<_>>().join(", ");
                    body.push_str(&contact_row_html(&c.title, &c.job, &phones, &emails));
                }
                // Fiche illisible : la ligne reste, avec ce que la liste en connaît.
                Err(_) => body.push_str(&contact_row_html(&r.title, &r.subtitle, "", "")),
            }
        }
        ui.status.set(None);
        let html = format!(
            "<h1>Contacts — {}</h1><table class=\"list\"><thead><tr><th>Nom</th><th>Société / fonction</th><th>Téléphones</th><th>Courriel</th></tr></thead><tbody>{body}</tbody></table>",
            esc(&folder)
        );
        print_sheet(&html, false);
    });
}

// ---------- Tâches ----------

fn tasks_html(l: &TaskList) -> String {
    const STATUSES: [&str; 5] = ["Pas commencée", "En cours", "Terminée", "En attente", "Reportée"];
    const IMPORTANCES: [&str; 3] = ["Basse", "Normale", "Haute"];
    let rows: String = l
        .rows
        .iter()
        .map(|t| {
            let status = if t.complete { STATUSES[2] } else { STATUSES.get(t.status as usize).copied().unwrap_or("") };
            let importance = IMPORTANCES.get(t.importance as usize).copied().unwrap_or("");
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(&t.title),
                esc(&date_fr(&t.due)),
                status,
                importance,
                esc(&t.categories.join(", "))
            )
        })
        .collect();
    if l.rows.is_empty() {
        return "<h1>Tâches</h1><p>Aucune tâche.</p>".into();
    }
    format!("<h1>Tâches</h1><table class=\"list\"><thead><tr><th>Tâche</th><th>Échéance</th><th>État</th><th>Importance</th><th>Catégories</th></tr></thead><tbody>{rows}</tbody></table>")
}

// ---------- Notes ----------

fn notes_html(l: &NoteList) -> String {
    if l.rows.is_empty() {
        return "<h1>Notes</h1><p>Aucune note.</p>".into();
    }
    let blocks: String = l
        .rows
        .iter()
        .map(|n| format!("<article class=\"note keep\"><h2>{}</h2><p class=\"date\">{}</p><p class=\"pre\">{}</p></article>", esc(&n.title), esc(&date_fr(&n.date)), esc(&n.text)))
        .collect();
    format!("<h1>Notes</h1>{blocks}")
}
