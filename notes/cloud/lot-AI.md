# Lot AI — importer depuis les fichiers de données de MPE (lecture seule)

Branche : `cloud/lot-ai-import-mpe`. Lis d'abord `notes/cloud/README.md`, `docs/mpe-fichiers.md`, `notes/mpe-plan.md` § 1.1 et § 1.5, `crates/exchange/src/vcal10.rs` (lot AE, lecteur vCalendar 1.0 déjà fusionné) et l'écran `app/ui/src/exchange.rs` (lot AF).

Contexte constaté : MPE garde pour chaque appareil, dans `%APPDATA%\MyPhoneExplorer\<appareil>\`, `Phonebook.dat` (vCard 2.1, `X-IRMC-LUID`, `OUTLOOK-UID`), `Organizer.dat` (vCalendar 1.0, 375 VEVENT sur le S20, `X-IRMC-LUID`, `X-OUTLOOK-UID`) et `Notes.dat` (VNOTE). Ces fichiers reflètent le téléphone à la dernière synchro. ThunderLook ne lit pas encore ces fichiers.

Périmètre de fichiers : NOUVEAU `crates/exchange/src/mpe_files.rs` (+ ligne `mod` dans `crates/exchange/src/lib.rs`), NOUVEAU `crates/exchange/tests/mpe_files.rs`, NOUVEAU `app/src-tauri/src/mpe_import_cmds.rs` (une ligne `mod`, enregistrement dans `invoke_handler`), NOUVEAU `app/ui/src/mpe_import.rs` + branchement minimal dans `app/ui/src/exchange.rs` (un bouton). Rien d'autre.

À faire :
1. Lire en LECTURE SEULE (ouverture sans écriture, jamais de verrou, copie en mémoire ou dans un dossier temporaire avant analyse) les trois fichiers d'un dossier d'appareil choisi par l'utilisateur ; proposer par défaut le dossier `%APPDATA%\MyPhoneExplorer\` et lister les appareils trouvés. Ne JAMAIS écrire, renommer ni supprimer quoi que ce soit dans ce dossier ; ne jamais lancer, fermer ni piloter MPE.
2. Passer les éléments au même circuit de comparaison que l'échange par fichiers (nouveaux, modifiés, supprimés, conflits — lots AC et AF), avec correspondance par `X-IRMC-LUID` mémorisé (`exchange-map.json` ou équivalent) puis par nom + e-mail / objet + début, jamais d'application automatique, aperçu obligatoire, annulation possible.
3. Bouton « Importer depuis MPE » dans l'écran téléphone (lot AF), avec rappel que MPE reste la source et ne reçoit rien en retour : sens téléphone → ThunderLook seulement.
4. Notes : le téléphone est la source (lot AG). Calendrier : ne regarder que la plage où MPE synchronise (−90 / +90 jours) pour ne pas proposer de supprimer des rendez-vous hors plage.
5. Tests sur des fichiers SYNTHÉTIQUES au format décrit (aucune donnée réelle, ne jamais afficher le contenu de vraies notes ni fiches), dont : élément ajouté, modifié, supprimé sur le téléphone ; fichier absent ; fichier en cours d'écriture (tronqué) → refus propre.
6. `cargo test -p thunderlook-exchange` ; `cargo check -p thunderlook-ui --target wasm32-unknown-unknown`.

Fin : push, résumé de 10 lignes (dont ce que Paul doit vérifier à l'écran).
