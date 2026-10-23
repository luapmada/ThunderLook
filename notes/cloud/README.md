# Lots de travail pour le cloud

Chaque lot est indépendant (fichiers distincts) et se lance dans sa propre session :

    claude --cloud "Sur main, lis notes/cloud/lot-X.md et exécute-le."

Règles communes : branche `cloud/<nom du lot>` depuis `main`, jamais de push sur `main` ; rester dans le périmètre de fichiers du lot ;
réponses courtes ; « je ne sais pas » plutôt qu'une invention ; les points incertains vont dans `REA-LIMITES.md`
(section « À vérifier avec REA/Outlook ») ; aucune écriture dans Outlook ; à la fin : push, puis résumé de 10 lignes maximum
(fait, reste, ce que Paul doit vérifier à l'écran chez lui). Les tests s'exécutent sous Linux : `cargo test -p <crate>`.
Le code d'interface (`app/ui`) et Tauri (`app/src-tauri`) ne se teste pas en cloud : l'écrire proprement, vérifier
`cargo check -p thunderlook-ui --target wasm32-unknown-unknown` si la cible est installable, et dire ce qui n'a pas pu être compilé.
Nouvelles commandes Tauri : dans un NOUVEAU fichier de `app/src-tauri/src/`, avec une seule ligne `mod …;` et les enregistrements
ajoutés à la liste `invoke_handler` de `lib.rs` (Paul résout les conflits de fusion).
