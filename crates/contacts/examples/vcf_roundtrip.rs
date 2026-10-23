//! Aller-retour vCard sur les vrais contacts d'un dossier : `form_of` → export → parse doit redonner les mêmes formulaires.
use thunderlook_contacts::vcard::{Version, export, parse};

fn main() -> Result<(), String> {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&a[1]);
    let folder: i64 = a[2].parse().map_err(|_| "dossier")?;
    let conn = thunderlook_store::open(dir).map_err(|e| e.to_string())?;
    let all = thunderlook_contacts::load(&conn, Some(folder), None)?;
    let forms: Vec<_> = all.contacts.iter().map(|c| thunderlook_contacts::write::form_from(c, dir)).collect();
    for (name, v) in [("2.1", Version::V21), ("3.0", Version::V30)] {
        let text = export(&forms, v);
        let back = parse(&text)?;
        let (mut same, mut diff) = (0, 0);
        for (f, p) in forms.iter().zip(&back) {
            let mut g = p.form.clone();
            g.id = f.id.clone();
            g.folder_id = f.folder_id.clone();
            if &g == f {
                same += 1;
            } else {
                diff += 1;
                if diff <= 3 {
                    println!("  écart ({name}) : {:?} / {:?}", f.surname, f.given_name);
                    macro_rules! cmp { ($($x:ident),*) => { $( if f.$x != g.$x { println!("    {} : {:?} -> {:?}", stringify!($x), f.$x, g.$x); } )* } }
                    cmp!(given_name, surname, company, job_title, department, email1, email2, email3, mobile, business, home_phone, work_street, work_city, work_postal, work_country, home_street, home_city, home_postal, home_country, web_page, birthday, notes);
                }
            }
        }
        println!("vCard {name} : {} contacts, {} octets, relus {} ; identiques {same}, différents {diff}", forms.len(), text.len(), back.len());
    }
    Ok(())
}
