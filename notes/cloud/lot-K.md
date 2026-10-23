# Lot K — courrier : le reste de l'inventaire

Branche : `cloud/lot-k-courrier-suite`. Lis d'abord `notes/cloud/README.md` et `notes/courrier-ecarts.md` (inventaire écrit par le lot J : ce qui est fait est coché, le reste ne l'est pas).

Périmètre de fichiers : `crates/mailbox/`, `crates/rules/`, `crates/mime/`, `crates/model/src/lib.rs` (types du courrier), `app/ui/src/{list,reading,compose,folders,menus,actions}.rs`, nouveau fichier Tauri `app/src-tauri/src/mail_cmds2.rs`. Ne touche pas à `crates/sync` (IMAP), ni aux comptes, ni au calendrier, aux contacts, aux tâches et aux notes.

À faire : réalise, dans l'ordre, les écarts NON cochés de `notes/courrier-ecarts.md` qui n'exigent ni IMAP ni Windows. Pour chacun : test sous Linux si c'est de la logique, `cargo check -p thunderlook-ui --target wasm32-unknown-unknown` pour l'interface. Coche chaque ligne faite dans `notes/courrier-ecarts.md`. Écarts qui demandent IMAP ou Windows : les laisser non cochés avec la raison.

Fin : push, résumé de 10 lignes (fait, reste, gestes à essayer à l'écran).
