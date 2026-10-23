# Lot T — aide en ligne et guide d'utilisation

Branche : `cloud/lot-t-aide`. Lis d'abord `notes/cloud/README.md`.

Périmètre de fichiers : `docs/utilisation.md` (nouveau), nouveau fichier d'interface `app/ui/src/help.rs`, et UNIQUEMENT des lignes de branchement dans `app/ui/src/{main,app,commandbar,state}.rs` (onglet « Aide » du ruban, touche F1). Aucun fichier d'un autre module.

À faire :
1. `docs/utilisation.md` en français, court et concret, par module : Courrier, Calendrier, Contacts, Tâches, Notes ; raccourcis clavier (relevés dans le code : `app/ui/src/app.rs`, `commandbar.rs`, `calendar.rs`) ; échange avec le téléphone ; sauvegardes et annulation ; où sont les données. Décrire seulement ce qui existe (lire le code et `notes/handoff.md`).
2. Interface : l'onglet « Aide » et F1 ouvrent une page d'aide intégrée (le contenu de `docs/utilisation.md` converti en HTML simple à la compilation avec `include_str!` et un convertisseur Markdown MINIMAL écrit à la main : titres, listes, tableaux simples, gras ; pas de nouvelle dépendance), avec table des matières et champ de recherche dans la page.
3. Page « À propos » : version (celle du `Cargo.toml`) et dossier de données ; taille de la base et nombre d'éléments par module seulement si une commande Tauri existante le permet, sinon omettre et le dire.
4. `cargo check -p thunderlook-ui --target wasm32-unknown-unknown`.

Fin : push, résumé de 10 lignes.
