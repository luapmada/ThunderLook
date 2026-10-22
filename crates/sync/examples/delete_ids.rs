//! Suppression définitive de messages précis d'un compte (opération de ThunderLook, reportée sur le serveur au
//! passage suivant), puis relève du compte pour que le report ait lieu.
//! Usage : `cargo run -p thunderlook-sync --example delete_ids -- <dossier de données> <source> m<id>,m<id>,…`

use std::path::Path;

use thunderlook_mailbox::Mailbox;
use thunderlook_model::{MessageOp, Selection};
use thunderlook_sync::sync_account_blocking;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, source) = (Path::new(&args[0]), args[1].parse::<i64>().map_err(|e| e.to_string())?);
    let ids: Vec<String> = args[2].split(',').map(str::to_string).collect();
    let mut mb = Mailbox::open(dir)?;
    let r = mb.apply(&Selection::ids(ids.clone()), &MessageOp::DeletePermanently)?;
    println!("{} élément(s) supprimé(s) localement : {}", r.affected, r.message);
    drop(mb);
    let report = sync_account_blocking(dir, source)?;
    println!("relève : {} dossier(s), report : {:?}", report.folders, report.push_error);
    Ok(())
}
