# Échange avec le téléphone par fichiers (MPE) : procédure pour Paul

Option B de `notes/mpe-plan.md`. Légende : **[constaté]** = fait vérifié (plan MPE ou test automatisé de `crates/exchange`) ;
**[supposé]** = à vérifier à l'écran sur le S20 FE. Rien ici n'a été essayé avec MPE : ce qui touche à MPE est supposé
tant que l'étape 5 du plan n'est pas faite.

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
| `calendrier.ics` | oui (iCalendar 2.0) | rendez-vous, rappels, catégories, séries et exceptions | rendez-vous simples seulement |
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

## 6. Écarts connus (tests `#[ignore]`, à lancer avec `cargo test -p thunderlook-exchange -- --ignored`)

| Écart | Effet | Où se corrige |
|---|---|---|
| Série **modifiée** sur le téléphone : non relue (avertissement) | modification perdue côté ThunderLook | `thunderlook-calendar` (écriture d'exceptions importées) |
| Série **créée** sur le téléphone : ignorée | série absente de ThunderLook | idem (RRULE → `Repeat`) |
| Élément modifié dans ThunderLook et resté intact sur le téléphone, alors que le téléphone modifie un autre élément du même fichier : proposé « modifié » (valeur ancienne) | si accepté, écrase la modification faite dans ThunderLook | `crates/exchange` : demande de retenir la valeur exportée de chaque élément dans le manifeste ; non fait ici (changement de format du manifeste, à décider). **Relire la liste avant « Appliquer »** |
| Listes de distribution : non exportées | le téléphone ne les voit pas ; rien n'est perdu dans ThunderLook (lecture seule) | décision de Paul |
| Rappel d'une **tâche** : non écrit dans `taches.ics` | le téléphone ne le voit pas | `thunderlook-items` (`vtodo::export`) |

Hors de ces écarts, tout ce qui précède est vrai des fichiers simulés ; **la façon dont MPE réécrit ces fichiers reste à relever** (plan § 6, étape 5, point 3 et § 8 bis).
