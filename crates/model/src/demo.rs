//! Données de démonstration entièrement fictives (domaine réservé example.org, RFC 2606).
//! Remplacées par le magasin local à partir du jalon J1.

use chrono::{DateTime, Duration, Local, TimeZone, Timelike};

use crate::{
    CategoryColor::*, Folder, FolderRole, FolderSection, MailSnapshot, MessageBody, MessageSummary,
    format::{date_group, full_date, list_time},
};

fn folder(id: &str, name: &str, role: FolderRole, unread: u32, total: u32, children: Vec<Folder>) -> Folder {
    Folder { id: id.into(), name: name.into(), role, unread, total, children }
}

fn sections() -> Vec<FolderSection> {
    use FolderRole::*;
    vec![
        FolderSection {
            id: "favorites".into(),
            name: "Favoris".into(),
            folders: vec![
                folder("fav-inbox", "Réception", Inbox, 3, 6, vec![]),
                folder("fav-sent", "Envoyés", Sent, 0, 1, vec![]),
            ],
        },
        FolderSection {
            id: "store-demo".into(),
            name: "Archive importée".into(),
            folders: vec![
                folder(
                    "inbox",
                    "Réception",
                    Inbox,
                    3,
                    6,
                    vec![
                        folder("inbox-projects", "Projets", Folder, 1, 4, vec![]),
                        folder("inbox-invoices", "Factures", Folder, 0, 2, vec![]),
                    ],
                ),
                folder("drafts", "Brouillons", Drafts, 0, 1, vec![]),
                folder("sent", "Envoyés", Sent, 0, 1, vec![]),
                folder("deleted", "Supprimés", Deleted, 0, 0, vec![]),
                folder("junk", "Indésirables", Junk, 0, 2, vec![]),
                folder("outbox", "À envoyer", Outbox, 0, 0, vec![]),
                folder("archive", "Archives", Archive, 0, 0, vec![]),
            ],
        },
        FolderSection {
            id: "virtual".into(),
            name: "Dossiers virtuels".into(),
            folders: vec![
                folder("v-unread", "Non lus", Virtual, 3, 3, vec![]),
                folder("v-flagged", "Suivi", Virtual, 0, 1, vec![]),
            ],
        },
    ]
}

struct Demo {
    id: &'static str,
    from: &'static str,
    address: &'static str,
    subject: &'static str,
    preview: &'static str,
    body: &'static [&'static str],
    days_ago: i64,
    hour: u32,
    minute: u32,
    unread: bool,
    flagged: bool,
    attachments: bool,
    categories: &'static [crate::CategoryColor],
}

