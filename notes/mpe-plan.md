# Plan : synchronisation du smartphone par MyPhoneExplorer (MPE)

Établi le 2026-10-22, en lecture seule : aucune modification de MPE, d'Outlook ni du téléphone ; aucune synchro déclenchée.
Légende : **[constaté]** = lu sur ce PC (fichier, registre, COM en lecture, journal) ; **[documenté]** = source publiée
citée ; **[supposé]** = déduction à vérifier.

## 0. En bref
- MPE 2.3.0 synchronise **contacts, calendrier, tâches et notes du téléphone avec le PST `A:\Fichiers Outlook\Outlook_300923.pst`
  par MAPI (Redemption), en ouvrant le profil `pop-original`**, pas le profil « Outlook » actif depuis le 21/10 16 h 38.
- Les correspondances téléphone ↔ PC sont des **EntryID du PST** (`X-OUTLOOK-UID`) ; elles restent valables quel que soit
  le profil qui ouvre ce PST, mais n'ont aucun sens pour ThunderLook.
- Dernière synchro : 21/10 15 h 36, avant la bascule. Aucune depuis. Les synchros sont **automatiques** à la connexion Wi-Fi.
- Trois dangers immédiats, indépendants de ThunderLook : (1) l'effacement prévu de `pop-original` casse la synchro ;
  (2) dans le profil IMAP, Outlook crée par défaut les nouveaux contacts/rendez-vous/notes dans les dossiers
  « (uniquement cet ordinateur) » de l'OST, que MPE ne voit pas ; (3) le Galaxy S20 FE, s'il se reconnecte avec sa
  configuration actuelle, se synchronise sur le PST de production avec un carnet vieux du 15/10.
