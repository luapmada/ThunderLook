//! Sonde de lecture PST avec le crate `outlook-pst` (Microsoft, MIT), en lecture seule.
//!
//! Usage : pst-probe <copie.pst> <dossier-de-sortie> [--open-all]
//! Parcourt tous les dossiers, compte contenus et contenus associés (FAI), relève les
//! classes des messages FAI et extrait dans le dossier de sortie les flux utiles à
//! l'interopérabilité (règles 0x6802, XML de configuration 0x7C08, dictionnaire 0x7C07).
//! La sortie contient des données personnelles : la garder dans data/ (ignoré par git).

use std::{collections::BTreeMap, env, fs, path::PathBuf, rc::Rc, time::Instant};

use outlook_pst::{
    ltp::prop_context::PropertyValue,
    messaging::{folder::Folder, store::Store},
    ndb::node_id::NodeId,
};

const NID_ROOT_FOLDER: u32 = 0x122;
const DUMP_PROPS: [u16; 4] = [0x6802, 0x7C06, 0x7C07, 0x7C08];

#[derive(Default)]
struct Stats {
    folders: usize,
    contents_rows: usize,
    fai_rows: usize,
    opened: usize,
    errors: usize,
    classes: BTreeMap<String, usize>,
    fai_classes: BTreeMap<String, usize>,
    dumps: usize,
    error_kinds: BTreeMap<String, usize>,
}

fn text(value: Option<&PropertyValue>) -> Option<String> {
    match value {
        Some(PropertyValue::Unicode(v)) => Some(String::from_utf16_lossy(v.buffer())),
        Some(PropertyValue::String8(v)) => Some(String::from_utf8_lossy(v.buffer()).into_owned()),
        _ => None,
    }
    .map(|s| s.trim_end_matches('\0').to_string())
}

fn bytes(value: &PropertyValue) -> Option<Vec<u8>> {
    match value {
        PropertyValue::Binary(v) => Some(v.buffer().to_vec()),
        PropertyValue::Unicode(v) => Some(v.buffer().iter().flat_map(|c| c.to_le_bytes()).collect()),
        PropertyValue::String8(v) => Some(v.buffer().to_vec()),
        _ => None,
    }
}

fn walk(store: &Rc<dyn Store>, folder: Rc<dyn Folder>, path: &str, st: &mut Stats, out: &PathBuf, open_all: bool) {
    st.folders += 1;
    let name = folder.properties().display_name().unwrap_or_else(|_| "?".into());
    let here = format!("{path}/{name}");

    if let Some(table) = folder.contents_table() {
        for row in table.rows_matrix() {
            st.contents_rows += 1;
            if !open_all {
                continue;
            }
            let opened = store
                .properties()
                .make_entry_id(NodeId::from(u32::from(row.id())))
                .and_then(|id| store.open_message(&id, Some(&[0x001A])));
            match opened {
                Ok(m) => {
                    st.opened += 1;
                    let class = text(m.properties().get(0x001A)).unwrap_or_else(|| "(absent)".into());
                    *st.classes.entry(class).or_default() += 1;
                }
                Err(e) => {
                    st.errors += 1;
                    let key = format!("{e}");
                    *st.error_kinds.entry(key).or_default() += 1;
                }
            }
        }
    }

    if let Some(table) = folder.associated_table() {
        for (i, row) in table.rows_matrix().enumerate() {
            st.fai_rows += 1;
            let opened = store
                .properties()
                .make_entry_id(NodeId::from(u32::from(row.id())))
                .and_then(|id| store.open_message(&id, None));
            let m = match opened {
                Ok(m) => m,
                Err(e) => {
                    st.errors += 1;
                    *st.error_kinds.entry(format!("FAI: {e}")).or_default() += 1;
                    continue;
                }
            };
            let class = text(m.properties().get(0x001A)).unwrap_or_else(|| "(absent)".into());
            *st.fai_classes.entry(format!("{class}  @ {name}")).or_default() += 1;
            for prop in DUMP_PROPS {
                if let Some(data) = m.properties().get(prop).and_then(bytes) {
                    let file = out.join(format!("f{:03}_{i:03}_{prop:04X}.bin", st.folders));
                    if fs::write(&file, &data).is_ok() {
                        st.dumps += 1;
                        println!("  dump {} ({} octets) classe={class} dossier={here}", file.display(), data.len());
                    }
                }
            }
        }
    }

    if let Some(table) = folder.hierarchy_table() {
        let children: Vec<_> = table.rows_matrix().map(|r| u32::from(r.id())).collect();
        for nid in children {
            match store
                .properties()
                .make_entry_id(NodeId::from(nid))
                .and_then(|id| store.open_folder(&id))
            {
                Ok(sub) => walk(store, sub, &here, st, out, open_all),
                Err(_) => st.errors += 1,
            }
        }
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let (src, out) = (&args[1], PathBuf::from(&args[2]));
    let open_all = args.iter().any(|a| a == "--open-all");
    fs::create_dir_all(&out)?;

    let t0 = Instant::now();
    let store = outlook_pst::open_store(src)?;
    let root_id = store.properties().make_entry_id(NodeId::from(NID_ROOT_FOLDER))?;
    let root = store.open_folder(&root_id)?;
    let mut st = Stats::default();
    walk(&store, root, "", &mut st, &out, open_all);

    println!(
        "dossiers={} lignes_contenu={} ouverts={} lignes_fai={} dumps={} erreurs={} duree={:.1}s",
        st.folders, st.contents_rows, st.opened, st.fai_rows, st.dumps, st.errors, t0.elapsed().as_secs_f64()
    );
    println!("-- classes FAI (classe @ dossier) --");
    for (k, v) in &st.fai_classes {
        println!("{v:6}  {k}");
    }
    println!("-- erreurs --");
    for (k, v) in &st.error_kinds {
        println!("{v:6}  {k}");
    }
    if open_all {
        println!("-- classes des contenus --");
        for (k, v) in &st.classes {
            println!("{v:6}  {k}");
        }
    }
    Ok(())
}
