# Passation — état au 2026-10-22

## À reprendre en premier
1. ~~Outlook n'affiche aucun sous-dossier IMAP~~ — **réglé le 21/10 à 21 h 35.**
   - Vraie cause : la valeur de registre `IMAP Folder Path` du compte *est* le « chemin d'accès au dossier racine » (onglet Options avancées), et elle contenait le chemin d'un .ost (`A:\Fichiers Outlook\<compte>.ost`) pour les trois comptes. Les OST réellement utilisés sont dans `%LOCALAPPDATA%\Microsoft\Outlook` ; ceux de `A:\Fichiers Outlook` datent du 13/07 et ne servent plus.
   - Correctif : racine = `INBOX` sur les trois comptes, via le panneau Courrier (MLCFG32.CPL) piloté par messages Win32 et UI Automation (scripts dans `$CLAUDE_JOB_DIR/tmp` : `uia.ps1`, `set_root.ps1`). Vérifié par COM : Sent, Drafts, Junk, Trash et les dossiers des règles apparaissent, avec les mêmes nombres que le serveur.
   - Lancer Outlook : `explorer.exe "C:\Program Files\Microsoft Office\root\Office16\OUTLOOK.EXE"`. Lancé par `Start-Process` ou par COM (`-Embedding`) depuis la session, il reste figé sans fenêtre. Ne lire par COM qu'un Outlook déjà ouvert, dans un `Start-Job` avec délai.
   - Pendant le réglage, une frappe clavier est arrivée dans la boîte de dialogue au premier plan (« rrent a charger ? ») : relire chaque champ avant OK.
2. ~~Bogue du point dans un nom de dossier~~ — corrigé (`50e825a`) : le séparateur est remplacé par U+2024 / U+2215 côté serveur. paul@ : `INBOX.[electromagnetisme.fr]` renommé `INBOX.[electromagnetisme&ICQ-fr]`, cibles des 2 règles mises à jour. electromagnetisme : 7 règles copiées du PST désactivées (dont « contact@electromagnetisme.fr (1) », qui aurait déplacé tout le courrier du compte) ; restent actives AR, Mail Delivery System ×2, « a été lu le », catégories. Les 6 dossiers vides d'electromagnetisme ont été supprimés du serveur le 21/10 avec l'accord de Paul (`folder_fix.rs … delete`) ; restent INBOX, Accusés, Trash, Sent, Junk, Drafts. Sauvegarde de la base avant ces changements : `data/store/thunderlook.db.bak-2026-10-21-regles`.
3. **Message à Mondial Relay** toujours dans la Boîte d'envoi d'Outlook (profil IMAP) : Paul doit le supprimer (garde-fou : suppression refusée à l'agent).
4. ~~J9 Calendrier, option A~~ — **clos le 22/10 en v0.9.0** : étapes 1, 2, 3 (vues), 5 (`.ics`/CSV) et rechargement depuis le PST (`tl-import --calendars`) ; occurrences identiques à Outlook sur 2023-2026. Étape 4 sans objet (Outlook fait foi), étape 6 (réunions iTIP/iMIP) abandonnée à la demande de Paul. Détail : `notes/j9-plan.md` ; mesures : `PLAN.md` §5. Prochain : J10 (Contacts, puis Tâches).

Fait : construction à jour (`aa30da2`), `ui_check` 5/5 au vert, ThunderLook relancé. Une instance sans fenêtre (20 h 35) bloquait l'exécutable : `CloseMainWindow` ne suffit pas, il faut alors `Stop-Process`.

## Fait le 21/10 après la v0.8.0 (non publié, pas d'étiquette)
- Schéma v9 : `account.rules_on_arrival` ; J8 POP3 abandonné.
- Outlook basculé sur le profil IMAP « Outlook », PST rattaché ; sauvegardes `A:\Fichiers Outlook\*_BACKUP_2026-10-21_1638.*`.
- ThunderLook : comptes paul@ (source 3), contact@electromagnetisme.fr (4), contact@angeliquemaillet.art (5), identiques au serveur ; hypso redirigé vers paul@ (Hostinger, copie gardée).
- Règles du PST copiées par compte (`account:3`, `account:4`), règles à l'arrivée actives sur 3 et 4 ; dossiers de règles créés sur les deux serveurs (noms IMAP en UTF-7 modifié).
- Corrections : sortie de Courrier indésirable (UID avant changement de dossier), menu Indésirables ▾, cibles de règles par compte (`imap:<source>:<nom>`), déplacement entre comptes = dépôt puis suppression, INTERNALDATE au dépôt (entre guillemets), mkdir + abonnement, aperçu obligatoire au-delà de 20 déplacements.
- Incident réparé : 111 messages de paul@ partis par erreur chez electromagnetisme, ramenés (`rehome.rs`), dates conservées.
- Outils (`crates/sync/examples`) : compare, find, dates, subscriptions, copy_rules, rehome, cross_move, junk_roundtrip, mkdir, account_rules. Dossier de test : `$CLAUDE_JOB_DIR/tmp/j7store` (comptes test et test2).

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
- Ne jamais réécrire un fichier source avec `Set-Content -Encoding Ascii` ni `utf8` sous PowerShell 5.1 (accents perdus, BOM) : passer par Python (`io.open(..., encoding="utf-8", newline="\n")`) ou l'outil d'édition. Heredocs bash avec apostrophes : écrire le script dans `target/tl-scripts/`.
- Calendrier : `UI_ONLY=J9 node tools/ui_check.mjs` pour le seul cas J9 ; `tools/shot.mjs` fait une capture par CDP ; référence des occurrences : `target/tl-scripts/outlook_occ.ps1` (COM, lecture seule), à comparer avec `crates/calendar/examples/occurrences`.
- Outlook se lance par `explorer.exe` (jamais `Start-Process` ni COM depuis la session : il reste figé sans fenêtre).
- Vérification d'interface : `cargo tauri build --no-bundle` (faire `Remove-Item Env:NO_COLOR` avant), puis `node tools/ui_check.mjs` (cas J3b, J5, J6 ×2, J7).
  - Le script pilote l'application par CDP et nettoie derrière lui (fichiers de réglages, messages de test jusque sur le serveur, compte de test).
  - Premier lancement après un redémarrage (cache froid) : le cas J5 peut dépasser le délai d'ouverture de la rédaction ; relancer.
  - Les passages d'envoi/réception s'enchaînent (IDLE) : suivre l'événement « sync », pas la barre d'état.
- Ne **pas** piloter à la souris système.
- Paul ne veut pas tester à notre place, ni faire les démarches à notre place (« je ne bosse pas pour toi ») : les faire soi-même.
- Heredocs bash contenant des apostrophes : ils échouent dans l'outil. Écrire les scripts dans `target/tl-scripts/` et les lancer avec `python -I` (qui ignore `PYTHONIOENCODING` : `sys.stdout.reconfigure(encoding="utf-8")`).
- Commandes Tauri `async` : elles tournent déjà sur un runtime tokio ; pour un `block_on`, passer par un fil dédié.
- Un autre outil a déjà écrit dans l'arbre de travail sans valider (schéma v8, 20/10) : regarder `git status` en début de session.
- REA : la session MCP est perdue à chaque reconnexion du serveur, et la réanalyse d'`OUTLOOK.EXE` prend environ 20 min.
- Ne jamais lire les PST sous `A:\Fichiers Outlook` sur place : travailler sur des copies.
