# Lot J — courrier : ce que Outlook faisait et qui manque encore (inventaire puis réalisation)

Branche : `cloud/lot-j-courrier`. Lis d'abord `notes/cloud/README.md`, `notes/handoff.md` et `docs/architecture.md`.

Périmètre de fichiers : `crates/mailbox/`, `crates/rules/`, `crates/mime/`, `crates/model/src/lib.rs` (types du courrier), `app/ui/src/{list,reading,compose,folders}.rs`, nouveau fichier Tauri `app/src-tauri/src/mail_cmds.rs`. Ne touche pas à `crates/sync` (IMAP) ni aux comptes.

À faire :
1. Inventaire d'abord : écris `notes/courrier-ecarts.md` — liste COURTE (15 lignes maximum) de ce qu'Outlook fait au courrier et que ThunderLook ne fait pas encore, d'après le code et `notes/outlook-couleurs-menus.md` (drapeaux, indicateurs, catégories, signatures, pièces jointes enregistrer-sous/ouvrir/glisser, dossiers de recherche, impression, vue conversations, tri/groupement, prévisualisation HTML, images distantes…). Ne rien supposer : lire le code.
2. Réalise, dans l'ordre de l'inventaire et sans dépasser le périmètre, les écarts qui n'exigent ni IMAP ni Windows : par exemple groupement par date/expéditeur, tri multi-critères, aperçu des pièces jointes, enregistrer toutes les pièces jointes, impression via le navigateur de la page (`window.print()` de la vue de lecture), signature par compte.
3. Chaque écart réalisé a un test sous Linux quand c'est de la logique (tri, groupement, filtre) ; l'interface se vérifie par `cargo check -p thunderlook-ui --target wasm32-unknown-unknown`.
4. Dans le résumé : ce qui est fait, ce qui reste de l'inventaire, et ce que Paul doit essayer à l'écran.

Points INCERTAINS : tout comportement d'Outlook qu'on ne peut pas confirmer par le code ou les notes → `REA-LIMITES.md` (« À vérifier avec REA/Outlook »).
