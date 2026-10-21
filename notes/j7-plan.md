# Plan du J7 : IMAP + SMTP (Hostinger)

Établi le 2026-10-21. Critère du jalon (`PLAN.md` §4) : envoi et réception réels sur un compte de test.
Compte : `thunderlook-test@adam-declic.fr` (voir `notes/handoff.md`), IMAP `imap.hostinger.com:993`, SMTP `smtp.hostinger.com:465`, TLS implicite des deux côtés.

## 1. Choix techniques

| Besoin | Choix | Raison |
|---|---|---|
| IMAP | `async-imap` 0.12 sur `tokio` + `tokio-rustls` | Client en production chez Delta Chat ; IDLE, UID, APPEND, MOVE. `imap` 3 est encore en alpha. Tauri embarque déjà tokio |
| SMTP | `lettre` 0.11 (rustls, transport asynchrone tokio) | Bibliothèque de référence en Rust, SMTPS 465 et STARTTLS 587 |
| TLS | `rustls` + racines `webpki-roots` | Pas d'OpenSSL à installer, même pile pour IMAP et SMTP |
| Mot de passe | Gestionnaire d'identifiants de Windows (`keyring` 4) | Jamais dans SQLite ni dans un JSON ; chiffré par Windows pour la session de Paul |
| MIME | `mail-parser` / `mail-builder` (déjà là) | Les `.eml` reçus sont stockés tels quels, comme ceux du PST |

Nouvelle crate **`crates/sync`** (`thunderlook-sync`) : connexion, synchronisation, envoi, file d'envoi. Elle écrit dans la même base et le même magasin `.eml` que l'import PST, si bien que liste, recherche, conversations, règles et actions rapides fonctionnent sans changement sur un compte IMAP.

## 2. Stockage (schéma v8, ajouts seulement)

- `source.kind = 'imap'` : un compte est une source, comme un PST.
- Table **`account`** : `source_id`, nom affiché, adresse, nom de l'expéditeur, serveur IMAP (hôte, port, sécurité), serveur SMTP (idem), identifiant de connexion, intervalle de relève, dernière synchronisation, dernière erreur. **Pas de mot de passe.**
- Mot de passe : entrée `ThunderLook/<adresse>` du gestionnaire d'identifiants de Windows.
- `folder` : `remote_name` (nom IMAP, séparateur compris), `uidvalidity`, `uidnext` (dernier état vu). Rôles tirés des attributs SPECIAL-USE (`\Sent`, `\Drafts`, `\Junk`, `\Trash`) et de `INBOX`.
- `message` : `remote_uid`. Un message sans `remote_uid` dans un dossier IMAP est local et reste à envoyer au serveur.
- Table **`pending_op`** : opérations locales à rejouer sur le serveur (lu/non lu, indicateur, déplacer, copier, supprimer), avec tentatives et dernière erreur. Le J3 écrit ici quand la source est IMAP ; l'interface reste immédiate, le serveur suit.
- Table **`outbox`** : message à envoyer, état (en attente, en cours, envoyé, échec), tentatives, erreur, date. La Boîte d'envoi du compte affiche ces messages.

## 3. Synchronisation

1. **Dossiers** : `LIST "" "*"`, création ou renommage locaux ; un dossier disparu du serveur est retiré avec ses messages.
2. **Par dossier** (`SELECT`) :
   - `UIDVALIDITY` différent de celui enregistré : le contenu local du dossier est rechargé ;
   - nouveaux messages : `UID FETCH <dernier+1>:* (UID FLAGS INTERNALDATE RFC822.SIZE BODY.PEEK[])`, par lots de 50 ; chaque message devient un `.eml` et une ligne `message` (expéditeur, destinataires, objet, dates, aperçu, pièces jointes, conversation), puis il est indexé (J4) ;
   - drapeaux : `UID FETCH 1:* (FLAGS)` → lu (`\Seen`), indicateur (`\Flagged`), répondu ;
   - suppressions faites ailleurs : `UID SEARCH ALL` comparé aux UID locaux.
3. **Opérations locales** : la file `pending_op` est vidée avant la relève (`UID STORE`, `UID MOVE` ou `COPY` + `\Deleted` + `EXPUNGE`).
4. **En continu** : `IDLE` sur la Boîte de réception (nouveau message → relève immédiate et notification Windows) ; autres dossiers toutes les 10 min ; **F9** = tout relever et vider la boîte d'envoi.
5. **Règles à l'arrivée** : les règles actives sont appliquées aux nouveaux messages de la Boîte de réception (moteur du J6, `run_rules` limité aux nouveaux identifiants). Les actions d'envoi des règles deviennent exécutables.
6. **Hors connexion** : bascule « Travailler hors connexion » ; plus aucune connexion, les opérations et les envois s'accumulent dans les files.

