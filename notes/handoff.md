# Passation — reprise du J6 (état au 2026-10-20)

À lire en premier dans une nouvelle session, avec `PLAN.md` §4-5 et `git log`.

## Où on en est
- Livré : J0 à J5 et J3b. La version courante est **v0.6.0** (étiquette git, commit `405e624`).
- Règle de version : une mineure par jalon, voir `notes/dev.md` § Versions. Le J6 clora en **v0.7.0**.
- J6 (règles et actions rapides) **en cours** : commit `55e0ac5`.
  - Crate `crates/rules` (`thunderlook-rules`) : décodeur du flux de règles (`parse`), types `Rule` / `Element` / `Value`, description en français (`describe`).
  - Catalogue généré par `tools/gen_rule_catalog.py`. 3 tests passent.
- ADI Facturation : **hors sujet pour l'instant**, à la demande de Paul (« concentre-toi sur Outlook »). Ne pas relancer.

## Données de test du J6 (non versionnées)
- `data/samples/regles240913.rwz` (30 règles, version 0x140000) et `data/samples/regles.rwz` (17 règles, version 0x124F80).
- Flux du PST : contenu associé `IPM.RuleOrganizer` de la Boîte de réception (fai id 19), propriété `0x6802`.
  - Extrait par `target/tl-scripts/fai_rules.py` dans `data/rea/pst-rules-19.bin` (13 règles).
- Le prototype Python `target/tl-scripts/rwz_proto.py` décode **60 règles sur 60**. Il sert de référence pour vérifier la crate Rust.
- Spec communautaire en local : `data/specs/outlook_rule_spec.txt`.
- Résultats REA : `notes/rea/regles-outlook.md`. Catalogue des 104 éléments avec libellés : `notes/rea/rule_catalog.md`.

## Reste à faire pour le J6, dans l'ordre
1. **Vérifier la crate sur les 3 vrais flux** : un exemple `crates/rules/examples/dump.rs`, à comparer à la sortie du prototype. Seul le prototype a été confronté aux vrais fichiers.
2. **Import en base** (schéma v7, ajout seulement) :
   - table `rule` : source, origine (`pst:<fai>` ou chemin du `.rwz`), position, nom, active, version, JSON, octets bruts ;
   - import du flux du PST à l'ouverture et des `.rwz` à la demande.
3. **Dossiers cibles** : EntryID de dossier de 24 octets (dont le NID en fin) et EntryID de magasin de 126 octets (il contient le chemin du PST).
   - Relier au `folder.nid` de la bonne source ; à défaut, par le nom du dossier.
4. **Moteur « Exécuter maintenant »** :
   - simulation (rapport : règle → messages concernés → actions), puis application ;
   - conditions courantes : de, à, mots de l'objet, du corps et des en-têtes, compte, catégorie, pièce jointe, taille, date, importance ; « sur cet ordinateur » toujours vrai ;
   - actions : déplacer, copier, supprimer, catégories, importance, lu, indicateur, arrêter ;
   - les envois (transférer, répondre…) sont signalés « non exécutés avant J7 ».
5. **Critère du jalon** : les 2 `.rwz` et le flux du PST importés puis rejoués **sur une copie**.
   - Simulation sur `data/store`, application sur une copie de `data/store-small` ou sur des données synthétiques.
   - Ne jamais appliquer sur `data/store` lui-même.
6. Boîte « Règles et alertes » : liste, case active, description, Exécuter maintenant, Importer `.rwz`.
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
