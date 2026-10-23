# Lot H — recherche globale et rappels dans l'interface

Branche : `cloud/lot-h-recherche-rappels-ui`. Lis d'abord `notes/cloud/README.md`.

Contexte : les crates `thunderlook-search` (`query`) et `thunderlook-reminders` (`due`, `dismiss`, `snooze`) et leurs commandes Tauri existent (voir `docs/architecture.md`, `app/src-tauri/src/reminders_cmds.rs`) mais ne sont pas branchés à l'interface.

Périmètre de fichiers : nouveaux fichiers `app/ui/src/global_search.rs` et `app/ui/src/reminders.rs`, nouveau fichier Tauri `app/src-tauri/src/search_cmds.rs`, et UNIQUEMENT les lignes de branchement dans `app/ui/src/app.rs`, `app/ui/src/state.rs`, `app/ui/src/statusbar.rs`, `app/ui/src/topbar.rs`, `app/ui/src/main.rs`, `app/ui/src/ipc.rs`, `app/src-tauri/src/lib.rs` (déclaration de module et enregistrement des commandes). Rien d'autre.

À faire :
1. Recherche : la barre « Rechercher (Ctrl+E) » en haut cherche dans le module affiché ; ajouter un menu de portée « Tout (courrier, rendez-vous, contacts, tâches, notes) » qui appelle `thunderlook_search::query` via `search_cmds.rs` et affiche une liste groupée par type ; un clic ouvre l'élément (bascule de module + sélection : rendez-vous → jour concerné ; contact → fiche ; tâche → sélection ; note → carte).
2. Rappels : compteur « Rappels : N » dans la barre d'état (comme Outlook), clic = panneau listant les rappels dus avec « Ouvrir », « Reporter » (5 min, 1 h, demain) et « Ignorer » ; écoute de l'événement `reminders-due` ; les rappels ignorés disparaissent.
3. Ne change aucune signature des crates `search` et `reminders`. S'il manque quelque chose dans ces crates, l'écrire dans le résumé au lieu de le modifier.

Vérification : `cargo check -p thunderlook-ui --target wasm32-unknown-unknown` ; dire ce qui n'a pas pu être compilé. Dans le résumé, les scénarios à essayer à l'écran.
