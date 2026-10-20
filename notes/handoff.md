# Passation — reprise du J6 (état au 2026-10-20)

À lire en premier dans une nouvelle session, avec `PLAN.md` §4-5 et `git log`.

## Où on en est
- Livré : J0 à J5 et J3b. La version courante est **v0.6.0** (étiquette git, commit `405e624`).
- Règle de version : une mineure par jalon, voir `notes/dev.md` § Versions. Le J6 clora en **v0.7.0**.
- J6 (règles et actions rapides) **en cours**. Fait le 2026-10-20 :
  1. `crates/rules/examples/dump.rs` : identique au prototype Python sur les 3 flux réels (60 règles, 198 textes et 611 nombres comparés, `target/tl-scripts/compare_rules.py`). `b5d4aea`
  2. Schéma **v7** : table `rule` (origine `pst:<fai>` ou chemin du `.rwz`, position, nom, active, version, JSON, octets bruts). Flux du PST importé à l'ouverture (une seule fois : la case « active » est gardée), `.rwz` par `Mailbox::import_rwz` (réimport = remplacement). `97e857d`
  3. Dossiers cibles : `FolderResolver` (EntryID du dossier dans `folder.props` 0x0FFF, sinon nom replié ; entre homonymes, le plus rempli, avec avertissement). Flux du PST : 7/7 par EntryID ; `.rwz` : tout par le nom. `79b4fb0`
  4. et 5. Moteur `Mailbox::run_rules` (`crates/mailbox/src/rule_run.rs`) : simulation ou application, évaluation parallèle, actions sans objet retirées. Critère du jalon tenu sur une copie. `234bd55`

## Mesures du J6 (à reporter dans `PLAN.md` à la clôture)
- Simulation, Boîte de réception (22 188 éléments), 12 règles actives du PST : 74 messages modifiés, 1,5 s à chaud (168 s avant parallélisation, à froid).
- Simulation, archive entière (217 257) : `regles240913.rwz` 132 264 changements, `regles.rwz` 130 892 ; environ 160-180 s à froid.
- Application sur une copie (`$CLAUDE_JOB_DIR/tmp/apply` : base copiée, `.eml` de la Boîte de réception copiés, pas de jonction) :
  - flux du PST : 74 modifiés (73 déplacés, 1 catégories retirées), puis rejeu : 0 ;
  - `regles240913.rwz` : 219 déplacés, rejeu : 0 ; `regles.rwz` : 0 (déjà couvert).
  - Contrôle : [electromagnetisme.fr] +35 (28 + 7), CEM +7, LBC | FB +5, Ebay +3, Hébergements WEB +2, Accusés +28.
- Règle écartée : « OXALIS » (carnet d'adresses : J9). Sous-chaînes fidèles à Outlook : « one.com » attrape « aroma-zone.com ».

## Points ouverts du J6
- Homonymes (`.rwz` d'anciens PST) : « le plus rempli » n'est pas cohérent d'un dossier à l'autre (« listes Oxalis » dans `La Ruche - Oxalis1`, « Comox » dans une autre copie de l'arborescence). Prévoir le choix du dossier dans l'interface, ou préférer les candidats d'une même branche.
- Ouvrir `data/store` avec le code actuel le migre au schéma 7 : l'application v0.6.0 installée refusera alors la base. Reconstruire l'application avant (ou travailler sur copie : `target/tl-scripts/mini_store.py [--props]`).
- La phrase « l'assigner à la ADE spécifiée » est le libellé d'OUTLOOK.EXE tel quel (`%s` = catégorie).

## Données de test du J6 (non versionnées)
- `data/samples/regles240913.rwz` (30 règles, version 0x140000) et `data/samples/regles.rwz` (17 règles, version 0x124F80).
- Flux du PST : contenu associé `IPM.RuleOrganizer` de la Boîte de réception (fai id 19), propriété `0x6802`.
  - Extrait par `target/tl-scripts/fai_rules.py` dans `data/rea/pst-rules-19.bin` (13 règles).
- Spec communautaire en local : `data/specs/outlook_rule_spec.txt`.
- Résultats REA : `notes/rea/regles-outlook.md`. Catalogue des 104 éléments avec libellés : `notes/rea/rule_catalog.md`.
- Outil : `cargo run --release -p thunderlook-mailbox --example rules -- <dossier> [x.rwz…] [--run f<id> [--subfolders] [--apply] [--only <origine>]]`.

## Reste à faire pour le J6, dans l'ordre
6. Boîte « Règles et alertes » : liste, case active, description, Exécuter maintenant (rapport de simulation, puis appliquer), Importer `.rwz`.
7. Actions rapides : défauts et éditeur.
8. Clôture :
   - mesures dans `PLAN.md`, cas J6 dans `tools/ui_check.mjs` ;
   - version 0.7.0 dans tous les `Cargo.toml` et dans `tauri.conf.json` ;
   - étiquette `v0.7.0`, commit et push.

## Méthode et pièges
- Vérification d'interface : `cargo tauri build --no-bundle` (faire `Remove-Item Env:NO_COLOR` avant), puis `node tools/ui_check.mjs`.
  - Le script pilote l'application par CDP, ne modifie pas la base et nettoie derrière lui.
  - Ne **pas** piloter à la souris système : des clics sont déjà partis hors de la fenêtre.
- Paul ne veut pas tester à notre place : faire les vérifications soi-même.
- Heredocs bash contenant des apostrophes : ils échouent dans l'outil. Écrire les scripts dans `target/tl-scripts/` (non versionné) et les lancer avec `python -I`.
- REA : la session MCP est perdue à chaque reconnexion du serveur, et la réanalyse d'`OUTLOOK.EXE` prend environ 20 min.
  - Le délai `REA_GHIDRA_STARTUP_TIMEOUT_MS=14400000` est déjà dans `~/.claude.json`.
- Les `.eml` importés avant le schéma v6 gardent le marqueur de préfixe du PST dans `Subject`. Il faudra le retirer à l'export.
- Ne jamais lire les PST sous `A:\Fichiers Outlook` sur place : travailler sur des copies.
