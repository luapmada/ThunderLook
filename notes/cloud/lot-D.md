# Lot D — rappels : moteur de calcul (rendez-vous et tâches)

Branche : `cloud/lot-d-rappels`. Lis d'abord `notes/cloud/README.md`.

Contexte : les rendez-vous ont un rappel (`Appointment.reminder_minutes`), les séries aussi (par occurrence), mais rien ne déclenche de notification. Outlook affichait « Rappels : 171 » dans sa barre d'état. ThunderLook doit afficher une liste de rappels à échéance et pouvoir les reporter ou les ignorer.

Périmètre de fichiers : NOUVEAU crate `crates/reminders/` (+ ajout dans `[workspace] members` du `Cargo.toml` racine), et nouveau fichier Tauri `app/src-tauri/src/reminders_cmds.rs`. Rien d'autre (pas d'interface dans ce lot).

À faire :
1. `thunderlook_reminders::due(conn, data_dir, now_ms, window: Window) -> Vec<Reminder>` : rappels dont l'heure est dans `[now - lookback, now + lookahead]` pour : occurrences de rendez-vous (via `thunderlook_calendar::expand::occurrences`, heure = début − `reminder_minutes`, occurrences supprimées exclues) ; tâches non terminées avec rappel (`PidLidReminderTime` 0x8502 / échéance à 8 h si pas d'heure). `Reminder { id (stable : `a<id>@<jour>` ou `t<id>`), kind, title, when_ms, event_start_ms, location }`.
2. État des rappels ignorés ou reportés : fichier JSON `reminders.json` dans le dossier de données (`dismissed: {id: until_ms}`) ; fonctions `dismiss(dir, id)` et `snooze(dir, id, until_ms)` ; `due` les respecte. Écriture atomique (fichier temporaire puis renommage).
3. Tauri (nouveau fichier) : commandes `reminders_due`, `reminder_dismiss`, `reminder_snooze` ; un thread qui, chaque minute, émet l'événement `reminders-due` avec la liste (sans doublon déjà notifié) — s'inspirer de `app/src-tauri/src/toast.rs` pour la notification Windows ; ne pas modifier `toast.rs`, l'appeler.
4. Tests sous Linux : un rendez-vous dans 10 min avec rappel 15 min est dû ; une série hebdomadaire a un rappel par occurrence ; une occurrence supprimée n'en a pas ; une tâche terminée n'en a pas ; report puis ignorer ; heure d'été (changement d'heure) sans décalage.

Points INCERTAINS à consigner sans deviner : traitement d'un rappel manqué plusieurs jours de suite (ThunderLook fermé) — proposer un plafond et le consigner.
