//! Écarts ThunderLook ↔ Outlook, à blanc : la base de ThunderLook est ouverte en lecture seule.
//! Usage : `cargo run -p thunderlook-exchange --example outlook_diff -- <données ThunderLook> <source PST> <base> <base des
//! calendriers> <Outlook> <rapport.json>` ; les trois derniers dossiers de données contiennent chacun une seule source
//! PST importée (`tl-import --in-place`).

use std::{env, fs, path::Path, process::ExitCode};

use thunderlook_exchange::{
    ItemKind,
    outlook_diff::{Side, Snapshot, compare, snapshot},
};
use thunderlook_store::rusqlite::{Connection, OpenFlags};

fn read_only(dir: &Path) -> Connection {
    Connection::open_with_flags(dir.join("thunderlook.db"), OpenFlags::SQLITE_OPEN_READ_ONLY).expect("base")
}

fn snap(dir: &str, source: i64) -> Snapshot {
    let dir = Path::new(dir);
    let conn = read_only(dir);
    snapshot(&Side { conn: &conn, dir, source }).expect("lecture")
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [here, source, base, base_cal, outlook, out] = args.as_slice() else {
        eprintln!("usage : outlook_diff <données ThunderLook> <source PST> <base> <base des calendriers> <Outlook> <rapport.json>");
        return ExitCode::FAILURE;
    };
    let here = snap(here, source.parse().expect("source"));
    let (base, base_cal, outlook) = (snap(base, 1), snap(base_cal, 1), snap(outlook, 1));
    let d = compare(&|k| if k == ItemKind::Event { &base_cal } else { &base }, &outlook, &here);
    println!("{}", serde_json::to_string_pretty(&d.by_kind).unwrap_or_default());
    fs::write(out, serde_json::to_string_pretty(&d).unwrap_or_default()).expect("rapport");
    ExitCode::SUCCESS
}
