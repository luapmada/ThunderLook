//! Surveillance IDLE de la Boîte de réception du compte de test : affiche chaque signal du serveur pendant `secondes`,
//! puis l'arrête comme au retrait du compte (`WatchStop`) et mesure le temps que met le fil à rendre la main.
//! Usage : `cargo run -p thunderlook-sync --example idle -- <dossier de test> <source> <secondes>`

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use thunderlook_sync::WatchStop;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, source, secs) = (PathBuf::from(&args[0]), args[1].parse::<i64>().unwrap_or(1), args[2].parse::<u64>().unwrap_or(60));
    let offline = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(WatchStop::default());
    let started = Instant::now();
    let watcher = {
        let (offline, cancel) = (offline.clone(), cancel.clone());
        std::thread::spawn(move || {
            thunderlook_sync::watch_inbox_blocking(
                &dir,
                source,
                &offline,
                &cancel,
                move || println!("signal IDLE après {:.1} s", started.elapsed().as_secs_f32()),
                |e| println!("coupure : {e}"),
            );
        })
    };
    println!("surveillance pendant {secs} s");
    std::thread::sleep(Duration::from_secs(secs));
    let t = Instant::now();
    cancel.stop();
    let _ = watcher.join();
    println!("arrêt demandé : surveillance terminée en {:.2} s (attente IDLE de 10 min interrompue)", t.elapsed().as_secs_f32());
}
