//! Lecture d'un modèle .oft : objet, destinataires, corps, pièces jointes.
//! Usage : cargo run -p thunderlook-mailbox --example oft -- <modèle.oft>

fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("chemin du .oft"));
    let scratch = std::env::temp_dir().join(format!("tl-oft-{}", std::process::id()));
    let d = thunderlook_mailbox::draft_from_oft(&path, &scratch).expect("lecture");
    println!("objet : {:?}\nà : {:?}\ncc : {:?}\npièces jointes : {:?}", d.subject, d.to, d.cc, d.attachments.iter().map(|a| (&a.name, &a.size_label)).collect::<Vec<_>>());
    println!("corps HTML : {} caractères, styles : {} ; début : {:?}", d.html.len(), d.styles.len(), d.html.chars().filter(|c| !c.is_control()).take(300).collect::<String>());
    let _ = std::fs::remove_dir_all(&scratch);
}
