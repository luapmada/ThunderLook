# ThunderLook
a REA work on MSoutlook21

## Objectif
- Clone autonome d'Outlook 2021 classique pour Windows 11 : courrier, calendrier, contacts, tâches et notes, 100 % local, **sans Outlook**.
- Reprend l'ergonomie d'Outlook : barre de commandes à onglets, trois volets, vues, clic droit, raccourcis clavier.
- Habillage visuel original (`DESIGN.md`). Projet sans lien avec Microsoft. Pas de compte Microsoft, pas de service cloud payant.
- Les données de départ sont importées depuis des **copies** de fichiers PST, jamais depuis les originaux.
- Second objectif : éprouver l'outil de rétro-ingénierie REA. Journal dans `REA-LIMITES.md`.
- Cadre légal : `notes/juridique.md`. Aucun code, icône, image, texte, police ni marque de Microsoft.

## Fonctionnalités actuelles (version 0.10.0)
Détail des jalons dans `PLAN.md` ; limites connues et reste à faire dans `notes/handoff.md`.

- **Import PST** (`tl-import`) : dossiers, courrier, pièces jointes, rendez-vous, contacts, tâches, notes, règles, catégories ; rien ne se perd en silence (`import_issue`).
- **Courrier** : arborescence, liste triée et groupée, conversations, volet de lecture (HTML isolé, images distantes bloquées), lu/non lu, déplacer, copier, supprimer, drapeaux, catégories, dossiers et favoris, annulation, clic droit.
- **Recherche courrier** : index plein texte, mots-clés, étendues, dossiers de recherche.
- **Rédaction** : réponse, transfert avec citation, signatures, modèles `.oft`, brouillons.
- **Règles et actions rapides** : règles d'Outlook (flux de règles du PST, `.rwz`), assistant, exécution à l'arrivée et à la demande, expéditeurs bloqués/approuvés.
- **Comptes IMAP/SMTP** : relève, report des changements, envoi, boîte d'envoi, IDLE, notifications, réponse automatique locale.
- **Calendrier** : vues Jour, Semaine de travail, Semaine, Mois ; périodicités et exceptions ; création, modification et suppression (y compris séries et une occurrence) dans la base ; import/export `.ics` et CSV.
- **Contacts** : dossiers, liste triée, fiche (photo, notes), création, modification, suppression ; export/import vCard.
- **Tâches et notes** : listes, création, modification, suppression ; export/import vTodo/vNote.
- **Recherche globale** (moteur seulement, `crates/search`) : contacts, rendez-vous, tâches, notes. Pas encore branchée à l'interface.

Pas encore faits : voir « Reste » dans `notes/handoff.md` (listes de distribution, photo de contact en édition, tâches périodiques, échange avec le téléphone par MyPhoneExplorer, etc.).

## Compiler
Prérequis (Windows 11) : Rust (rustup) avec la cible `wasm32-unknown-unknown`, `trunk`, `cargo-tauri`, MSVC Build Tools, WebView2. Détails et pièges : `notes/dev.md`.

```powershell
cd app; cargo tauri dev                 # développement
cd app; cargo tauri build --no-bundle   # target\release\thunderlook.exe
cargo test --workspace --exclude thunderlook   # tests des crates (aussi sous Linux ; c'est ce que lance la CI)
```
Import d'un PST : `cargo build --release -p thunderlook-import-pst`, puis `target\release\tl-import.exe <copie.pst> <dossier-de-données>`.
Intégration continue : `.github/workflows/ci.yml` (voir `docs/ci.md`).

## Où sont les données
Dossier de données, par ordre de priorité : variable d'environnement `THUNDERLOOK_DATA`, `dataDir` dans `config.json` (dossier de configuration de l'application), sinon `<données locales>\data`. Il contient `thunderlook.db` (SQLite), `eml/` (un `.eml` par élément), `sources/` (copies des PST) et `reports/`. Les mots de passe des comptes sont dans le Gestionnaire d'identifiants de Windows. Le dossier `data/` du dépôt est ignoré par git. Détail : `docs/architecture.md`.

## Fichiers
- `SPEC.md` : fonctions inventoriées et priorités (le mail d'abord).
- `DESIGN.md` : direction visuelle.
- `REA-LIMITES.md` : ce qui marche et ce qui échoue avec REA.
- `PLAN.md` : architecture, stack et jalons.
- `notes/` : analyses (interface, formats) et points juridiques.
- `app/` : application (cœur Tauri `app/src-tauri`, interface Leptos `app/ui`) ; `crates/model` : types partagés.
- `crates/store`, `crates/mime`, `crates/import-pst` : stockage SQLite + `.eml`, conversions de contenu, import PST (`tl-import`).
- `crates/calendar`, `crates/contacts`, `crates/items`, `crates/search` : calendrier, contacts, tâches et notes, recherche globale.
- `crates/mailbox` : lecture pour l'interface (arborescence, vues triées servies par tranches, message complet).
- `notes/integration-adi.md` : contrat d'intégration proposé avec ADI Facturation.
- `vendor/outlook-pst` : copie corrigée du lecteur PST de Microsoft (MIT), correctifs dans `PATCH.md`.
- `tools/` : outils d'analyse et de vérification (relevé et test clavier UI Automation, contrastes, en-têtes PST, sondes libpff et `outlook-pst`).
- `notes/dev.md` : construire et tester.
- `docs/architecture.md` : crates, flux de données, base SQLite, format des sacs MAPI. `docs/ci.md` : intégration continue.
- `notes/handoff.md` : état au jour le jour, reste à faire.
