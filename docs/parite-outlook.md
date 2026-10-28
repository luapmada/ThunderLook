# Parité ThunderLook / Outlook — audit du 24/10 (lot AK)

Méthode : chaque « état » vient du CODE LU (fichier cité), pas d'une supposition. Rien n'a été lancé ni vu à l'écran.
Colonne « fonction d'Outlook » : seul ce que les notes du dépôt documentent (`notes/outlook-couleurs-menus.md`, libellés du
ruban dans `app/ui/src/commandbar.rs`, `notes/courrier-ecarts.md`, `REA-LIMITES.md`) est affirmé ; le reste est marqué
**[à vérifier]** (connaissance générale d'Outlook, non documentée ici).

Légende : **fait** · **partiel** · **absent**. Priorité : **B** bloquant · **G** gênant · **S** secondaire.

Fichiers fréquemment touchés par tous les lots (points de conflit de fusion, à régler par Paul comme dit dans
`notes/cloud/README.md`) : `app/ui/src/{commandbar,state,app,ipc,menus,dialogs}.rs`, `app/src-tauri/src/lib.rs`,
`crates/model/src/lib.rs`. Les lots ci-dessous n'y font que des ajouts d'une ligne.

---

## 1. Courrier

| Fonction d'Outlook | État dans ThunderLook | Preuve |
|---|---|---|
| Comptes IMAP multiples, SMTP | **fait** (993 TLS / 143 STARTTLS, SMTP, IDLE, hors connexion) | `crates/sync/src/{lib,account,fetch,push}.rs` |
| Configuration automatique d'un compte [à vérifier] | **absent** : seul `Endpoints::hostinger` ; autres serveurs saisis à la main | `crates/sync/src/lib.rs:51` |
| POP3 | **absent** (J8 abandonné) | `notes/handoff.md` |
| Connexion OAuth2 (Gmail, Microsoft 365) [à vérifier] | **absent** : aucune occurrence `oauth`/`XOAUTH` dans `crates/` ni `app/` | grep |
| Lire, volet de lecture (droite/bas/masqué), conversations | **fait** | `reading.rs`, `list.rs`, `crates/mailbox/src/conv.rs` |
| Tri/groupes (Date, De, À, Objet, Taille, Catégories, Indicateur, PJ, Importance, Type, Compte, Lu) | **fait** ; tri secondaire « Puis par » : cœur fait, **sélecteur absent** | `notes/courrier-ecarts.md` §1,4,5 ; `crates/mailbox/tests/sort_more.rs` |
| Drapeaux, dates libres, rappel de suivi | **partiel** : drapeaux et `FlagAction::DueOn` faits ; dialogue de date et rappel de suivi absents | `courrier-ecarts.md` §9 ; `crates/reminders/src/lib.rs` (rendez-vous et tâches seulement) |
| Catégories colorées (25 couleurs) | **fait** | `outlook-couleurs-menus.md`, `crates/mailbox/src/lib.rs` |
| Répondre / Répondre à tous / Transférer | **fait** ; « Réunion » (répondre par une réunion) **absent** | `commandbar.rs` (MAIL_HOME) |
| Rédaction : champs À/Cc/Cci, De (choix du compte), pièces jointes, gras/italique/souligné/listes | **fait** | `app/ui/src/compose.rs` |
| Rédaction : lien, image, police/taille/couleur, alignement, tableau [à vérifier] | **absent** (barre : Envoyer, Enregistrer, Joindre, G/I/S, 2 listes, Fermer) | `compose.rs:253-284` |
| Rédaction : importance, accusé de lecture/réception, envoi différé [à vérifier] | **absent** (aucun champ dans `Draft`) | `crates/model/src/lib.rs:533` |
| Autocomplétion des destinataires, Carnet d'adresses (Ctrl+Maj+B) | **absent** : champs `<input>` bruts ; bouton « Carnet d'adresses » = `cmd(…)` désactivé | `compose.rs:307-319` ; `commandbar.rs` (Rechercher) |
| Signatures : choix, insertion | **partiel** : lues dans `%APPDATA%\Microsoft\Signatures` (dossier d'Outlook) ; pas de création/édition, pas de choix par compte ni nouveau/réponse | `crates/mailbox/src/compose.rs:226-312` ; `courrier-ecarts.md` §8 |
| Modèles `.oft` | **fait** (menu « Nouveaux éléments ») | `crates/mailbox/src/oft.rs` |
| Pièces jointes : ouvrir, enregistrer, tout enregistrer | **fait** | `reading.rs:76-83`, `courrier-ecarts.md` §2 |
| Aperçu des PJ (PDF, images, Office), glisser vers l'Explorateur | **absent** | `courrier-ecarts.md` §6,7 |
| Images distantes bloquées + « Télécharger les images » | **fait** | `handoff.md`, `crates/safety` / `reading.rs` |
| Indésirables : expéditeurs bloqués/approuvés, options | **fait** | `junk.rs`, `junk.json` (`handoff.md`) ; « Ce n'est pas du courrier indésirable » hors dossier indésirable = désactivé (`menus.rs:157`) |
| Règles (création, gestion, exécution, à l'arrivée par compte) | **fait** | `rules.rs`, `create_rule.rs`, `crates/rules`, `crates/mailbox/src/{rules,rule_run}.rs` |
| Actions rapides | **fait** | `quicksteps.rs` |
| Réponses automatiques | **fait** | `autoreply.rs`, `crates/sync/src/autoreply.rs` |
| Dossiers : créer, renommer, déplacer, supprimer, vider, favoris, tout marquer lu | **fait** (renommer/déplacer/supprimer en clic droit : entrées désactivées au `menus.rs:236-238` pour certains cas) | `folders.rs`, `crates/folders`, `commandbar.rs` (FOLDER) |
| Dossiers de recherche (créés + 2 fixes) ; « Nouveau dossier virtuel » | **partiel** : créer par « Enregistrer la recherche » ; bouton « Nouveau dossier virtuel » désactivé ; « Dossier de recherche prédéfini » désactivé | `commandbar.rs` (FOLDER) ; `menus.rs:224` |
| Ignorer / Élaguer la conversation | **absent** (boutons désactivés) | `commandbar.rs` (Supprimer) |
| Filtrer le courrier (non lus, avec PJ…) | **absent** (bouton désactivé) ; la recherche à mots-clés le remplace en partie | `commandbar.rs` ; `crates/mailbox/src/search.rs` |
| Trier les dossiers de A à Z, Propriétés du dossier | **absent** | `commandbar.rs` (FOLDER) |
| Envoyer/Recevoir : Mettre à jour le dossier, Tout envoyer, progression, groupes, en-têtes seulement, marquer pour téléchargement | **absent** (désactivés) ; « Tout envoyer et recevoir » (F9) et hors connexion faits | `commandbar.rs` (SEND_RECEIVE) |
| Importer un PST | **fait** (outil `tl-import` / crate) | `crates/import-pst` |
| Annuler (Ctrl+Z) | **fait** | `crates/mailbox/src/ops.rs:843` |
| Notifications d'arrivée | **fait** (Windows ; désactivées sur le poste de Paul, voir `handoff.md`) | `app/src-tauri/src/toast.rs` |
| Réunions : lire une invitation (`text/calendar`), Accepter/Refuser [à vérifier] | **absent** : seule occurrence de `text/calendar` = type MIME d'import PST | grep (`import-pst/src/convert.rs:216`) |
| Lien `mailto:` géré par ThunderLook [à vérifier] | **absent** (aucune occurrence) | grep |

## 2. Calendrier

| Fonction d'Outlook | État | Preuve |
|---|---|---|
| Vues Jour, Semaine de travail, Semaine, Mois | **fait** | `app/ui/src/calendar.rs`, `crates/calendar/src/view.rs` |
| « 7 prochains jours », vue Planning [à vérifier] | **absent** (bouton désactivé) | `commandbar.rs` (CALENDAR_HOME) |
| Créer / modifier / supprimer un rendez-vous, journée entière, lieu, rappel, privé, disponibilité, catégories | **fait** | `appointment.rs`, `crates/calendar/src/write.rs` |
| Séries : jour/semaine/mois/an, fin jamais/N/date ; modifier toute la série, UNE occurrence, supprimer UNE occurrence | **fait** (jamais testé dans Outlook) | `write.rs:354-402`, `REA-LIMITES.md` § séries |
| Glisser-déposer, poignée de fin | **fait** | `write.rs:519` (`move_appointment`) |
| Plusieurs calendriers, couleur, calendrier de la base | **partiel** : plusieurs calendriers lus ; liste/couleurs fixes, doublons de noms signalés | `handoff.md` (Calendar3098), `calendar.rs` |
| Couleurs : catégories, bande de disponibilité | **fait** | `outlook-couleurs-menus.md` |
| Clic droit (créneau, rendez-vous) | **partiel** : « Nouveau rendez-vous périodique », « événement périodique », « Transférer », réunions inactifs | `menus.rs:53,105-106` |
| Réunions : participants, invitations, réponses, assistant de planification, disponibilité [à vérifier] | **absent** : aucun envoi ; les « participants » n'existent qu'en lecture CSV | `appointment.rs` (pas de champ), `crates/calendar/src/csv.rs:22-23,104-105` |
| Fuseaux horaires multiples [à vérifier] | **absent** : un seul fuseau codé (Europe/Paris), 34 usages de `TimeZone::PARIS` | `crates/calendar/src/tz.rs:28` ; grep |
| Jours fériés [à vérifier] | **absent** (hors jeux de test) | grep |
| Numéros de semaine, début de semaine, heures de travail | **fait** | `crates/model/src/settings.rs`, `options.rs` |
| `.ics` import/export, CSV | **fait** | `crates/calendar/src/{ics,ics_series,csv}.rs` |
| Abonnement à un calendrier Internet/CalDAV [à vérifier] | **absent** (non voulu : MPE ne parle pas CalDAV, `handoff.md`) | — |
| Rappels | **fait** (voir §7) | `crates/reminders` |
| Modifier la série d'un événement venu d'un fichier | **fait** (`replace_series`) | `write.rs:462` |

## 3. Contacts

| Fonction d'Outlook | État | Preuve |
|---|---|---|
| Liste + fiche, filtre, Ctrl+N, supprimer | **fait** | `contacts.rs`, `contact_form.rs` |
| Champs : prénom, nom, société, fonction, service, 3 courriels, mobile/bureau/domicile, adresses bureau et domicile, page web, anniversaire, notes, photo (JPEG), catégories | **fait** | `crates/model/src/contacts.rs:119-153` |
| Champs : titre, second prénom, suffixe, surnom, fax, téléphones supplémentaires, adresse « autre », messagerie instantanée, conjoint [à vérifier] | **absent** : `ContactForm` ne les porte pas ; en vCard, FAX/PAGER/CAR… sont ignorés avec un avertissement | `crates/model/src/contacts.rs:119` ; `crates/contacts/src/vcard.rs:241,471` ; `REA-LIMITES.md` (doublons) |
| Photo | **partiel** : JPEG seulement | `REA-LIMITES.md` § contacts |
| Listes de distribution | **partiel** : lecture seule | `crates/contacts/src/distlist.rs` (`REA-LIMITES.md`) |
| Doublons | **fait** (fusion, annulable) | `crates/contacts/src/dedupe.rs`, `dedupe.rs` (UI) |
| `.vcf` import/export | **fait** (2.1/3.0/4.0 lus) | `crates/contacts/src/vcard.rs` |
| Impression fiche et liste | **fait** | `print.rs` |
| Vues Cartes / Liste / Téléphone, tri et regroupement par nom/société [à vérifier] | **absent** : un seul affichage, filtre texte | `contacts.rs` |
| Boutons « Communiquer » (nouveau message au contact), « Catégories », « Suivi » | **absent** (désactivés) | `commandbar.rs` (CONTACTS_HOME) |
| Dossiers de contacts multiples | **partiel** (dossier affiché ; doublons entre dossiers non traités) | `REA-LIMITES.md` (doublons) |
| Envoyer à une liste/un groupe [à vérifier] | **absent** (pas d'autocomplétion, listes en lecture seule) | — |

## 4. Tâches

| Fonction d'Outlook | État | Preuve |
|---|---|---|
| Créer, modifier, supprimer, terminer, échéance, début, importance, statut, catégories, notes | **fait** | `tasks.rs`, `crates/items`, `crates/model/src/items.rs:63` |
| Rappel (heure par défaut) | **fait** | `settings.rs` (`default_task_reminder`), `crates/reminders` |
| Tâches périodiques | **fait** (format **non confirmé** sur une tâche réelle) | `crates/items/src/trecur.rs`, `REA-LIMITES.md` § tâches |
| Affichages (Toutes, Aujourd'hui, En retard, Cette semaine, Terminées), tri (échéance, importance, titre) | **fait** | `tasks.rs:34-35` |
| % d'avancement, travail total/réel, propriétaire [à vérifier] | **partiel** (lot AY) : % achevé lu/écrit (`PidLidPercentComplete`, VTODO `PERCENT-COMPLETE`), 5 statuts affichés dans la liste ; travail total/réel et propriétaire **absents** | `crates/model/src/items.rs` (`task_progress`), `crates/items/src/{lib,vtodo}.rs`, `tasks.rs` |
| Affecter une tâche (demande de tâche) [à vérifier] | **absent** | — |
| `.ics` (VTODO) import/export | **fait** | `crates/items/src/vtodo.rs` |
| Liste « À FAIRE » du calendrier [à vérifier] | **absent** | `calendar.rs` |

## 5. Notes

| Fonction d'Outlook | État | Preuve |
|---|---|---|
| Créer, modifier, supprimer, couleur, Imprimer | **fait** | `notes.rs`, `crates/items` |
| `.vnt` import/export | **fait** | `crates/items/src/vnote.rs` |
| Catégories des notes [à vérifier] | **absent** (`NoteForm` : texte + couleur) | `crates/model/src/items.rs:114` |
| Doublon d'entrée « Gérer / Supprimer » dans le ruban | anomalie : groupe « Gérer » déclaré deux fois | `commandbar.rs` (NOTES_HOME) |

## 6. Recherche

| Fonction d'Outlook | État | Preuve |
|---|---|---|
| Recherche courrier plein texte (FTS5) + mots-clés `de: à: cc: objet: corps: pj: catégorie: lu: suivi: importance: reçu:` | **fait** | `crates/mailbox/src/search.rs`, `docs/utilisation.md` |
| Portées : dossier, sous-dossiers, fichier de données, tous | **fait** | `commandbar.rs` (SEARCH_TAB) |
| Recherche globale contacts/rendez-vous/tâches/notes (portée « Tout ») | **fait** | `crates/search/src/lib.rs`, `global_search.rs` |
| Dossier de recherche enregistré | **fait** | `FolderOp::SaveSearch` |
| Recherche dans le contenu des pièces jointes, recherche côté serveur IMAP [à vérifier] | **absent** (index local seulement ; `pj:` = nom de fichier) | `search.rs` (en-tête) |
| Recherche avancée (boîte à critères) [à vérifier] | **absent** (mots-clés seuls) | — |

## 7. Rappels

| Fonction d'Outlook | État | Preuve |
|---|---|---|
| Rappels de rendez-vous (y compris occurrences de série) et de tâches ; Ouvrir, Reporter, Ignorer ; barre d'état « Rappels : N » | **fait** | `crates/reminders/src/lib.rs`, `app/ui/src/reminders.rs` |
| Notifications Windows + son | **fait** (Options) | `settings.rs`, `toast.rs` |
| Rappel de suivi d'un message (drapeau avec échéance) | **absent** | `Reminder.kind` ne couvre pas le courrier (`lib.rs:52`) |
| Fenêtre des rappels (onglet Affichage) | **absent** (bouton désactivé) ; le panneau existe par la barre d'état | `commandbar.rs` (VIEW) |

## 8. Options

| Fonction d'Outlook | État | Preuve |
|---|---|---|
| Thème, zoom, densité, volets, marquer comme lu (délai) | **fait** | `settings.rs`, `options.rs` |
| Calendrier : jours ouvrés, heures, durée et rappel par défaut, grille, n° de semaine, début de semaine | **fait** | `settings.rs:60-90` |
| Notes/Tâches : couleur, rappel par défaut | **fait** | idem |
| Options du courrier : format de réponse, police par défaut, enregistrement auto des brouillons, accusés, envoi différé, signature par défaut, dossier de démarrage [à vérifier] | **absent** : aucun champ dans `Settings` | `crates/model/src/settings.rs` |
| Options par compte (signature, dossier d'envoi, fréquence de relève) [à vérifier] | **partiel** : règles à l'arrivée par compte, ports/chiffrement ; reste absent | `account.rs`, `accounts.rs` |
| Raccourcis clavier personnalisables, langue [à vérifier] | **absent** | — |

## 9. Impression

| Fonction d'Outlook | État | Preuve |
|---|---|---|
| Imprimer message, calendrier (vue), fiche/liste de contacts, tâches, notes | **fait** (`window.print()`) | `print.rs`, `courrier-ecarts.md` §3 |
| Impression « mémo », aperçu avant impression, options de page [à vérifier] | **absent** (WebView2, non testable en cloud) | `courrier-ecarts.md` §10 |
| Impression rapide d'un rendez-vous (clic droit) | **présente dans le menu d'Outlook** (`outlook-couleurs-menus.md`) ; ThunderLook : **absent** du clic droit | `menus.rs` |

## 10. Échange téléphone (MPE)

| Fonction | État | Preuve |
|---|---|---|
| Export `contacts.vcf` / `taches.ics` / `notes.vnt` / `calendrier.ics` + manifeste, atomique, automatique toutes les N min | **fait** | `crates/exchange/src/lib.rs` |
| Relecture : changements, conflits, rien d'écrit sans accord, suppressions jamais cochées, annulation du dernier import | **fait** | `crates/exchange/src/{import,conflict}.rs`, `exchange.rs` |
| « Importer depuis MPE » (`Phonebook.dat`, `Organizer.dat`, `Notes.dat`), lecture seule, sens téléphone → ThunderLook | **fait** (lot AI ; jamais lancé sur le vrai dossier) | `crates/exchange/src/mpe_files.rs`, `mpe_import.rs` |
| vCalendar 1.0 (MPE) | **partiel** : converti, refus et pertes en avertissements | `import.rs:472`, `vcal10.rs` |
| Aller-retour réel avec un téléphone par fichiers | **non vérifié** : la cible « fichier » de MPE n'est pas réglée (à faire par Paul dans MPE, `handoff.md` 24/10) ; recherche en cours (lot AJ, `docs/mpe-cible-fichier.md`) | `handoff.md` |
| Écriture vers `Phonebook.dat` etc. (ThunderLook → téléphone sans passer par l'échange de fichiers) | **absent** et voulu (« MPE n'écrit jamais dans ThunderLook », « dossier de MPE jamais écrit ») | `mpe_files.rs:1-12` |
| Champs perdus à l'échange (fax, pagers, téléphones supplémentaires) | **partiel** : avertissement vCard, pas de conservation | `vcard.rs:241,471` |
| Séries sans plage ±90 j / cas refusés | **partiel** : ni modification ni suppression hors plage | `mpe_files.rs` (en-tête) |

## 11. Sauvegardes

| Fonction | État | Preuve |
|---|---|---|
| Sauvegarde quotidienne (14) et hebdomadaire (8) de rendez-vous, contacts, tâches, notes ; lancement en fil à part | **fait** | `crates/safety/src/lib.rs:600-614`, `safety.rs` |
| Restauration « Fusionner » / « Tout remettre » | **fait** | `lib.rs:814` (`restore_backup`) |
| Corbeille interne 60 jours + Ctrl+Z hors courrier | **fait** | `lib.rs:225-402` |
| Sauvegarde du courrier local (`eml/`, messages du PST importés, dossiers locaux) | **absent** : « jamais le courrier » | `docs/utilisation.md` § Sauvegardes |
| Copie `.bak-…` avant migration de la base | **fait** | `docs/utilisation.md` |
| Sauvegarde hors machine (dossier réseau, disque externe) [à vérifier] | **partiel** : dossier fixe `backups/` (« Ouvrir le dossier » seulement) | `backups_dir` (`lib.rs:434`) |

---

## Manques classés (usage quotidien sans Outlook : IMAP multi-comptes, calendrier, contacts, synchro téléphone MPE)

### Bloquants
| # | Manque | Taille | Fichiers concernés |
|---|---|---|---|
| B1 | Pas d'autocomplétion ni de Carnet d'adresses à la rédaction (aucune aide pour saisir un destinataire) | moyen | `app/ui/src/compose.rs`, `app/ui/src/commandbar.rs`, NEW `crates/contacts/src/suggest.rs`, NEW `app/ui/src/address_book.rs`, NEW `app/src-tauri/src/address_cmds.rs` |
| B2 | Synchro téléphone non bouclée : cible « fichier » de MPE non réglée (action de Paul dans MPE, lot AJ pour la procédure) ; aller-retour jamais vu sur le vrai S23/S20 | petit côté code (jeux de test), le reste = Paul | `crates/exchange/`, `docs/mpe-cible-fichier.md`, `docs/jeu-de-test-s20.md` |
| B3 | Fidélité des contacts à l'échange : fax, téléphones supplémentaires, adresse « autre », second prénom, surnom, messagerie, titre sont ignorés (avertissement) : risque de perte à chaque synchro bidirectionnelle des 2 139 contacts. **À confirmer** : `apply` n'a pas été lu en détail pour savoir si une fiche modifiée côté téléphone écrase des champs non portés par `ContactForm` | moyen | `crates/model/src/contacts.rs`, `crates/contacts/src/{write,vcard,textfmt}.rs`, `app/ui/src/contact_form.rs` |

### Gênants
| # | Manque | Taille | Fichiers |
|---|---|---|---|
| G1 | Signatures : dépendance au dossier d'Outlook, pas de création/édition, pas de choix par compte ni nouveau/réponse | moyen | `crates/mailbox/src/compose.rs` (fonction `signatures`), NEW `crates/mailbox/src/signature_store.rs`, NEW `app/ui/src/signatures.rs`, NEW `app/src-tauri/src/signatures_cmds.rs` |
| G2 | Rédaction pauvre : lien, image, police/couleur, importance, accusé de lecture, envoi différé | moyen | `app/ui/src/compose.rs`, `crates/model/src/lib.rs` (`Draft`), `crates/mailbox/src/compose.rs` |
| G3 | Invitations de réunion : ni lecture (Accepter/Refuser) ni envoi, pas de participants | gros | NEW `crates/calendar/src/itip.rs`, `app/ui/src/appointment.rs`, `app/ui/src/reading.rs`, NEW `app/src-tauri/src/invite_cmds.rs` |
| G4 | Rappels de suivi du courrier + dialogue de date du drapeau + fenêtre des rappels | moyen | `crates/reminders/src/`, `app/ui/src/reminders.rs`, `app/ui/src/dialogs.rs` (point de conflit) |
| G5 | Aucune sauvegarde du courrier local (messages importés du PST) | moyen | `crates/safety/src/lib.rs`, `app/src-tauri/src/safety_cmds.rs`, `app/ui/src/safety.rs` |
| G6 | Options du courrier absentes (format de réponse, police, brouillons auto, signature par défaut, dossier de démarrage) | moyen | `crates/model/src/settings.rs`, `app/ui/src/options.rs`, `app/src-tauri/src/options_cmds.rs` |
| G7 | Aperçu des pièces jointes (images, PDF, texte) et glisser vers l'Explorateur | moyen | `app/ui/src/reading.rs`, NEW `app/ui/src/attach_preview.rs`, NEW `app/src-tauri/src/attach_cmds.rs` |
| G8 | Comptes : pas de configuration automatique, pas d'OAuth2 (gênant seulement pour un compte Gmail/Microsoft 365 ; les comptes actuels sont Hostinger) | petit (autoconfig) / gros (OAuth) | `crates/sync/src/{lib,account}.rs`, `app/ui/src/accounts.rs` |
| G9 | Contacts : un seul affichage, pas de « Communiquer » (nouveau message au contact), listes de distribution en lecture seule | moyen | `app/ui/src/contacts.rs`, `crates/contacts/src/distlist.rs`, `app/ui/src/commandbar.rs` |
| G10 | Tâches : pas de % d'avancement ni d'affectation | petit | `crates/items/`, `crates/model/src/items.rs`, `app/ui/src/tasks.rs` |

### Secondaires
Un seul fuseau (Paris) · jours fériés · vue « 7 prochains jours »/Planning · tri « Puis par » (sélecteur) · dossier virtuel ·
Ignorer/Élaguer la conversation · Filtrer le courrier · Trier les dossiers A–Z · Propriétés du dossier · groupes
d'envoi/réception · POP3 · recherche dans les pièces jointes · aperçu avant impression · impression rapide d'un rendez-vous ·
raccourcis personnalisables · catégories des notes · ruban Notes (groupe « Gérer » en double) · `mailto:`.

---

## Découpage en lots cloud (périmètres de fichiers DISJOINTS entre lots lancés ensemble)

Numérotation à partir de AM (AL est le dernier existant). Les fichiers partagés (`commandbar.rs`, `state.rs`, `ipc.rs`,
`lib.rs`, `crates/model/src/lib.rs`) ne reçoivent que des ajouts d'une ligne ; Paul résout ces conflits.
**Dépendances** : AS et AV touchent `compose.rs` comme AM → à lancer APRÈS AM (ou fusionner AM d'abord). Les autres sont
parallélisables.

| Lot | Contenu | Périmètre exclusif de fichiers | Taille |
|---|---|---|---|
| **AM** | Autocomplétion des destinataires + boîte Carnet d'adresses (contacts du dossier, adresses déjà utilisées dans le courrier envoyé/reçu) | `app/ui/src/compose.rs` (3 champs → composant), NEW `app/ui/src/address_book.rs`, NEW `crates/contacts/src/suggest.rs` (+ `mod`), NEW `app/src-tauri/src/address_cmds.rs` | moyen |
| **AN** | Fidélité des contacts : champs manquants dans `ContactForm`, vCard 2.1/3.0/4.0 aller-retour sans perte, test d'échange (aucun champ perdu) | `crates/model/src/contacts.rs`, `crates/contacts/src/{write,vcard,textfmt}.rs`, `app/ui/src/contact_form.rs`, tests `crates/exchange/tests/` | moyen |
| **AO** | Signatures autonomes : stockage propre (config), éditeur, une par compte, nouveau/réponse ; repli sur le dossier d'Outlook | `crates/mailbox/src/compose.rs` (seulement `signatures`/`default_signature`), NEW `crates/mailbox/src/signature_store.rs`, NEW `app/ui/src/signatures.rs`, NEW `app/src-tauri/src/signatures_cmds.rs` | moyen |
| **AP** | Sauvegarde du courrier local (`eml/` + base cohérente), restauration testée ; écran Sauvegardes | `crates/safety/src/lib.rs`, `app/src-tauri/src/safety_cmds.rs`, `app/ui/src/safety.rs` | moyen |
| **AQ** | Rappels de suivi du courrier + dialogue de date du drapeau + fenêtre des rappels | `crates/reminders/src/*`, `app/ui/src/reminders.rs`, NEW `app/ui/src/flag_dialog.rs`, NEW `app/src-tauri/src/mail_reminders_cmds.rs` | moyen |
| **AR** | Options du courrier (réponse, police par défaut, brouillon auto, signature par défaut, dossier de démarrage) | `crates/model/src/settings.rs`, `app/ui/src/options.rs`, `app/src-tauri/src/options_cmds.rs` | moyen |
| **AS** | Rédaction enrichie : lien, image, police/couleur, importance, accusé de lecture (après AM) | `app/ui/src/compose.rs`, `crates/model/src/lib.rs` (`Draft`), `crates/mailbox/src/compose.rs` (hors signatures : après AO) | moyen |
| **AT** | Cœur iTIP/invitations : lire un `text/calendar` (REQUEST/REPLY/CANCEL), construire REQUEST/REPLY, tests ; sans interface | NEW `crates/calendar/src/itip.rs` (+ `mod`), NEW tests | moyen |
| **AU** | Aperçu des pièces jointes (image, texte, PDF si faisable) + glisser vers l'Explorateur | `app/ui/src/reading.rs`, NEW `app/ui/src/attach_preview.rs`, NEW `app/src-tauri/src/attach_cmds.rs` | moyen |
| **AV** | Configuration automatique d'un compte (autoconfig Mozilla/MX/SRV) | `crates/sync/src/lib.rs` (+ NEW `crates/sync/src/autoconfig.rs`), `app/ui/src/accounts.rs` | petit |
| **AW** | Interface des invitations (après AT) : barre Accepter/Refuser dans la lecture, participants dans le rendez-vous | `app/ui/src/appointment.rs`, `app/ui/src/reading.rs` (après AU), NEW `app/src-tauri/src/invite_cmds.rs` | gros |
| **AX** | Contacts : vue Liste/Cartes, « Communiquer » (nouveau message au contact), écriture des listes de distribution | `app/ui/src/contacts.rs`, `crates/contacts/src/distlist.rs`, `crates/contacts/src/write.rs` (après AN) | moyen |
| **AY** | Tâches : % d'avancement, statuts complets | `crates/items/`, `crates/model/src/items.rs`, `app/ui/src/tasks.rs` | petit |
| **AZ** | Fuseaux horaires (remplacer `TimeZone::PARIS` en dur) + jours fériés France + « 7 prochains jours » | `crates/calendar/src/{tz,view}.rs`, `crates/reminders/src/lib.rs`, `app/ui/src/calendar.rs` | gros |
| **BA** | OAuth2 (Gmail / Microsoft 365) | `crates/sync/src/*` (hors `autoconfig.rs`), NEW `crates/sync/src/oauth.rs` | gros |

B2 n'est pas un lot cloud : Paul règle la cible fichier dans MPE (procédure : lot AJ), puis on vérifie l'aller-retour à l'écran.

## Les 10 premiers lots à lancer, dans l'ordre

1. **AN** — fidélité des contacts (supprime le risque de perte à la synchro MPE ; parallélisable).
2. **AM** — autocomplétion + Carnet d'adresses (bloquant au quotidien).
3. **AP** — sauvegarde du courrier local (les messages du PST n'ont aucune copie hors base).
4. **AO** — signatures autonomes (plus de dépendance au dossier d'Outlook).
5. **AQ** — rappels de suivi du courrier.
6. **AR** — options du courrier.
7. **AT** — cœur iTIP (prépare les réunions, sans interface).
8. **AU** — aperçu des pièces jointes.
9. **AS** — rédaction enrichie (après AM et AO).
10. **AV** — configuration automatique des comptes.

Au-delà : **AW** (après AT et AU), **AX** (après AN), **AY**, **AZ**, **BA**.

## Limites de cet audit
- Lu dans le code, jamais lancé : aucune affirmation « fait » n'a été vue à l'écran.
- `crates/exchange/src/import.rs` (`apply`) n'a été parcouru que par mots-clés : le point B3 (écrasement de champs non portés) reste à confirmer.
- Seuls `compose.rs`, `commandbar.rs`, `tasks.rs`, `appointment.rs` et les structures de `model` ont été regardés pour les champs ; un affichage ou filtre absent de ces fichiers pourrait exister ailleurs (`app.rs`, `list.rs`) : recherche par `grep` sur les mots-clés, pas lecture intégrale des 41 900 lignes.
- Ce que fait Outlook au-delà des notes du dépôt est marqué [à vérifier].
