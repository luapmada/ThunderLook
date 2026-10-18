# Formats et sources

Statut : étape 2. Pour chaque besoin, on note la source, la couverture et ce qui reste inconnu.
Règle (voir `notes/juridique.md`) : REA n'intervient que là où ni les specs ni les sources publiques ne suffisent.

## 1. Constats sur tes données (copies, lecture seule)

Outils : `tools/pst_header.py` (en-tête [MS-PST] 2.2.2.6) et `tools/pst_explore.py` (libpff 20260926).

### En-têtes
| Fichiers | `wMagicClient` | `wVer` | `wVerClient` | Codage | Couvert par [MS-PST] ? |
|---|---|---|---|---|---|
| Tous les `.pst` et `.bak` (271 Ko à 15 Go) | `SM` | 23 (Unicode) | 19 | `NDB_CRYPT_PERMUTE` | Oui |
| Les 3 `.ost`, plus une sauvegarde « avant-scanpst » de 16 Mo nommée `.pst` | `SO` | 36 | 12 | `NONE` | **Non** : [MS-PST] impose `SM` et ne décrit pas les pages de 4 Ko |

Aucun PST ANSI, aucun chiffrement CYCLIC ou WIP.

### PST principal (copie de la sauvegarde du 2026-10-15, 3,9 Go)
- Parcours libpff : 43,6 s.
- 73 dossiers, 35 563 éléments, 7 702 pièces jointes. Plus gros dossier : 22 188 éléments.
- 26 éléments illisibles par libpff, 1 orphelin.

Classes de messages :

| Classe | Nombre |
|---|---|
| `IPM.Note` | 28 284 |
| `IPM.Contact` | 6 046 |
| `IPM.Appointment` | 854 |
| `Report.IPM.Note.IPNRN` | 301 |
| `IPM.StickyNote` | 47 |
| `IPM.Remote` | 24 |
| `REPORT.IPM.Note.NDR` | 10 |
| `IPM.Note.Rules.ReplyTemplate.Microsoft` | 6 (modèles de réponse automatique) |
| `IPM.Note.SMIME.MultipartSigned` | 6 |
| `IPM.DistList` | 3 |
| `IPM.Schedule.Meeting.Request` | 2 |
| `IPM.Task`, `IPM.Microsoft.SniffData` | 1 chacune |

Formats de corps :

| Corps | Nombre |
|---|---|
| HTML seul | 19 944 |
| Texte seul | 7 817 |
| HTML + texte | 3 068 |
| RTF compressé (avec ou sans texte) | 633 |
| Aucun (contacts, rendez-vous…) | 4 127 |

- 454 balises de propriétés MAPI distinctes. Types : `PT_UNICODE` en tête, puis `PT_BINARY`, `PT_LONG`, `PT_SYSTIME`, `PT_BOOLEAN`, multivalués…
- Dossiers système vus dans la petite copie :
  - « Paramètres des étapes rapides » : contient les actions rapides ;
  - « Paramètres d'action de conversation » ;
  - `IPM_VIEWS`, `IPM_COMMON_VIEWS` ;
  - « Racine (pour la recherche) », « Rappels », « Recherche de tâche », `ItemProcSearch`, `SPAM Search Folder 2`.

**Limite de libpff** : l'API Python n'expose pas les contenus associés (FAI). Les règles, les actions rapides et les vues enregistrées y vivent. Pour les lire, il faut un lecteur qui accède à la table des contenus associés : `outlook-pst` (Rust) ou notre propre lecteur.

### Sonde `outlook-pst` 1.2.0 (`tools/pst-probe`, même copie)
- **Version publiée du crate** : 6 248 éléments sur 35 589 échouent avec « Missing Sub-Node Tree on message ».
  - Touche les contacts, rendez-vous et notes écrits par Outlook sans sous-arbre de nœuds.
  - 82 FAI sont touchés aussi.
- **Correctif local** (`vendor/outlook-pst`, versionné depuis le J1) : une absence de sous-arbre = pas de table de destinataires ni de pièces jointes.
  - Résultat : 0 erreur, 35 589 éléments ouverts en **1,2 s**.
  - Les comptes par classe sont identiques à libpff, qui échoue sur 26 éléments.
  - Correctif à proposer en amont : licence MIT.
