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
- 22/10 : Paul veut **le même calendrier** dans Outlook et ThunderLook → **calendrier commun par Outlook** : Outlook
  garde les rendez-vous (son PST, que MPE synchronise avec le téléphone) ; ThunderLook les lit en direct et y écrit
  par le modèle objet d'Outlook (COM). Outlook doit être ouvert ; fermé, ThunderLook montre la dernière synchro.

## Avancement
- Étape 1 faite le 21/10 : crate `crates/calendar` (sac MAPI, propriétés nommées, `PidLidTimeZoneStruct`,
  `PidLidAppointmentRecur` avec exceptions et ExtendedException). `examples/read_all` : 4 198 rendez-vous lus, 0 échec,
  392 séries (351 annuelles, 28 mensuelles, 9 hebdomadaires, 4 quotidiennes), 142 exceptions, 206 occurrences supprimées,
  toutes en fuseau Paris. `examples/crosscheck_ics` contre l'export du 03/10/2023 : 466 événements trouvés par UID
  (`PidLidCleanGlobalObjectId`) sur 472, 466 concordants (début, journée entière, objet, fréquence) ; 50 occurrences
  modifiées sur 54 retrouvées dans leur série. Les 6 UID absents (et les 4 exceptions de leurs séries) sont des
  éléments supprimés ou recréés dans Outlook entre l'export (10/2023) et la copie du PST.
- Étape 2 faite le 21/10 : `src/expand.rs` (dates locales par motif, retrait des occurrences supprimées, exceptions
  ajoutées, conversion UTC par le fuseau de la série) ; `TimeZone::to_local`. Référence plus sûre que le `.ics` : les
  occurrences calculées par Outlook lui-même (`target/tl-scripts/outlook_occ.ps1`, COM, `IncludeRecurrences`, lecture
  seule) sur le PST rattaché, comparées à `examples/occurrences` (source 1) :
  2023 507 = 507, 2024 148 = 148, 2025 158 = 158 (dont 524 occurrences de séries et 58 exceptions sur les quatre ans) ;
  2026 : 403 communes, 8 écarts, tous des rendez-vous créés ou déplacés après la copie du PST (15/10/2026).
- Étape 3 faite le 21/10 (`f900cad`) : module Calendrier. Vue calculée par le cœur (`crates/calendar/src/view.rs` :
  périodes, titres, bandeau « Journée », colonnes des chevauchements, navigateur à deux mois, jours chargés en gras),
  affichée par `app/ui/src/calendar.rs` ; Ctrl+Alt+1 à 4, Pg.Préc/Pg.Suiv, détail en lecture seule ; calendriers
  affichés dans `calendar.json` (défaut : « Calendrier »). `ui_check` J9 : bandeau et rendez-vous identiques aux
  occurrences d'Outlook (6-12 février et 6 mars 2023, Calendar Annexe superposé).
- Rechargement (option A) fait le 21/10 (`72577ac`) : `tl-import <pst> <données> --calendars <source>` remplace les
  éléments des dossiers de rendez-vous (retrouvés par NID) depuis une copie plus récente du PST ; source 1 rechargée
  depuis la sauvegarde du 21/10 16 h 38 → occurrences identiques à Outlook sur 2023-2026 (410/410 en 2026).
- Étape 4 faite le 22/10 (calendrier commun, crate `crates/outlook`) :
  - Liaison : PST ouvert dans Outlook dont le nom commence celui du fichier d'origine d'une source
    (`Outlook_300923.pst` ↔ source 1) et dont le calendrier par défaut est connu de la source sous le même NID. Les
    807 rendez-vous des 4 calendriers ont les mêmes NID (fin de l'EntryID) dans Outlook et dans la base.
  - Lecture : table d'Outlook (EntryID, date de modification) ; rendez-vous nouveaux ou modifiés enregistrés en `.msg`
    temporaire (`SaveAs`, olMSGUnicode) et lus par `msg.rs` ([MS-OXMSG]) : `PropertyAccessor` refuse la périodicité
    (0x8216) et le fuseau (0x8233) (« L'objet ne prend pas en charge la propriété »). L'export `.msg` date l'élément
    de l'export et omet les identifiants globaux (0x0003, 0x0023) : ceux-là viennent de `PropertyAccessor` (dates en
    UTC). Pour un rendez-vous déjà en base, seules les propriétés lues par le calendrier sont fusionnées dans le sac
    (`bag::merge`). Disparus d'Outlook : retirés de la base. État : `outlook-link.json` du dossier de données
    (dates de modification en millisecondes : un réel ne survit pas à l'aller-retour JSON).
  - Essai à blanc (`examples/calendar_sync -- <données> --dry-run`) : après correction, seuls des écarts réels ;
    occurrences 2023 et 2026 identiques à celles d'Outlook après synchronisation (`outlook_occ.ps1`).
  - Constat : 209 rendez-vous modifiés dans Outlook le 21/10 à 16 h 42 (4 min après la sauvegarde de la bascule de
    profil) ont depuis « pas de rappel » ; cause inconnue ; la sauvegarde `Outlook_300923_BACKUP_2026-10-21_1638.pst`
    a encore leurs rappels. Signalé à Paul le 22/10.
  - Écriture : boîte « Rendez-vous » (Ctrl+N, ruban, double clic, « Ouvrir » du détail) ; « Supprimer… » : le
    rendez-vous, ou pour une série l'occurrence (heure du motif d'Outlook) ou toute la série. Série : heures et
    périodicité restent à modifier dans Outlook. Vérifié dans Outlook (COM) : création, modification, suppression,
    suppression d'une occurrence (exception supprimée) puis de la série (`target/tl-scripts/appt_test.mjs`,
    `outlook_test_series.ps1`, `outlook_series_check.ps1`) ; les éléments de test sont dans les Éléments supprimés.
  - Synchronisation : au démarrage, toutes les 5 min, à l'ouverture du module ; événement `calendar-changed`.
    Mention : « Calendrier commun avec Outlook — synchronisé le … ». Cas `ui_check` J9b (sans écriture).
