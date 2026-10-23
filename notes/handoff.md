# Passation — état au 2026-10-22 au soir (fin de contexte)

## À reprendre en premier
1. **v0.9.2 publiée le 22/10** : les 8 points ouverts du J7 (voir « Points ouverts du J7 », rayés) fusionnés depuis la
   branche `j7-points-ouverts` (worktree retiré). **Base au schéma v10** (`message.last_verb`, `last_verb_at`) :
   un exécutable antérieur refuse `data/store` ; sauvegarde d'avant migration
   `data/store/thunderlook.db*.bak-2026-10-22-v092`. `ui_check` 8/8 ; vérifié à l'écran par CDP (icône et barre
   « répondu », `open-item` sélectionne le message, champs Port/Chiffrement d'« Ajouter un compte »). Correctif de la
   fusion : un compte retiré pendant que son dossier est affiché laissait une liste vide (`77bcc14`).
   Script de vérification : `target/tl-scripts/verify_v092.mjs` (coupe puis rétablit « marquer comme lu »).
- **24/10 — ÉTAT ET PROCÉDURE (lire d'abord).**
  - **Cap** : ThunderLook = clone AUTONOME d'Outlook (Paul : plus aucune dépendance à Outlook, code COM retiré). Calendrier (séries, exceptions), contacts (photo, listes, catégories), tâches (périodiques, rappels), notes, rappels, recherche globale, échange téléphone (fichiers .vcf/.ics/.vnt, dossier d'échange, annulation), options, impression, corbeille interne + Ctrl+Z, sauvegardes, aide : TOUT fusionné dans `main` jusqu'aux lots A à O ; lots P à T (robustesse base, créer depuis un message, dossiers, doublons de contacts, aide) fusionnés EN FIN DE SESSION ; build + `ui_check` lancés en arrière-plan (résultat non lu) : à refaire en début de session (`cd app/src-tauri; cargo tauri build --no-bundle; cd ../..; node tools/ui_check.mjs`, ThunderLook fermé).
  - **Reste connu** : MPE (jamais touché ; plan `notes/mpe-plan.md`, à faire AVEC Paul) ; vérifier à l'écran les lots récents (Paul n'a pas tout essayé) ; temps de chargement du calendrier au lancement (lot F a mesuré, voir `notes/perf.md`) ; sauvegarde différée de 240 s (`safety_cmds.rs`) en attendant le lot P ; `Calendar3098` et doublons de noms de calendriers dans le volet gauche.
  - **Procédure cloud (Paul veut un MAXIMUM de travail dans le cloud, quota hebdo local à ~90 %)** : écrire des lots dans `notes/cloud/lot-X.md` (périmètres de fichiers DISJOINTS, voir `notes/cloud/README.md`), pousser sur `main`, puis lancer `powershell -NoProfile -Command "Start-Process powershell -ArgumentList '-NoExit','-ExecutionPolicy','Bypass','-File','A:\PROJETS\IA-LABea-outlook-clone\lancer-cloud.ps1','U,V,W,X,Y' -WorkingDirectory 'A:\PROJETS\IA-LABea-outlook-clone'"` (liste SÉPARÉE PAR DES VIRGULES, un seul argument ; `claude --cloud` exige un terminal interactif, donc jamais depuis l'outil Bash). Le script ouvre l'onglet de chaque session. Surveiller avec l'outil Monitor : boucle `git fetch` + compter `git branch -r | grep -iE "lot-[u-y]"`, ré-armer toutes les 30 min. Les branches s'appellent `claude/cloud-lot-x-xxxx` ou `claude/lot-x-cloud-xxxx`. Fusionner chaque branche avec `git merge --no-edit origin/<branche>` ; en cas de conflit (listes `invoke_handler`, `mod`, `members`) : `python -I target/tl-scripts/keep_both.py <fichiers>` puis VÉRIFIER les accolades et les clés dupliquées (`Cargo.toml` : une seule ligne `members`/`default-members`). Puis `cargo check` (ui en wasm32 + src-tauri), `cargo test --workspace`, build, `ui_check`. Paul exige que je décide seul de la suite et lance sans demander.
  - **Pièges** : un `git diff --name-only` dans `$(…)` est réécrit par le hook rtk (utiliser `git status --short | grep -E "^(UU|AA)"`) ; un `sed -i` avec `${…}` dans bash vide la variable (passer par Python) ; heredocs bash avec apostrophes et gros blocs échouent (utiliser l'outil Write) ; une classe CSS `.cat` existe pour le courrier (ne jamais l'utiliser dans le calendrier : `cal-cat`) ; ThunderLook est à instance unique (le fermer avant de compiler ou tester) ; les essais d'écriture se font sur une COPIE de la base (`THUNDERLOOK_DATA=<copie>`, copie dans `$CLAUDE_JOB_DIR/tmp/ctstore`, les réglages d'affichage sont partagés : les remettre) ; ne JAMAIS afficher le contenu des notes de contacts (mots de passe).