- **Défauts trouvés au J1** (import complet, pièces jointes comprises ; détail dans `vendor/outlook-pst/PATCH.md`) :
  - `PtypObject` (`0x000D`) absent des types reconnus, et lecture des feuilles de BTH arrêtée à la première
    entrée illisible : tout PC contenant un objet était **tronqué sans erreur**. 298 pièces jointes concernées
    sur la copie de 3,9 Go, toutes des messages joints.
  - Messages joints cherchés dans les sous-nœuds du message au lieu de ceux de la pièce jointe, puis
    interblocage sur le verrou du fichier à leur lecture.
  - Pièces jointes de 0 octet (HNID nul) rejetées avec un message d'erreur trompeur.
  - Après correction : 7 702 pièces jointes sur la copie de 3,9 Go, le même nombre que libpff, dont 295 messages joints.
- **106 FAI lus.** Classes rencontrées :

| Classe FAI | Dossier |
|---|---|
| `IPM.RuleOrganizer` (flux `0x6802`, 6 809 octets, commence par `00 00 14 00 00 00 14 06` = « OL2016+ ») | Boîte de réception |
| `IPM.Configuration.CategoryList` (XML `CategoryList.xsd`) | Calendrier |
| `IPM.Configuration.WorkHours` (XML `WorkingHours.xsd`) | Calendrier |
| `IPM.Configuration.Calendar`, `AvailabilityOptions` | Calendrier |
| `IPM.Configuration.Autocomplete` | Boîte de réception |
| `IPM.Configuration.ConversationPrefs`, `TCPrefs`, `TableViewPreviewPrefs`, `RssRule` | Boîte de réception |
| `IPM.Configuration.ContactPrefs` | Contacts |
| `IPM.Microsoft.WunderBar.Link` (64) et `IPM.Microsoft.WunderBar.SFInfo` (2) : favoris et raccourcis de navigation ([MS-OXOCFG] §2.2.9) | `IPM_COMMON_VIEWS` |
| `IPM.Microsoft.FolderDesign.NamedView` : vues enregistrées ([MS-OXOCFG] §2.2.6) | `IPM_VIEWS` et par dossier |
| `IPC.MS.Outlook.AgingProperties` : archivage automatique | Plusieurs dossiers |
| `IPM.MessageManager`, `IPM.Microsoft.ContactLink.TimeStamp`, `IPM.Microsoft.MigrationStatus` | Divers |

- **Actions rapides** : les dossiers « Quick Step Settings » et « Paramètres des étapes rapides » existent mais sont **vides**, contenus comme FAI. Celles visibles dans ton ruban sont probablement dans un des OST (comptes IMAP). Je ne sais pas lequel ; à vérifier.

### Règles exportées `.rwz`
| Fichier | Premiers octets | Interprétation (spec communautaire) |
|---|---|---|
| `regles240913.rwz` | `00 00 14 00 00 00 14 06` | `RW_Magic` « OL2016+ » |
| `regles.rwz` (2010) | `80 4F 12 00 80 4F 12 05` | Variante « XR_Rule alternative 2 », plus ancienne |

### Modèles `.oft`
- Fichiers composés OLE (`D0 CF 11 E0`), c'est-à-dire le format `.msg` de [MS-OXMSG].

## 2. Sources publiques par sujet

