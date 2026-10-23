# Lot AG — notes : le téléphone est la source (mode MPE « forcé téléphone → PC »)

Branche : `cloud/lot-ag-notes-telephone`. Lis d'abord `notes/cloud/README.md`, `notes/mpe-plan.md` § 1.2 (notes en mode `NoteSyncMode=2` = forcé téléphone → PC ; 46 notes, `X-OUTLOOK-UID`) et `crates/items/src/vnote.rs`.

Périmètre de fichiers : `crates/items/src/vnote.rs` (+ tests de `crates/items`), `crates/exchange/src/import.rs` UNIQUEMENT dans la branche des notes (le lot AE édite la branche calendrier : n'y touche pas), `crates/exchange/tests/notes.rs` (nouveau). Rien d'autre.

À faire :
1. Import des notes du téléphone traité comme prioritaire : une note modifiée sur le téléphone remplace celle de ThunderLook SANS la proposer comme conflit (mode forcé), en gardant la couleur de la note existante ; une note supprimée sur le téléphone est proposée à la suppression (décochée) ; une note nouvelle est créée.
2. Réglage « les notes viennent du téléphone » activé par défaut pour les notes seulement (les autres modules gardent le comportement bidirectionnel) ; désactivable.
3. Corps long, accents, lignes vides, retours chariot, `CHARSET`/QP, dates `DCREATED`/`LAST-MODIFIED` conservées.
4. Scénarios de test : création/modification/suppression côté téléphone ; modification côté ThunderLook écrasée au prochain import (mode forcé) avec la valeur ancienne gardée dans la corbeille/annulation de l'échange (`exchange-undo`).
5. `cargo test -p thunderlook-items -p thunderlook-exchange`.

Aucune écriture dans Outlook. Fin : push, résumé de 10 lignes.
