# Migration du S23 : d'Outlook (PST) aux fichiers ThunderLook

Procédure du plan `notes/mpe-plan.md` § 6 (étapes 3 et 6) et de `docs/mpe-fichiers.md`. Rédigée sans accès à MPE, au PC ni au
téléphone : **rien ici n'a été essayé**. Légende : **[constaté]** = relevé dans le plan ou testé automatiquement ;
**[supposé]** = à vérifier à l'écran. Appareil : SM-S916U1 (« S23 », supposé Galaxy S23+ [plan § 1.1]).

Ne pas commencer avant : étape 5 du plan réussie sur le S20 FE (0 doublon, 0 perte), accord de Paul, étape 3 du plan faite
(MPE repointé sur le profil « Outlook »), `pop-original` encore présent.

## 0. Ne JAMAIS faire

- **Ne pas relancer MPE sans prévenir** : `MultiSyncOnStart=1` et `MultiSyncOnAutoConnect=1` lancent une multi-synchro à chaque
  démarrage et à chaque connexion Wi-Fi [constaté, plan § 9, incident du 22/10]. Les couper (étape 2) avant tout démarrage.
- **Ne pas supprimer le profil `pop-original`** avant la fin de la procédure et une semaine d'usage sans écart (critère § 7 étape 6).
- Ne jamais écrire dans `Outlook_300923.pst` depuis ThunderLook (risque R5) ni ouvrir une copie du PST dans un profil (même `RecordKey`).
- Ne pas modifier `manifest.json` à la main ; ne pas exporter entre la synchro MPE et la relecture (docs/mpe-fichiers.md § 3).
- Ne pas passer en bidirectionnel avant la comparaison des effectifs (étape 8).
- Ne jamais lancer la première synchro vers une cible **vide** dans le mauvais sens : « Force fichier → téléphone » avec des fichiers
  vides effacerait le téléphone (risque R4).
- Ne pas migrer pendant que le S20 FE est connectable à ce MPE sans que ses `MultiSync*` soient à 0 (étape 2 du plan [constaté]).

## 1. Sauvegardes, le jour même

Toutes à refaire, même si celles du 22/10 existent (le PST a changé depuis) :

