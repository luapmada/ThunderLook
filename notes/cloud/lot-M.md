# Lot M — Options (Fichier › Options) : réglages d'affichage, de calendrier et de rappels

Branche : `cloud/lot-m-options`. Lis d'abord `notes/cloud/README.md`.

Contexte : « Réglages de l'affichage » est grisé dans la barre d'état ; il n'y a pas d'écran d'options. `crates/model/src/settings.rs` porte les réglages (`Settings`, enregistrés en JSON).

Périmètre de fichiers : `crates/model/src/settings.rs`, nouveau fichier d'interface `app/ui/src/options.rs`, nouveau fichier Tauri `app/src-tauri/src/options_cmds.rs`, et UNIQUEMENT les lignes de branchement dans `app/ui/src/{main,app,state,statusbar,ipc}.rs`, `app/src-tauri/src/lib.rs`. Pour appliquer les réglages au calendrier : lire `Settings` dans `app/ui/src/calendar.rs` en NE MODIFIANT que les constantes/valeurs concernées (début de semaine, heures de travail, pas de 15/30/60 min).

À faire :
1. Boîte « Options » (menu Fichier, et le bouton « Réglages de l'affichage » de la barre d'état) à onglets : Général (thème clair/sombre/système, zoom), Calendrier (premier jour de la semaine, jours ouvrés, heures de travail, durée par défaut d'un rendez-vous, rappel par défaut, afficher les numéros de semaine), Courrier (marquer comme lu après N secondes, volet de lecture), Tâches/Notes (couleur de note par défaut, rappel de tâche par défaut), Rappels (notifications Windows oui/non, son).
2. Nouveaux champs de `Settings` avec `#[serde(default)]` et valeurs par défaut identiques au comportement actuel (aucune régression : un `settings.json` existant reste valide) ; test de désérialisation d'un ancien fichier.
3. Appliquer : durée et rappel par défaut dans `appointment_form` (nouveau rendez-vous) via les réglages (lecture seule dans `shared_calendar.rs` : ajoute un paramètre ou lis le fichier, sans changer les signatures publiques existantes) ; les autres réglages dans les vues quand c'est simple, sinon le dire.
4. Tests sous Linux sur `Settings` ; `cargo check -p thunderlook-ui --target wasm32-unknown-unknown`.

Fin : push, résumé de 10 lignes (réglages réellement appliqués, ceux seulement enregistrés, gestes à essayer).
