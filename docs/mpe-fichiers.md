# Échange avec le téléphone par fichiers (MPE) : procédure pour Paul

Option B de `notes/mpe-plan.md`. Légende : **[constaté]** = fait vérifié (plan MPE ou test automatisé de `crates/exchange`) ;
**[supposé]** = à vérifier à l'écran sur le S20 FE. Rien ici n'a été essayé avec MPE : ce qui touche à MPE est supposé
tant que l'étape 5 du plan n'est pas faite.

## 0. Dossier écrit par MPE : relecture automatique (lot BK)

Cas du S23 de Paul : MPE (cibles « fichier », déjà réglées) écrit toutes les heures dans `A:\AI_related\Claude_Code_Work\mpe-s23\` les fichiers `contacts.vcf`, `calendrier.ics`, `notes.vnt`. ThunderLook les relit **sans intervention** et reste à jour. Mode « fichiers seuls » (dossier sans `manifest.json`) :

- **Mise en route (une fois)** : Fichier › Échange avec le téléphone › choisir le dossier `mpe-s23`. La relecture automatique est active d'office (toutes les 15 min, 5 à 120). Ne **jamais** y lancer « Exporter maintenant » : refusé (message), et l'export programmé ignore ce dossier. ThunderLook n'y écrit rien (ni export, ni manifeste).
- **Première lecture** : 2 140 fiches et 372 rendez-vous d'un coup dépassent le seuil de sécurité (5 % ou 50 éléments par module) : le passage est **bloqué, rien n'est appliqué**, et la zone « Synchronisation automatique » propose **« Appliquer malgré le seuil… »** (une confirmation). Instantané annulable (« Annuler le dernier passage automatique »). Un contact déjà dans ThunderLook est reconnu par nom + société + e-mail ; s'il diffère de la fiche du téléphone, c'est un **conflit à part** (jamais appliqué seul, à trancher dans « Vérifier les changements ») ; sinon il est créé : **si MPE connaît quelqu'un sous un autre nom ou e-mail que dans ThunderLook, la première lecture crée un doublon** (outil de dédoublonnage de ThunderLook ensuite).
- **Ensuite, à chaque passage** : création ou modification faite sur le téléphone → appliquée (rendez-vous, séries, contacts, notes). Suppression sur le téléphone, conflit (modifié des deux côtés), série refusée → **mis de côté et comptés** (barre d'état), jamais appliqués seuls. Un élément supprimé dans ThunderLook n'est **pas ressuscité** par le fichier. Une modification faite seulement dans ThunderLook n'est pas écrasée (et n'est pas renvoyée au téléphone : le sens ThunderLook → téléphone n'existe pas dans ce mode).
- **Comment ThunderLook sait ce qui a changé** (pas de manifeste) : sa propre base `mpe-baseline.json` (dossier de données) retient, par `UID` de MPE, l'empreinte de la valeur lue la dernière fois qu'elle a été réglée, et `exchange-map.json` l'élément local correspondant. Un passage dont les fichiers sont **identiques** au précédent (signature SHA-256) ne relit rien et n'est pas journalisé (« vérifié HH:MM » dans la barre d'état ; « Relire maintenant » relit quand même).
- **Garde-fous** : fichiers lus seulement s'ils sont stables (aucun octet changé en 5 s, plus vieux que 10 s) ; fichier **vide ou tronqué** (dernière ligne ≠ `END:VCARD` / `END:VCALENDAR` / `END:VNOTE`) : ignoré avec le motif ; seuil de sécurité ; annulation du dernier passage (un élément annulé n'est pas recréé au passage suivant).
- **Champs non repris** (comptés à part, « champ(s) de fiche non repris », rien à traiter) : `TEL;FAX`, deuxième téléphone du même type, téléphone sans type (rangé dans la première case libre), `X-ANNIVERSARY`, photos (jamais reprises par l'échange).
- **Fuseau `TZID=Romance`** (nom Windows tronqué écrit par MPE 2.3.0) : reconnu comme l'heure de Paris, y compris pour les séries et `EXDATE`.
- **Limites connues** : `notes.vnt` — le format réel de MPE n'est pas relevé ; une note sans `X-OUTLOOK-UID` n'a pas d'identifiant stable : retrouvée par son texte, donc **une note modifiée sur le téléphone apparaît comme une nouvelle note** et l'ancienne reste. Séries refusées (voir § 6) : mensuelles/annuelles aux jours 29 à 31, fréquence horaire ou à la minute, `RDATE`, etc. — comptées « séries refusées ». `taches.ics` : MPE n'en écrit pas, rien à relire.

## 1. Dossier

- Banc d'essai : `A:\AI_related\Claude_Code_Work\mpe-test\` [plan MPE, étape 5, point 2]. Données de test seulement, jamais le téléphone principal.
- Dans ThunderLook : **Fichier › Échange avec le téléphone** (Ctrl+Maj+T) › choisir ce dossier. Export automatique : laisser **désactivé** pendant les essais.
- Dans MPE (appareil SM-G781B uniquement) : contacts = vCard-File, calendrier = iCal-File, notes = vNote-File, pointant vers ces fichiers ; multi-synchro automatique **coupée** [plan]. Le réglage exact de MPE (une cible par fichier ou par dossier ?) n'est pas connu [supposé].

## 2. Fichiers

| Fichier | Écrit par ThunderLook | Contenu | Relu par ThunderLook |
|---|---|---|---|
| `contacts.vcf` | oui (vCard 3.0, UTF-8) | contacts, avec photo JPEG et catégories | oui |
| `taches.ics` | oui (iCalendar 2.0, VTODO) | tâches (sans rappel, voir § 6) | oui |
| `notes.vnt` | oui (vNote 1.1, texte en quoted-printable UTF-8) | notes (sans couleur) | oui |
| `calendrier.ics` | oui (iCalendar 2.0) | rendez-vous, rappels, catégories, séries et exceptions | oui, séries comprises (RRULE, EXDATE, RECURRENCE-ID) ; ce qui n'est pas représentable est refusé avec un avertissement (§ 6) |
| `manifest.json` | oui | date, nombre d'éléments, SHA-256 et identifiants de chaque fichier | oui |

- Le téléphone ne doit modifier que les quatre premiers ; **jamais `manifest.json`** (c'est lui qui permet de savoir ce que le téléphone a changé).
- MPE lit-il et écrit-il bien ces noms-là, et dans quel format (vCard 2.1 ou 3.0, `UID` ou `X-OUTLOOK-UID`) ? **[supposé]** : à relever à la première synchro. ThunderLook sait lire du vCard 2.1/3.0, du vNote en QP et du Windows-1252.
- Ce que fait ThunderLook de l'identifiant de MPE [constaté, tests] : à la première application, il retient l'identifiant donné par le téléphone (`exchange-map.json`, dossier de données) et le **réécrit tel quel** aux exports suivants ; MPE ne voit donc pas un « nouvel » élément à chaque export.

## 3. Ordre des opérations (toujours le même)

1. **Sauvegarder** : Fichier › Sauvegardes… › Sauvegarder maintenant (et, avant l'étape 6 du plan, les sauvegardes de l'étape 1).
2. **ThunderLook : Exporter maintenant.** Le dossier contient l'état de ThunderLook.
3. **MPE : synchroniser** (le téléphone lit/écrit les fichiers). Première synchro : mode choisi avec soin (plan § 6 étape 6 : « Force phone → fichier » seulement pour la migration, avec accord).
4. **ThunderLook : Vérifier les changements.** Lire la liste : nouveau / modifié / supprimé.
5. Cocher, puis **Appliquer**. Les suppressions ne sont **jamais cochées d'office** ; une suppression est proposée seulement pour un élément qui figurait dans le dernier export.
6. **Exporter maintenant** de nouveau, puis resynchroniser MPE. Ne jamais exporter entre l'étape 3 et l'étape 5 : l'export écraserait les modifications du téléphone non relues (ThunderLook prévient, et l'export automatique s'arrête de lui-même).
7. Pendant la synchro de MPE, ne pas lancer d'export ni d'import (MPE écrit dans les fichiers ; il n'y a pas de verrou) **[supposé : à confirmer, le plan prévoit un « verrou pendant la synchro » non fait]**.

## 4. Retour arrière

- **Dernier import appliqué** : « Annuler le dernier import » ; le détail est dans `data\exchange-undo\<horodatage>.json`. Un élément supprimé puis rétabli reprend un nouvel identifiant local.
- **Plusieurs imports** : sauvegardes de ThunderLook (Fichier › Sauvegardes… › Fusionner les éléments manquants, ou Tout remettre).
- **Côté téléphone / MPE** : restaurer le `settings.dat` de l'étape 1 du plan (cible Outlook, EntryID intacts) ; le PST n'a jamais été touché [plan § 6, étape 6].
- **Fichiers d'échange** : supprimer le dossier d'échange et `exchange-map.json` (dossier de données) revient à « jamais échangé » ; le prochain échange repart sans historique (risque R4 du plan : doublons si MPE est en bidirectionnel). Ne le faire que MPE débranché.

## 5. Ce que le banc d'essai a constaté (`cargo test -p thunderlook-exchange`)

Constaté par tests automatisés sur des fichiers simulés (pas par MPE) :

- Trois aller-retours complets (export → réécriture du fichier par le « téléphone » → import → export) : **0 changement proposé, 0 doublon, 0 perte** (contacts, tâches, notes, rendez-vous, séries et exceptions, photo, catégories, rappels des rendez-vous).
- Élément modifié sur le téléphone : 1 changement par élément, rien d'autre ; photo, catégories et rappel conservés.
- Élément supprimé sur le téléphone : proposé « supprimé », jamais d'office ; accepté, seul cet élément disparaît ; annulable.
- Élément supprimé dans ThunderLook : le téléphone qui le garde ne le ressuscite pas (corrigé pendant ce lot, y compris pour les rendez-vous et les éléments nés sur le téléphone).
- Élément créé sur le téléphone avec son propre identifiant : créé une fois, reconnu ensuite, identifiant conservé à l'export (corrigé pendant ce lot).
- Accents, œ/ñ, « : ; , » dans les textes, lignes longues repliées (aucune ligne de plus de 76 octets, espace de pliage compris), sauts de ligne dans notes, adresses et descriptions.
- Une fiche sans photo envoyée par le téléphone n'efface pas la photo de ThunderLook.
- 2 000 contacts synthétiques : 0 changement, 0 doublon.

## 5 bis. Conflits (modifié dans ThunderLook ET sur le téléphone)

Le manifeste retient, pour chaque élément exporté, l'empreinte SHA-256 de sa valeur exportée (`values`). À l'import, pour un élément dont le fichier diffère de la base :

| Fichier vs valeur exportée | ThunderLook vs valeur exportée | Résultat |
|---|---|---|
| identique | (modifié ou non) | **rien proposé** : le téléphone n'a pas touché l'élément, la valeur de ThunderLook fait foi |
| différent | inchangé | **modification du téléphone**, proposée et appliquée comme avant |
| différent | modifié | **conflit** : rendu à part (`ImportReport.conflicts`), jamais coché, jamais appliqué sans choix |

- Choix d'un conflit (`Change.resolution`) : **garder ThunderLook** (rien n'est écrit ; le prochain export renvoie la valeur au téléphone) ou **garder le téléphone** (appliqué comme une modification, annulable ; la sauvegarde/corbeille interne reste le filet).
- Manifeste d'avant ce format (pas de `values`) : valeur exportée **inconnue** → tout écart du fichier est proposé en conflit d'origine « inconnue », jamais appliqué d'office. Le prochain export écrit les empreintes.
- Constaté par tests automatisés (`scenarios.rs`, section « Conflits »). Non essayé avec MPE.

## 6. Écarts connus (tests `#[ignore]`, à lancer avec `cargo test -p thunderlook-exchange -- --ignored`)

