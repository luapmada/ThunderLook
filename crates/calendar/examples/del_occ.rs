//! Essai : supprime une occurrence d'une série de la base et relit les occurrences (`<dossier> <id> <AAAA-MM-JJ>`).
fn main() -> Result<(), String> {
    let a: Vec<String> = std::env::args().collect();
    let mut conn = thunderlook_store::open(std::path::Path::new(&a[1])).map_err(|e| e.to_string())?;
    let id: i64 = a[2].parse().map_err(|_| "id")?;
    thunderlook_calendar::write::delete_occurrence(&mut conn, id, &a[3])?;
    let cal = thunderlook_calendar::load(&conn)?;
    let ap = cal.appointments.iter().find(|x| x.id == id).ok_or("série absente de la lecture")?;
    let r = ap.recurrence.as_ref().ok_or("pas de périodicité")?;
    println!("supprimées : {:?} ; fin {:?} ; issues {}", r.deleted, r.end, cal.issues.len());
    for i in &cal.issues {
        println!("  issue {} {}", i.id, i.detail);
    }
    Ok(())
}
