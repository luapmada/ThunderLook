//! Un passage de synchronisation, comme « Relire maintenant » + « Appliquer malgré le seuil » de ThunderLook, quand
//! l'application est fermée (première lecture d'un téléphone : le seuil de sécurité bloquerait les compléments).
//! Instantané annulable comme tout passage (`exchange-undo/`), consigné dans le journal.
//! Usage : `cargo run -p thunderlook-exchange --example pass_now -- <dossier de données> <dossier d'échange>`

use std::{env, path::Path, process::ExitCode};

use thunderlook_exchange::auto::{Params, run_and_record};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [data, dir] = args.as_slice() else {
        eprintln!("usage : pass_now <dossier de données> <dossier d'échange>");
        return ExitCode::FAILURE;
    };
    let (data, dir) = (Path::new(data), Path::new(dir));
    let mut conn = match thunderlook_store::open(data) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("base : {e}");
            return ExitCode::FAILURE;
        }
    };
    let p = Params { recheck: true, ignore_threshold: true, auto_export: true, ..Params::default() };
    let (r, saved) = run_and_record(&mut conn, data, dir, &p);
    println!(
        "{:?} : créés {}, modifiés {}, conflits {}, suppressions proposées {}, instantané {:?}, envoi {:?} {}",
        r.outcome, r.created, r.modified, r.aside.conflicts, r.aside.deletions, r.undo, r.exported, r.export_note
    );
    if !r.errors.is_empty() {
        println!("erreurs : {}", r.errors.join(" ; "));
    }
    if let Err(e) = saved {
        eprintln!("état non enregistré : {e}");
    }
    ExitCode::SUCCESS
}