| Écart | Effet | Où se corrige |
|---|---|---|
| Série **modifiée** / **créée** sur le téléphone (lot AA) | relue : remplacée (même identifiant) en gardant catégories et rappel que le téléphone n'envoie pas ; nouvelle série créée ; série retirée du fichier = suppression proposée (jamais cochée) | **Refusées avec un avertissement visible (rien n'est importé de travers)** : fréquence autre que jour/semaine/mois/an, `BYxxx` hors `BYDAY`/`BYMONTHDAY`/`BYSETPOS`/`BYMONTH`, `BYDAY` sans rang en mensuel, `RDATE`, « cette occurrence et les suivantes », fuseau hors heure de Paris, heure fixe en UTC qui traverse un changement d'heure, occurrence modifiée avec une autre description/catégorie/confidentialité. **Cas inexacts** (jour 29-31 du mois, 29 février annuel : la RFC saute les périodes trop courtes, ThunderLook tombe sur le dernier jour) : refusés sauf s'ils sont identiques à ce que ThunderLook a exporté |
| Listes de distribution : non exportées | le téléphone ne les voit pas ; rien n'est perdu dans ThunderLook (lecture seule) | décision de Paul |
| Rappel d'une **tâche** : non écrit dans `taches.ics` | le téléphone ne le voit pas | `thunderlook-items` (`vtodo::export`) |

Hors de ces écarts, tout ce qui précède est vrai des fichiers simulés ; **la façon dont MPE réécrit ces fichiers reste à relever** (plan § 6, étape 5, point 3 et § 8 bis).
