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
  (jalon 7) sync/         IMAP, SMTP, boîte d'envoi
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
| J3b | Conversations et affichages | « Afficher en conversations », « Organiser par » au-delà des 4 tris (catégories, indicateur, pièces jointes), boîte « Réglages de l'affichage », options de lecture (marquer comme lu) dans l'interface | Liste en conversations sur la Boîte de réception de 22 000 éléments en moins de 300 ms | M |
| J4 ✅ | Recherche | Index FTS5, zone de recherche et portées, onglet Recherche, mots-clés, recherche avancée, dossiers de recherche (Non lus, Pour le suivi, personnalisés) | Moins de 300 ms sur l'archive de 217 000 éléments | M |
| J5 | Rédaction (brouillons) | Fenêtre de rédaction HTML, réponse et transfert avec citation, signatures (import HTM), modèles `.oft`, brouillons | Réponse à tous fidèle (en-têtes, citation) enregistrée en brouillon | M |
| J6 | Règles et actions rapides | Import du flux de règles et des `.rwz`, assistant de règles, « Exécuter maintenant », actions rapides (défauts plus éditeur) | Tes 2 `.rwz` et le flux du PST importés, et rejoués sur une copie | L |
| J7 | IMAP + SMTP (Hostinger) | Comptes, synchronisation, envoi, boîte d'envoi, F9, hors connexion, notifications, réponse automatique locale | Envoi et réception réels sur un compte de test | L |
| J8 | POP3 | Si toujours utile (profil « pop-original ») | — | S |
| J9+ | Calendrier, puis Contacts, puis Tâches | Ordre fixé par Q3. Import de 854 rendez-vous, 6 046 contacts, tâches, .ics/.vcf | Jalons détaillés plus tard | L |

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
