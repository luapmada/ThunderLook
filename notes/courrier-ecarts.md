# Courrier : écarts ThunderLook / Outlook (lot J, 23/10, d'après le code)

Déjà en place (lu dans le code) : tri et groupes Date/De/Objet/Taille/Catégories/Indicateur/Pièces jointes, conversations,
drapeaux (Aujourd'hui…Terminé), catégories colorées, répondu/transféré, importance, pièces jointes ouvrir + enregistrer sous,
images distantes bloquées + « Télécharger les images », dossiers de recherche (2 fixes + créés), règles, indésirables, signatures.

| # | Écart | État |
|---|---|---|
| 1 | Tri/groupe par **À** et par **Importance** (Outlook « Organiser par ») | FAIT |
| 2 | **Enregistrer toutes** les pièces jointes | FAIT (bouton si > 1 pièce jointe) |
| 3 | **Imprimer** le message | FAIT (`window.print()`, bouton « Imprimer ») |
| 4 | Tri **multi-critères** (clé secondaire choisie) : seul le 1er critère, puis date décroissante | RESTE (touche `ListQuery`, `app.rs`, `lib.rs` Tauri : hors périmètre) |
| 5 | Organiser par Type, Compte, Lu/non lu | RESTE |
| 6 | **Aperçu** des pièces jointes (PDF, images, Office) dans le volet | RESTE (visionneuse par format) |
| 7 | **Glisser** une pièce jointe vers l'Explorateur / le bureau | RESTE (Windows, Tauri) |
| 8 | Signature : choix par compte, nouveau/réponse, création/édition ; aujourd'hui seulement nom contenant « (adresse) » | RESTE (réglages de compte : hors périmètre) |
| 9 | Indicateur avec dates libres, rappel de suivi | RESTE |
| 10 | Impression « mémo » (en-têtes, options) ; aperçu avant impression | RESTE |
| 11 | Mise en forme/regroupement par date « Hier/Cette semaine… » modifiable, groupes repliés mémorisés | À vérifier |
