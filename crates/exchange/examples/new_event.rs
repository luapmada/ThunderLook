//! Crée un rendez-vous dans un calendrier de ThunderLook (application fermée), pour éprouver l'envoi vers le téléphone
//! au passage suivant (seul le calendrier par défaut part vers le téléphone).
//! Usage : `cargo run -p thunderlook-exchange --example new_event -- <dossier de données> <n° du calendrier> <sujet> <début> <fin>`
//! (dates locales `AAAA-MM-JJTHH:MM`).

use std::{env, path::Path, process::ExitCode};

use thunderlook_calendar::write::{Edit, create};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [data, folder, subject, start, end] = args.as_slice() else {
        eprintln!("usage : new_event <dossier de données> <n° du calendrier> <sujet> <début> <fin>");
        return ExitCode::FAILURE;
    };
    let data = Path::new(data);
    let mut conn = match thunderlook_store::open(data) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("base : {e}");
            return ExitCode::FAILURE;
        }
    };
    let Ok(folder) = folder.parse::<i64>() else {
        eprintln!("dossier : nombre attendu");
        return ExitCode::FAILURE;
    };
    let edit = Edit { subject: subject.clone(), start: start.clone(), end: end.clone(), busy: 2, ..Edit::default() };
    match create(&mut conn, data, folder, &edit) {
        Ok(id) => {
            println!("créé : message {id} dans le dossier {folder}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("création : {e}");
            ExitCode::FAILURE
        }
    }
}
