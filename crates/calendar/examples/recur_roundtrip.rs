//! Aller-retour des périodicités de la base : `encode(parse(blob))` doit redonner le blob des séries sans exception.
use std::collections::HashMap;

use thunderlook_calendar::{
    bag::Bag,
    named::{Named, PSETID_APPOINTMENT},
    recur::Recurrence,
};

fn main() -> Result<(), String> {
    let dir = std::env::args().nth(1).ok_or("dossier de données")?;
    let conn = thunderlook_store::open(std::path::Path::new(&dir)).map_err(|e| e.to_string())?;
    let mut named: HashMap<i64, Named> = HashMap::new();
    {
        let mut st = conn.prepare("SELECT id, props FROM source WHERE props IS NOT NULL").map_err(|e| e.to_string())?;
        let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))).map_err(|e| e.to_string())?;
        for r in rows {
            let (id, raw) = r.map_err(|e| e.to_string())?;
            named.insert(id, Named::parse(&raw)?);
        }
    }
    let mut st = conn
        .prepare("SELECT f.source_id, m.props FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.message_class LIKE 'IPM.Appointment%'")
        .map_err(|e| e.to_string())?;
    let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))).map_err(|e| e.to_string())?;
    let (mut total, mut simple, mut same, mut shown) = (0, 0, 0, 0);
    for r in rows {
        let (source, props) = r.map_err(|e| e.to_string())?;
        let Some(n) = named.get(&source) else { continue };
        let Some(id) = n.id(PSETID_APPOINTMENT, 0x8216) else { continue };
        let Ok(bag) = Bag::parse(&props) else { continue };
        let Some(blob) = bag.binary(id) else { continue };
        let Ok(rec) = Recurrence::parse(blob) else { continue };
        total += 1;
        if !rec.exceptions.is_empty() {
            continue;
        }
        simple += 1;
        let enc = rec.encode();
        if enc == blob {
            same += 1;
        } else if shown < 4 {
            shown += 1;
            let first = enc.iter().zip(blob).position(|(a, b)| a != b);
            println!("écart : {:?}/{:?} len {} / {} premier octet différent {:?}", rec.frequency, rec.pattern, enc.len(), blob.len(), first);
            println!("  blob {}", blob.iter().take(80).map(|b| format!("{b:02x}")).collect::<String>());
            println!("  enc  {}", enc.iter().take(80).map(|b| format!("{b:02x}")).collect::<String>());
        }
    }
    println!("séries {total}, sans exception {simple}, identiques après encodage {same}");
    Ok(())
}
