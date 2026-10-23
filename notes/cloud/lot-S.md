# Lot S — doublons de contacts : détecter, comparer, fusionner

Branche : `cloud/lot-s-doublons`. Lis d'abord `notes/cloud/README.md`.

Contexte : la base contient 2 140 contacts dans « Contacts » et un dossier « doublons2 » de 2 779 contacts : Paul a des doublons. Outlook proposait « Nettoyer » ; ThunderLook rien.

Périmètre de fichiers : `crates/contacts/src/dedupe.rs` (nouveau, + `pub mod dedupe;` dans `lib.rs`), nouveau fichier Tauri `app/src-tauri/src/dedupe_cmds.rs`, nouveau fichier d'interface `app/ui/src/dedupe.rs`, et UNIQUEMENT des lignes de branchement dans `lib.rs`, `main.rs`, `ipc.rs`, `state.rs`, `commandbar.rs`, `app.rs`.

À faire :
1. `find_duplicates(contacts: &[Contact]) -> Vec<Group>` : regroupe les contacts qui désignent probablement la même personne : même adresse électronique (normalisée), sinon même nom (insensible à la casse et aux accents, prénom et nom inversés) ET même mobile (chiffres seuls, +33 et 0 équivalents). Chaque groupe a une raison et un score. Pas de regroupement transitif abusif : une chaîne A~B, B~C sans A~C est signalée mais séparée.
2. `merge(group, keep_id, choices) -> ContactForm` : le contact conservé reçoit, champ par champ, la valeur choisie (par défaut la plus complète ; téléphones et courriels distincts ajoutés dans les emplacements libres ; notes concaténées avec une ligne de séparation). `apply_merge` écrit le formulaire (`write::update`) puis supprime les autres (`write::delete`) : si l'écriture échoue, rien n'est supprimé.
3. Interface : « Nettoyer les doublons » (ruban Contacts) : liste des groupes, fiches côte à côte, choix du contact conservé, aperçu de la fiche fusionnée, boutons « Fusionner » et « Ce ne sont pas des doublons » (mémorisé dans `dedupe-ignore.json`), « Fusionner les groupes sûrs (même courriel ET même nom) » avec récapitulatif avant confirmation.
4. Tests sous Linux : noms inversés, accents, numéros +33, courriel en majuscules, groupes non transitifs, fusion sans perte de données.

Fin : push, résumé de 10 lignes.
