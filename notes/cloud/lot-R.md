# Lot R — gérer les dossiers : calendriers, contacts, tâches, notes

Branche : `cloud/lot-r-dossiers`. Lis d'abord `notes/cloud/README.md`.

Contexte : calendriers, dossiers de contacts, de tâches et de notes viennent tous de l'import du PST ; on ne peut ni en créer, ni en renommer, ni en supprimer, ni changer la couleur d'un calendrier.

Périmètre de fichiers : nouveau crate `crates/folders/` (+ `[workspace] members`), nouveau fichier Tauri `app/src-tauri/src/folders_cmds.rs`, nouveau fichier d'interface `app/ui/src/item_folders.rs`, et UNIQUEMENT des lignes de branchement dans `lib.rs`, `main.rs`, `ipc.rs`, `state.rs`, `calendar.rs` (clic droit sur un calendrier du volet gauche), `contacts.rs`, `tasks.rs`, `notes.rs`.

À faire :
1. `thunderlook_folders::{create, rename, delete, set_color}` sur la table `folder` pour les dossiers de rendez-vous, contacts, tâches et notes (classes `IPF.Appointment`, `IPF.Contact`, `IPF.Task`, `IPF.StickyNote`) : nom unique par fichier de données ; suppression refusée tant que le dossier contient des éléments, sauf « déplacer le contenu vers… » ou « supprimer aussi le contenu » (via `thunderlook_safety::snapshot` si ce crate existe). NID des nouveaux dossiers ≥ 0x7000_0000, jamais de collision. Couleur d'un calendrier : fichier `calendar-colors.json` du dossier de données (0 à 24, palette des catégories d'Outlook, voir `notes/outlook-couleurs-menus.md`).
2. Interface : clic droit sur un calendrier du volet gauche (Nouveau calendrier, Renommer, Couleur ›, Supprimer) ; idem pour les dossiers de contacts ; dans Tâches et Notes, un sélecteur de dossier avec « Nouveau dossier ».
3. Tests sous Linux sur une base fabriquée : création, doublon refusé, suppression refusée, déplacement du contenu.

Fin : push, résumé de 10 lignes.
