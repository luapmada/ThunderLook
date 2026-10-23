# Lot I — échange avec le téléphone : dossier d'échange et détection des changements

Branche : `cloud/lot-i-echange`. Lis d'abord `notes/cloud/README.md` et `notes/mpe-plan.md` (option B : MPE lit et écrit des FICHIERS ; ThunderLook ne parle jamais à MPE directement).

Périmètre de fichiers : NOUVEAU crate `crates/exchange/` (+ `[workspace] members`), nouveau fichier Tauri `app/src-tauri/src/exchange_cmds.rs`, nouveau fichier d'interface `app/ui/src/exchange.rs`, et UNIQUEMENT les lignes de branchement dans `lib.rs`, `main.rs`, `ipc.rs`, `state.rs`, `app.rs`. Ne touche ni MPE, ni ses réglages, ni aucun fichier de `C:\` ou `A:\` hors du dossier de données et du dossier d'échange choisi.

À faire :
1. `thunderlook_exchange::export_all(conn, data_dir, out_dir) -> Report` : écrit dans le dossier d'échange `contacts.vcf` (vCard 3.0), `taches.ics` (VTODO), `notes.vnt` — via les convertisseurs existants (`vcard::export`, `vtodo::export`, `vnote::export`) — et `calendrier.ics` via `thunderlook_calendar::ics::export`. Écriture ATOMIQUE (fichier temporaire puis renommage) ; un `manifest.json` (horodatage, nombre d'éléments, empreinte SHA-256 de chaque fichier — écrire le SHA-256 à la main ou avec le crate `sha2` si nécessaire).
2. `import_changes(conn, data_dir, in_dir) -> ImportReport` : relit les fichiers du dossier, compare à la base par identifiant (`UID` / `X-OUTLOOK-UID`, sinon nom + courriel / objet + échéance), et ne propose QUE ce qui a changé : nouveaux, modifiés, supprimés. Renvoie une liste de changements `Change { kind, id, summary }` SANS rien écrire ; `apply(changes)` les applique après accord. Jamais de suppression automatique : les suppressions sont proposées, pas appliquées par défaut.
3. Sauvegarde : avant tout `apply`, copie de la base impossible (4 Go) → à la place, écrire dans `data_dir/exchange-undo/<horodatage>.json` les éléments qui vont être modifiés ou supprimés (leur formulaire), et fournir `undo(timestamp)`.
4. Tauri + interface : un écran « Échange avec le téléphone » (menu Fichier) : choisir le dossier d'échange, boutons « Exporter maintenant », « Vérifier les changements » (liste avec cases à cocher), « Appliquer », « Annuler le dernier import » ; export automatique optionnel toutes les N minutes (désactivé par défaut).
5. Tests sous Linux : export → import sans modification = zéro changement ; un contact modifié dans le `.vcf` = un changement ; suppression proposée mais non appliquée ; `undo` rétablit.

Points INCERTAINS à consigner dans `notes/mpe-plan.md` (section « À valider sur le S20 FE avec Paul ») : comment MPE nomme et encode ses exports (vCard 2.1 ou 3.0 ? UID ?), gestion des suppressions côté téléphone.