1. MPE fermé (Fichier → Quitter), **sans le relancer**. Copier `%APPDATA%\MyPhoneExplorer` vers
   `A:\AI_related\Claude_Code_Work\mpe-backup\<date du jour>\` ; vérifier le nombre de fichiers et la taille (le 22/10 : 1 032 fichiers,
   133 010 584 octets [constaté]). Contient `Phonebook.dat`, `Organizer.dat`, `Notes.dat`, `Calls.dat`, `Messages.dat` et les `settings.dat`.
2. Outlook fermé : copier `A:\Fichiers Outlook\Outlook_300923.pst` en `…_BACKUP_<date>_S23.pst`, comparer le SHA-256.
3. Export du registre `HKCU\Software\Microsoft\Office\16.0\Outlook\Profiles`.
4. Téléphone : export `.vcf` des contacts du compte « Téléphone » (appli Contacts Samsung ou Smart Switch), gardé sur le PC. Seule copie
   indépendante de MPE [plan § 1.5]. Si Paul ne l'a pas fait : s'arrêter ici.
5. ThunderLook : Fichier › Sauvegardes… › Sauvegarder maintenant.
6. Relever les effectifs de référence : `node tools/mpe_check.mjs "<dossier MPE du S23>" "<dossier d'échange>"` (lecture seule ; avant
   migration, le dossier d'échange peut être vide : le noter).

Retour arrière : rien n'a été modifié à ce stade.

## 2. Couper la synchro automatique

MPE fermé, dans `samsung SM-S916U1 […]\settings.dat` : `MultiSyncOnAutoConnect=0`, `MultiSyncOnStart=0` (méthode utilisée pour le S20 FE
[constaté] ; passer par l'interface est possible, libellé exact non relevé [supposé]). Sur le téléphone, désactiver aussi le Wi-Fi de
l'application MPE Client pour éviter une connexion pendant les réglages [supposé utile].
Retour : restaurer `settings.dat` de l'étape 1.

## 3. Préparer ThunderLook

1. ThunderLook à jour, base sauvegardée (étape 1.5). Fichier › Échange avec le téléphone (Ctrl+Maj+T) : choisir le dossier d'échange de
   production (nom à fixer par Paul ; le banc d'essai est `…\mpe-test\`), export automatique **désactivé**.
2. **Ne pas exporter à ce stade** : la première synchro se fait téléphone → fichiers (étape 5).
Retour : fermer la fenêtre ; aucun fichier n'est écrit.

## 4. Réglages à faire dans MPE (S23 seulement)

Tout est **[supposé]** : le plan ne relève que les clés de `settings.dat` et les libellés de `English.lng`, pas l'écran.

| À changer | Clé / libellé relevé | Remarque |
|---|---|---|
| Cible contacts : fichier vCard | `vCardSync=vCard-File (local or network)` | pointer vers `contacts.vcf` du dossier d'échange |
| Cible calendrier : fichier iCal | `iCalSync=iCal-File (local or network)` | `calendrier.ics` |
| Cible notes : fichier vNote | `vNoteSync=vNote-File (local or network)` | `notes.vnt` |
| Tâches | inconnu | prise en charge du VTODO par MPE non établie (plan § 8 bis, point 2) ; `taches.ics` peut rester sans lecteur |
| Un fichier par type ou un dossier ? | inconnu | MPE doit lire/écrire exactement ces noms (docs/mpe-fichiers.md § 2) |
| Mode contacts / calendrier | `ContactSyncMode`, `EventSyncMode` (0 = bidirectionnel, 1 = forcé PC → téléphone, 2 = forcé téléphone → PC d'après le libellé `SyncModes`) | **1re synchro : forcer téléphone → fichier** ; correspondance indice ↔ libellé supposée par l'ordre [plan § 1.2] |
| Notes | `NoteSyncMode=2` (forcé téléphone → PC) | mode actuel ; une note créée dans ThunderLook peut être écrasée à la synchro [supposé, R8] : décision de Paul avant le passage en bidirectionnel |
| Fenêtres de confirmation | `*Confirmation=1`, `*ConfirmationLimit=1` | **les laisser actives** pendant toute la migration |
| Plage du calendrier | `SyncDaysBefore=90`, `SyncDaysAfter=90` | MPE ne verra que ±90 jours ; ThunderLook exporte tout [supposé] : écart de nombre attendu |
| Multi-synchro | `MultiSync=pb,org,calls,sms,filelists,notes` | retirer `calls`, `sms`, `filelists` n'est pas nécessaire (voir § 8) ; pour la 1re synchro, ne lancer que `pb` puis `org` puis `notes`, un à la fois [supposé prudent] |

Relever à l'écran, avant de continuer : les libellés réels, et ce que MPE écrit à la première synchro (version vCard 2.1/3.0, `UID` ou
`X-OUTLOOK-UID` conservé ou réécrit, encodage, séries) — plan § 8 bis. Retour : restaurer `settings.dat`.

## 5. Première synchro : téléphone → fichiers (manuelle)

1. Prévenir Paul : l'étape démarre MPE (toujours manuel, § 0).
2. Démarrer MPE (maintenant `MultiSyncOnStart=0`), connecter le S23, lancer **une** synchro à la fois, mode « Force phone → fichier » :
   d'abord contacts, puis rendez-vous, puis notes.
3. Lire la fenêtre de confirmation ligne à ligne. Attendu : créations côté fichier (cible vide) ; **aucune suppression sur le téléphone**.
   Toute suppression côté téléphone, ou une direction « fichier → téléphone » → Annuler.
4. Fermer MPE (Fichier → Quitter).
Retour : annuler dans la fenêtre ; sinon restaurer `settings.dat` et les fichiers du dossier d'échange (supprimer ceux écrits),
le PST n'a pas bougé [plan § 6 étape 6].

## 6. Comparer

1. `node tools/mpe_check.mjs "<dossier MPE du S23>" "<dossier d'échange>" --max 20` : lit `Phonebook.dat`, `Organizer.dat`, `Notes.dat` et les
   fichiers d'échange, compte, signale doublons, éléments d'un seul côté et écarts de nombre. Code 0 = rien à signaler, 1 = à examiner,
   2 = erreur. Les identifiants de MPE (EntryID) et de ThunderLook (`p12`…) peuvent différer : le script le dit au lieu de lister des
   milliers d'éléments « d'un seul côté ». Il ne montre jamais de contenu.
2. ThunderLook : « Vérifier les changements », lire la liste (nouveau / modifié / supprimé), cocher, **Appliquer** ; les suppressions
   ne sont jamais cochées d'office [constaté].
3. Effectifs attendus [plan § 7] : contacts 2 139 = 2 139 (Outlook en voyait 2 140 : écart non élucidé, à noter) ; rendez-vous = MPE
   dans la plage ; notes = MPE (46).
Retour : « Annuler le dernier import » ; sinon sauvegarde ThunderLook de l'étape 1.

## 7. Exporter, resynchroniser, bidirectionnel

1. ThunderLook : Exporter maintenant (état de ThunderLook = fichiers).
2. MPE : repasser en **bidirectionnel** (`ContactSyncMode=0`, `EventSyncMode=0`), une synchro manuelle ; fenêtre de confirmation lue :
   attendu 0 action. Sinon Annuler.
3. Refaire § 6 : 0 doublon, mêmes effectifs. Deuxième tour complet de contrôle le lendemain.
4. Noter dans `handoff.md` : Outlook n'est plus la cible de MPE ; PST conservé en l'état.
Retour : restaurer `settings.dat` de l'étape 1 (cible Outlook, EntryID intacts) ; le PST n'a pas bougé.

Critères de réussite [plan § 7, étape 6] : contacts 2 139 = 2 139, rendez-vous dans la plage = MPE, notes = MPE ; **une semaine d'usage
sans écart** (relevé quotidien avec `mpe_check.mjs`). Seulement alors, `pop-original` peut être supprimé et la sauvegarde PST archivée.

## 8. Appels, SMS, journaux, listes de fichiers

- `Calls.dat`, `Messages.dat` et les listes de fichiers sont des fichiers de MPE dans `%APPDATA%\MyPhoneExplorer\<appareil>\` [constaté :
  présence de `Calls.dat` et `Messages.dat` ; `MultiSync` les cite avec `calls`, `sms`, `filelists`]. Rien n'indique qu'ils passent par le PST.
- Ce que la migration **ne change pas** [supposé] : leur stockage, leur synchro (le journal `Debug.txt` ne montre que contacts,
  calendrier, notes comme échangés avec Outlook, plan § 1.4), leur historique, tant que le dossier de l'appareil n'est ni supprimé ni recréé.
- Ce qu'elle **peut changer** [supposé] : rien directement ; le risque est de supprimer ou renommer le dossier de l'appareil, ou
  d'utiliser « Reset sync-history » (libellé `ResetSyncHistory`) : à ne pas faire.
- ThunderLook n'importe ni appels ni SMS [supposé : aucun module de ce lot ; à confirmer dans `docs/architecture.md`]. La sauvegarde du
  dossier MPE (étape 1) est donc la seule copie de ces journaux côté PC.
- Contrôle après migration : `Calls.dat` et `Messages.dat` toujours présents, de taille non décroissante.

## 9. Retour arrière complet

1. MPE fermé. 2. Restaurer `settings.dat` du S23 (ou tout le dossier sauvegardé à l'étape 1). 3. Ne pas toucher au PST (jamais écrit).
4. Supprimer le dossier d'échange et `exchange-map.json` seulement MPE débranché (R4 : sinon doublons). 5. Relancer MPE manuellement,
une synchro avec fenêtre de confirmation : effectifs identiques à ceux relevés à l'étape 1.6.
