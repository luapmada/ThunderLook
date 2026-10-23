# Lot B — contacts complets : photo, listes de distribution, catégories, recherche

Branche : `cloud/lot-b-contacts`. Lis d'abord `notes/cloud/README.md`.

Contexte : les contacts se créent, se modifient et se suppriment dans la base (`crates/contacts/src/write.rs`, `app/ui/src/contact_form.rs`) ; les vCard s'exportent et s'importent (`crates/contacts/src/vcard.rs`). La photo (`ContactPicture.jpg` dans le `.eml` du contact) n'est ni éditable ni exportée ; les 3 listes de distribution (`IPM.DistList`) ne sont pas lues ; les catégories d'un contact ne sont pas éditables.

Périmètre de fichiers : `crates/contacts/` (tout), `crates/model/src/contacts.rs`, `app/ui/src/contact_form.rs`, `app/ui/src/contacts.rs`, nouveau fichier Tauri `app/src-tauri/src/contacts_cmds.rs`. Rien d'autre.

À faire (dans cet ordre ; chaque étape avec ses tests sous Linux) :
1. Photo : `ContactForm` gagne `photo: Option<String>` (`data:image/jpeg;base64,…`, `#[serde(default)]`). `write::create`/`update` écrivent la photo dans le `.eml` (déjà géré par `write_eml(.., photo)`) et mettent à jour `PidLidHasPicture` (named 0x8015) ; une photo absente du formulaire en modification est CONSERVÉE (ne la perds pas). Export/import vCard : `PHOTO` (ENCODING=b / BASE64, TYPE=JPEG) dans `vcard.rs`. Tests : aller-retour.
2. Interface : dans la boîte « Contact », un champ fichier pour choisir une image (la redimensionner à 256 px de côté maximum côté interface en JPEG avant envoi, via canvas) et un bouton « Retirer la photo ».
3. Catégories : `ContactForm.categories: String` (virgules) lues et écrites dans `Keywords` (comme `crates/calendar/src/write.rs`) ; champ dans la boîte ; export vCard `CATEGORIES`.
4. Listes de distribution : lecture seule d'abord. `thunderlook_contacts::dist_lists(conn) -> Vec<DistList { id, name, members: Vec<{name, email}> }>` en lisant `PidLidDistributionListMembers` ([MS-OXOCNTC] 2.2.2) ; les montrer dans la liste des contacts sous une rubrique « Listes de distribution » (nouveau type de ligne dans `ContactRow`, `kind`). Si le format des membres est trop incertain : lecture de ce qui est sûr et le reste en `REA-LIMITES.md`.
5. Recherche : un contact se trouve par société, courriel, téléphone (pas seulement par le titre) dans le filtre de `contacts.rs`.

Points INCERTAINS à consigner sans deviner : format exact des membres de liste (EntryID de pièce/one-off), `PidLidHasPicture` après suppression de la photo.