- Recommandation : **Outlook (le PST) reste le pivot de MPE** tant que ThunderLook n'est pas la référence ; corriger tout
  de suite le profil MPE. Pour la suite, le seul point d'entrée réaliste pour ThunderLook est la synchro MPE sur
  **fichiers vCard / iCal / vNote** (MPE n'a pas de CalDAV/CardDAV), à valider sur le S20 FE avec des données de test.

## 1. État actuel constaté

### 1.1 Logiciel et emplacements [constaté]
- `C:\Program Files (x86)\MyPhoneExplorer\MyPhoneExplorer.exe` (fichier 2.03, journal « Programmversion: 2.3.0 »),
  processus 42720 lancé le 21/10 14 h 58. Langue `French.lng` (`default.ini`).
- Composants MAPI : `DLL\fjsoftMAPI.dll`, `DLL\fjsoftMAPI64.dll` ; le journal indique « InitApplication cOutlookRedemption »,
  « Session.Logon Redemption Lib v6.6.0.6338 » : MPE parle au profil MAPI par **Redemption (RDOSession)**, pas par le
  modèle objet d'Outlook.
- Complément Outlook VSTO `MyPhoneExplorer_AddIn` enregistré (HKCU, HKLM et WOW6432Node, `LoadBehavior=3`).
- Autres connecteurs livrés : `DLL\MozillaConnector.exe`, `mpe-addin.xpi` (Thunderbird/SeaMonkey), `kWab.dll`
  (Contacts Windows), `DLL\adb.exe` (connexion USB).
- Données : `%APPDATA%\MyPhoneExplorer\`
  - `general.ini` : `AutoConnect=1`, `ConnectionMode=1` (Wi-Fi), `Log=1`,
    `LastUser=samsung SM-S916U1 [6C568FCE-C43F-452D-8649-A69C40379DF6]`.
  - `Debug.txt` : journal de la session en cours (8 665 lignes, depuis le 21/10 14 h 58).
  - Un dossier par appareil :
    - `samsung SM-S916U1 [6C568FCE-…]` : téléphone actuel (Android 36, client MPE 4.13 beta2, Wi-Fi 192.168.1.80,
      opérateur Bouygues). D'après la nomenclature Samsung, SM-S916U1 = Galaxy S23+ **[supposé]**.
    - `samsung SM-G781B [6C568FCE-…]` : ancien appareil (Android 33), dernière synchro le 15/10/2026. SM-G781B =
      Galaxy S20 FE 5G **[supposé]**. C'est vraisemblablement « le S20 » proposé pour les tests (§8, à confirmer).
  - Par appareil : `settings.dat` (réglages), `Phonebook.dat`, `Organizer.dat`, `Notes.dat` (état du téléphone à la
    dernière synchro = historique de synchro), `pb_metadata.xml`, `cal_metadata.xml`, `notes_metadata.xml`,
    `Messages.dat`, `Calls.dat`, plus des sauvegardes `*.bak_2026-08-13`, `-09-11`, `-10-15`, `-10-15_avantcontacts`,
    `-10-16`, `Organizer.dat.bak_2026-10-15_postsync` (faites lors du passage à l'appareil S916U1 les 15-16/10).

### 1.2 Réglages de synchro du téléphone actuel (`samsung SM-S916U1 […]\settings.dat`) [constaté]
```
MultiSync=pb,org,calls,sms,filelists,notes
MultiSyncOnAutoConnect=1
MultiSyncOnStart=1
MultiSyncOnConnect=0
ContactSync=1  CalendarSync=1  NotesSync=1  UsedSyncProtocol=1
[Outlook]
ContactSyncMode=0          EventSyncMode=0          NoteSyncMode=2
ContactConfirmation=1      EventConfirmation=1      NoteConfirmation=1
ContactConfirmationLimit=1 EventConfirmationLimit=1 NoteConfirmationLimit=1
EventConfirmationOutsideTimerangeItems=1
SyncDaysBefore=90  SyncDaysAfter=90  DateMode=0
DeleteCompletedTasks=0  AddOnlyWithNumber=0  ComparePhotosOnSync=0
ContactProfile=pop-original
CalendarProfile=pop-original
NotesProfile=pop-original
TodoFolder=000000007A74605D63138146A2FAB80C64497BA022870000
NoteFolder=000000007A74605D63138146A2FAB80C64497BA022810000
LastContactSync=21102026153605  LastCalendarSync=21102026153612  LastNotesSync=21102026153608
[OutlookContactFolders]
000000007A74605D63138146A2FAB80C64497BA0A2800000
[OutlookCalendarFolders]
000000007A74605D63138146A2FAB80C64497BA002870000
[OutlookCustomCalendarExeptions]   (2 éléments exclus)
[OutlookContactExeptions]          (1 élément exclu)
```
- Sens : la chaîne de langue `SyncModes=Sync both|Force %1 -> phone|Force phone -> %1` (`languages\English.lng`) donne
  la signification des indices : contacts `0` = **bidirectionnel**, calendrier/tâches `0` = **bidirectionnel**, notes `2` =
  **forcé téléphone → Outlook** **[constaté pour le libellé ; correspondance indice ↔ libellé supposée par l'ordre]**.
- Conflits et suppressions : fenêtre de confirmation active pour les trois (`*Confirmation=1`, `*ConfirmationLimit=1`),
  y compris pour les rendez-vous hors plage supprimés. Les choix proposés (`[Sync]` de `English.lng`) : « Use entry from
  phone / from %1 », « Create new entry in … », « Delete entry in … », « Never sync this entry in future ». Il n'y a pas
  de règle automatique « le plus récent gagne » visible dans la configuration.
- Calendrier : plage −90 / +90 jours ; seul le dossier « Calendrier » du PST (NID 0x8702) est synchronisé, pas
  « Calendar Annexe ».
- Le 15-16/10, pour le nouvel appareil, le mode contacts est passé temporairement à `1` (« Outlook → téléphone ») avec
  `MultiSync=pb` (sauvegarde `settings.dat.bak_2026-10-16`), puis remis à `0` : c'est la méthode qui a servi à peupler le
  nouveau téléphone sans doublons.

### 1.3 Cible réelle : le PST, via le profil `pop-original` [constaté]
Registre `HKCU\Software\Microsoft\Office\16.0\Outlook` : `DefaultProfile = Outlook`. Profils :

| Profil | Magasin | Fichier | `PR_RECORD_KEY` (0FF9) | Indicateurs (3009) |
|---|---|---|---|---|
| pop-original | Fichier de données Outlook | `A:\Fichiers Outlook\Outlook_300923.pst` | `7A74605D63138146A2FAB80C64497BA0` | 0x2 = **magasin par défaut** |
| pop-original | contact@angeliquemaillet.art | `A:\Fichiers Outlook\contact@angeliquemaillet.art - paul.pst` | `2055E17D…` | 0x0 |
| Outlook | Fichier de données Outlook | `A:\Fichiers Outlook\Outlook_300923.pst` | `7A74605D63138146A2FAB80C64497BA0` | 0x0 |
| Outlook | paul@adam-declic.Fr | `%LOCALAPPDATA%\Microsoft\Outlook\paul@adam-declic.Fr.ost` | `180F0DA7…` | 0x2 = **magasin par défaut** |
| Outlook | electromagnetisme, angeliquemaillet | `.ost` | — | 0x0 |

- Les identifiants de dossiers de MPE commencent tous par `7A74605D63138146A2FAB80C64497BA0` : c'est la clé du PST
  `Outlook_300923.pst`, la même dans les deux profils. Dans un PST, un EntryID = 4 octets d'indicateurs + l'UID du
  magasin (sa `PidTagRecordKey`) + le NID de l'objet [documenté : [MS-PST] §2.4.3.2 « Mapping between EntryID and NID »,
  https://learn.microsoft.com/openspecs/office_file_formats/ms-pst/]. Les EntryID de MPE restent donc valables si l'on
  ouvre le même PST par le profil « Outlook ».
- Lecture COM (Outlook déjà ouvert, profil « Outlook », `Start-Job` avec délai, lecture seule) :

  | Dossier (EntryID de MPE) | Chemin Outlook | Éléments |
  |---|---|---|
  | `…A2800000` contacts | `\\Fichier de données Outlook\Contacts` | 2 141 |
  | `…02870000` calendrier | `\\Fichier de données Outlook\Calendrier` | 326 |
  | `…22870000` tâches | `\\Fichier de données Outlook\Tâches` | 1 |
  | `…22810000` notes | `\\Fichier de données Outlook\Notes` | 47 |

  Dossiers **par défaut** du profil « Outlook » (`GetDefaultFolder`) : `\\paul@adam-declic.Fr\Calendrier (uniquement cet
  ordinateur)` (142), `Contacts (uniquement cet ordinateur)` (**2 128**), `Tâches (…)` (0), `Notes (…)` (45). Aucun
  élément créé dans ces quatre dossiers depuis la bascule.

### 1.4 Ce que fait une synchro (journal `Debug.txt`, 21/10) [constaté]
```
14:58:10.21  InitApplication cOutlookRedemption
14:58:10.58  Session.Logon Redemption Lib v6.6.0.6338
14:58:10.71  Default store found
14:58:10.73  Try to add (StoreKind=2): contact@angeliquemaillet.art
14:58:10.73  GetAllContacts (cOutlookRedemption) FoldersToSync=000000007A74605D63138146A2FAB80C64497BA0A2800000
14:58:12.16  … Handy: 2139 Einträge, Outlook: 2139 Einträge
14:58:12.29  SyncOperations fertig erstellt, SyncMode=0
14:58:12.29  SyncActs Count=1 AP=0 AO=1 …        (1 contact créé dans Outlook, confirmé à 14:58:18)
14:58:20.32  ReadEventsMapi finished, 369 Items … ReadTodosMapi finished, 1 Items
14:58:21.02  GetAllNotes OutlookProfile=pop-original
14:58:21.15  SyncOperations fertig erstellt, SyncMode=2
15:36:10.74  … Handy: 2139 Einträge, Outlook: 2140 Einträge      SyncActs Count=0
15:36:12.95  … Handy: 370 Einträge, Outlook: 370 Einträge         SyncActs Count=0
15:36:13.71  GetAllNotes (Outlook): 46 Notes                       SyncActs Count=0
```
- Déclenchement : connexion Wi-Fi automatique (notification FCM via `sendpush.php`), puis multi-synchro automatique
  (`MultiSyncOnAutoConnect=1`). Protocole IrMC/OBEX (`telecom/pb|cal|notes|msg/initsync`).
- Aucune connexion TCP du processus MPE vers le téléphone en ce moment ; plus rien depuis 15 h 36.
- Écart en suspens : Outlook 2 140 contacts lus par MPE, téléphone 2 139, sans action proposée (un contact exclu ou
  ignoré ; non élucidé).

### 1.5 Données du téléphone, vues par MPE [constaté]
- `Phonebook.dat` : 2 139 vCard 2.1, **toutes** dans le compte local Samsung `vnd.sec.contact.phone`, chacune avec
  `X-IRMC-LUID` (identifiant côté téléphone) et `OUTLOOK-UID:000000007A74605D…` (EntryID du PST).
- `Organizer.dat` : vCalendar 1.0, 369 VEVENT (tous dans « My calendar@LOCAL ») + 1 VTODO, chacun avec `X-OUTLOOK-UID`.
- `Notes.dat` : 46 VNOTE avec `X-OUTLOOK-UID`.
- Comptes présents sur le téléphone (`pb_metadata.xml`, `cal_metadata.xml`) : Google luapmada@ et paul.laurent.adam@,
  Samsung, e-mail paul@adam-declic.fr, Telegram, Meet ; **MPE ne synchronise que le compte local** (contacts) et
  « My calendar » (calendrier). Les contacts du téléphone n'ont donc **pas d'autre sauvegarde que MPE/le PST**.

### 1.6 Modifications récentes dans le PST [constaté ; cause supposée]
- 160 rendez-vous du dossier Calendrier ont `LastModificationTime = 21/10/2026 16:42`, quatre minutes après le
  rattachement du PST au profil IMAP ; tous ont aujourd'hui `ReminderSet = False`, tous commencent avant le 23/07/2026,
  35 sont périodiques. **[supposé]** : Outlook a traité (ignoré) des rappels échus à la première ouverture dans le nouveau
  profil. Conséquence : la prochaine synchro MPE peut proposer des mises à jour de rendez-vous vers le téléphone
  (séries d'anniversaires, échéances annuelles) ; elles apparaîtront dans la fenêtre de confirmation.

### 1.7 Le S20 et adb [constaté]
- `A:\Android\ADB\adb.exe` 33.0.3 : `adb devices` → aucun appareil (pas de téléphone en USB). Le serveur adb lancé pour
  ce contrôle a été arrêté (`adb kill-server`). Aucun accès au téléphone n'a donc été fait ; tout ce qui précède sur le
  téléphone vient des fichiers de MPE.

## 2. Sources
- Locales : chemins ci-dessus ; `languages\English.lng` (libellés de synchro) ; chaînes de `MyPhoneExplorer.exe` :
  « …zum Kalender ( iCal-Format, Protokolle: ftp://, http://, https:// ) », classes `cWebDAV`, `WebDAVObject`,
  `X-WEBDAV-UID`, `\WEBDAV_TMP\`.
- Site officiel, page d'accueil https://www.fjsoft.at/en/ (lue le 22/10/2026) : « MyPhoneExplorer does support: » Outlook
  (2003 et plus), Thunderbird, SeaMonkey, Lotus Notes, Tobit David, Fritz!Box, fichiers vCard et iCal, base de synchro
  interne (plusieurs téléphones entre eux). Version 2.3.
- Forum officiel (fjsoft.at/forum, pages refusées en 403 à la lecture automatique ; extraits obtenus par la recherche,
  donc à relire par Paul au besoin) : réponse du développeur (octobre 2025) : « MyPhoneExplorer never had a
  implementation of CardDAV and CalDAV. MyPhoneExplorer can sync vCard and iCal Files which are located locally or on a
  server » — https://fjsoft.at/forum/viewtopic.php?p=170417 et https://fjsoft.at/forum/viewtopic.php?p=170406 (un
  utilisateur y signale qu'une URL DAV avec identifiants, valide dans Thunderbird, apparaît vide dans MPE).
- Libellés de MPE sur les cibles fichiers : `iCalSync=iCal-File (local or network)`, `vCardSync=vCard-File (local or
  network)`, `vNoteSync=vNote-File (local or network)`, `NetsharedURL=URL to calendar file (iCal-Format, Protocols: %1)`,
  `SyncDatabaseTitle1=internal Sync-Database`, `SyncOnlyMPE=Only MyPhoneExplorer`, `WindowsContacts=Windows Contacts`,
  `ResetSyncHistory=Reset sync-history`.
- [MS-PST] (EntryID = UID du magasin + NID) : https://learn.microsoft.com/openspecs/office_file_formats/ms-pst/

## 3. Risques

| # | Risque | Constat | Gravité |
|---|---|---|---|
| R1 | **Effacement de `pop-original`** (prévu « 14 jours » après le 21/10, soit vers le 04/11) : MPE ne peut plus ouvrir sa cible ; selon Redemption, échec (« Connection to %1 failed ») ou boîte de choix de profil. | `ContactProfile/CalendarProfile/NotesProfile=pop-original` | Haute, datée |
| R2 | **Créations Outlook invisibles pour MPE** : dans le profil « Outlook », le magasin par défaut est l'OST de paul@ ; nouveau contact (« Ajouter aux contacts »), rendez-vous créé depuis le calendrier par défaut, note ou tâche vont dans « (uniquement cet ordinateur) ». Ils ne vont jamais au téléphone. En sens inverse, la copie de 2 128 contacts dans l'OST crée deux carnets presque identiques. | §1.3 | Haute, déjà active |
| R3 | **S20 FE reconnecté tel quel** : ses réglages visent le même PST, en bidirectionnel, avec multi-synchro automatique et un état du 15/10 ; ce qui a été supprimé sur ce téléphone depuis pourrait être supprimé dans le PST, et inversement. | `samsung SM-G781B […]\settings.dat` | Haute si on « teste » sans préparation |
| R4 | **Changement de cible = synchro sans historique** : les correspondances sont des EntryID du PST. Toute nouvelle cible (fichier, ThunderLook, autre PST, copie du PST) part de zéro → doublons massifs en mode bidirectionnel, ou suppressions si une cible vide est poussée en mode forcé dans le mauvais sens. | `X-OUTLOOK-UID` partout | Critique lors de la migration |
| R5 | **Deux écrivains sur le PST** : ThunderLook ne doit jamais écrire dans `Outlook_300923.pst` (MPE et Outlook y écrivent). Une copie du PST rattachée ailleurs a la même `RecordKey` : MPE pourrait la confondre si elle était ouverte dans un profil. | §1.3 | Haute (à éviter par conception) |
| R6 | **Synchro MPE sous un profil différent de celui d'Outlook ouvert** : Redemption ouvre `pop-original` pendant qu'Outlook tient « Outlook » ; non testé depuis la bascule (dernière synchro 15 h 36, bascule 16 h 38). Le PST est alors ouvert par deux sessions MAPI. | §1.4 | Moyenne, inconnue |
| R7 | 160 rendez-vous modifiés le 21/10 16 h 42 → mises à jour proposées à la prochaine synchro. | §1.6 | Faible si on lit la fenêtre de confirmation |
| R8 | Notes en mode forcé téléphone → Outlook : une note créée côté PC (Outlook ou ThunderLook) peut être écrasée/supprimée par la synchro. **[supposé d'après le libellé]** | `NoteSyncMode=2` | Moyenne |

## 4. Points d'intégration possibles pour ThunderLook

| Option | Principe | Preuves | Limites |
|---|---|---|---|
| **A. Outlook (PST) reste le pivot MPE** | MPE ↔ PST comme aujourd'hui ; ThunderLook relit une copie fraîche du PST (`tl-import --calendars`, puis contacts/notes/tâches au J10). | Fonctionne aujourd'hui (§1.4) ; option A déjà retenue pour le J9. | ThunderLook en lecture seule pour ces données ; Outlook doit rester installé ; créations à faire dans les dossiers du PST (R2). |
| **B. Fichiers vCard / iCal / vNote locaux** | MPE synchronise le téléphone avec `contacts.vcf`, `calendrier.ics`, `notes.vnt` (ou dossier) ; ThunderLook les produit et relit les changements écrits par MPE. | [documenté] page d'accueil et forum ; libellés `vCardSync`, `iCalSync`, `vNoteSync`. ThunderLook sait déjà exporter/importer `.ics` sans écart (`ics_roundtrip`). | ThunderLook devient un partenaire de synchro par fichier : UID stables, relecture des créations/modifications/suppressions faites par MPE, verrouillage pendant la synchro (MPE écrit dans le fichier). Format exact écrit par MPE et prise en charge des tâches à établir par essai. Migration = R4. |
| **C. Fichier servi par HTTP/FTP local** | Même chose que B via une URL (`http://127.0.0.1/...`). | Chaîne « Protokolle: ftp://, http://, https:// » et classe `cWebDAV` dans l'exécutable. | Aucun gain sur B, plus de pannes possibles ; retour du forum : URL DAV vue vide. À écarter sauf besoin de synchro hors du PC. |
| **D. Serveur CalDAV/CardDAV dans ThunderLook** | — | [documenté] forum oct. 2025 : jamais implémenté dans MPE. | **Inutilisable avec MPE.** Utile seulement si Paul remplaçait MPE par un client CalDAV/CardDAV Android (DAVx⁵…) : autre projet, à décider par Paul. |
| **E. Imiter Thunderbird** | ThunderLook répond au connecteur `MozillaConnector.exe` / extension `mpe-addin.xpi`. | Fichiers présents. | Protocole non documenté : rétro-ingénierie, fragile à chaque version de MPE. À écarter. |
| **F. ThunderLook écrit dans le PST ou dans Outlook (COM)** | ThunderLook pousse ses changements dans le PST que MPE lit. | — | Écriture concurrente dans un PST de production (R5), garde-fou du projet. À écarter ; la variante « COM vers Outlook » revient à garder Outlook (option A). |
| **G. Lire les fichiers de MPE** | ThunderLook lit `Phonebook.dat` / `Organizer.dat` / `Notes.dat` (vCard 2.1 / vCalendar 1.0 / VNOTE en clair) comme contrôle ou sauvegarde. | §1.5 | Lecture seule, jamais d'écriture : ce sont l'historique de synchro de MPE. Utile pour vérifier (« le téléphone a N contacts ») et pour une sauvegarde, pas pour synchroniser. |
| H. Base de synchro interne de MPE | MPE garde lui-même la donnée (« Only MyPhoneExplorer »). | Libellés `SyncDatabaseTitle1`, `SyncOnlyMPE`. | Format interne non documenté : ThunderLook ne pourrait que la lire (G). |

## 5. Recommandation
1. **Maintenant (transition, option A)** : Outlook/PST reste la seule cible de MPE. Repointer MPE sur le profil
   « Outlook » (mêmes EntryID, donc historique conservé), garder `pop-original` jusqu'à validation, et créer les
   contacts/rendez-vous/notes dans les dossiers du « Fichier de données Outlook ».
2. **Quand ThunderLook deviendra la référence** (après le J10 Contacts et l'écriture du calendrier) : option **B**
   (fichiers locaux), d'abord validée de bout en bout sur le S20 FE avec des données de test, puis migration du téléphone
   principal par une première synchro **forcée téléphone → fichier** (le téléphone est alors la copie de référence, le
   fichier part vide), contrôle des effectifs, puis passage en bidirectionnel.
3. Écarter C, D, E, F. G sert de filet de contrôle dans toutes les étapes.

## 6. Étapes (réversibles)

### Étape 1 : sauvegardes (avant toute autre action) — Paul ou agent avec accord
- Fermer MPE (Fichier → Quitter), puis copier tout `%APPDATA%\MyPhoneExplorer` vers
  `A:\AI_related\Claude_Code_Work\mpe-backup\2026-10-xx\` (la copie contient `Phonebook.dat` = 2 139 contacts du téléphone
  au format vCard).
- Copier `C:\Program Files (x86)\MyPhoneExplorer\` n'est pas nécessaire (réinstallable) ; noter la version (2.3.0).
- Exporter le registre `HKCU\Software\Microsoft\Office\16.0\Outlook\Profiles` (déjà fait le 21/10 16 h 38 :
  `Outlook_profils_BACKUP_2026-10-21_1638.reg`).
- Outlook fermé : copie du PST (la sauvegarde du 21/10 16 h 38 précède les 160 modifications de 16 h 42 ; en refaire une).
- Côté téléphone : export des contacts du compte « Téléphone » en `.vcf` dans l'appli Contacts de Samsung (ou sauvegarde
  Smart Switch), conservé sur le PC. C'est la seule copie indépendante de MPE.

### Étape 2 : neutraliser le risque S20 FE (R3)
- Ne pas allumer/connecter le S20 FE à MPE tant que son `settings.dat` n'est pas modifié : `MultiSyncOnAutoConnect=0`,
  `MultiSyncOnStart=0`, et cible de test (étape 5). Ou, plus simple, désactiver le Wi-Fi/MPE Client sur ce téléphone.

### Étape 3 : repointer MPE (téléphone principal) sur le profil « Outlook » (R1, R6)
- MPE fermé, dans `samsung SM-S916U1 […]\settings.dat` : `ContactProfile=Outlook`, `CalendarProfile=Outlook`,
  `NotesProfile=Outlook` (ou par l'interface : Fichier → Paramètres → Synchronisation → Outlook ; libellé exact à relever).
  Retour arrière : restaurer `settings.dat` de l'étape 1.
- Synchro **manuelle** suivante, fenêtre de confirmation lue ligne à ligne. Attendu : contacts 0 action, notes 0 action,
  calendrier : éventuellement des mises à jour vers le téléphone (R7), **aucune suppression ni création en masse**.
  Toute suppression ou plus de quelques créations → Annuler, restaurer, analyser.
- Contrôle après coup : `Debug.txt` doit montrer `Session.Logon` puis les mêmes `FoldersToSync` ; effectifs
  téléphone = Outlook (2 139/2 140, 370/370, 46).
- `pop-original` n'est effacé qu'après deux synchros correctes sur « Outlook ».

### Étape 4 : discipline de saisie pendant la transition (R2)
- Dans Outlook, travailler dans `Fichier de données Outlook\Contacts|Calendrier|Notes|Tâches`. Option à valider par
  Paul : masquer ou vider les dossiers « (uniquement cet ordinateur) » — c'est une écriture dans Outlook, donc à faire par
  Paul. Faire de ce PST le magasin par défaut du profil IMAP n'est pas possible sans déplacer la remise du courrier :
  ne pas le faire.
- Contrôle hebdomadaire par COM (lecture) : nombre d'éléments créés depuis la bascule dans les dossiers « (uniquement
  cet ordinateur) » (aujourd'hui 0).

### Étape 5 : prototype « fichiers » sur le S20 FE (option B), sans données réelles
1. S20 FE réinitialisé ou compte local vidé, peuplé de **données de test** (20 contacts, 20 rendez-vous dont périodiques
   et exceptions, 5 notes, 2 tâches) ; aucune donnée réelle.
2. Dans MPE, appareil SM-G781B : cible contacts = vCard-File, calendrier = iCal-File, notes = vNote-File, fichiers dans
   `A:\AI_related\Claude_Code_Work\mpe-test\` ; multi-synchro automatique coupée.
3. Synchros successives en relevant à chaque fois ce qu'écrit MPE (UID, propriétés, encodage, périodicités, tâches).
4. Écrire dans ThunderLook (branche dédiée) l'adaptateur : export vers ces fichiers, relecture des changements MPE,
   détection des suppressions, verrou pendant la synchro.
5. Scénarios : création/modification/suppression de chaque côté, conflit (même élément modifié des deux côtés),
   série modifiée, contact avec photo, 2 000 contacts synthétiques (temps et absence de doublons).

### Étape 6 : migration du téléphone principal (seulement après l'étape 5 réussie et accord de Paul)
- Sauvegardes de l'étape 1 refaites le jour même.
- Première synchro vers les fichiers en mode **« Force phone → fichier »**, puis comparaison ThunderLook ↔ `Phonebook.dat`
  (effectifs et contenus), puis mode bidirectionnel. Outlook n'est plus cible : le noter dans `handoff.md`.
- Retour arrière : restaurer `settings.dat` (cible Outlook, EntryID intacts) ; le PST n'a pas bougé.

## 7. Critères de réussite
- Étape 3 : deux synchros consécutives sur le profil « Outlook » avec 0 suppression, 0 doublon ; effectifs identiques
  des deux côtés ; `pop-original` toujours présent pendant ce temps.
- Étape 5 : sur le S20 FE, 0 doublon et 0 perte après chaque scénario ; un élément supprimé d'un côté l'est de l'autre et
  seulement lui ; périodicités et exceptions identiques (même contrôle d'occurrences que le J9).
- Étape 6 : contacts 2 139 = 2 139, rendez-vous dans la plage = MPE, notes = MPE ; une semaine d'usage sans écart.

## 8. À faire valider par Paul
1. Quel appareil est « le S20 » : le SM-G781B (Galaxy S20 FE, dernière synchro le 15/10) ? Le téléphone principal est-il
   bien le SM-S916U1 (Galaxy S23+) ? Le S20 FE peut-il être vidé pour servir de banc d'essai (étape 5) ?
2. Accord pour l'étape 3 (repointer MPE sur « Outlook ») avant l'effacement de `pop-original`, et report de cet
   effacement après validation.
3. Règle de saisie de l'étape 4 ; sort des dossiers « (uniquement cet ordinateur) » (2 128 contacts, 142 rendez-vous,
   45 notes).
4. Le mode des notes (`NoteSyncMode=2`, forcé téléphone → Outlook) est-il voulu ?
5. Calendrier synchronisé : seul « Calendrier » (pas « Calendar Annexe »), sur ±90 jours : à conserver ?
6. Option B comme cible finale, ou maintien durable d'Outlook comme pivot MPE (option A) ; une alternative hors MPE
   (CalDAV/CardDAV + DAVx⁵) n'est à étudier que sur demande.


## 9. Journal d'exécution
- **2026-10-22, étape 1 (sauvegardes)**, avec l'accord de Paul :
  - MPE fermé proprement ; `%APPDATA%\MyPhoneExplorer` copié dans
    `A:\AI_related\Claude_Code_Work\mpe-backup6-10-22\MyPhoneExplorer` (1 032 fichiers, 133 010 584 octets,
    identiques à la source). Version de MPE lue sur l'exécutable : 2.03.
  - Outlook fermé proprement ; PST copié en `A:\Fichiers Outlook\Outlook_300923_BACKUP_2026-10-22_MPE.pst`
    (4 146 799 616 octets, SHA-256 identique, 21B088F5C8F202BB…). Outlook relancé par `explorer.exe`, MPE relancé.
  - **Reste à faire par Paul** : export `.vcf` des contacts du téléphone principal (appli Contacts Samsung ou Smart
    Switch) — le téléphone n'est pas joignable par adb, et c'est la seule copie indépendante de MPE.
- **2026-10-22, étape 2 (S20 FE neutralisé)** : MPE fermé, dans `samsung SM-G781B […]\settings.dat`,
  `MultiSyncOnAutoConnect` et `MultiSyncOnStart` passés de 1 à 0 (seuls octets changés ; script
  `target/tl-scripts/mpe_s20_noauto.py`). Valeurs conservées après relance de MPE. Le SM-S916U1 n'est pas touché.
  Retour arrière : restaurer ce fichier depuis la sauvegarde de l'étape 1.
- **En attente** : étape 3 (repointer sur « Outlook », synchro manuelle à faire avec Paul devant la fenêtre de
  confirmation) ; étape 5 (banc d'essai S20 FE) : Paul doit confirmer que le S20 FE peut être vidé et le connecter.
