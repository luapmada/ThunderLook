//! Reporte dans ThunderLook (application fermée) les changements faits dans Outlook seul : voir `outlook_diff`, mêmes
//! arguments sans le rapport. Les conflits et les changements faits ici ne sont pas touchés ; annulation par le fichier
//! `exchange-undo/<horodatage>.json` du dossier de données.
//! Usage : `cargo run -p thunderlook-exchange --example outlook_align -- <données ThunderLook> <source PST> <base> <base
//! des calendriers> <Outlook> [NID,…]` ; les NID facultatifs désignent des conflits tranchés pour Outlook.

use std::{env, path::Path, process::ExitCode};

use thunderlook_exchange::{
    ItemKind,
    outlook_diff::{Side, Snapshot, align, snapshot},
};
use thunderlook_store::rusqlite::{Connection, OpenFlags};

fn read_only(dir: &Path) -> Connection {
    Connection::open_with_flags(dir.join("thunderlook.db"), OpenFlags::SQLITE_OPEN_READ_ONLY).expect("base")
}

fn snap(conn: &Connection, dir: &str, source: i64) -> Snapshot {
    snapshot(&Side { conn, dir: Path::new(dir), source }).expect("lecture")
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let (args, wins) = match args.as_slice() {
        [a @ .., w] if args.len() == 6 => (a, w.split(',').filter_map(|n| n.trim().parse().ok()).collect::<Vec<i64>>()),
        a => (a, vec![]),
    };
    let [here_dir, source, base, base_cal, outlook_dir] = args else {
        eprintln!("usage : outlook_align <données ThunderLook> <source PST> <base> <base des calendriers> <Outlook> [NID,… des conflits tranchés pour Outlook]");
        return ExitCode::FAILURE;
    };
    let source: i64 = source.parse().expect("source");
    let mut conn = thunderlook_store::open(Path::new(here_dir)).expect("base de ThunderLook");
    let here = snap(&conn, here_dir, source);
    let (base_conn, cal_conn, outlook_conn) = (read_only(Path::new(base)), read_only(Path::new(base_cal)), read_only(Path::new(outlook_dir)));
    let (base, base_cal, outlook) = (snap(&base_conn, base, 1), snap(&cal_conn, base_cal, 1), snap(&outlook_conn, outlook_dir, 1));
    let from = Side { conn: &outlook_conn, dir: Path::new(outlook_dir), source: 1 };
    match align(&mut conn, Path::new(here_dir), source, &from, &|k| if k == ItemKind::Event { &base_cal } else { &base }, &outlook, &here, &wins) {
        Ok(r) => {
            println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
            if r.errors.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        Err(e) => {
            eprintln!("report : {e}");
            ExitCode::FAILURE
        }
    }
}
