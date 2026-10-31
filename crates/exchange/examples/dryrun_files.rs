//! Relecture À BLANC d'un dossier écrit par MPE : base ThunderLook ouverte en lecture seule, rien n'est appliqué ni écrit
//! ailleurs que dans `<données temporaires>` (base de comparaison et correspondances, vides au départ).
//! Usage : `cargo run -p thunderlook-exchange --example dryrun_files -- <dossier de la base> <données temporaires> <dossier d'échange>`
//! Les données temporaires doivent donner accès aux corps (`eml/`) et aux sources : jonctions vers ceux de la vraie base.
//! N'affiche que des comptes et des objets de rendez-vous (jamais le contenu des fiches).

use std::{collections::BTreeMap, env, path::Path, process::ExitCode};

use thunderlook_exchange::{ChangeKind, Payload};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [db, data, dir] = args.as_slice() else {
        eprintln!("usage : dryrun_files <dossier de la base> <données temporaires> <dossier d'échange>");
        return ExitCode::FAILURE;
    };
    let conn = match thunderlook_store::open_readonly(Path::new(db)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("base : {e}");
            return ExitCode::FAILURE;
        }
    };
    let rep = match thunderlook_exchange::import_changes(&conn, Path::new(data), Path::new(dir)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("relecture : {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for c in rep.changes.iter().chain(&rep.conflicts) {
        let what = match (c.kind, c.conflict.is_some()) {
            (_, true) => "conflit",
            (ChangeKind::New, _) => "nouveau",
            (ChangeKind::Modified, _) => "modifié",
            (ChangeKind::Deleted, _) => "supprimé",
        };
        *counts.entry(format!("{} {what}", c.item.name())).or_default() += 1;
    }
    println!("identiques : {}", rep.unchanged);
    for (k, n) in &counts {
        println!("{k} : {n}");
    }
    println!("avertissements : {}", rep.warnings.len());
    let mut why: BTreeMap<String, usize> = BTreeMap::new();
    for w in &rep.warnings {
        *why.entry(w.rsplit(" : ").next().unwrap_or("").chars().take(90).collect()).or_default() += 1;
    }
    for (w, n) in &why {
        println!("  avertissement « {w} » : {n}");
    }
    // Champs en cause (le résumé finit par « : champ, champ ») : dit si les écarts sont réels ou de forme.
    let mut fields: BTreeMap<String, usize> = BTreeMap::new();
    for c in &rep.conflicts {
        let inside = c.summary.rsplit_once(" : ").map_or("", |(_, r)| r);
        for f in inside.trim_start_matches("série (").trim_end_matches(')').split(", ") {
            *fields.entry(format!("{} {f}", c.item.name())).or_default() += 1;
        }
    }
    let mut fields: Vec<_> = fields.into_iter().collect();
    fields.sort_by(|a, b| b.1.cmp(&a.1));
    for (f, n) in fields.iter().take(30) {
        println!("  écart {f} : {n}");
    }
    // Rappels : valeur du téléphone contre celle de ThunderLook.
    let cal = thunderlook_calendar::load(&conn).map(|c| c.appointments).unwrap_or_default();
    let mut pairs: BTreeMap<String, usize> = BTreeMap::new();
    for c in rep.conflicts.iter().filter(|c| c.summary.contains("reminder")) {
        let Payload::Event(e) = &c.payload else { continue };
        let here = cal.iter().find(|a| format!("a{}", a.id) == c.id).and_then(|a| a.reminder_minutes);
        *pairs.entry(format!("téléphone {:?} / ThunderLook {:?}", e.reminder, here)).or_default() += 1;
    }
    for (p, n) in &pairs {
        println!("  rappel {p} : {n}");
    }
    // Contacts : écart réel ou de forme ? Comparaison des seuls chiffres (sans afficher les valeurs).
    let all = thunderlook_contacts::load(&conn, None, None).map(|c| c.contacts).unwrap_or_default();
    let digits = |s: &str| {
        let d: String = s.chars().filter(char::is_ascii_digit).collect();
        d.strip_prefix("33").map(|r| format!("0{r}")).unwrap_or(d)
    };
    let (mut form, mut real, mut kinds) = (0, 0, BTreeMap::<String, usize>::new());
    for c in rep.conflicts.iter().filter(|c| c.item.name() == "contact") {
        let Payload::Contact(p) = &c.payload else { continue };
        let Some(here) = all.iter().find(|x| format!("p{}", x.id) == c.id) else { continue };
        let h = thunderlook_contacts::write::form_from(here, Path::new(data));
        for (name, a, b) in [("mobile", &p.mobile, &h.mobile), ("otherPhone", &p.other_phone, &h.other_phone), ("homePhone", &p.home_phone, &h.home_phone), ("business", &p.business, &h.business)] {
            if a != b {
                if digits(a) == digits(b) { form += 1 } else { real += 1; *kinds.entry(format!("{name} {}", if a.is_empty() { "vide au tél." } else if b.is_empty() { "vide ici" } else { "différent" })).or_default() += 1; }
            }
        }
    }
    println!("  téléphones : {form} écarts de forme, {real} réels {kinds:?}");
    // Notes de contacts : écart de blancs seulement, ou de texte ? (le texte n'est jamais affiché)
    let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let (mut blanks, mut text, mut longer) = (0, 0, 0);
    for c in rep.conflicts.iter().filter(|c| c.summary.contains("notes")) {
        let Payload::Contact(p) = &c.payload else { continue };
        let Some(here) = all.iter().find(|x| format!("p{}", x.id) == c.id) else { continue };
        let h = thunderlook_contacts::write::form_from(here, Path::new(data));
        if squash(&p.notes) == squash(&h.notes) { blanks += 1 } else { text += 1; if squash(&p.notes).starts_with(&squash(&h.notes)) || squash(&h.notes).starts_with(&squash(&p.notes)) { longer += 1 } }
    }
    println!("  notes : {blanks} blancs seulement, {text} texte ({longer} l'un prolonge l'autre)");
    for c in rep.changes.iter().chain(&rep.conflicts).filter(|c| matches!(c.payload, Payload::Event(_))).take(25) {
        let Payload::Event(e) = &c.payload else { continue };
        println!("  rdv {:?}{} : {} | {}", c.kind, if c.conflict.is_some() { " (conflit)" } else { "" }, e.start, e.subject);
    }
    ExitCode::SUCCESS
}