## 4. Envoi

- Bouton **Envoyer** de la rédaction (Ctrl+Entrée et Alt+S) : le brouillon devient un message MIME final (même construction qu'au J5, sans `X-Unsent`, `Bcc` retiré des en-têtes), placé dans `outbox` et dans la Boîte d'envoi.
- Un fil d'envoi le transmet en SMTP (enveloppe : À + Cc + Cci), puis :
  - `APPEND` dans le dossier Envoyés du serveur (Hostinger ne le fait pas lui-même) et copie locale dans Éléments envoyés ;
  - brouillon d'origine supprimé (local et serveur) ;
  - message source marqué « répondu » ou « transféré ».
- Échec : le message reste dans la Boîte d'envoi avec l'erreur ; nouvel essai à la relève suivante (3 au plus, puis intervention).
- Le compte d'envoi : celui de la source du brouillon, sinon le compte par défaut. Un brouillon d'un fichier PST est envoyé par le compte par défaut.

## 5. Interface

- **Fichier › Paramètres du compte** : liste des comptes, Ajouter (adresse, nom, mot de passe ; serveurs préremplis pour les domaines hébergés chez Hostinger, modifiables), Tester la connexion, Supprimer (le compte et sa copie locale ; rien n'est supprimé sur le serveur).
- Volet des dossiers : une section par compte, comme un fichier de données.
- Onglet **Envoyer/Recevoir** : Tout envoyer et recevoir (F9), Mettre à jour le dossier, Tout envoyer, Afficher la progression, Travailler hors connexion.
- Barre d'état : « Relève en cours… », « Tous les dossiers sont à jour », « Hors connexion », erreur de connexion.
- Notifications Windows pour les nouveaux messages de la Boîte de réception (`tauri-plugin-notification`).
- **Réponse automatique locale** : texte et période ; une réponse par expéditeur et par période, jamais aux listes (`List-Id`, `Precedence: bulk`), ni aux réponses automatiques, ni à soi-même.

## 6. Ordre des tâches

| # | Tâche | Vérification |
|---|---|---|
| 0 | Cas J7 de `tools/ui_check.mjs` : envoi réel puis réception (écrit d'abord, échoue tant que le J7 n'est pas fait) | Échec attendu, puis réussite |
| 1 | Crate `sync` : connexion IMAP et SMTP (TLS rustls), exemple qui liste les dossiers du compte de test | Exemple en ligne de commande |
| 2 | Schéma v8, comptes, mot de passe dans le gestionnaire d'identifiants | Tests synthétiques ; aucun secret en base (vérifié par requête) |
| 3 | Relève initiale et incrémentale (dossiers, messages, drapeaux, suppressions) | Exemple sur le compte de test ; second passage sans changement |
| 4 | Opérations locales rejouées sur le serveur (`pending_op`) | Lu/déplacer/supprimer dans ThunderLook, vérifiés côté serveur par `check_account.py` étendu |
| 5 | Envoi : `outbox`, SMTP, APPEND Envoyés, bouton Envoyer | Message reçu par le compte de test |
| 6 | Fond : IDLE, relève périodique, F9, hors connexion, notifications, règles à l'arrivée | Message envoyé depuis l'extérieur visible sans action |
| 7 | Interface : paramètres du compte, Envoyer/Recevoir, barre d'état | `ui_check` |
| 8 | Réponse automatique locale | Test synthétique et réel |
| 9 | Clôture : mesures dans `PLAN.md`, version 0.8.0, étiquette, push | — |

## 7. Sécurité et prudence

- Mot de passe : gestionnaire d'identifiants de Windows uniquement, jamais dans les journaux ni dans les messages d'erreur ; `data/test-account.json` (non versionné) ne sert qu'aux tests.
- Les tests n'agissent que sur `thunderlook-test@adam-declic.fr`. Aucun compte réel de Paul n'est configuré dans ThunderLook sans sa demande.
- Suppression d'un compte dans ThunderLook : rien n'est effacé sur le serveur.
- Certificats TLS vérifiés (pas d'option « accepter tout »).
