# Lot Z — doublons de noms de calendriers et dossiers parasites dans le volet gauche

Branche : `cloud/lot-z-volet-gauche`. Lis d'abord `notes/cloud/README.md`, `notes/handoff.md` (« `Calendar3098` et doublons de noms de calendriers ») et `crates/folders/`.

Périmètre de fichiers : `crates/folders/` (+ tests), `app/ui/src/item_folders.rs`, `app/ui/src/folders.rs`, `app/src-tauri/src/folders_cmds.rs`. Aucun autre fichier.

À faire :
1. Reproduire sur une base fabriquée : calendriers de noms identiques (comptes ou PST différents) et dossiers à nom technique (`Calendar3098`).
2. Affichage : nom lisible (nom du propriétaire/compte ou de la source en suffixe si doublon), jamais de nom technique brut ; un calendrier vide d'origine PST technique est masqué (ou regroupé) sans rien supprimer.
3. Distinguer visuellement les calendriers/contacts/tâches/notes de comptes différents ; ordre stable.
4. Aucune suppression ni fusion de données : affichage et libellés seulement.
5. `cargo test -p thunderlook-folders` (nom réel à vérifier) ; `cargo check -p thunderlook-ui --target wasm32-unknown-unknown`.

Fin : push, résumé de 10 lignes (dont ce que Paul doit vérifier à l'écran).
