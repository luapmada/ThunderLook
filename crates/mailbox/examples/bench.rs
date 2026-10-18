//! Mesures du J2 sur un dossier de données réel, et balayage de robustesse du volet de lecture.
//! Usage : cargo run --release -p thunderlook-mailbox --example bench -- <dossier-de-données> [pas]
//! `pas` : un message sur `pas` passe par `message_view` (1 = tous).

use std::{collections::BTreeMap, env, path::PathBuf, time::Instant};

use thunderlook_mailbox::Mailbox;
use thunderlook_model::{BodyView, Folder, ListQuery, SearchQuery, SearchScope, SortField};

fn all_folders(f: &[Folder], out: &mut Vec<Folder>) {
    for x in f {
        out.push(x.clone());
        all_folders(&x.children, out);
    }
}

fn main() {
    let data = PathBuf::from(env::args().nth(1).expect("dossier de données"));
    let step: usize = env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(50);

    let t = Instant::now();
    let mut mb = Mailbox::open(&data).expect("ouverture");
    println!("ouverture (migrations comprises) : {:?}", t.elapsed());

    let t = Instant::now();
    let tree = mb.folder_tree().expect("arborescence");
    println!("arborescence : {:?}", t.elapsed());
    let mut folders = Vec::new();
    for s in &tree.sections {
        println!("  section {} : {} dossiers de premier niveau", s.name, s.folders.len());
        if s.id != "favorites" && s.id != "searches" {
            all_folders(&s.folders, &mut folders);
        }
    }
    folders.sort_by_key(|f| std::cmp::Reverse(f.total));
    let big = folders.first().expect("au moins un dossier").clone();
    println!("plus gros dossier : {} ({} éléments, {} non lus)", big.name, big.total, big.unread);

    for sort in SortField::ALL {
        for unread_only in [false, true] {
            let q = ListQuery { folder_id: big.id.clone(), unread_only, sort, descending: sort.default_descending() };
            let t = Instant::now();
            let view = mb.open_list(&q).expect("vue");
            let open = t.elapsed();
            let t = Instant::now();
            let rows = mb.rows(view.token, 0, 60).expect("lignes");
            println!(
                "  tri {:?}{} : vue {:?} ({} éléments, {} groupes), 60 premières lignes {:?} — 1re : {} | {}",
                sort,
                if unread_only { " non lus" } else { "" },
                open,
                view.count,
                view.groups.len(),
                t.elapsed(),
                rows.first().map(|r| r.time_label.as_str()).unwrap_or("-"),
                view.groups.first().map(|g| g.label.as_str()).unwrap_or("-"),
            );
        }
    }

    // Recherche (J4) : étendue « tous les fichiers de données », tri par date.
    let tree = mb.folder_tree().expect("arborescence");
    for s in tree.sections.iter().filter(|s| s.id == "searches") {
        for f in &s.folders {
            let t = Instant::now();
            let v = mb
                .open_list(&ListQuery { folder_id: f.id.clone(), unread_only: false, sort: SortField::Date, descending: true })
                .expect("dossier de recherche");
            println!("  dossier de recherche « {} » : {} éléments en {:?} (compteur du volet : {})", f.name, v.count, t.elapsed(), f.total);
        }
    }
    for text in [
        "facture",
        "fact",
        "\"bon de commande\"",
        "de:paul",
        "facture pj:oui",
        "objet:devis reçu:2015",
        "lu:non",
        "suivi:oui",
        "adresse:paul@adam-declic.fr",
        "facture OU devis -relance",
    ] {
        let q = SearchQuery { text: text.into(), scope: SearchScope::All, folder_id: big.id.clone() };
        let t = Instant::now();
        let v = mb.search(&q, SortField::Date, true).expect("recherche");
        let open = t.elapsed();
        let rows = mb.rows(v.token, 0, 60).expect("lignes");
        println!(
            "  recherche {text:?} : {} résultats en {open:?} (+60 lignes {:?}) — 1er : {}",
            v.count,
            t.elapsed() - open,
            rows.first().map(|r| format!("{} [{}]", r.subject, r.folder.clone().unwrap_or_default())).unwrap_or_default()
        );
    }

    // Balayage : toutes les vues de tous les dossiers, et un message sur `step` jusqu'au volet de lecture.
    let t = Instant::now();
    let (mut seen, mut viewed, mut html, mut text, mut empty, mut remote, mut attachments) = (0u64, 0u64, 0, 0, 0, 0, 0u64);
    let mut slowest = (std::time::Duration::ZERO, String::new());
    let mut failures: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in &folders {
        let view = mb
            .open_list(&ListQuery { folder_id: f.id.clone(), unread_only: false, sort: SortField::Date, descending: true })
            .expect("vue");
        let ids: Vec<i64> = mb.view_ids(view.token).unwrap().to_vec();
        for (i, id) in ids.iter().enumerate() {
            seen += 1;
            if i % step != 0 {
                continue;
            }
            let id = format!("m{id}");
            let t1 = Instant::now();
            match mb.message_view(&id, false) {
                Ok(v) => {
                    viewed += 1;
                    match v.body {
                        BodyView::Html { .. } => html += 1,
                        BodyView::Text { .. } => text += 1,
                        BodyView::Empty => empty += 1,
                    }
                    remote += u64::from(v.remote_blocked);
                    attachments += v.attachments.len() as u64;
                    for a in &v.attachments {
                        if let Err(e) = mb.attachment(&id, a.index) {
                            failures.entry(format!("pièce jointe : {e}")).or_default().push(id.clone());
                        }
                    }
                }
                Err(e) => failures.entry(e).or_default().push(id.clone()),
            }
            let d = t1.elapsed();
            if d > slowest.0 {
                slowest = (d, id);
            }
        }
    }
    println!(
        "balayage : {} dossiers, {seen} éléments, {viewed} ouverts dans le volet en {:?} (HTML {html}, texte {text}, vide {empty}, contenu distant bloqué {remote}, pièces jointes {attachments})",
        folders.len(),
        t.elapsed()
    );
    println!("plus lent : {} en {:?}", slowest.1, slowest.0);
    if failures.is_empty() {
        println!("aucun échec");
    }
    for (e, ids) in &failures {
        println!("échec ({}) : {e} — ex. {:?}", ids.len(), &ids[..ids.len().min(5)]);
    }
}