- **23/10 nuit — AUTONOMIE faite pour les 4 modules** (tout écrit dans la base ThunderLook, plus d'Outlook) : rendez-vous (créer, modifier, supprimer ; **périodiques** : créer jour/semaine/mois/an avec fin jamais/N/date, supprimer UNE occurrence ; `calendar/src/write.rs`, `recur.rs` `encode`/`with_deleted_instance`) ; contacts (`contacts/src/write.rs`, boîte `contact_form.rs`) ; tâches et notes (`crates/items`, `tasks.rs`, `notes.rs`, modules Ctrl+4/Ctrl+5). Testé de bout en bout sur une COPIE de la base (`target/tl-scripts/{ctx,ct,rp,it}_test.mjs`, `TL_DATA=<copie>`), `ui_check` 10/10. **Reste** : modifier heures/périodicité d'une série existante ; rendez-vous avec exceptions (modifier UNE occurrence) ; tâches périodiques ; contacts : photo, listes de distribution, catégories ; **échange avec le téléphone par MPE** (export/import vCard-iCal-vNote, étape 4 de la passation, à faire AVEC Paul : MPE est sensible) ; retirer le code COM mort (`crates/outlook`).
- **23/10 soir — CAP : ThunderLook = clone AUTONOME d'Outlook (Paul : « je me branle d'Outlook »).** Fini : écriture des rendez-vous dans la base ThunderLook (`crates/calendar/src/write.rs` : créer, modifier, supprimer ; `shared_calendar.rs` n'appelle plus Outlook ; plus de synchronisation COM). Pas encore : supprimer UNE occurrence d'une série, créer un rendez-vous PÉRIODIQUE (encoder `PidLidAppointmentRecur`, `recur.rs` ne sait que lire), écriture des contacts, Tâches, Notes, export/import vCard-iCal-vNote pour MPE. Le code COM (`crates/outlook`) reste mais n'est plus appelé (contacts : `contacts.rs` non appelé non plus). Essais d'interface sur une COPIE de la base : `THUNDERLOOK_DATA=<copie>` (copie 4,3 Go dans `$CLAUDE_JOB_DIR/tmp/ctstore` ; les réglages d'affichage sont, eux, partagés : les remettre après un essai).
- 23/10 : molette validée par CDP (d371b3d) ; palette des 25 couleurs d'Outlook appliquée et rendez-vous remplis de la couleur de leur catégorie ; clic droit du calendrier (créneau : Nouveau rendez-vous / journée entière ; rendez-vous : Privé, Afficher en tant que, Classer, Supprimer ; Transférer et réunions/périodiques inactifs). Écriture du calendrier déjà faite (8eb2371). REA : RT_MENU absents d'OUTLOOK.EXE, menus du calendrier = barre Office (voir REA-LIMITES.md). Reste : drapeaux/indicateurs de la liste de courrier (relevé à faire, demander l'accord avant de piloter la vue d'Outlook), tester Privé/Afficher en tant que/Classer sur un rendez-vous de test (écrit dans Outlook ; non testé), non publié (pas de version). Test d'interface : instance unique, fermer ThunderLook avant ; compiler avec CARGO_TARGET_DIR séparé.
2. **22/10 nuit — demande de Paul en cours** : couleurs du calendrier comme Outlook (catégories, disponibilité,
   drapeaux, indicateurs), menus clic droit d'Outlook (Paul choisit lesquels intégrer), défilement à la roulette.
   - Relevés faits : `notes/outlook-couleurs-menus.md` (palette des 25 couleurs, menus exacts). **Faits sans REA** :
     Paul le reproche (« tu réinventes la roue ») ; reprendre avec les outils REA (observe_native_ui,
     capture_native_ui_scenario, trace_native_ui_action) et consigner dans REA-LIMITES.md.
   - Roulette : code **non validé** dans l'arbre de travail (vue Mois par semaine via `CalendarQuery.start`, navigateur
     par mois ; `app/ui/src/calendar.rs`, `crates/calendar/src/view.rs`) : compile, pas encore testé.
   - Reste : poser la question des menus à Paul ; appliquer la palette (remplacer `CATEGORY_COLORS` de
     `crates/mailbox/src/lib.rs`, teintes « propres au projet ») ; reprendre `Store.Categories` à la synchro ;
     drapeaux et indicateurs non relevés.
   - Incident : une capture d'écran a pris une fenêtre Notepad++ (`TOKENS.txt`, clés d'API) ; fichier effacé ;
     Paul prévenu. Ne capturer que des fenêtres d'Outlook (PrintWindow) ou le rectangle d'un menu.
