# Lot AL — documentation utilisateur à jour (échange, import MPE, séries, notes)

Branche : `cloud/lot-al-doc`. Lis d'abord `notes/cloud/README.md`, `docs/utilisation.md`, `docs/mpe-fichiers.md`, `docs/jeu-de-test-s20.md`, `docs/migration-s23.md` et le code de `app/ui/src/{exchange,mpe_import}.rs`.

Périmètre de fichiers : `docs/utilisation.md` UNIQUEMENT.

À faire :
1. Mettre à jour la section « Échange avec le téléphone » : boîte `Ctrl+Maj+T` ou Fichier › Échange avec le téléphone…, export/import de dossier, « Importer depuis MPE » (lecture seule, sens téléphone → ThunderLook), aperçu, conflits (garder ThunderLook / le téléphone / ignorer), annulation du dernier import, séries importées et cas refusés, notes (le téléphone est la source), rappels de tâches.
2. Mettre à jour les sections Calendrier (séries, exceptions : modifier une occurrence ou toute la série), Tâches (périodiques), Sauvegardes (nouvelle sauvegarde cohérente, test de restauration), dossiers (noms lisibles du volet gauche).
3. Décrire uniquement ce qui existe dans le code (le lire) ; une ligne par fonction ; liens vers les autres documents. Ne pas toucher à l'aide intégrée (`app/ui/src/help.rs`).

Fin : push, résumé de 10 lignes.
