# Lot Y — sauvegardes : retirer le délai fixe de 240 s, vérifier les restaurations

Branche : `cloud/lot-y-sauvegardes`. Lis d'abord `notes/cloud/README.md`, `notes/handoff.md` (« sauvegarde différée de 240 s ») et `crates/safety/`.

Périmètre de fichiers : `crates/safety/` (+ tests), `app/src-tauri/src/safety_cmds.rs`, `app/ui/src/safety.rs`. Aucun autre fichier.

À faire :
1. Comprendre pourquoi la sauvegarde est différée de 240 s au lancement (lire le code et `notes/perf.md`) ; la remplacer par un déclenchement fondé sur l'inactivité réelle ou sur la fin du chargement, sans bloquer l'interface ni la base.
2. Sauvegarde cohérente d'une base SQLite en écriture (API de sauvegarde en ligne ou `VACUUM INTO`, pas une copie de fichier brute) ; rotation (garder N), vérification d'intégrité (`PRAGMA integrity_check`) de la copie.
3. Test de restauration : créer, sauvegarder, corrompre/modifier, restaurer, comparer.
4. Interface : afficher date et taille de la dernière sauvegarde, bouton « Sauvegarder maintenant », échec visible.
5. `cargo test -p thunderlook-safety` (nom réel à vérifier) ; `cargo check -p thunderlook-ui --target wasm32-unknown-unknown`.

Fin : push, résumé de 10 lignes (dont ce que Paul doit vérifier à l'écran).
