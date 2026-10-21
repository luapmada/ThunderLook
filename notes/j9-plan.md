# Plan du J9 : Calendrier

Établi le 2026-10-21. `SPEC.md` §2 : vues Jour, Semaine de travail, Semaine, Mois, Planification ; navigateur de dates ;
rendez-vous, événements d'une journée, périodicité (exceptions comprises), rappels ; réunions iCalendar/iTIP/iMIP ;
calendriers multiples et superposition ; import/export `.ics` et CSV ; catégories.

## 1. Données disponibles
- Déjà importées (J1) dans `data/store`, classe `IPM.Appointment` : 4 198 éléments, avec leur sac MAPI complet.
  - Fichier de données Outlook : Calendrier (320, jusqu'au 10/2026), Calendar Annexe (466), Angèle (11), Jardinage (4).
  - archive : Calendar (3 098, 2010-2019), Alternance Delphine Enfants (6), Jardinage (3).
  - Réunions : 45 demandes, 10 acceptations, 2 annulations.
- `data/samples/031023-Calendrier de Paul Adam  Adam- déclic.ics` (1,1 Mo) et `calendrier-outlook-export-191223.CSV` : critères d'import/export.
- Outlook (profil IMAP depuis le 21/10) : le calendrier de travail reste celui du PST rattaché ; le profil IMAP a aussi
  un « Calendrier (uniquement cet ordinateur) » local (142 éléments, dans l'OST).

## 2. Partage pendant la transition
- Le courrier est partagé par le serveur (IMAP). Pour le calendrier, aucun serveur commun : sondage du 21/10 avec le
  compte de test (PROPFIND sur `mail`, `calendar`, `caldav`, `dav`, `imap`.hostinger.com) : 403 ou renvoi vers le
  webmail ; la documentation trouvée ne mentionne de CalDAV que pour les applications Baïkal et Radicale sur VPS.
- Options :
  1. **Outlook reste la référence du calendrier** pendant la transition ; ThunderLook relit le calendrier depuis une
     copie fraîche du PST (import partiel, calendriers seulement, à la demande ou à l'ouverture). Lecture seule dans
     ThunderLook tant qu'Outlook fait foi.
  2. **Serveur CalDAV personnel** (Radicale ou Baïkal, sur une machine à soi ou un VPS) : partage réel dans les deux
     sens ; Outlook n'a pas de CalDAV natif (module tiers « CalDav Synchronizer »).
  3. **ThunderLook devient la référence** du calendrier dès le J9 ; Outlook ne sert plus pour le calendrier.
- Décision à prendre par Paul avant de coder l'écriture ; la lecture et l'affichage (ci-dessous) ne dépendent pas du choix.

## Décision
- 21/10 : **option A** (Outlook fait foi, ThunderLook en lecture seule pendant la transition).

## Avancement
- Étape 1 faite le 21/10 : crate `crates/calendar` (sac MAPI, propriétés nommées, `PidLidTimeZoneStruct`,
  `PidLidAppointmentRecur` avec exceptions et ExtendedException). `examples/read_all` : 4 198 rendez-vous lus, 0 échec,
  392 séries (351 annuelles, 28 mensuelles, 9 hebdomadaires, 4 quotidiennes), 142 exceptions, 206 occurrences supprimées,
  toutes en fuseau Paris. `examples/crosscheck_ics` contre l'export du 03/10/2023 : 466 événements trouvés par UID
  (`PidLidCleanGlobalObjectId`) sur 472, 466 concordants (début, journée entière, objet, fréquence) ; 50 occurrences
  modifiées sur 54 retrouvées dans leur série. Les 6 UID absents (et les 4 exceptions de leurs séries) sont des
  éléments supprimés ou recréés dans Outlook entre l'export (10/2023) et la copie du PST.
- Prochaine : étape 2 (expansion des périodicités ; critère : mêmes occurrences que le `.ics` sur 2023).

## 3. Ordre proposé
| # | Tâche | Vérification |
|---|---|---|
| 1 | Crate `calendar` : lecture des rendez-vous depuis les sacs MAPI (début, fin, journée entière, lieu, organisateur, participants, rappel, catégories, `PidLidAppointmentRecur` : périodicité et exceptions [MS-OXOCAL]) | Tests sur les 4 198 éléments, 0 échec de lecture ; contrôle croisé avec le `.ics` exporté par Outlook |
| 2 | Expansion des périodicités sur une plage (fuseau, exceptions, suppressions) | Mêmes occurrences que le `.ics` sur 2023 |
| 3 | Vues Jour, Semaine de travail, Semaine, Mois ; navigateur de dates ; superposition des calendriers | `ui_check` (CDP) |
| 4 | Création, modification, suppression, rappels | Tests et `ui_check` |
| 5 | Import/export `.ics` (RFC 5545) et CSV | Aller-retour sur les fichiers d'exemple |
| 6 | Réunions par courrier (iTIP/iMIP) : demandes reçues (accepter, refuser), envoi | Compte de test |
| 7 | Clôture : mesures, version 0.9.0, étiquette, push | — |
