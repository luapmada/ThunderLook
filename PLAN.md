# PLAN — architecture, stack, jalons

Statut : **validé le 2026-10-16** (§7 accepté tel quel : ruban simplifié par défaut avec mode classique, nom « ThunderLook », REA MCP repoussé à J6). **Modifié le même jour à ta demande (« plus il y a de Rust mieux c'est »)** : interface en Rust (Leptos → WebAssembly) au lieu de React/TypeScript, outillage cargo au lieu de npm. J0 terminé. J1 terminé le 2026-10-18, J2, J3 (organisation, sans les conversations) et J4 (recherche) le 2026-10-19 (mesures en §5). Lien avec ADI Facturation : §8.
Entrées : `SPEC.md` (priorités Q1 à Q4), `DESIGN.md`, `notes/ui-reference.md`, `notes/formats.md`, `notes/juridique.md`.

## 1. Principes
- 100 % local. Aucune télémétrie, aucun appel réseau hors des comptes de messagerie (jalon IMAP).
- Les PST sources ne sont **jamais ouverts en écriture**.
  - L'import travaille sur une copie faite par l'application dans son dossier de données.
  - Il refuse un chemin situé sous `A:\Fichiers Outlook`, sauf pour copier.
- Le stockage du clone est propre (décision Q2) : SQLite, plus un `.eml` par message. On n'écrit pas de PST.
- Import fidèle : toute propriété MAPI non convertie est conservée telle quelle, pour pouvoir la reprendre plus tard sans réimporter.
- Interface : structure, parcours et raccourcis d'Outlook 2021. Habillage d'après `DESIGN.md`. Libellés tirés de notre propre glossaire.

## 2. Stack proposée

| Couche | Choix | Pourquoi (mesures et sources) |
|---|---|---|
| Application | **Tauri 2** : cœur Rust, interface dans WebView2 | WebView2 155 déjà présent. Binaire léger (8,4 Mo au J0) |
| Lecture PST | **`outlook-pst`** (Microsoft, MIT, implémentation de [MS-PST]), avec notre correctif « sous-arbre absent » proposé en amont | Mesuré : 35 589 éléments en 1,2 s, 217 257 éléments (15 Go) en 54 s, 0 erreur, FAI lisibles. libpff : 43,6 s et 26 échecs, sans FAI |
| MIME | `mail-parser` / `mail-builder` (Apache-2.0/MIT) | Conversion MAPI → `.eml` selon [MS-OXCMAIL] |
| RTF | Décompression [MS-OXRTFCP] (crate `compressed-rtf`, licence à vérifier, sinon ~200 lignes) et dé-encapsulation [MS-OXRTFEX] | 633 messages RTF dans ton PST principal |
| Base | SQLite (`rusqlite`, embarqué) + FTS5 | Index et recherche locale (pas Windows Search) |
| Interface | **Leptos 0.8** (Rust compilé en WebAssembly, rendu client), construit par **Trunk** ; icônes Lucide via `icondata_lu` (MIT/ISC) ; police Inter (OFL) embarquée | Même langage et **mêmes types** (`crates/model`) que le cœur. Pas de Node. Liste virtualisée à écrire en Rust (J2). Éditeur HTML riche (J5) : seule exception JS envisagée si aucun équivalent Rust n'est viable |
| Affichage des mails | `iframe` en bac à sable (`srcdoc`, sans scripts), CSP qui bloque le contenu distant par défaut | Même comportement qu'Outlook : images distantes bloquées, barre d'information |
| IMAP / SMTP (jalon 7) | `async-imap` ou `imap-next`, `lettre` | Moins mûrs que MailKit (.NET). Risque accepté, car hors du premier jalon |

**Alternative écartée** : .NET 8 WPF + WebView2 + MimeKit/MailKit.
- Avantages : SDK déjà installé, excellentes bibliothèques mail.
- Rejetée car :
  - aucun lecteur PST C# ne lit les FAI de façon éprouvée ;
  - il faudrait WebView2 de toute façon pour l'affichage et l'édition HTML, d'où deux technologies d'interface.

**Outillage installé** :
- Rust 1.99.1 dans `%USERPROFILE%\.cargo` (ajouté au PATH utilisateur), cible `wasm32-unknown-unknown`, `trunk` 0.21.14, `cargo-tauri` 2.12.3 ;
- MSVC 2019 Build Tools et SDK Windows 10.0.19041 déjà présents ;
- Node 24.

## 3. Architecture

```
app/                      Application Tauri (index.html + Trunk.toml pour l'interface)
  src-tauri/              Cœur : commandes IPC, fenêtres (principale, message, rédaction), réglages
  ui/                     Interface Leptos (crate thunderlook-ui → WebAssembly)
    src/                  app (racine, raccourcis, volets), commandbar (registre des commandes, modes de barre),
                          folders (arbre + navigation basse), list, reading, statusbar, splitter, menu, ipc
    styles/               Jetons DESIGN.md (clair/sombre, densité), mise en page ; assets/ (police Inter)
crates/
  model/                  Types partagés cœur ↔ interface, formats français, groupes de dates, réglages, démo
  store/                  Schéma SQLite, migrations, magasin .eml, sac de propriétés MAPI brut
  import-pst/             outlook-pst → modèle (dossiers, messages, pièces jointes, drapeaux, catégories)
                          + FAI : catégories, vues, favoris, heures de travail, flux de règles
  mime/                   MAPI → MIME ([MS-OXCMAIL]), RTF, TNEF ([MS-OXTNEF]), .msg/.oft ([MS-OXMSG])
  search/                 Syntaxe de recherche (mots-clés FR), requêtes FTS5, dossiers de recherche
  rules/                  Modèle de règle, analyseur du flux / .rwz (spec communautaire), moteur côté client
  sync/                   Comptes (J7) : relève IMAP, report des changements, envoi SMTP, IDLE, réponse automatique
tools/                    Outils d'analyse existants (UIA, sondes PST, en-têtes)
```

**Modèle de données (extrait)** :
- `store` : un PST importé ou un compte ;
- `folder` : arborescence, rôle spécial, vue par défaut ;
- `message` : identifiants, conversation, expéditeur et destinataires, dates, lu, drapeau et échéance, catégories, importance, taille, aperçu, chemin du `.eml`, propriétés MAPI brutes ;
- `attachment` ;
- `category` ;
- `rule` ;
- `search_folder` ;
- `view_settings`.

## 4. Jalons (le mail d'abord)
Taille relative : S, M, L. Chaque jalon se termine par un commit, un push et une démo.

| # | Jalon | Contenu | Critère de fin | Taille |
|---|---|---|---|---|
| J0 ✅ | Socle | Projet Tauri, jetons `DESIGN.md` clair/sombre, fenêtre à 3 volets statique, navigation basse, barre d'état, barre de commandes vide, police Inter, icônes Lucide | Contrastes AA vérifiés. Arbre UIA du clone relevé avec `tools/uia_dump.ps1` et comparé à la référence | S |
| J1 ✅ | Import PST (lecture seule) | Copie de travail, import vers SQLite + `.eml`, pièces jointes, lu, drapeaux, catégories, FAI de configuration, rapport d'import | Sur tes 3 copies (271 Ko, 3,9 Go, 15 Go) : effectifs identiques à `pst-probe`, 0 perte silencieuse, import de 15 Go en moins de 10 min | M |
| J2 ✅ | Lecture | Volet dossiers (compteurs non lus / [total]), liste virtualisée (colonnes, tri, groupes par date, styles non lu et drapeau), volet de lecture (HTML isolé, texte, RTF, pièces jointes, images bloquées), raccourcis de lecture, F6 | Ouverture d'un dossier de 22 000 éléments en moins de 200 ms. Parcours clavier complet | M |
| J3 ✅ | Organisation | Lu/non lu, supprimer, déplacer/copier, glisser-déposer, indicateurs, catégories, CRUD des dossiers, favoris, conversations, « Organiser par », filtres Tous/Non lus, paramètres d'affichage | Ruban Accueil, Dossier et Affichage fonctionnels (sous-ensemble de `ui-reference.md` §3) | M |
| J3b ✅ | Conversations et affichages | « Afficher en conversations », « Organiser par » au-delà des 4 tris (catégories, indicateur, pièces jointes), boîte « Réglages de l'affichage », options de lecture (marquer comme lu) dans l'interface | Liste en conversations sur la Boîte de réception de 22 000 éléments en moins de 300 ms | M |
| J4 ✅ | Recherche | Index FTS5, zone de recherche et portées, onglet Recherche, mots-clés, recherche avancée, dossiers de recherche (Non lus, Pour le suivi, personnalisés) | Moins de 300 ms sur l'archive de 217 000 éléments | M |
| J5 ✅ | Rédaction (brouillons) | Fenêtre de rédaction HTML, réponse et transfert avec citation, signatures (import HTM), modèles `.oft`, brouillons | Réponse à tous fidèle (en-têtes, citation) enregistrée en brouillon | M |
| J6 ✅ | Règles et actions rapides | Import du flux de règles et des `.rwz`, assistant de règles, « Exécuter maintenant », actions rapides (défauts plus éditeur) | Tes 2 `.rwz` et le flux du PST importés, et rejoués sur une copie | L |
| J7 ✅ | IMAP + SMTP (Hostinger) | Comptes, synchronisation, envoi, boîte d'envoi, F9, hors connexion, notifications, réponse automatique locale | Envoi et réception réels sur un compte de test | L |
| J8 ✖ | POP3 | Abandonné le 2026-10-21 : pendant la transition, Outlook et ThunderLook relèvent les mêmes boîtes en IMAP (en POP3, ce qu'un client relève, l'autre ne le voit pas) | — | — |
| J9 ✅ | Calendrier (option A : Outlook fait foi) | Lecture des 4 198 rendez-vous du PST (périodicités, exceptions, fuseaux), expansion des occurrences, vues Jour, Semaine de travail, Semaine, Mois, navigateur de dates, superposition des calendriers, détail en lecture seule ; rechargement depuis une copie plus récente du PST (`tl-import --calendars`) ; import/export `.ics` et CSV d'Outlook, calendriers importés en lecture seule. Création/modification sans objet en option A ; réunions iTIP/iMIP abandonnées le 2026-10-22 à la demande de Paul (« pas besoin ») | Occurrences identiques à celles calculées par Outlook sur 2023-2026 ; aller-retour `.ics` sans écart (`notes/j9-plan.md`) | L |
| J10+ | Contacts, puis Tâches | Ordre fixé par Q3. Import de 6 046 contacts, tâches, .vcf | Jalons détaillés plus tard | L |

## 5. Vérification
- Tests Rust : unitaires, plus des tests « dorés » d'import sur les copies locales (non versionnées), comparés à `pst-probe` et libpff.
- Jeu d'essai versionnable : PST synthétiques générés (par exemple avec `outlook-pst-rw`, à évaluer) et `.eml` d'exemple. Aucune donnée réelle dans git.
- Interface :
  - tests de bout en bout via WebDriver (`tauri-driver`) ;
  - comparaison structurelle de l'arbre UIA du clone avec `notes/ui-reference.md` ;
  - contrôle des contrastes WCAG AA.
- Performance : mesures J1, J2 et J4 sur l'archive de 15 Go.

### Mesures du J1 (2026-10-18, Ryzen 7 7700, NVMe)
Référence : `pst-probe --open-all` (même lecteur) et, pour les pièces jointes, libpff.
Vérification : `verify_store` relit chaque `.eml` avec mail-parser (sujet, nombre de pièces jointes, corps) : 0 écart sur les trois copies.

| Copie | Dossiers | Éléments importés / référence | FAI | Pièces jointes (dont messages joints) | Incidents | Copie de travail | Import |
|---|---|---|---|---|---|---|---|
| 271 Ko | 17 / 17 | 4 / 4 | 2 / 2 | 0 | 0 | — | < 0,1 s |
| 3,9 Go | 73 / 73 | 35 589 / 35 589 | 106 / 106 | 7 702 (295), = libpff | 0 | 2,4 s | 26,5 s |
| 15 Go | 124 / 124 | 217 257 / 217 257 | 122 / 122 | 40 633 (404) | 0 | 11 s | 170 s |

- La copie de travail est probablement servie en partie par le cache de Windows : les fichiers venaient d'être lus.
- Taille produite pour 15 Go : 13,1 Go de `.eml`, base de 2,2 Go (les sacs MAPI bruts, RTF compressé compris, en font l'essentiel).
- Catégories : 23 définitions (FAI `CategoryList`) et 7 814 attributions sur 5 309 éléments dans la copie de 3,9 Go.
  L'archive a 13 839 attributions mais aucune liste de définitions : le J3 devra créer les catégories manquantes.
- Indicateurs de suivi : 918 éléments marqués dans la copie de 3,9 Go, 111 232 dans l'archive, dont 110 363 dans
  « listes Oxalis » (tous « Assurer un suivi », 2007-2017). C'est la valeur du PST, sans doute posée par une
  ancienne règle ; à garder en tête pour le dossier de recherche « Pour le suivi » (J4).
- Six défauts d'`outlook-pst` corrigés en chemin, dont deux pertes silencieuses : `vendor/outlook-pst/PATCH.md`.

### Mesures du J2 (2026-10-19, même machine, base fusionnée : 2 sources, 252 846 éléments, 2,8 Go)
- **Ouverture d'un dossier dans l'application** (aller-retour IPC, première tranche de 100 lignes et affichage, journal `perf.log`) :
  - Boîte de réception (22 188 éléments) : **145 ms** au premier lancement, 78 ms ensuite ;
  - Inbox de l'archive (61 642 éléments) : 44 ms.
- **Cœur seul** (`cargo run --release -p thunderlook-mailbox --example bench -- data/store`), sur le plus gros dossier, « listes Oxalis » (110 539 éléments) :

  | Tri | Ouverture de la vue | Groupes |
  |---|---|---|
  | Date | 62 ms | 1 |
  | De | 216 ms | 88 |
  | Objet | 278 ms | 312 |
  | Taille | 235 ms | 6 |

  Pour les tris autres que la date, il faut lire les lignes (expéditeur, objet, taille) : c'est l'index FTS du J4 qui réduira ces temps.
- **Arborescence** avec compteurs : 14 ms (index couvrant `message_folder_read`, schéma v2).
  - Migration v1 → v2 sur la base fusionnée : 2,1 s, une seule fois.
- **Volet de lecture** : balayage d'un message sur 20 (12 160 messages, avec extraction de toutes les pièces jointes), 0 échec.
  - 1,4 ms par message en moyenne, 109 ms au plus.
  - Corps : 11 039 HTML, 864 texte, 257 vides. Contenu distant bloqué dans 4 239 messages.
- **Clavier**, vérifié avec `tools/uia_keys.ps1` :
  - flèches, Début, Fin (dernier des 22 188 éléments, chargé à la demande), Pg Suiv ;
  - Ctrl+. et Ctrl+, depuis n'importe quel volet, corps du message compris ;
  - F6 et Maj+F6.
- **Aperçus** : 78 269 entités HTML non décodées dans les aperçus écrits au J1 (`&zwnj;` 70 316, `&shy;` 7 497 : remplissage de mails publicitaires).
  - Corrigé dans `thunderlook-mime` (table d'entités étendue, caractères invisibles retirés).
  - Le correctif s'applique à l'affichage, sans réimport.
- **Rôles de dossiers dans l'archive** : « Courrier indésirable » est marqué Brouillons et « YouSendIt Outbox » Indésirables.
  - C'est ce que disent les EntryID de ce PST, sans doute hérités d'anciens profils.
  - On les respecte, mais l'ordre et les compteurs de ces dossiers en dépendent. À revoir au J3 (rôle modifiable).

### Vérifications du J3 (2026-10-19)
- **Tests d'intégration** (`crates/mailbox/tests/ops.rs`, données synthétiques) :
  - lu et non lu, « marqué lu en quittant » sans entrée d'annulation ;
  - indicateurs (dates de la semaine de travail), catégories (y compris sans définition) ;
  - déplacer, copier (le `.eml` est dupliqué), supprimer en deux temps (corbeille, puis définitif avec effacement du `.eml`), archiver (dossier « Archives » créé au premier usage) ;
  - dossiers : créer (doublon refusé), renommer (dossier spécial refusé), déplacer (pas dans lui-même), supprimer en deux temps ;
  - favoris, tout marquer comme lu, vider le dossier, annulation de chaque opération.
- **Dans l'application**, sur la base réelle (`tools/uia_keys.ps1`, et un vrai glisser à la souris) :
  - marquer lu en quittant, Ctrl+U, Inser, Maj+↓ (3 éléments), Suppr puis Ctrl+Z ;
  - Ctrl+Maj+E, F2, Suppr avec confirmation puis Ctrl+Z ;
  - menu contextuel (Maj+F10) avec sous-menu de catégories en couleur ;
  - glisser un message sur un dossier.
- **Réimport** : `tl-import` refuse désormais de remplacer une source déjà importée sans `--replace`, puisque cela effacerait le classement fait dans ThunderLook.
  - La suppression d'une source n'efface plus que les `.eml` de ses propres messages.

### Mesures du J4 (2026-10-19, base fusionnée de 252 846 éléments)
- **Indexation plein texte** :
  - **259 s**, soit 4 min 19, en tâche de fond, sur 16 fils ;
  - corps limité à 200 000 caractères par message ;
  - index sans contenu stocké (FTS5, insensible aux accents, préfixes de 2 et 3 caractères) ;
  - reprise là où elle s'est arrêtée, nouvelles copies comprises.
- **Recherche** sur « tous les fichiers de données », tri par date, cœur seul (`examples/bench.rs`) :

  | Requête | Résultats | Temps |
  |---|---|---|
  | `facture` | 5 211 | 38 ms |
  | `fact` (préfixe) | 8 046 | 55 ms |
  | `"bon de commande"` | 316 | 6 ms |
  | `de:paul` | 13 530 | 78 ms |
  | `facture pj:oui` | 968 | 26 ms |
  | `objet:devis reçu:2015` | 57 | 3 ms |
  | `lu:non` | 38 627 | 88 ms |
  | `suivi:oui` | 112 151 | 223 ms |
  | `adresse:paul@adam-declic.fr` | 41 807 | 218 ms |
  | `facture OU devis -relance` | 9 247 | 62 ms |
  | Dossier de recherche « Pour le suivi » | 112 148 | 231 ms |

  L'objectif de 300 ms est tenu partout.
  - Pour y arriver sur les gros résultats : conditions écrites en clair (index partiels), index couvrant `message_flagged(folder_id, received_at)`, et une union plutôt qu'un OU pour `adresse:`.
- **Dans l'application** : `facture pj:oui` dans le fichier de données courant donne 188 résultats affichés en **51 ms**.
  - L'onglet Rechercher s'affiche, et chaque ligne indique son dossier.
- La recherche ne couvre que les dossiers de courrier (ni calendrier, ni contacts, ni tâches).
  - Les dossiers de recherche prédéfinis excluent « Éléments supprimés » et « Courrier indésirable ».
- **Incident pendant les tests** : une fenêtre de HWiNFO64 (« Sauvegarder et fermer ») est passée au premier plan. Une saisie de test (`Ctrl+E`, « facture pj:oui », Entrée) lui a été envoyée.
  - `tools/uia_keys.ps1` vérifie désormais, avant chaque touche, que la fenêtre au premier plan est bien celle de l'application testée.
  - `tools/uia_search.ps1` saisit une recherche par UI Automation, sans clavier.

### Mesures du J3b (2026-10-19, même base)
- **Identifiant de conversation** : calculé selon [MS-OXOMSG] §2.2.1.2 (`PidTagConversationId`).
  - C'est la partie GUID de `ConversationIndex` si `ConversationIndexTracking` est vrai et l'en-tête valide ; sinon le MD5 du sujet de conversation en majuscules.
  - Couverture : 250 520 messages sur 252 846 ont un index, et le suivi est actif pour 22 154 des 22 188 messages de la Boîte de réception.
  - Calcul fait à l'import (`tl-import`). Pour une base existante, il est fait une seule fois à l'ouverture : **46 s** sur la base fusionnée (lecture des sacs MAPI).
- **Vue en conversations**, cœur seul (`examples/conversations.rs`) :

  | Dossier | Éléments | Lignes | Ouverture | 100 premières lignes |
  |---|---|---|---|---|
  | Boîte de réception | 22 188 | 21 521 | **16 ms** | 3 ms |
  | Inbox (archive) | 61 642 | 59 977 | 46 ms | 5 ms |
  | listes Oxalis | 110 539 | 232 | 20 ms | 5 ms |

  - L'objectif de 300 ms est tenu.
  - Dans l'application, la Boîte de réception s'ouvre en conversations en **38,7 ms** de bout en bout (`perf.log`).
  - Dépliage : 3 à 30 ms. Les éléments des autres dossiers du même fichier de données (« Éléments envoyés »…) apparaissent avec leur dossier. « Éléments supprimés » et « Courrier indésirable » en sont exclus.
- **Conversations géantes** : « listes Oxalis » contient deux conversations de 60 014 et 49 536 éléments (« [coop] Relance séminaire », 2007-2017). C'est l'emballement déjà vu au J1.
  - Les en-têtes sont résumés en SQL : une première version passait les identifiants en paramètres et dépassait la limite de 32 766 variables de SQLite.
  - Chacun coûte environ 350 ms quand il entre à l'écran. C'est acceptable pour ce cas, sans cache.
- **Organiser par** :
  - Catégories : un élément apparaît sous chacune de ses catégories, et la casse est ignorée.
  - Indicateur et pièces jointes : 9 à 282 ms, pire cas « Indicateur » sur l'Inbox de 61 642 éléments, comparable aux tris « De » et « Objet » du J2.
- **Défaut du J1 corrigé** : 224 734 objets étaient stockés avec le marqueur de préfixe du PST ([MS-PST] §2.5.3.1.1.1 : `\x01` puis la longueur du préfixe plus un). Ce marqueur est invisible mais faussait le tri par objet.
  - Il est retiré à l'import et dans la base (schéma v6, 15,6 s, une seule fois).
  - Les `.eml` déjà écrits le gardent dans leur en-tête `Subject`. Il faudra en tenir compte à l'export.
- **Choix faits** (non vérifiés sur Outlook) :
  - conversations seulement pour le classement par date ;
  - réglage global, pour tous les dossiers (Outlook demande « ce dossier / toutes les boîtes ») ;
  - en-tête en gras si un élément non lu est dans le dossier affiché ;
  - au plus 8 participants dans l'en-tête.
- **Tests** : `crates/mailbox/tests/conversations.rs` (données synthétiques) couvre le calcul à l'ouverture, la migration v6, le dépliage, une opération sur un en-tête (elle vise tous les éléments du dossier), le dépliage conservé après rouverture, et les trois nouveaux classements.
- **Dans l'application** (`tools/cdp.mjs`, page pilotée par CDP) :
  - test sur « commande », Boîte de réception de la seconde source, 6 éléments ;
  - le chevron déplie : 6 éléments en retrait, du plus récent au plus ancien, les réponses marquées « Éléments envoyés » ;
  - ← sur l'en-tête replie, → redéplie, ← sur un élément déplié replie et sélectionne l'en-tête ;
  - l'en-tête sélectionné affiche le dernier message dans le volet de lecture.
  - Une première tentative à la souris système (`uia_invoke.ps1 -Click`) a manqué sa cible ; un clic est probablement parti sur la barre des tâches.
    `uia_invoke.ps1` refuse désormais tout clic hors de la fenêtre. Le pilotage par CDP est à préférer.

### Vérifications du J5 (2026-10-19, même base)
- **Fidélité de la réponse** (`examples/fidelity.rs`, lecture seule) :
  - la réponse à tous préparée par ThunderLook pour le message de Roland MULLER (`m29555`) est **identique** à celle qu'Outlook a réellement écrite (`m7233`) ;
  - c'est vrai pour les destinataires, la copie, l'objet et le texte du bloc cité (« De : … Envoyé : jeudi 8 octobre 2026 22:14 À : … Objet : … ») ;
  - en chemin, une erreur a été corrigée : Outlook remplace un premier « Re: » par « RE: », et ne le garde pas tel quel. Les préfixes intérieurs restent (« RE: TR: » 21 fois, « TR: Fw: » dans les Éléments envoyés).
- **Format relevé sur un message envoyé** de la copie :
  - objet préfixé par `RE: ` ou `TR: ` ;
  - filet `border-top:solid #E1E1E1 1.0pt`, libellés en gras suivis d'une espace insécable ;
  - message d'origine sans retrait ;
  - `In-Reply-To` et `References` chaînés.
- **Brouillon** : `.eml` du dossier Brouillons de la source, `X-Unsent: 1`, `MSGFLAG_UNSENT`.
  - Même conversation que l'original : index enfant de 27 octets, même `conversation_id`.
  - Images en `cid:` dans le fichier et en `data:` dans l'éditeur.
  - Un brouillon rouvert puis réenregistré reste un seul brouillon et garde son fil.
- **Signatures** : les `.htm` de `%APPDATA%\Microsoft\Signatures` (windows-1252), images incluses.
  - Aucune signature par défaut n'est définie dans le registre de la référence. ThunderLook prend celle dont le nom contient « (adresse du compte) ».
  - Les autres s'insèrent par le menu « Signature ».
- **« Mes adresses »**, exclues de « Répondre à tous » : les expéditeurs des Éléments envoyés de la source.
- **Modèles `.oft`** (lecteur [MS-OXMSG] minimal, `oft.rs`) : les deux modèles de `data/samples` donnent leur objet et leur corps, accents compris. Menu « Nouveaux éléments ▾ ».
- **Dans l'application** (`tools/ui_check.mjs`, CDP) :
  - « Répondre à tous » ouvre le volet de rédaction : « À » rempli, objet `RE: …`, bloc cité présent ;
  - texte saisi, enregistrement : Brouillons passe de [2] à [3] ;
  - Entrée rouvre le brouillon avec son texte ;
  - suppression définitive : Brouillons revient à [2].
  - Défaut trouvé par ce test : `dyn_into::<HtmlDocument>()` échoue sur le document de l'iframe, qui vient d'un autre royaume JavaScript. L'éditeur restait donc inerte.
- **Tests** : `crates/mailbox/tests/compose.rs` (réponse à tous, réponse, transfert avec pièce jointe, nouveau message, adresses) et le test des préfixes d'objet.
- **Non fait** (reporté) :
  - fenêtre de rédaction séparée : la rédaction se fait dans le volet de lecture, comme la réponse intégrée d'Outlook ;
  - carnet d'adresses, saisie semi-automatique, orthographe ;
  - glisser-déposer de pièces jointes ;
  - signature différente selon nouveau message ou réponse.

### Vérifications du J6 (2026-10-20, même base)
- **Décodage** : crate `thunderlook-rules` identique au prototype Python sur les 3 flux réels (60 règles : 30 et 17 dans les `.rwz`, 13 dans le flux du PST ; 198 textes et 611 nombres comparés).
- **Import** (schéma v7, table `rule`) : flux du PST à l'ouverture, `.rwz` à la demande ; 60 règles, 0 en échec.
- **Dossiers cibles** : flux du PST, 7 sur 7 par EntryID ; `.rwz` (anciens PST non importés), tous retrouvés par le nom.
- **« Exécuter maintenant »** :
  - simulation sur la Boîte de réception (22 188 éléments), 12 règles actives du PST : 74 messages à modifier, **1,5 s** (168 s avant l'évaluation parallèle) ;
  - simulation sur l'archive entière (217 257 éléments) : environ 3 min à froid (lecture des `.eml` pour les conditions sur le corps).
- **Critère du jalon**, sur une copie (base et `.eml` de la Boîte de réception copiés) :
  - flux du PST : 74 messages modifiés, puis rejeu : 0 ;
  - `regles240913.rwz` : 219 déplacés, rejeu : 0 ; `regles.rwz` : 0 (déjà couvert) ;
  - contrôle par dossier : [electromagnetisme.fr] +35, catégorie CEM +7, LBC | FB +5, Ebay & Paypal +3, Hébergements WEB +2, Accusés +28, conformes à la simulation.
- Règle écartée : « OXALIS » (carnet d'adresses), sans objet : listes anciennes, archivées.
- **Interface** (`tools/ui_check.mjs`, CDP) : Règles et alertes (case active gardée puis rétablie, aperçu 74 à modifier en 1,5 s), actions rapides (première utilisation, création, exécution annulée par Ctrl+Z, suppression).
- Actions rapides par défaut d'après l'aide Microsoft en français (libellés non trouvés dans les ressources d'Outlook installées) ; aucune action rapide personnalisée dans les PST.

### Vérifications du J7 (2026-10-21, compte de test Hostinger `thunderlook-test@adam-declic.fr`)
- **Critère du jalon** (`tools/ui_check.mjs`, dans l'application) : compte ajouté par « Paramètres du compte », message rédigé puis envoyé en **4,0 s**, reçu dans la Boîte de réception du compte **6,1 s** après l'envoi ; messages de test supprimés jusque sur le serveur, compte retiré.
- **Relève** (dossier de données de test) : 5 dossiers SPECIAL-USE ; première relève de 2 messages en 1,2 s, relève sans changement en 0,8 à 1,0 s ; nouveau message, message marqué lu et message supprimé sur le serveur : chacun répercuté, `.eml` compris.
- **Report des changements** : lu, indicateur, copie, déplacement, suppression définitive vérifiés côté serveur (relu par IMAP) ; déplacements successifs fusionnés en un seul MOVE ; copie puis suppression annulées avant d'atteindre le serveur ; nouvel UID retrouvé par Message-ID.
- **Envoi** : Bcc retiré des en-têtes transmis (enveloppe À + Cc + Cci), `X-Unsent` retiré, copie déposée dans Éléments envoyés du serveur.
- **IDLE** : signal 1,5 s après un envoi extérieur ; relève au démarrage et toutes les 10 min s'il existe un compte ; « Travailler hors connexion ».
- **Réponse automatique** (seconde boîte de test `thunderlook-test2@`) : une réponse pour deux messages du même expéditeur, aucune pour le suivant ; `Auto-Submitted: auto-replied`.
- **Sécurité** : mot de passe dans le gestionnaire d'identifiants de Windows (test : absent de tous les fichiers du dossier de données) ; TLS vérifié (rustls, racines Mozilla).
- Schéma v8 repris d'une ébauche non validée trouvée dans l'arbre de travail (20/10, 22 h 48, autre outil), complété d'une colonne `last_error`.

### Vérifications du J9 (2026-10-22, même base)
- **Lecture** (`crates/calendar`, `examples/read_all`) : 4 198 rendez-vous lus sur 4 198, 0 échec ; 392 séries (351 annuelles, 28 mensuelles, 9 hebdomadaires, 4 quotidiennes), 142 exceptions, 206 occurrences supprimées, toutes en fuseau Paris.
- **Contrôle croisé** avec l'export `.ics` d'Outlook du 03/10/2023 (`examples/crosscheck_ics`) : 466 événements retrouvés par UID sur 472, 466 concordants (début, journée entière, objet, fréquence) ; 50 occurrences modifiées sur 54 retrouvées dans leur série. Les 6 UID absents sont des éléments supprimés ou recréés dans Outlook depuis l'export.
- **Occurrences** comparées à celles calculées par Outlook lui-même (COM, `IncludeRecurrences`, lecture seule, `examples/occurrences`) : 2023 507 = 507, 2024 148 = 148, 2025 158 = 158 ; après rechargement depuis la sauvegarde du PST du 21/10 (`tl-import --calendars`), 2026 : 410 = 410.
- **`.ics`** (`examples/ics_roundtrip`) : export puis relecture des 4 calendriers de la source 1, 1 217 occurrences 2023-2026, 0 écart ; l'export `.ics` d'Outlook (10/2023) importé redonne les 414 occurrences de Calendar Annexe calculées aujourd'hui par Outlook (7 de plus : éléments supprimés depuis). Série incohérente chez Outlook (OccurrenceCount 3, fin à la 2e) : UNTIL écrit pour donner ce qu'Outlook affiche.
- **CSV** (`examples/csv_check`) contre l'export CSV d'Outlook du 19/12/2023 (Calendar Annexe) : en-tête identique ; sur 294 lignes communes, objet, dates, journée entière, lieu, confidentialité, participants identiques ; description 289/294 (mise en forme RTF par Outlook) ; écarts restants dus aux modifications faites depuis 2023. Aller-retour : 468 lignes, 0 écart.
- **Interface** (`tools/ui_check.mjs`, cas J9, CDP) : bandeau « Journée » et rendez-vous identiques aux occurrences d'Outlook (6-12 février et 6 mars 2023, Calendar Annexe superposé) ; export `.ics` d'Outlook déposé dans `calendars/`, section, superposition et rendez-vous du 6 mars 2023 vérifiés, puis calendrier retiré.
- Hors périmètre : création et modification (option A, Outlook fait foi) ; réunions iTIP/iMIP abandonnées le 2026-10-22 à la demande de Paul.

## 6. Suite de l'expérience REA
- REA sur `OUTLOOK.EXE` : uniquement pour les champs inconnus du flux de règles, au jalon J6 et seulement s'ils bloquent. Nécessite le serveur MCP (redémarrage de Claude Code).
- Limites de REA en analyse statique : sur **nos propres binaires** (Rust/Tauri, donc sans enjeu juridique). REA annonce un support des binaires Rust.
- Tout est journalisé dans `REA-LIMITES.md`.

## 8. Lien avec ADI Facturation
ThunderLook et ADI Facturation (Electron, presque en production) travailleront en relation étroite. Contrat proposé dans `notes/integration-adi.md` :
- chaque application reste maître de sa base, et les échanges se font en lecture seule ou par commande explicite ;
- ADI → ThunderLook :
  - rédaction d'un mail avec la facture jointe, par ligne de commande compatible Outlook (J5 et J7) ;
  - lecture de l'historique d'un client dans `thunderlook.db` (`open_read_only`, index par adresse au J4) ;
- ThunderLook → ADI : fiche client dans le volet de lecture, par lecture seule de la base d'ADI, quand son schéma sera connu.

Conséquences sur les jalons :
- **J4** : index des adresses (expéditeur et destinataires).
- **J5** : ligne de commande de rédaction.
- **Schéma** : migrations par ajout uniquement, puisque la base devient une interface.

## 7. Points à valider avant de coder
1. **Stack** Tauri 2 + Rust + React/TypeScript : OK ?
2. **Ruban par défaut** : tu utilises le ruban **simplifié**. Je propose les deux modes, avec le simplifié par défaut, comme chez toi. OK ?
3. **Nom du produit** : provisoirement « ThunderLook », comme le dépôt. Garde-t-on ce nom ?
4. **Redémarrage de Claude Code** quand tu veux, pour que le serveur MCP REA voie Ghidra. Pas nécessaire avant J6.
