//! Diagnostic : propriétés nommées résolues, table des pièces jointes d'un message, et contenu des
//! messages joints.
//! Usage : cargo run --release --example attach_debug -- <copie.pst> <nid décimal>

use std::{env, rc::Rc};

use outlook_pst::{
    messaging::{
        attachment::{Attachment, AttachmentData, UnicodeAttachment},
        message::{Message, UnicodeMessage},
        named_prop::NamedPropertyId,
    },
    ndb::node_id::NodeId,
};
use thunderlook_import_pst::props::NamedProps;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let store = outlook_pst::open_store(&args[1])?;
    let named = NamedProps::read(&store)?;
    println!(
        "nommées : {} ; Keywords={:?} TaskDueDate={:?} FlagRequest={:?}",
        named.len(),
        named.keywords.map(|v| format!("{v:04X}")),
        named.task_due_date.map(|v| format!("{v:04X}")),
        named.flag_request.map(|v| format!("{v:04X}"))
    );
    let map = store.named_property_map()?;
    for entry in map.properties().stream_entry()?.iter().take(400) {
        if let NamedPropertyId::StringOffset(off) = entry.id()
            && let Ok(s) = map.properties().lookup_string(off)
            && s.to_string().contains("eyword")
        {
            println!("   chaîne {:?} guid={:?} idx=0x{:04X}", s.to_string(), entry.guid(), entry.prop_id());
        }
    }

    let nid: u32 = args[2].parse().unwrap();
    let id = store.properties().make_entry_id(NodeId::from(nid))?;
    let msg: Rc<dyn Message> = store.open_message(&id, None)?;
    let table = msg.attachment_table().expect("pas de table des pièces jointes");
    let concrete = msg.clone().into_any().downcast::<UnicodeMessage>().ok().expect("unicode");
    for row in table.rows_matrix() {
        let row_id = u32::from(row.id());
        match UnicodeAttachment::read(concrete.clone(), NodeId::from(row_id), None) {
            Ok(a) => {
                println!("ligne 0x{row_id:08X} : {} propriétés", a.properties().iter().count());
                if let Some(AttachmentData::Message(inner)) = a.data() {
                    let p = inner.properties();
                    println!(
                        "   message joint : {} propriétés, ids {:?}",
                        p.iter().count(),
                        p.iter().map(|(k, _)| format!("{k:04X}")).take(30).collect::<Vec<_>>()
                    );
                }
            }
            Err(e) => println!("ligne 0x{row_id:08X} : erreur {e}"),
        }
    }
    Ok(())
}
