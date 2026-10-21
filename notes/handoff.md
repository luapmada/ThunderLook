# Passation — état au 2026-10-21, 21 h 40

## À reprendre en premier
1. ~~Outlook n'affiche aucun sous-dossier IMAP~~ — **réglé le 21/10 à 21 h 35.**
   - Vraie cause : la valeur de registre `IMAP Folder Path` du compte *est* le « chemin d'accès au dossier racine » (onglet Options avancées), et elle contenait le chemin d'un .ost (`A:\Fichiers Outlook\<compte>.ost`) pour les trois comptes. Les OST réellement utilisés sont dans `%LOCALAPPDATA%\Microsoft\Outlook` ; ceux de `A:\Fichiers Outlook` datent du 13/07 et ne servent plus.
   - Correctif : racine = `INBOX` sur les trois comptes, via le panneau Courrier (MLCFG32.CPL) piloté par messages Win32 et UI Automation (scripts dans `$CLAUDE_JOB_DIR/tmp` : `uia.ps1`, `set_root.ps1`). Vérifié par COM : Sent, Drafts, Junk, Trash et les dossiers des règles apparaissent, avec les mêmes nombres que le serveur.
   - Lancer Outlook : `explorer.exe "C:\Program Files\Microsoft Office
oot\Office16\OUTLOOK.EXE"`. Lancé par `Start-Process` ou par COM (`-Embedding`) depuis la session, il reste figé sans fenêtre. Ne lire par COM qu'un Outlook déjà ouvert, dans un `Start-Job` avec délai.
   - Pendant le réglage, une frappe clavier est arrivée dans la boîte de dialogue au premier plan (« rrent a charger ? ») : relire chaque champ avant OK.
2. **Bogue ThunderLook — point dans un nom de dossier** : `[electromagnetisme.fr]` a créé sur le serveur `INBOX.[electromagnetisme` (non abonné) avec `fr]` dessous, sur paul@ et electromagnetisme. Le nom local doit être converti (remplacer ou refuser le séparateur IMAP) avant le mkdir. À corriger, puis faire le ménage sur le serveur.
3. **Message à Mondial Relay** toujours dans la Boîte d'envoi d'Outlook (profil IMAP) : Paul doit le supprimer (garde-fou : suppression refusée à l'agent).
4. **J9 Calendrier, option A** (Outlook fait foi, ThunderLook en lecture seule) : pas commencé. Plan : `notes/j9-plan.md`.

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
- Livré : J0 à J7 et J3b. La version courante est **v0.8.0** (étiquette git). Prochaine mineure : 0.9.0 (`notes/dev.md` § Versions).
- `data/store` est au schéma 8 ; aucun compte n'y est configuré (le cas J7 de `ui_check` ajoute puis retire le compte de test).
- ADI Facturation : hors sujet pour l'instant, à la demande de Paul. Ne pas relancer.

## Prochain jalon
- `PLAN.md` : J8 POP3 « si toujours utile (profil pop-original) » — à confirmer avec Paul avant de l'entreprendre ; sinon J9 (Calendrier, puis Contacts, puis Tâches).

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

## Points ouverts du J7
- Créer, renommer, supprimer un **dossier** d'un compte n'est pas reporté sur le serveur (seulement les messages).
- IMAP en STARTTLS (port 143) non pris en charge (TLS 993 seulement) ; SMTP accepte les deux.
- Mot de passe rangé par adresse dans le gestionnaire d'identifiants : retirer le compte d'un dossier de données le retire pour tous (le cas J7 de `ui_check` l'ôte ; l'exemple `sync` le remet).
- Le message d'origine n'est pas marqué « répondu » / « transféré » après l'envoi.
- Règles à l'arrivée : toutes les règles actives, comme Outlook ; une règle qui déplace vers un dossier du PST retire le message du serveur.
- Un compte supprimé garde sa surveillance IDLE jusqu'à 10 min (fin de l'attente en cours).
- Notification Windows : pas d'action au clic.

## Méthode et pièges
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
