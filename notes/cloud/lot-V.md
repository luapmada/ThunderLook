# Lot V — tâches périodiques

Branche : `cloud/lot-v-taches-periodiques`. Lis d'abord `notes/cloud/README.md`, `notes/handoff.md` et `crates/items/`.

Périmètre de fichiers : `crates/items/src/` (+ tests), `app/ui/src/tasks.rs`, NOUVEAU `app/src-tauri/src/tasks_recur_cmds.rs` (une ligne `mod`, enregistrements dans `invoke_handler`). N'importe pas depuis `crates/calendar` au-delà de son API publique ; ne le modifie pas (le lot U l'édite).

À faire :
1. Tâche périodique : périodicité (jour/semaine/mois/an, tous les N, fin jamais/N/date) ET « recommencer N jours après la fin » (comme Outlook), stockée dans les propriétés de tâche d'Outlook (PidLidTaskRecurrence, PidLidTaskFRecurring) pour rester lisible par MPE et l'export .ics/.vnt.
2. Terminer une occurrence crée la suivante ; supprimer une occurrence ; la liste affiche la prochaine échéance.
3. Rappels des tâches périodiques : brancher sur `crates/reminders` par son API publique seulement (si une modification de ce crate est nécessaire, la décrire dans le résumé au lieu de la faire).
4. Tests : encodage/décodage, passage à l'occurrence suivante, fins de mois.
5. `cargo test -p thunderlook-items` ; `cargo check -p thunderlook-ui --target wasm32-unknown-unknown`.

Fin : push, résumé de 10 lignes (dont ce que Paul doit vérifier à l'écran).
