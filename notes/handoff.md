# Passation — reprise du J7 (état au 2026-10-20)

À lire en premier dans une nouvelle session, avec `PLAN.md` §4-5 et `git log`.

## Où on en est
- Livré : J0 à J6 et J3b. La version courante est **v0.7.0** (étiquette git). Le J7 clora en **v0.8.0** (`notes/dev.md` § Versions).
- `data/store` est au schéma 7 (table `rule`, règles du flux du PST importées).
- ADI Facturation : hors sujet pour l'instant, à la demande de Paul. Ne pas relancer.

## Prochain jalon : J7, IMAP + SMTP (Hostinger)
- Comptes, synchronisation, envoi, boîte d'envoi, F9, hors connexion, notifications, réponse automatique locale.
- Critère : envoi et réception réels sur un compte de test (à demander à Paul).
- Branchements déjà prêts :
  - règles : les actions d'envoi (transférer, répondre, copie à…) sont signalées « pas avant le J7 » dans `crates/mailbox/src/rule_run.rs` ; les règles devront aussi s'appliquer à l'arrivée des messages ;
  - actions rapides : « Au responsable », « E-mail de l'équipe », « Répondre et supprimer » ouvrent la rédaction, l'envoi reste à faire.

## Points ouverts hérités du J6
- `.rwz` d'anciens PST : entre homonymes, le dossier le plus rempli est retenu (pas forcément de la même branche). Sans importance pour Paul (listes Oxalis archivées).
- Libellés des actions rapides par défaut : d'après l'aide Microsoft, à corriger si l'Outlook 2021 de Paul diffère.
- Simulation sur toute l'archive : environ 3 min à froid, dominée par la lecture des `.eml`.

## Outils
- `cargo run --release -p thunderlook-mailbox --example rules -- <dossier> [x.rwz…] [--run f<id> [--subfolders] [--apply] [--only <origine>]]`.
- Copie de travail de la base : `target/tl-scripts/mini_store.py <data_dir> <cible> [--props]`, `.eml` d'un dossier : `copy_emls.py`.

## Méthode et pièges
- Vérification d'interface : `cargo tauri build --no-bundle` (faire `Remove-Item Env:NO_COLOR` avant), puis `node tools/ui_check.mjs`.
  - Le script pilote l'application par CDP, ne modifie pas la base et nettoie derrière lui.
  - Ne **pas** piloter à la souris système : des clics sont déjà partis hors de la fenêtre.
- Paul ne veut pas tester à notre place : faire les vérifications soi-même.
- Heredocs bash contenant des apostrophes : ils échouent dans l'outil. Écrire les scripts dans `target/tl-scripts/` (non versionné) et les lancer avec `python -I`.
  - `python -I` ignore `PYTHONIOENCODING` : mettre `sys.stdout.reconfigure(encoding="utf-8")` dans le script.
- REA : la session MCP est perdue à chaque reconnexion du serveur, et la réanalyse d'`OUTLOOK.EXE` prend environ 20 min.
- Les `.eml` importés avant le schéma v6 gardent le marqueur de préfixe du PST dans `Subject`. Il faudra le retirer à l'export.
- Ne jamais lire les PST sous `A:\Fichiers Outlook` sur place : travailler sur des copies.
