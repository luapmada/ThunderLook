//! Extrait des blobs `PidLidAppointmentRecur` réels de la base, sans texte personnel (objets et lieux d'exception
//! absents), pour les fixtures de test : `<dossier de données> <fichier de sortie>`.
use std::collections::HashMap;

use thunderlook_calendar::{
    bag::Bag,
    named::{Named, PSETID_APPOINTMENT},
    recur::{End, Frequency, Pattern, Recurrence},
};

fn main() -> Result<(), String> {
    let a: Vec<String> = std::env::args().collect();
    let conn = thunderlook_store::open(std::path::Path::new(&a[1])).map_err(|e| e.to_string())?;
    let mut named: HashMap<i64, Named> = HashMap::new();
    {
        let mut st = conn.prepare("SELECT id, props FROM source WHERE props IS NOT NULL").map_err(|e| e.to_string())?;
        for r in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))).map_err(|e| e.to_string())? {
            let (id, raw) = r.map_err(|e| e.to_string())?;
            named.insert(id, Named::parse(&raw)?);
        }
    }
    let mut st = conn
        .prepare("SELECT f.source_id, m.props FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.message_class LIKE 'IPM.Appointment%'")
        .map_err(|e| e.to_string())?;
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out = String::from("# Blobs PidLidAppointmentRecur réels (un par ligne : étiquette<TAB>hexadécimal). Aucun objet ni lieu d'exception.\n");
    for r in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))).map_err(|e| e.to_string())? {
        let (source, props) = r.map_err(|e| e.to_string())?;
        let Some(n) = named.get(&source) else { continue };
        let Some(id) = n.id(PSETID_APPOINTMENT, 0x8216) else { continue };
        let Ok(bag) = Bag::parse(&props) else { continue };
        let Some(blob) = bag.binary(id) else { continue };
        let Ok(rec) = Recurrence::parse(blob) else { continue };
        if rec.exceptions.iter().any(|e| e.subject.is_some() || e.location.is_some()) || blob.len() > 600 {
            continue;
        }
        let freq = match rec.frequency {
            Frequency::Daily => "jour",
            Frequency::Weekly => "semaine",
            Frequency::Monthly => "mois",
            Frequency::Yearly => "an",
        };
        let pat = match rec.pattern {
            Pattern::Day => "j",
            Pattern::Week { .. } => "sem",
            Pattern::Month { .. } => "jourdumois",
            Pattern::MonthNth { .. } => "nieme",
            Pattern::MonthEnd => "fin",
        };
        let end = match rec.end {
            End::Date => "date",
            End::Count(_) => "n",
            End::Never => "jamais",
        };
        let label = format!("{freq}-{pat}-{end}-exc{}", if rec.exceptions.is_empty() { "0" } else { "N" });
        let count = seen.entry(label.clone()).or_default();
        if *count >= 2 {
            continue;
        }
        *count += 1;
        out.push_str(&format!("{label}\t{}\n", blob.iter().map(|b| format!("{b:02x}")).collect::<String>()));
    }
    std::fs::write(&a[2], &out).map_err(|e| e.to_string())?;
    println!("{} blobs écrits", out.lines().count() - 1);
    Ok(())
}