- Étape 5 faite le 22/10 : `src/ics.rs` (export comme Outlook : VTIMEZONE « Romance Standard Time », RRULE, EXDATE,
  occurrences modifiées en RECURRENCE-ID, VALARM ; import : RRULE → périodicité [MS-OXOCAL], expansion réutilisée),
  `src/csv.rs` (format d'export d'Outlook en français, 22 colonnes), `src/imported.rs` (calendriers importés en lecture
  seule dans `calendars/` du dossier de données, superposables, retirables). Contrôles :
  - `examples/ics_roundtrip` : export puis relecture des 4 calendriers de la source 1, 1 217 occurrences 2023-2026,
    0 écart ; l'export `.ics` d'Outlook (10/2023) importé redonne les 414 occurrences de Calendar Annexe qu'Outlook
    calcule aujourd'hui (7 de plus : éléments supprimés depuis).
  - Série incohérente chez Outlook (« CONTRÔLE TECHNIQUE VOLVO V40 » : OccurrenceCount 3, EndDate à la 2e) : Outlook
    affiche 2 occurrences mais exporte COUNT=3 ; ThunderLook écrit UNTIL pour donner ce qu'Outlook affiche.
  - `examples/csv_check` contre l'export CSV d'Outlook du 19/12/2023 (Calendar Annexe, 01/06/2023-01/06/2024) :
    en-tête identique ; sur 294 lignes communes, objet, dates, journée entière, lieu, confidentialité, participants
    identiques ; écarts restants dus aux modifications faites depuis 2023 (vérifié par COM sur « Decla CAF » et « CFE ») ;
    description : 289/294 (mise en forme du texte RTF par Outlook). Aller-retour de l'exemple : 468 lignes, 0 écart.
  - Disponibilité CSV : libre → 3, occupé → 2 (établi sur l'exemple) ; provisoire 1, absent 4, ailleurs 5 non vérifiés.
  - Interface : ruban « Partager » (Ouvrir un calendrier, Enregistrer le calendrier (.ics), Exporter en CSV — période
    affichée) ; `ui_check` J9 dépose l'export `.ics` d'Outlook dans `calendars/`, vérifie section, superposition et
    rendez-vous du 6 mars 2023, puis le retire par ×.
- Étape 6 (réunions iTIP/iMIP) : **abandonnée le 22/10** à la demande de Paul (« pas besoin »).
- Étape 7 faite le 22/10 : clôture du J9 en version 0.9.0 (`PLAN.md` §4 et §5 « Vérifications du J9 »).

## 3. Ordre proposé
| # | Tâche | Vérification |
|---|---|---|
| 1 | Crate `calendar` : lecture des rendez-vous depuis les sacs MAPI (début, fin, journée entière, lieu, organisateur, participants, rappel, catégories, `PidLidAppointmentRecur` : périodicité et exceptions [MS-OXOCAL]) | Tests sur les 4 198 éléments, 0 échec de lecture ; contrôle croisé avec le `.ics` exporté par Outlook |
| 2 | Expansion des périodicités sur une plage (fuseau, exceptions, suppressions) | Mêmes occurrences que le `.ics` sur 2023 |
| 3 | Vues Jour, Semaine de travail, Semaine, Mois ; navigateur de dates ; superposition des calendriers | `ui_check` (CDP) |
| 4 | Création, modification, suppression, rappels | Tests et `ui_check` |
| 5 | Import/export `.ics` (RFC 5545) et CSV | Aller-retour sur les fichiers d'exemple |
| 6 | ~~Réunions par courrier (iTIP/iMIP) : demandes reçues (accepter, refuser), envoi~~ (abandonnée le 22/10) | Compte de test |
| 7 | Clôture : mesures, version 0.9.0, étiquette, push | — |