const MESSAGES: &[Demo] = &[
    Demo {
        id: "m1",
        from: "Équipe ThunderLook",
        address: "bienvenue@example.org",
        subject: "Bienvenue dans ThunderLook",
        preview: "Ce jeu de démonstration montre la disposition à trois volets avant l'import de vos archives.",
        body: &[
            "Bonjour,",
            "Ce message de démonstration montre la disposition à trois volets : dossiers, liste et lecture.",
            "À partir du jalon J1, ThunderLook importera une copie de vos fichiers PST, en lecture seule.",
            "Raccourcis déjà actifs : F6 et Maj+F6 pour passer d'un volet à l'autre, Ctrl+1 à Ctrl+4 pour changer de module, Ctrl+E pour la recherche, flèches pour parcourir la liste et les dossiers.",
            "L'équipe ThunderLook",
        ],
        days_ago: 0,
        hour: 9,
        minute: 12,
        unread: true,
        flagged: false,
        attachments: false,
        categories: &[Blue],
    },
    Demo {
        id: "m2",
        from: "Camille Martin",
        address: "camille.martin@example.org",
        subject: "Compte rendu de la réunion de lundi",
        preview: "Voici les points décidés et les prochaines étapes, avec le tableau en pièce jointe.",
        body: &["Bonjour,", "Voici les points décidés lors de la réunion de lundi.", "Bonne journée,", "Camille"],
        days_ago: 0,
        hour: 8,
        minute: 41,
        unread: true,
        flagged: true,
        attachments: true,
        categories: &[Green, Orange],
    },
    Demo {
        id: "m3",
        from: "Service facturation",
        address: "facturation@example.org",
        subject: "Votre facture n° 2026-104 est disponible",
        preview: "Le document est consultable dans votre espace client jusqu'au 30 novembre.",
        body: &["Bonjour,", "Votre facture est disponible dans votre espace client.", "Cordialement"],
        days_ago: 1,
        hour: 17,
        minute: 5,
        unread: false,
        flagged: false,
        attachments: true,
        categories: &[],
    },
    Demo {
        id: "m4",
        from: "Lucas Bernard",
        address: "lucas.bernard@example.org",
        subject: "Re: Planning du trimestre",
        preview: "D'accord pour décaler la livraison d'une semaine, je préviens l'équipe.",
        body: &["Bonjour,", "D'accord pour décaler la livraison d'une semaine.", "Lucas"],
        days_ago: 1,
        hour: 10,
        minute: 30,
        unread: true,
        flagged: false,
        attachments: false,
        categories: &[],
    },
    Demo {
        id: "m5",
        from: "Association du quartier",
        address: "contact@example.org",
        subject: "Assemblée générale annuelle",
        preview: "L'assemblée générale aura lieu samedi à 14 h, salle des fêtes.",
        body: &["Bonjour à toutes et à tous,", "L'assemblée générale aura lieu samedi à 14 h."],
        days_ago: 3,
        hour: 19,
        minute: 0,
        unread: false,
        flagged: false,
        attachments: false,
        categories: &[Purple],
    },
    Demo {
        id: "m6",
        from: "Bibliothèque municipale",
        address: "bibliotheque@example.org",
        subject: "Rappel : retour des ouvrages",
        preview: "Deux ouvrages sont à rendre avant la fin du mois.",
        body: &["Bonjour,", "Deux ouvrages sont à rendre avant la fin du mois."],
        days_ago: 12,
        hour: 11,
        minute: 20,
        unread: false,
        flagged: false,
        attachments: false,
        categories: &[],
    },
];

fn received(now: &DateTime<Local>, d: &Demo, index: usize) -> DateTime<Local> {
    let day = (*now - Duration::days(d.days_ago)).date_naive();
    let naive = day.and_hms_opt(d.hour, d.minute, 0).expect("heure valide");
    let mut at = Local.from_local_datetime(&naive).earliest().unwrap_or(*now);
    // Un message « d'aujourd'hui » ne doit pas être dans le futur si l'application démarre tôt.
    if at > *now {
        at = now.with_second(0).unwrap_or(*now) - Duration::minutes(7 + 23 * index as i64);
    }
    at
}

pub fn snapshot(now: DateTime<Local>) -> MailSnapshot {
    let messages = MESSAGES
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let at = received(&now, d, i);
            MessageSummary {
                id: d.id.into(),
                folder_id: "inbox".into(),
                from: d.from.into(),
                from_address: d.address.into(),
                to: "vous@example.org".into(),
                subject: d.subject.into(),
                preview: d.preview.into(),
                received: at.to_rfc3339(),
                group: date_group(&at, &now),
                time_label: list_time(&at, &now),
                date_label: full_date(&at),
                unread: d.unread,
                flagged: d.flagged,
                has_attachments: d.attachments,
                categories: d.categories.to_vec(),
            }
        })
        .collect();
    MailSnapshot { sections: sections(), messages, demo: true }
}

pub fn body(id: &str) -> Option<MessageBody> {
    MESSAGES
        .iter()
        .find(|d| d.id == id)
        .map(|d| MessageBody { id: d.id.into(), paragraphs: d.body.iter().map(|p| p.to_string()).collect() })
}
