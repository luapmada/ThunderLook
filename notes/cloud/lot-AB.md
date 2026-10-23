# Lot AB — rappels de tâches (VALARM) dans taches.ics

Branche : `cloud/lot-ab-valarm-taches`. Lis d'abord `notes/cloud/README.md` et le test `#[ignore]` `rappel_de_tache_exporte` de `crates/exchange/tests/scenarios.rs`.

Périmètre de fichiers : `crates/items/src/vtodo.rs` (+ tests de `crates/items`), `crates/exchange/tests/scenarios.rs` (UNIQUEMENT retirer l'`#[ignore]` de `rappel_de_tache_exporte` une fois corrigé). Rien d'autre.

À faire :
1. Export : le rappel d'une tâche (propriété de rappel déjà lue par le lot V / `crates/items`) devient un `VALARM` (`TRIGGER` absolu ou relatif à `DUE`, `ACTION:DISPLAY`) dans l'`VTODO`.
2. Import : un `VALARM` d'un `VTODO` reçu du téléphone met à jour le rappel de la tâche ; un VALARM absent n'efface pas un rappel (sauf si le fichier porte explicitement l'absence et que l'élément a été modifié côté téléphone : dans le doute, ne rien effacer et le signaler).
3. Tâches périodiques : rappel de l'occurrence suivante cohérent avec `trecur.rs`.
4. Tests : aller-retour avec rappel absolu, relatif, sans échéance, sans rappel.
5. `cargo test -p thunderlook-items -p thunderlook-exchange`.

Aucune écriture dans Outlook. Fin : push, résumé de 10 lignes.
