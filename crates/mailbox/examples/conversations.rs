//! Mesures du J3b : vue en conversations des Boîtes de réception et du plus gros dossier.
//! Usage : cargo run --release -p thunderlook-mailbox --example conversations -- <dossier-de-données>

use std::{env, path::PathBuf, time::Instant};

use thunderlook_mailbox::Mailbox;
use thunderlook_model::{Folder, FolderRole, ListQuery, SortField};

fn all_folders(f: &[Folder], out: &mut Vec<Folder>) {
    for x in f {
        out.push(x.clone());
        all_folders(&x.children, out);
    }
}

fn main() {
    let data = PathBuf::from(env::args().nth(1).expect("dossier de données"));
    let t = Instant::now();
    let mut mb = Mailbox::open(&data).expect("ouverture");
    println!("ouverture (migrations et calcul des conversations compris) : {:?}", t.elapsed());

    let tree = mb.folder_tree().expect("arborescence");
    let mut folders = Vec::new();
    for s in tree.sections.iter().filter(|s| s.id != "favorites" && s.id != "searches") {
        all_folders(&s.folders, &mut folders);
    }
    let mut targets: Vec<Folder> = folders.iter().filter(|f| f.role == FolderRole::Inbox).cloned().collect();
    if let Some(big) = folders.iter().max_by_key(|f| f.total) {
        targets.push(big.clone());
    }
    for f in &targets {
        for sort in [SortField::Categories, SortField::Flag, SortField::Attachments] {
            let q = ListQuery { folder_id: f.id.clone(), unread_only: false, sort, descending: false, conversations: false };
            let t = Instant::now();
            let view = mb.open_list(&q).expect("vue");
            println!(
                "{} ({} éléments), organiser par {:?} : {} lignes en {:?}, groupes {:?}",
                f.name,
                f.total,
                sort,
                view.count,
                t.elapsed(),
                view.groups.iter().take(4).map(|g| format!("{} ({})", g.label, g.count)).collect::<Vec<_>>()
            );
        }
    }
    for f in targets {
        for unread_only in [false, true] {
            let q = ListQuery { folder_id: f.id.clone(), unread_only, sort: SortField::Date, descending: true, conversations: true };
            let t = Instant::now();
            let view = mb.open_list(&q).expect("vue");
            let open = t.elapsed();
            let t = Instant::now();
            let rows = mb.rows(view.token, 0, 100).expect("lignes");
            let first_page = t.elapsed();
            let heads: Vec<_> = rows.iter().filter(|r| r.conversation.is_some()).collect();
            println!("  positions des en-têtes : {:?}", heads.iter().map(|h| h.pos).collect::<Vec<_>>());
            println!(
                "{} ({} éléments){} : {} lignes, {} groupes, vue {open:?}, 100 premières lignes {first_page:?}, dont {} en-têtes",
                f.name,
                f.total,
                if unread_only { " non lus" } else { "" },
                view.count,
                view.groups.len(),
                heads.len(),
            );
            if unread_only {
                continue;
            }
            // Toutes les lignes (500 au plus) : passe par les plus grosses conversations s'il y en a.
            let t = Instant::now();
            let all = mb.rows(view.token, 0, view.count.min(500)).expect("lignes");
            let biggest = all.iter().filter_map(|r| r.conversation.map(|c| (c.count, r.subject.clone()))).max();
            println!("  {} lignes en {:?} ; plus grosse conversation : {biggest:?}", all.len(), t.elapsed());
            // Dépliage de la première conversation à plusieurs éléments, puis repli.
            if let Some(h) = heads.first() {
                let t = Instant::now();
                let v = mb.toggle_conversation(view.token, h.pos, Some(true)).expect("dépliage");
                let unfold = t.elapsed();
                let kids = mb.rows(v.token, h.pos + 1, h.conversation.map_or(0, |c| c.count)).expect("lignes");
                println!(
                    "  dépliage de « {} » ({} éléments) : {unfold:?} ; {}",
                    h.subject,
                    h.conversation.map_or(0, |c| c.count),
                    kids.iter().map(|k| format!("{} [{}]", k.from, k.folder.clone().unwrap_or_else(|| "ici".into()))).collect::<Vec<_>>().join(", ")
                );
                let back = mb.toggle_conversation(view.token, h.pos, Some(false)).expect("repli");
                assert_eq!(back.count, view.count, "le repli rend la vue de départ");
            }
        }
    }
}
