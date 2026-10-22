//! Table des propriétés nommées d'un `.msg` ([MS-OXMSG] 2.2.3) : entrée brute, jeu, numéro ou nom, et si la propriété
//! est présente dans le flux des propriétés. Usage : `cargo run -p thunderlook-outlook --example msg_names -- <.msg>`

use std::io::Read;

fn stream(f: &mut cfb::CompoundFile<std::fs::File>, p: &str) -> Vec<u8> {
    let mut out = Vec::new();
    if let Ok(mut s) = f.open_stream(p) {
        let _ = s.read_to_end(&mut out);
    }
    out
}

fn main() -> Result<(), String> {
    let path = std::env::args().nth(1).ok_or(".msg manquant")?;
    let mut f = cfb::open(&path).map_err(|e| e.to_string())?;
    let guids = stream(&mut f, "/__nameid_version1.0/__substg1.0_00020102");
    let entries = stream(&mut f, "/__nameid_version1.0/__substg1.0_00030102");
    let props = stream(&mut f, "/__properties_version1.0");
    let present: Vec<u16> = props.get(32..).unwrap_or_default().chunks_exact(16).map(|e| u16::from_le_bytes([e[2], e[3]])).collect();
    println!("{} GUID, {} entrées, {} propriétés", guids.len() / 16, entries.len() / 8, present.len());
    for (i, e) in entries.chunks_exact(8).enumerate() {
        let key = u32::from_le_bytes([e[0], e[1], e[2], e[3]]);
        let ik = u32::from_le_bytes([e[4], e[5], e[6], e[7]]);
        let guid_index = (ik & 0xFFFF) >> 1;
        let prop = 0x8000 + (ik >> 16);
        let guid = if guid_index >= 3 { guids.get(((guid_index - 3) * 16) as usize..((guid_index - 2) * 16) as usize).map(|g| format!("{:02x?}", &g[..4])) } else { Some(format!("std{guid_index}")) };
        if key < 0x10 || (0x8200..0x8240).contains(&key) || (0x8500..0x8510).contains(&key) {
            println!("#{i} clé {key:#06x} chaîne {} guid {guid_index} {guid:?} → {prop:#06x} présente {}", ik & 1, present.contains(&(prop as u16)));
        }
    }
    Ok(())
}
