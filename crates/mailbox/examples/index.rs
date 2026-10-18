//! Indexation plein texte d'un dossier de données, en ligne de commande (mesures du J4).
//! Usage : cargo run --release -p thunderlook-mailbox --example index -- <dossier-de-données>

use std::{env, io::Write, path::PathBuf, sync::atomic::AtomicBool, time::Instant};

fn main() {
    let data = PathBuf::from(env::args().nth(1).expect("dossier de données"));
    let t0 = Instant::now();
    let mut last = Instant::now();
    let stop = AtomicBool::new(false);
    let n = thunderlook_mailbox::index_pending(&data, &stop, &mut |done, total| {
        if last.elapsed().as_secs() >= 2 || done == total {
            last = Instant::now();
            eprint!("\r{done} / {total} ({:.0} s)   ", t0.elapsed().as_secs_f64());
            let _ = std::io::stderr().flush();
        }
    })
    .expect("indexation");
    eprintln!();
    println!("{n} messages indexés en {:.1} s", t0.elapsed().as_secs_f64());
}