3. **22/10 soir** :
   - J10 Contacts (lecture) fait (`notes/j10-plan.md`). Paul : les contacts manquants arriveront par MPE — en fait
     ils sont déjà dans Outlook ; ThunderLook lit une copie du 15/10. Suite logique : contacts lus en direct par
     Outlook, comme le calendrier (même crate `crates/outlook`).
   - **Calendrier commun avec Outlook** fait (accord de Paul, « ok pour 1 ») : `notes/j9-plan.md` § étape 4.
   - Notifications : identité ThunderLook + lien `thunderlook:` (clic actif même depuis le Centre de notifications),
     `app/src-tauri/src/toast.rs`. **Les notifications Windows sont coupées sur le poste de Paul**
     (`ToastEnabled = 0`) : rien ne s'affiche tant qu'il ne les rallume pas (Paramètres › Système › Notifications).
   - Signalé à Paul : 209 rendez-vous ont perdu leur rappel dans Outlook le 21/10 à 16 h 42.
3. **Quand se passer d'Outlook** (réponse donnée à Paul le 22/10) : après 1) points J7 ; 2) écriture du calendrier
   dans ThunderLook (créer/modifier/supprimer, rappels — étape 4 du J9, sans objet en option A) ; 3) Contacts, Tâches,
   Notes (J10+) ; 4) synchro du téléphone par MPE sur fichiers vCard/iCal/vNote (MPE ne parle qu'à Outlook ou à des
   fichiers ; pas de CalDAV/CardDAV). Ordre : 1, 2, 3, 4. Aucune date promise.