| Sujet | Source | Couverture |
|---|---|---|
| Structure PST : NDB, LTP, messagerie, PERMUTE/CYCLIC | [MS-PST](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/) | Complète pour les PST Unicode et ANSI. Rien sur les OST 4 Ko |
| OST 4 Ko compressé (DEFLATE) | libyal/libpff, doc « PFF » ([wiki](https://github.com/libyal/libpff/wiki)) | Tierce partie, lue par libpff depuis 2013 |
| Propriétés MAPI | [MS-OXPROPS] | Référence des balises et noms |
| Messages, pièces jointes | [MS-OXCMSG], [MS-OXOMSG] | Oui |
| Conversion MAPI ↔ MIME (export `.eml`) | [MS-OXCMAIL] | Oui |
| RTF compressé | [MS-OXRTFCP] | Oui |
| HTML ou texte encapsulé dans le RTF | [MS-OXRTFEX] | Oui |
| `winmail.dat` (TNEF) | [MS-OXTNEF] | Oui |
| `.msg` / `.oft` | [MS-OXMSG] | Oui |
| Rendez-vous, périodicité, réunions | [MS-OXOCAL], [MS-OXCICAL] (iCalendar) | Oui |
| Contacts, listes de distribution | [MS-OXOCNTC], [MS-OXVCARD] | Oui |
| Tâches, indicateurs de suivi, notes | [MS-OXOTASK], [MS-OXOFLAG], [MS-OXONOTE] | Oui |
| Catégories, vues (`PidTagViewDescriptorBinary`), favoris (`Wlink`), heures de travail, actions de conversation | [MS-OXOCFG] (TOC vérifiée sur la révision 16.0 du 2025-05-20) | Oui |
| Dossiers de recherche | [MS-OXOSRCH] | Oui |
| Règles serveur (table de règles, FAI `IPM.Rule.Version2.Message`) | [MS-OXORULE] | Oui |
| Règles côté client : `PR_RW_RULES_STREAM` et `.rwz` | Spec communautaire de J. Engelhardt (CC-BY-SA-4.0, 2024-02-25), [grommunio](https://docs.grommunio.com/dev/gromox/outlook_rule_spec/) / [inai.de](https://inai.de/files/outlook_rule_spec.txt), Outlook 2016-2021 | Large. Points marqués inconnus : rôle de la condition `0x64`, bits de `XR_Magic`, codage de longueur sur 3 octets, champ « compte » |
| Actions rapides | Aucune spec. Selon la doc Redemption, du XML dans un message caché du dossier « Paramètres des étapes rapides » ([dimastr.com](https://dimastr.com/redemption/rdoquickstep.htm)) | À observer dans la copie du PST |
| Courrier indésirable (listes) | [MS-OXCSPAM] | Je ne l'ai pas encore vérifié |
| Mail Internet | RFC 5322, 2045-2049, 2047, 2231, 6532 | Normes |
| IMAP / SMTP / POP3 | RFC 9051 (IMAP4rev2), 3501, 5321, 6409, 8314, 1939 | Normes |
| Calendrier / contacts échangés | RFC 5545, 5546, 6047, 6350 | Normes |

Droits sur les Open Specifications : la notice Microsoft autorise à copier la documentation pour réaliser une implémentation. Pour les brevets, elle renvoie à l'Open Specifications Promise ou au Community Promise. Je n'ai pas encore vérifié lequel couvre [MS-PST].

## 3. Bibliothèques existantes (pour le choix de stack)

| Bibliothèque | Langage, licence | Lecture / écriture | Remarques |
|---|---|---|---|
| libpff 20260926 (libyal) | C + Python, LGPL | Lecture | PST, OST, OST 4 Ko compressé. Pas d'accès FAI en Python. 26 éléments illisibles sur 35 563 |
| `outlook-pst` 1.2.0 (github.com/microsoft/outlook-pst-rs) | Rust, MIT | Lecture | Implémentation « clean room » de [MS-PST] par Microsoft. FAI : oui. PST Unicode : oui après correctifs (6, voir `vendor/outlook-pst/PATCH.md`). OST 4 Ko : non testé. Écriture : non |
| `outlook-pst-rw` 1.2.14 | Rust | Lecture, création, ajout (PST Unicode) | Fork tiers, mis à jour le 2026-10-14. Non évalué |
| XstReader | C#, MS-PL | Lecture | Non évalué |

## 4. Points réservés à REA (interopérabilité non documentée)
1. **Champs inconnus du flux de règles** (condition `0x64`, bits `XR_Magic`) dans `OUTLOOK.EXE`. Utile seulement si l'import de tes `.rwz` bute dessus.
2. **OST 4 Ko** (`MSPST32.DLL`) : seulement si libyal ne suffit pas. Priorité P3.
3. **Actions rapides** : d'abord lire les données. REA seulement si le XML ne suffit pas.
