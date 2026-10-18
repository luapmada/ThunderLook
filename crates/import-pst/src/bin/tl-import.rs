//! Import PST en ligne de commande (mesures du J1).
//!
//! Usage : tl-import <fichier.pst> <dossier-de-données> [--in-place] [--replace]

use std::{env, io::Write, path::PathBuf, process::ExitCode, time::Instant};

use thunderlook_import_pst::{Options, Phase, import};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let [src, data] = positional[..] else {
        eprintln!("Usage : tl-import <fichier.pst> <dossier-de-données> [--in-place] [--replace]");
        return ExitCode::from(2);
    };
    let options = Options { in_place: args.iter().any(|a| a == "--in-place"), replace: args.iter().any(|a| a == "--replace") };
    let t0 = Instant::now();
    let mut last = Instant::now();
    let result = import(&PathBuf::from(src), &PathBuf::from(data), &options, &mut |p| {
        if last.elapsed().as_millis() >= 1000 {
            last = Instant::now();
            let what = match p.phase {
                Phase::Copy => format!("copie {} / {} Mo", p.done >> 20, p.total >> 20),
                Phase::Import => format!("{} éléments — {}", p.done, p.folder),
            };
            eprint!("\r{what:<100.100}");
            let _ = std::io::stderr().flush();
        }
    });
    eprintln!();
    match result {
        Ok(r) => {
            let mut summary = serde_json::to_value(&r).unwrap_or_default();
            if let Some(obj) = summary.as_object_mut() {
                obj.remove("issues");
            }
            println!("{}", serde_json::to_string_pretty(&summary).unwrap_or_default());
            for i in r.issues.iter().take(30) {
                println!("[{}] {} {} nid={:?} : {}", i.severity, i.kind, i.folder, i.nid, i.detail);
            }
            println!("Durée totale : {:.1} s", t0.elapsed().as_secs_f64());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Échec : {e}");
            ExitCode::FAILURE
        }
    }
}
