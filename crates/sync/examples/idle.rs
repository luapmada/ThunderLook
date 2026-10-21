//! Surveillance IDLE de la Boîte de réception du compte de test : affiche chaque signal du serveur pendant `secondes`.
//! Usage : `cargo run -p thunderlook-sync --example idle -- <dossier de test> <source> <secondes>`

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, source, secs) = (PathBuf::from(&args[0]), args[1].parse::<i64>().unwrap_or(1), args[2].parse::<u64>().unwrap_or(60));
    let stop = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    let watcher = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            thunderlook_sync::watch_inbox_blocking(
                &dir,
                source,
                &stop,
                move || println!("signal IDLE après {:.1} s", started.elapsed().as_secs_f32()),
                |e| println!("coupure : {e}"),
            );
        })
    };
    println!("surveillance pendant {secs} s");
    std::thread::sleep(Duration::from_secs(secs));
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    // Le fil rend la main au prochain signal ou à l'échéance de l'attente ; on n'attend pas jusque-là.
    drop(watcher);
    println!("fin");
}
