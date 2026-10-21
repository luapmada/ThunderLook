# Passation — après le J7 (état au 2026-10-21)

À lire en premier dans une nouvelle session, avec `PLAN.md` §4-5, `notes/j7-plan.md` et `git log`.

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
- Profil IMAP d'Outlook : comptes paul@, contact@electromagnetisme.fr, contact@angeliquemaillet.art ; contact@hypso-stephaniemilliot.fr n'est configuré dans aucun profil.
- Outlook se pilote en lecture par COM (`New-Object -ComObject Outlook.Application` ; `GetActiveObject` échoue quand Outlook a été lancé depuis la session). Écritures dans la messagerie (supprimer, déplacer) refusées par le garde-fou : à faire par Paul.

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
