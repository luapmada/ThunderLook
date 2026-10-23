# Lot P — robustesse de la base : verrous, sauvegarde légère et incrémentale

Branche : `cloud/lot-p-robustesse`. Lis d'abord `notes/cloud/README.md`.

Problème constaté le 23/10 : la sauvegarde du jour (lot O) écrivait 123 Mo de JSON deux fois (quotidienne + hebdomadaire), lisait toute la base au lancement et provoquait « database is locked » (l'arborescence des dossiers ne se chargeait pas). Un correctif provisoire diffère la sauvegarde de 240 s (`app/src-tauri/src/safety_cmds.rs`, `start`).

Périmètre de fichiers : `crates/safety/`, `crates/store/src/lib.rs` (uniquement l'ouverture : `busy_timeout`, `journal_mode` ; migrations inchangées), `app/src-tauri/src/safety_cmds.rs`. Rien d'autre.

À faire :
1. `thunderlook_store::open` : `PRAGMA busy_timeout = 8000` ; vérifier `journal_mode=WAL` et `synchronous=NORMAL` ; une fonction `open_readonly(dir)` (lecture seule, `busy_timeout`) pour les lectures longues. Test : une connexion écrit pendant que l'autre lit longtemps → pas d'erreur de verrou.
2. Sauvegarde : lire avec `open_readonly`, par paquets (transactions courtes), SANS les dossiers corbeille (« Éléments supprimés », « Deleted Items », « Sent »), sans les rapports ; contenus : rendez-vous, contacts, tâches, notes. Format JSON Lines (un élément par ligne), écrit en flux ; compressé en gzip seulement si `flate2` est déjà dans le dépôt.
3. Incrémental : `data_dir/backups/index.json` retient, par élément, `modified_at` et une empreinte ; la sauvegarde du jour ne contient que les éléments nouveaux ou modifiés depuis la précédente, plus un « plein » le dimanche. `restore_backup` reconstitue l'état à une date en rejouant plein + incréments. Tests sous Linux sur une base fabriquée (création, modification, suppression entre deux sauvegardes).
4. Priorité basse : la sauvegarde s'exécute dans un fil, pause de 20 ms toutes les 500 lignes, et attend si un autre fil écrit.
5. Mesure : taille avant/après sur la base fabriquée (10 000 contacts, 4 200 rendez-vous, 400 notes) dans `notes/perf.md`.

Fin : push, résumé de 10 lignes.
