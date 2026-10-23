# Lot N — impression (calendrier, contacts, tâches, notes) et export PDF

Branche : `cloud/lot-n-impression`. Lis d'abord `notes/cloud/README.md`.

Périmètre de fichiers : nouveau fichier d'interface `app/ui/src/print.rs`, nouveau fichier de style `app/ui/styles/print.css` (à référencer depuis `app/ui/index.html` ou le fichier de styles existant par une seule ligne), et UNIQUEMENT des lignes de branchement dans `app/ui/src/{main,commandbar,calendar,contacts,tasks,notes}.rs`. Ne modifie pas la logique de ces modules.

À faire :
1. `@media print` : masquer ruban, volets, barre d'état ; n'afficher que la vue concernée, en noir sur blanc, sans coupure au milieu d'une ligne de rendez-vous ou d'une fiche.
2. Calendrier : « Imprimer » (ruban) pour les vues Jour / Semaine / Mois (mise en page paysage pour Semaine et Mois) ; liste des rendez-vous du jour sous la grille pour Jour. Contacts : fiche seule, ou liste du dossier (nom, société, téléphones, courriel) sur plusieurs pages. Tâches : liste filtrée avec échéances. Notes : une note par bloc.
3. Appel : `window.print()` ; pour « Enregistrer en PDF », laisser l'imprimante PDF de Windows.
4. Aucune dépendance nouvelle. `cargo check -p thunderlook-ui --target wasm32-unknown-unknown`.

Fin : push, résumé de 10 lignes (ce qui s'imprime, ce que Paul doit voir à l'aperçu avant impression).
