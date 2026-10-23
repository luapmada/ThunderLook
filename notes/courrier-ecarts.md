# Courrier : écarts ThunderLook / Outlook (lot J, 23/10, d'après le code)

Déjà en place (lu dans le code) : tri et groupes Date/De/Objet/Taille/Catégories/Indicateur/Pièces jointes, conversations,
drapeaux (Aujourd'hui…Terminé), catégories colorées, répondu/transféré, importance, pièces jointes ouvrir + enregistrer sous,
images distantes bloquées + « Télécharger les images », dossiers de recherche (2 fixes + créés), règles, indésirables, signatures.

| # | Écart | État |
|---|---|---|
| 1 | Tri/groupe par **À** et par **Importance** (Outlook « Organiser par ») | FAIT |
| 2 | **Enregistrer toutes** les pièces jointes | FAIT (bouton si > 1 pièce jointe) |
| 3 | **Imprimer** le message | FAIT (`window.print()`, bouton « Imprimer ») |
| 4 | Tri **multi-critères** (clé secondaire choisie) : seul le 1er critère, puis date décroissante | FAIT côté cœur (lot K : `ListQuery.then_by` / `then_descending`, dossiers réels, test `sort_more`). RESTE : sélecteur « Puis par » (état `ui.sort` dans `state.rs`, `app.rs`, `dialogs.rs` : hors périmètre) ; ignoré en conversations et en recherche |
| 5 | Organiser par Type, Compte, Lu/non lu | FAIT (lot K : `SortField::{Type, Account, Read}`, entrées de menu et du dialogue via `SortField::ALL`) |
| 6 | **Aperçu** des pièces jointes (PDF, images, Office) dans le volet | RESTE (visionneuse par format ; non faite en cloud, invérifiable sans écran) |
| 7 | **Glisser** une pièce jointe vers l'Explorateur / le bureau | RESTE (Windows, Tauri) |
| 8 | Signature : choix par compte, nouveau/réponse, création/édition ; aujourd'hui seulement nom contenant « (adresse) » | RESTE (réglages de compte : hors périmètre) |
| 9 | Indicateur avec dates libres, rappel de suivi | Dates libres : FAIT côté cœur (lot K : `FlagAction::DueOn(ms)`, test dans `ops.rs`) ; RESTE : dialogue de choix de date (`dialogs.rs`) et rappel de suivi (module rappels) |
| 10 | Impression « mémo » (en-têtes, options) ; aperçu avant impression | RESTE (WebView2/Windows : aperçu et options d'impression non testables ici) |
| 11 | Mise en forme/regroupement par date « Hier/Cette semaine… » modifiable, groupes repliés mémorisés | Relu dans le code : libellés fixes (`format::date_group`, proches d'Outlook), non modifiables ; groupes repliés gardés à la réouverture du même dossier (`app.rs`), pas entre sessions. RESTE |
