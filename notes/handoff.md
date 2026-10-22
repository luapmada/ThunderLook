# Passation — état au 2026-10-22 au soir (fin de contexte)

## À reprendre en premier
1. **v0.9.2 publiée le 22/10** : les 8 points ouverts du J7 (voir « Points ouverts du J7 », rayés) fusionnés depuis la
   branche `j7-points-ouverts` (worktree retiré). **Base au schéma v10** (`message.last_verb`, `last_verb_at`) :
   un exécutable antérieur refuse `data/store` ; sauvegarde d'avant migration
   `data/store/thunderlook.db*.bak-2026-10-22-v092`. `ui_check` 8/8 ; vérifié à l'écran par CDP (icône et barre
   « répondu », `open-item` sélectionne le message, champs Port/Chiffrement d'« Ajouter un compte »). Correctif de la
   fusion : un compte retiré pendant que son dossier est affiché laissait une liste vide (`77bcc14`).
   Script de vérification : `target/tl-scripts/verify_v092.mjs` (coupe puis rétablit « marquer comme lu »).
2. **J10 Contacts en cours** (22/10, `notes/j10-plan.md`) : lecture et module Contacts faits et vérifiés (COM et
   `ui_check` J10). Paul a demandé « contacts d'abord » ; il demande si le calendrier peut être **le même** pour
   Outlook et ThunderLook : réponse donnée = oui, en écrivant dans Outlook par COM (Outlook reste le stockage, MPE
   inchangé, Outlook doit tourner). Attendre sa décision avant de coder l'écriture (calendrier et contacts).
   Reste au J10 : rechargement des contacts depuis une copie fraîche, listes de distribution, vCard.
   Demandé aussi par Paul : clic depuis le Centre de notifications (activation COM, AUMID + ToastActivatorCLSID).
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
