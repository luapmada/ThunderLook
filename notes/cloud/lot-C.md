# Lot C — tâches et notes complètes : périodicité, dossiers, filtres, couleur

Branche : `cloud/lot-c-taches-notes`. Lis d'abord `notes/cloud/README.md`.

Contexte : tâches et notes se lisent et s'écrivent dans la base (`crates/items/src/lib.rs`, `app/ui/src/tasks.rs`, `app/ui/src/notes.rs`) ; vTodo/vNote s'échangent (`vtodo.rs`, `vnote.rs`). Manquent : tâches périodiques, vues par dossier et filtres, rappels sur les tâches, couleur de note fiable.

Périmètre de fichiers : `crates/items/` (tout), `crates/model/src/items.rs`, `app/ui/src/tasks.rs`, `app/ui/src/notes.rs`, nouveau fichier Tauri `app/src-tauri/src/items_cmds.rs`. Rien d'autre.

À faire (dans cet ordre, chaque étape avec ses tests sous Linux) :
1. Vues et filtres des tâches : `TaskList` expose déjà les dossiers ; ajouter dans l'interface un sélecteur « Toutes / Aujourd'hui / En retard / Cette semaine / Terminées / par dossier » et un tri (échéance, importance, titre). Fonctions pures de filtrage dans `crates/items` (testées), l'interface les appelle via une commande.
2. Rappels de tâche : `TaskForm.reminder: Option<String>` (`AAAA-MM-JJTHH:MM`) → `PidLidReminderSet` (0x8503, PSETID_COMMON) et `PidLidReminderTime` (0x8502) ; lecture/écriture ; champ dans la boîte « Tâche ».
3. Tâches périodiques : `TaskForm.repeat*` comme `AppointmentForm` (jour/semaine/mois/an, fin jamais/N/date). Écrire `PidLidTaskRecurrence` (0x8116) selon [MS-OXOTASK] (RecurrencePattern identique à celui de `crates/calendar/src/recur.rs`, sans `AppointmentRecurrencePattern`), `PidLidTaskFRecurring` (0x8126). À la fin d'une tâche périodique : la marquer terminée ET créer l'occurrence suivante. Si le format est trop incertain : tout consigner en `REA-LIMITES.md` et ne faire que la lecture.
4. Couleur de note : retrouver dans les notes réelles de la base quelle propriété nommée porte la couleur ([MS-OXONOTE] `PidLidNoteColor` 0x8B00, jeu de propriétés `0006200E-0000-0000-C000-000000000046`) — le code actuel suppose cette propriété ; ajouter un test et, si la propriété manque dans une source, une valeur de repli explicite.
5. Notes : recherche plein texte dans la liste, tri (récentes, alphabétique), et choix du dossier à la création quand il y en a plusieurs.

Points INCERTAINS à consigner sans deviner : format de `PidLidTaskRecurrence`, sens de `PidLidTaskDateCompleted` pour une tâche périodique.
