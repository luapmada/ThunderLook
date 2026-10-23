# Lot AF — interface de l'échange : choix des conflits, aperçu, journal

Branche : `cloud/lot-af-interface-echange`. Lis d'abord `notes/cloud/README.md`, `docs/mpe-fichiers.md`, `crates/exchange/src/conflict.rs` (lot AC, déjà fusionné : les conflits sont classés « téléphone seul », « ThunderLook seul », « les deux ») et `app/ui/src/exchange.rs`.

Périmètre de fichiers : `app/ui/src/exchange.rs`, `app/src-tauri/src/exchange_cmds.rs`, `app/ui/src/ipc.rs` (ajouts seulement), `app/ui/style/` ou le CSS d'échange s'il existe (ajouts seulement). Rien d'autre.

À faire :
1. Écran « téléphone » : liste des changements groupée par module (contacts, rendez-vous, notes, tâches), avec pour chacun l'ancienne et la nouvelle valeur ; les CONFLITS (modifié des deux côtés) dans une section à part, jamais cochés d'office, avec le choix « garder ThunderLook / garder le téléphone / ignorer ».
2. Les suppressions proposées restent décochées d'office ; compteur clair de ce qui sera appliqué ; confirmation avec le nombre d'éléments avant « Appliquer ».
3. Après application : récapitulatif (créés, modifiés, supprimés, ignorés, avertissements) et bouton « Annuler le dernier import » toujours visible.
4. Avertissements de l'import (série refusée, champ perdu…) affichés lisiblement, pas seulement dans un journal.
5. `cargo check -p thunderlook-ui --target wasm32-unknown-unknown` ; dire ce qui n'a pas pu être compilé (src-tauri).

Fin : push, résumé de 10 lignes (dont ce que Paul doit vérifier à l'écran).