4. **MPE (MyPhoneExplorer) : FONDAMENTAL et très sensible pour Paul.** Plan et journal : `notes/mpe-plan.md` (§9).
   - MPE ne parle pas à ThunderLook : il synchronise le téléphone principal (SM-S916U1, S23+) avec le PST
     `Outlook_300923.pst` via le profil `pop-original`, en Wi-Fi. Il fonctionne normalement (synchro du 22/10 14 h 11 :
     contacts 2 139/2 140 sans action, calendrier 370/370, 36 mises à jour vers Outlook proposées, non validées).
   - **Ne jamais fermer/relancer MPE sans prévenir Paul** (`MultiSyncOnStart=1` : chaque démarrage lance une synchro).
     Pas d'analyse détaillée non demandée (Paul : « tu te barres dans des méandres pour me faire dépenser des tokens »).
   - Fait le 22/10 : sauvegardes (config MPE → `A:\AI_related\Claude_Code_Work\mpe-backup\2026-10-22\`, PST →
     `A:\Fichiers Outlook\Outlook_300923_BACKUP_2026-10-22_MPE.pst`, SHA-256 identique) ; synchro automatique du S20 FE
     (SM-G781B) coupée. Export .vcf : refusé par Paul (inutile). Banc d'essai S20 FE : pas nécessaire pour l'instant.
   - Étape 3 (repointer MPE de `pop-original` vers le profil « Outlook », 3 lignes de `settings.dat`) : acceptée **à
     condition de lister avant à Paul**, à faire avec lui devant la fenêtre de confirmation. EntryID des dossiers
     identiques dans les deux profils (vérifié). Pas encore faite.
5. **Message à Mondial Relay** toujours dans la Boîte d'envoi d'Outlook (profil IMAP) : à supprimer par Paul.
6. **Images distantes** : Paul a parlé d'un « message identifié Outlook » ; introuvable dans les messages (recherche du
   22/10). Lui demander l'objet du message concerné s'il revient dessus.

## Fait le 22/10 (v0.9.0 et v0.9.1, poussées)
- J9 clos en **v0.9.0** (calendrier option A : lecture, expansion, vues, `.ics`/CSV, rechargement `tl-import --calendars`).
- Courrier : clic droit complet (Répondre, Répondre à tous, Transférer, Catégoriser, Suivi, Actions rapides, Règles ›
  Créer une règle… / Gérer, Déplacer, Courrier indésirable ›, Archiver, Supprimer) ; clic droit direct sur le drapeau et
  sur les catégories. **Bogue réparé** : les entrées des sous-menus du clic droit ne faisaient rien (rappels d'une portée
  Leptos détruite ; le sous-menu n'est plus mémorisé que par son libellé) ; cas `ui_check` de non-régression.
- Expéditeurs bloqués/approuvés (`junk.json` du dossier de configuration), appliqués à l'arrivée avant réponse
  automatique et règles ; boîte « Options du courrier indésirable » ; bouton « Télécharger les images ».
- Champ « De » (`Draft.from_account`) : compte du message d'origine, sinon du dossier affiché ; sans compte,
  « (par défaut) ». Avant, un nouveau message partait toujours de paul@ : le cas J7 de ui_check envoyait depuis le vrai
  compte ; 13 messages et 2 brouillons de test supprimés de paul@ avec l'accord de Paul (`crates/sync/examples/delete_ids`),
  serveur = local (1 281). Cas J7 corrigé (envoi depuis le compte de test) + cas « expéditeur bloqué ».
- `ui_check` : 8 cas (J3b, J5, J6 ×2, J7 ×2, J9, clic droit) ; `UI_ONLY=<préfixe>` pour n'en lancer qu'un.
- Piège : un script Python de `target/tl-scripts/` peut déjà exister sous le même nom (l'outil Write refuse alors
  d'écrire) : vérifier le résultat de Write avant de lancer le script.

## Fait le 21/10 après la v0.8.0 (non publié, pas d'étiquette)
- Schéma v9 : `account.rules_on_arrival` ; J8 POP3 abandonné.
- Outlook basculé sur le profil IMAP « Outlook », PST rattaché ; sauvegardes `A:\Fichiers Outlook\*_BACKUP_2026-10-21_1638.*`.
- ThunderLook : comptes paul@ (source 3), contact@electromagnetisme.fr (4), contact@angeliquemaillet.art (5), identiques au serveur ; hypso redirigé vers paul@ (Hostinger, copie gardée).
- Règles du PST copiées par compte (`account:3`, `account:4`), règles à l'arrivée actives sur 3 et 4 ; dossiers de règles créés sur les deux serveurs (noms IMAP en UTF-7 modifié).
- Corrections : sortie de Courrier indésirable (UID avant changement de dossier), menu Indésirables ▾, cibles de règles par compte (`imap:<source>:<nom>`), déplacement entre comptes = dépôt puis suppression, INTERNALDATE au dépôt (entre guillemets), mkdir + abonnement, aperçu obligatoire au-delà de 20 déplacements.
- Incident réparé : 111 messages de paul@ partis par erreur chez electromagnetisme, ramenés (`rehome.rs`), dates conservées.
- Outils (`crates/sync/examples`) : compare, find, dates, subscriptions, copy_rules, rehome, cross_move, junk_roundtrip, mkdir, account_rules. Dossier de test : `$CLAUDE_JOB_DIR/tmp/j7store` (comptes test et test2).

## Où on en est
- Livré : J0 à J7, J3b et J9 (J8 abandonné). Version courante **v0.9.2** ; prochaine : 0.10.0.
- `data/store` au schéma 9 ; comptes paul@ (source 3), electromagnetisme (4), angeliquemaillet (5) ; le cas J7 de
  `ui_check` ajoute puis retire le compte de test.
- ADI Facturation : hors sujet pour l'instant, à la demande de Paul. Ne pas relancer.

## Prochain jalon
- Écriture du calendrier, puis J10 Contacts (voir « À reprendre en premier » § 2).

## Transition Outlook ↔ ThunderLook (2026-10-21)
- Outlook basculé sur son profil **« Outlook » (IMAP, déjà configuré)** à la place de « pop-original » (POP3, copie gardée 14 jours puis effacée) : plus rien n'est retiré du serveur. PST `A:\Fichiers Outlook\Outlook_300923.pst` rattaché comme fichier de données secondaire (historique visible ; règles inactives pendant la transition).
- Sauvegardes avant bascule : `A:\Fichiers Outlook\Outlook_300923_BACKUP_2026-10-21_1638.pst` (SHA-256 identique) et `Outlook_profils_BACKUP_2026-10-21_1638.reg`. Retour arrière : `DefaultProfile` = `pop-original`.
- `paul@adam-declic.fr` ajouté dans ThunderLook par Paul (source 3, règles à l'arrivée décochées) : 1 256 messages, identiques au serveur dossier par dossier (`crates/sync/examples/compare.rs`) ; Boîte de réception 942 dans Outlook, le serveur et ThunderLook.
- Profil IMAP d'Outlook : comptes paul@, contact@electromagnetisme.fr, contact@angeliquemaillet.art.
- ThunderLook (21/10) : comptes paul@ (source 3, règles du PST copiées dans `account:3`, règles à l'arrivée actives), contact@electromagnetisme.fr (4 ; 94 messages = serveur), contact@angeliquemaillet.art (5 ; 5 = serveur). Mots de passe saisis par Paul (`target/tl-scripts/prefill_account.mjs` préremplit le formulaire par CDP, port 9336).
- contact@hypso-stephaniemilliot.fr (boîte d'une cliente, réception seulement) : pas de compte ; redirection Hostinger vers paul@ avec copie gardée (FO8d8cf98a0366be8478d6796b31ff), confirmée le 21/10.
- Dossiers créés sur le serveur de paul@ pour les règles : Accusés, LBC | FB, Allo Voisins, Ebay & Paypal, Hébergements WEB (noms IMAP en UTF-7 modifié).
- Outlook se pilote en lecture par COM (`New-Object -ComObject Outlook.Application` ; `GetActiveObject` échoue quand Outlook a été lancé depuis la session). Écritures dans la messagerie (supprimer, déplacer) refusées par le garde-fou : à faire par Paul.

## Incident du 21/10 au soir (réparé)
- En appliquant les règles de paul@ à sa Boîte de réception, 111 messages ont été déplacés vers les dossiers homonymes de contact@electromagnetisme.fr : cibles de règles désignées par le seul nom IMAP (`imap:INBOX.…`), dossiers créés dans les deux comptes. Supprimés de paul@, déposés chez electromagnetisme, rien de perdu.
- Correction `2b0354e` : cibles `imap:<source>:<nom>` ; déplacement entre comptes = dépôt puis suppression (`after_append`) ; INTERNALDATE au dépôt. Test de non-régression `crates/mailbox/tests/account_rules.rs`.
- Réparation (`crates/sync/examples/rehome.rs`) : 111 messages ramenés dans les dossiers de paul@ (32 Allo Voisins, 33 Accusés, 26 LBC | FB, 20 Ebay & Paypal), dates d'origine conservées ; paul@ 1 271 = serveur, electromagnetisme 94 = serveur. Règles recopiées (`account:3`, `account:4`), règles à l'arrivée réactivées pour ces deux comptes.

## Comptes de test (Hostinger, offre gratuite « Free Business Email » d'adam-declic.fr, créés via le MCP Hostinger)
- `thunderlook-test@adam-declic.fr` (critère du J7) et `thunderlook-test2@adam-declic.fr` (expéditeur extérieur : réponse automatique).
- Identifiants : `data/test-account.json`, `data/test-account2.json` (non versionnés). IMAP `imap.hostinger.com:993`, SMTP `smtp.hostinger.com:465`, TLS.
- Outils : `target/tl-scripts/server_side.py` (send / seen / delete / list ; `TL_ACCOUNT`, `TL_TO`), `check_account.py`.
- Exemples (`crates/sync/examples`) : `probe`, `sync` (crée le compte dans un dossier de test et remet le mot de passe), `ops_roundtrip`, `send_roundtrip`, `idle`, `autoreply_roundtrip`. Dossier de test habituel : `$CLAUDE_JOB_DIR/tmp/j7store` (jamais `data/store`).
- MCP Hostinger : `claude mcp add -s user --transport http hostinger https://mcp.hostinger.com` ; OAuth fait ; `mcp__hostinger__execute` autorisé par Paul.

## Points ouverts du J7 (tous traités en v0.9.2, 22/10)
- ~~Dossiers de compte non reportés~~ : renommer, déplacer, supprimer (→ `INBOX.Trash`), suppression définitive, Ctrl+Z.
- ~~IMAP STARTTLS~~ : 143 STARTTLS et 993 TLS ; port et chiffrement dans « Ajouter un compte ».
- ~~Mot de passe partagé~~ : liste `<adresse>#dossiers` ; effacé au retrait du dernier dossier ; ancien mot de passe « (antérieur) » jamais effacé.
- ~~Original non marqué~~ : `last_verb` (102/103/104), `\Answered` / `$Forwarded` sur le serveur et repris à la relève.
- ~~Règle vers un dossier du PST~~ : comportement d'Outlook gardé, avec garde-fous (recherche par nom limitée au compte, avertissements, suppression serveur seulement si la copie locale existe).
- ~~IDLE d'un compte supprimé~~ : arrêt immédiat (0,29 s).
- ~~Notification sans action~~ : clic → Boîte de réception du compte ou message.
- Limites restantes : clic depuis le Centre de notifications sans effet (activation COM non faite) ; messages du PST
  déjà traités dans Outlook (propriété 0x1081) non marqués.

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
