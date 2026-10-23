# Lot G — calendrier : glisser-déposer, redimensionner, créer à la souris, chevauchements

Branche : `cloud/lot-g-calendrier-souris`. Lis d'abord `notes/cloud/README.md`.

Périmètre de fichiers : `app/ui/src/calendar.rs`, `app/ui/styles/app.css` (section calendrier seulement : `.cal-*`), nouveau fichier Tauri `app/src-tauri/src/calendar_cmds.rs`, `crates/calendar/src/write.rs` UNIQUEMENT pour AJOUTER une fonction `move_appointment(conn, message_id, start_local, end_local)` (ne modifie rien d'existant). Rien d'autre.

À faire :
1. Créer à la souris : en vue Jour / Semaine, tirer dans une colonne vide sélectionne un créneau (arrondi à 15 min) et ouvre la boîte « Rendez-vous » préremplie (début/fin) ; en vue Mois, double clic sur un jour = nouveau rendez-vous ce jour-là.
2. Déplacer : glisser un rendez-vous (non périodique) change son début/fin (même durée, arrondi 15 min), entre jours et heures ; Échap annule ; rendu fantôme pendant le glissement. Pour un rendez-vous périodique : glisser déplace UNE occurrence (`write::update_occurrence`).
3. Redimensionner : poignée en bas d'un rendez-vous (vues Jour/Semaine) change la fin.
4. Chevauchements : vérifier que des rendez-vous simultanés se placent côte à côte sans se recouvrir (colonnes `column`/`columns` de `TimedEvent`), y compris à l'impression de 5 rendez-vous au même moment ; en vue Mois, `+ N autres` ouvre la vue Jour. Corriger tout recouvrement visuel par CSS/hauteurs (une classe `.cat` existe déjà pour le courrier : ne jamais l'utiliser dans le calendrier).
5. Les commandes Tauri nouvelles vont dans `calendar_cmds.rs` ; les écritures passent par `thunderlook_calendar::write`.

Vérification : `cargo test -p thunderlook-calendar` (test de `move_appointment`), `cargo check -p thunderlook-ui --target wasm32-unknown-unknown`. Dans le résumé, liste les 6 gestes que Paul doit essayer à l'écran.
