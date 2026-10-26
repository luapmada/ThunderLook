# MPE : cibler des fichiers vCard / iCal / vNote (lot AJ, recherche)

Recherche du 2026-10-24. Légende : **[source : URL]** = lu dans un extrait de recherche ; **[supposé]** = déduction ; **[non trouvé]** = rien de publié trouvé.

**Limite de la recherche** : `fjsoft.at` (site, FAQ, forum) est injoignable en lecture directe depuis le cloud (résolution DNS refusée). Tout ce qui est cité vient d'extraits renvoyés par un moteur de recherche, donc **à relire par Paul sur les pages citées**. **Il n'existe pas, dans ce que j'ai pu atteindre, de documentation officielle décrivant la cible « vCard-File / vNote-File »** [non trouvé]. Seul le cas iCal est décrit (par un message du forum).

## 1. Ce qui est établi

- MPE sait synchroniser contacts et notes avec des fichiers vCard / vNote « locaux ou sur un serveur », et le calendrier avec un fichier iCal. Fonction annoncée pour la version 1.8.8 (2017-03-01) [source : https://www.filehorse.com/download-myphoneexplorer/35129/ (notes de version recopiées, non confirmées sur le journal officiel https://fjsoft.at/en/Versions/)].
- Le développeur (oct. 2025) : « MyPhoneExplorer can sync vCard and iCal Files which are located locally or on a server » ; ni CardDAV ni CalDAV [source : https://fjsoft.at/forum/viewtopic.php?p=170417 ; voir `notes/mpe-plan.md` § 2].
- Libellés présents dans MPE 2.03 : `vCardSync`, `iCalSync`, `vNoteSync` « (local or network) » [constaté sur le PC de Paul, `notes/mpe-plan.md` § 2].
- Une autre cible, « Internal Sync-Database », sert à synchroniser plusieurs téléphones entre eux (menu F2 › Sync › Contacts › Sync with) [source : forum fjsoft, extrait de recherche ; page non identifiée avec certitude].

## 2. Où se règle la cible (calendrier : documenté ; contacts, notes : par analogie)

Calendrier [source : https://fjsoft.at/forum/viewtopic.php?p=20520, message de l'administrateur, extrait de recherche] :
1. MPE › Fichier › Options (**F2**) › **Sync**.
2. Dans la liste déroulante sous **Calendar** : « iCal-File (local or network) ».
3. Bouton **Advanced…** → fenêtre « Settings for iCal-File » : on ajoute un calendrier (titre + « URL to calendar file »).
4. Chemin local sous la forme `file://V:/Mon_Cal.ics` (exemple du forum) ; le chemin doit commencer par `file://`.
5. Si aucun fichier n'existe : créer un fichier texte vide `MonCalendrier.ics` et le désigner [même source].

Contacts (« vCard-File ») et notes (« vNote-File ») : même écran Sync, liste déroulante propre à chaque rubrique, présumée avec un bouton Advanced équivalent **[supposé : aucune page trouvée]**. Nom exact des cases, onglet, chemin de fichier ou de dossier : **[non trouvé]**.

Pour un appareil précis (le S20 FE) : les réglages de synchro sont stockés **par appareil** dans `%APPDATA%\MyPhoneExplorer\<appareil>\settings.dat` [constaté, `notes/mpe-plan.md` § 1.1–1.2]. Il faut donc être connecté au S20 FE (SM-G781B) quand on change la cible **[supposé : à vérifier à l'écran que F2 › Sync n'agit que sur l'appareil courant]**.

## 3. Fichier unique ou un fichier par fiche ?

- Pour le calendrier : **un fichier `.ics` unique** (un seul chemin) [source : p=20520].
- Contacts / notes : **[non trouvé]**. Indice : l'*export* manuel de MPE écrit **un `.vcf` par contact** quand on sélectionne des contacts, et un fichier multi-contacts pose problème à Outlook [source : https://www.fjsoft.at/forum/viewtopic.php?t=15166 et https://www.fjsoft.at/forum/viewtopic.php?t=36435, extraits de recherche]. Ce n'est pas la synchro, ne pas en déduire son comportement.
- Le forum CardBook (dossier de `.vcf`) : un utilisateur n'a pas su faire synchroniser un dossier CardBook via « vCard-File » et ne trouvait pas de `.vcf` local [source : https://fjsoft.at/forum/viewtopic.php?p=167341, extrait]. Prise en charge d'un **dossier** : **[non trouvé]**.

## 4. Ce que « Synchroniser » écrit, suppressions, conflits, confirmation

- Rubriques : contacts, calendrier, notes, selon les trois listes ; tâches : **[non trouvé]** pour les fichiers.
- Sens : le menu de modes de MPE propose « Sync both / Force X → phone / Force phone → X » (libellé de `English.lng`) [constaté, plan § 1.2]. Valable pour la cible fichier **[supposé]**.
- Suppressions, conflits : la fenêtre de confirmation de MPE (« Use entry from phone / from … », « Delete entry in … », « Never sync this entry in future ») existe pour Outlook [constaté, plan § 1.2]. Pour les fichiers : **[non trouvé]** ; **[supposé]** identique, car le moteur de synchro est commun.
- Un message du forum indique qu'un calendrier Google lu via lien ICS privé est **en lecture seule** (pas d'édition ni suppression côté source) [source : forum fjsoft, extrait ; ancien]. Vaut pour une URL, pas forcément pour un fichier local **[non trouvé]**.

## 5. Formats réellement écrits

**[non trouvé]** : version vCard, encodage, `UID`, `X-IRMC-LUID`, vCalendar 1.0 ou iCalendar 2.0 pour la cible fichier, VNOTE, noms de fichiers : aucune source publiée.
Seul fait établi (fichiers d'état de MPE, pas la cible fichier) : `Phonebook.dat` = vCard 2.1 avec `X-IRMC-LUID` ; `Organizer.dat` = vCalendar 1.0 ; `Notes.dat` = VNOTE [constaté, plan § 1.5]. La liste déroulante dit « iCal-File » : **[supposé]** iCalendar 2.0. À relever par essai (§ 7).

## 6. Limites connues rapportées

- Les réglages de la fenêtre iCal ne persistaient pas après OK dans la version 1.6.5 : un seul rapport, ancien [source : forum fjsoft, extrait].
- Le calendrier natif de Windows 8.1 n'est pas géré (sans objet ici) [source : https://fjsoft.at/forum/viewtopic.php?p=99866].
- CardBook : MPE signale un carnet vide comme une erreur (mettre un contact factice) ; sauts de ligne `\n` parasites dans les adresses après une mise à jour [source : https://fjsoft.at/forum/viewtopic.php?p=167341]. Concerne le connecteur Thunderbird, pas les fichiers ; à garder en tête pour les adresses multilignes.
- Doublons, photos, séries, fuseaux avec la cible fichier : **[non trouvé]**. Changer de cible repart sans historique (risque R4, plan § 3) [constaté].

## 7. Procédure pour Paul (à faire sur le banc d'essai, jamais sur le téléphone principal)

À blanc, rien n'est lancé tant que l'étape 1 du plan n'est pas faite (sauvegardes de `settings.dat`, etc.).

1. Copier `%APPDATA%\MyPhoneExplorer\samsung SM-G781B […]\settings.dat` en sauvegarde. Couper la multi-synchro automatique de cet appareil.
2. Créer dans `A:\AI_related\Claude_Code_Work\mpe-test\` trois fichiers vides : `contacts.vcf`, `calendrier.ics`, `notes.vnt`.
3. Connecter **le S20 FE seulement** (vérifier le nom d'appareil dans la barre de MPE).
4. **F2 › Sync** : Contacts › « Sync with » = **vCard-File (local or network)** ; Calendrier = **iCal-File (local or network)** ; Notes = **vNote-File (local or network)**. Pour chacun : **Advanced…**, indiquer `file://A:/AI_related/Claude_Code_Work/mpe-test/<fichier>` (forme `file://` documentée pour iCal ; à confirmer pour les deux autres).
5. **Première synchro en mode non destructif** : relever à l'écran le mode proposé et lire la fenêtre de confirmation avant d'accepter. Ne rien accepter qui supprime.
6. Après synchro, **copier les trois fichiers** et noter : version (`VERSION:` de la vCard), `UID` / `X-IRMC-LUID`, `BEGIN:VCALENDAR`/`VERSION:`, encodage, un fichier ou plusieurs, noms. Reporter ces relevés dans ce document (§ 5), à la place des « [non trouvé] ».
7. Retour arrière : restaurer le `settings.dat` sauvegardé (cible Outlook), MPE fermé.

## 8. Pistes si la recherche reste muette

- Demander à l'éditeur sur le forum officiel (https://www.fjsoft.at/forum/), question : format et fichier unique/dossier de la cible « vCard-File ».
- Observer MPE en essai (§ 7) : c'est la seule source fiable pour § 5.

## 9. Relevé sur le S20 FE (SM-G781B), 2026-10-25 [constaté dans settings.dat]
Après réglage dans MPE (F2 › Sync, cible « fichier » pour contacts, calendrier, notes) :
- `ContactSync=7`, `CalendarSync=7`, `NotesSync=7` (1 = Outlook, 7 = cible fichier).
- Nouvelle section `[WebDAV]` : `vCardUrl=file://A:/…/contacts.vcf`, `vCardVersion=2`, `IcsURL_1=file://A:/…/calendrier.ics`, `Title_1`, `ReadOnly_1=0`, `vNoteUrl=file://A:/…/notes.vnt`.
- `ContactSyncMode`, `EventSyncMode`, `NoteSyncMode` = 0 (bidirectionnel) ; les confirmations (`*Confirmation=1`) restent actives.
- Sauvegarde avant réglage : `A:\AI_related\Claude_Code_Work\mpe-backup\2026-10-25\settings_S20.dat` (retour arrière : MPE fermé, remettre ce fichier).
- Format écrit par MPE dans les trois fichiers : voir § 10 (relevé statistique du 2026-10-26, S23).

## 10. Format réellement écrit par MPE 2.3.0 (relevé statistique sur le S23, 2026-10-26, aucun contenu lu)
Dossier `A:\AI_related\Claude_Code_Work\mpe-s23\`, écrit toutes les heures. **Remplace les « [non trouvé] » du § 5.**
- `contacts.vcf` : **vCard 2.1**, 2 140 fiches. Chaque fiche : `UID` (**GUID aléatoire de MPE, pas un identifiant Outlook**), `REV`, `X-ACCOUNT:vnd.sec.contact.phone`, `N` et `FN` (parfois `;CHARSET=UTF-8`), `CATEGORIES`, `EMAIL;OTHER` (majoritaire) ou `EMAIL;HOME/WORK`, `TEL;CELL/HOME/WORK/MAIN/FAX;WORK` ou `TEL` sans type, `ADR;WORK` ou `HOME` (parfois `ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8`), `ORG`, `TITLE`, `NOTE` (QP et/ou `CHARSET=UTF-8`), `URL`, `BDAY`, `X-ANNIVERSARY`, `PHOTO;ENCODING=BASE64;TYPE=JPEG`.
- `calendrier.ics` : **iCalendar 2.0**, `PRODID:-//FJ Software//MyPhoneExplorer 2.3.0//EN`, 372 `VEVENT` dont 61 avec `RRULE`, des `EXDATE`, `DTSTART;VALUE=DATE` pour les journées entières, `DTSTART;TZID=Romance` pour les autres (fuseau Windows « Romance Standard Time » tronqué).
- `notes.vnt` : format non relevé.
- Pas de `manifest.json`, pas de `taches.ics`. Le dossier est relu par la synchronisation automatique de ThunderLook (`docs/mpe-fichiers.md` § 0, `crates/exchange/tests/mpe_auto.rs`).
- Pris en charge par ThunderLook : tout ce qui précède (testé sur des fichiers reconstitués d'après ce relevé). `TZID=Romance` était **refusé** pour les séries avant le lot BK (corrigé : reconnu comme Paris).
