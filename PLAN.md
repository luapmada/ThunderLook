# PLAN — architecture, stack, jalons

Statut : **proposition, en attente de ton accord avant tout code applicatif.**
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
| Application | **Tauri 2** : cœur Rust, interface web dans WebView2 | WebView2 155 déjà présent. Binaire léger. Une seule technologie pour l'interface, l'affichage HTML des mails et l'éditeur |
| Lecture PST | **`outlook-pst`** (Microsoft, MIT, implémentation de [MS-PST]), avec notre correctif « sous-arbre absent » proposé en amont | Mesuré : 35 589 éléments en 1,2 s, 217 257 éléments (15 Go) en 54 s, 0 erreur, FAI lisibles. libpff : 43,6 s et 26 échecs, sans FAI |
| MIME | `mail-parser` / `mail-builder` (Apache-2.0/MIT) | Conversion MAPI → `.eml` selon [MS-OXCMAIL] |
| RTF | Décompression [MS-OXRTFCP] (crate `compressed-rtf`, licence à vérifier, sinon ~200 lignes) et dé-encapsulation [MS-OXRTFEX] | 633 messages RTF dans ton PST principal |
| Base | SQLite (`rusqlite`, embarqué) + FTS5 | Index et recherche locale (pas Windows Search) |
| Interface | TypeScript + React, TanStack Virtual (listes), TipTap/ProseMirror (éditeur), icônes Lucide (ISC), police Inter (OFL) | Écosystème mûr pour les listes de plus de 20 000 lignes et l'édition HTML |
| Affichage des mails | `iframe` en bac à sable (`srcdoc`, sans scripts), CSP qui bloque le contenu distant par défaut | Même comportement qu'Outlook : images distantes bloquées, barre d'information |
| IMAP / SMTP (jalon 7) | `async-imap` ou `imap-next`, `lettre` | Moins mûrs que MailKit (.NET). Risque accepté, car hors du premier jalon |

**Alternative écartée** : .NET 8 WPF + WebView2 + MimeKit/MailKit.
- Avantages : SDK déjà installé, excellentes bibliothèques mail.
- Rejetée car :
  - aucun lecteur PST C# ne lit les FAI de façon éprouvée ;
  - il faudrait WebView2 de toute façon pour l'affichage et l'édition HTML, d'où deux technologies d'interface.

**Outillage installé** :
- Rust 1.99.1 dans `%USERPROFILE%\.cargo` (pas encore dans le PATH) ;
- MSVC 2019 Build Tools et SDK Windows 10.0.19041 déjà présents ;
- Node 24.

## 3. Architecture

```
app/                      Application Tauri
  src-tauri/              Commandes IPC, fenêtres (principale, message, rédaction), thème Windows
  ui/                     Interface TypeScript
    shell/                Barre de commandes à onglets (classique / simplifiée, KeyTips), navigation basse, barre d'état
    mail/                 Volet dossiers, liste virtualisée, volet de lecture, barre de dossier
    compose/              Fenêtre de rédaction
    dialogs/              Règles, actions rapides, paramètres d'affichage, recherche avancée…
    theme/                Jetons DESIGN.md (clair/sombre, densité), contrôles WCAG AA
crates/
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
| J0 | Socle | Projet Tauri, jetons `DESIGN.md` clair/sombre, fenêtre à 3 volets statique, navigation basse, barre d'état, barre de commandes vide, police Inter, icônes Lucide | Contrastes AA vérifiés. Arbre UIA du clone relevé avec `tools/uia_dump.ps1` et comparé à la référence | S |
| J1 | Import PST (lecture seule) | Copie de travail, import vers SQLite + `.eml`, pièces jointes, lu, drapeaux, catégories, FAI de configuration, rapport d'import | Sur tes 3 copies (271 Ko, 3,9 Go, 15 Go) : effectifs identiques à `pst-probe`, 0 perte silencieuse, import de 15 Go en moins de 10 min | M |
| J2 | Lecture | Volet dossiers (compteurs non lus / [total]), liste virtualisée (colonnes, tri, groupes par date, styles non lu et drapeau), volet de lecture (HTML isolé, texte, RTF, pièces jointes, images bloquées), raccourcis de lecture, F6 | Ouverture d'un dossier de 22 000 éléments en moins de 200 ms. Parcours clavier complet | M |
| J3 | Organisation | Lu/non lu, supprimer, déplacer/copier, glisser-déposer, indicateurs, catégories, CRUD des dossiers, favoris, conversations, « Organiser par », filtres Tous/Non lus, paramètres d'affichage | Ruban Accueil, Dossier et Affichage fonctionnels (sous-ensemble de `ui-reference.md` §3) | M |
| J4 | Recherche | Index FTS5, zone de recherche et portées, onglet Recherche, mots-clés, recherche avancée, dossiers de recherche (Non lus, Pour le suivi, personnalisés) | Moins de 300 ms sur l'archive de 217 000 éléments | M |
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

## 6. Suite de l'expérience REA
- REA sur `OUTLOOK.EXE` : uniquement pour les champs inconnus du flux de règles, au jalon J6 et seulement s'ils bloquent. Nécessite le serveur MCP (redémarrage de Claude Code).
- Limites de REA en analyse statique : sur **nos propres binaires** (Rust/Tauri, donc sans enjeu juridique). REA annonce un support des binaires Rust.
- Tout est journalisé dans `REA-LIMITES.md`.

## 7. Points à valider avant de coder
1. **Stack** Tauri 2 + Rust + React/TypeScript : OK ?
2. **Ruban par défaut** : tu utilises le ruban **simplifié**. Je propose les deux modes, avec le simplifié par défaut, comme chez toi. OK ?
3. **Nom du produit** : provisoirement « ThunderLook », comme le dépôt. Garde-t-on ce nom ?
4. **Redémarrage de Claude Code** quand tu veux, pour que le serveur MCP REA voie Ghidra. Pas nécessaire avant J6.
