# Lot O — sécurité des données : annulation (Ctrl+Z) et sauvegardes automatiques

Branche : `cloud/lot-o-annuler-sauvegardes`. Lis d'abord `notes/cloud/README.md`.

Contexte : ThunderLook est maintenant le SEUL dépositaire des rendez-vous, contacts, tâches et notes de Paul (plus d'Outlook). Rien ne permet d'annuler une suppression ni de restaurer la base. C'est le risque n°1.

Périmètre de fichiers : nouveau crate `crates/safety/` (+ `[workspace] members`), nouveau fichier Tauri `app/src-tauri/src/safety_cmds.rs`, nouveau fichier d'interface `app/ui/src/safety.rs`, et UNIQUEMENT des lignes de branchement dans `lib.rs`, `ipc.rs`, `state.rs`, `app.rs`, `commandbar.rs`. N'appelle jamais de code qui supprime ou réécrit la base existante sans copie préalable.

À faire :
1. Corbeille interne : avant toute suppression d'un rendez-vous, contact, tâche ou note, enregistrer son contenu complet (ligne `message` + pièces + `.eml`) dans `data_dir/trash/<horodatage>-<type>-<id>.json` (fonction `thunderlook_safety::snapshot(conn, data_dir, message_id)`), et `restore(data_dir, file) -> message_id`. Les fonctions `delete` des crates `calendar`, `contacts`, `items` appellent `snapshot` (ajoute l'appel dans ces trois fonctions seulement, sans changer leurs signatures). Purge au bout de 60 jours.
2. Annuler : pile d'annulation (50 niveaux) des suppressions et modifications de ces quatre types : Ctrl+Z (dans les modules Calendrier, Contacts, Tâches, Notes) et bouton « Annuler » du ruban ; message de barre d'état « Annulé : … ». Une modification se défait en restaurant l'ancien contenu (instantané avant écriture).
3. Sauvegardes automatiques : `thunderlook_safety::backup(data_dir, keep)` exporte les contenus modifiables (rendez-vous, contacts, tâches, notes) en un seul fichier JSON compressé-ou-non (au choix, sans dépendance nouvelle : JSON texte) dans `data_dir/backups/AAAA-MM-JJ.json` une fois par jour au lancement, garde les 14 dernières, plus une copie hebdomadaire gardée 8 semaines. Ne copie JAMAIS la base entière (4 Go).
4. Restauration : `restore_backup(conn, data_dir, file, mode)` (mode « fusionner les éléments manquants » ou « tout remettre ») et un écran « Sauvegardes » (menu Fichier) pour lister, restaurer, ouvrir le dossier.
5. Tests sous Linux sur une base fabriquée : supprimer puis annuler redonne le même formulaire ; sauvegarde puis restauration fusionnante ne duplique rien ; purge.

Points INCERTAINS : taille du JSON de sauvegarde sur ~10 000 contacts — mesurer et dire.

Fin : push, résumé de 10 lignes (fait, reste, gestes à essayer : supprimer un contact puis Ctrl+Z, etc.).
