# Architecture

Décrit ce qui existe dans le dépôt au 2026-10-23. Les jalons et décisions sont dans `PLAN.md`, l'état au jour le jour
dans `notes/handoff.md`.

## Vue d'ensemble

```
   Interface (Leptos → WebAssembly)            app/ui        crate thunderlook-ui
        │  appels IPC (invoke)
        ▼
   Cœur Tauri (commandes, fenêtres, réglages)  app/src-tauri crate thunderlook
        │
        ├── thunderlook-mailbox   courrier : arborescence, listes, message, recherche plein texte, actions, rédaction
        ├── thunderlook-sync      comptes IMAP/SMTP, relève, report des changements, envoi, IDLE, réponse automatique
        ├── thunderlook-rules     règles : modèle, lecture du flux / .rwz, moteur
        ├── thunderlook-calendar  rendez-vous : lecture, périodicités, vues, écriture, .ics / CSV
        ├── thunderlook-contacts  contacts : lecture, liste, fiche, écriture, vCard
        ├── thunderlook-items     tâches et notes : lecture, écriture, vTodo / vNote
        ├── thunderlook-search    recherche globale (contacts, rendez-vous, tâches, notes) — moteur seul
        └── thunderlook-mime      conversions de contenu (MAPI → MIME, RTF)
                  │
                  ▼
        thunderlook-store  ──►  <données>/thunderlook.db (SQLite) + <données>/eml/…
                  ▲
        thunderlook-import-pst (tl-import)  ◄──  copie d'un fichier PST (vendor/outlook-pst)

   thunderlook-model : types partagés cœur ↔ interface (sérialisés en JSON).
```

| Crate | Rôle |
|---|---|
| `crates/model` | Types d'affichage partagés avec l'interface (listes, fiches, formulaires), formats français, réglages. |
| `crates/store` | Schéma SQLite et migrations (`PRAGMA user_version`, schéma 10), chemins des `.eml`, ouverture de la base. |
| `crates/mime` | Conversions de contenu : pages de code, RTF. |
| `crates/import-pst` | Import d'un PST en lecture seule vers la base (`tl-import`) ; rapport d'import. |
| `crates/mailbox` | Lecture et actions sur le courrier ; index plein texte (FTS5) ; règles par compte ; rédaction. |
| `crates/rules` | Règles d'Outlook : modèle, lecture du flux de règles et des `.rwz`, moteur côté client. |
| `crates/sync` | Comptes IMAP/SMTP : relève, report des changements, envoi, boîte d'envoi, IDLE, réponse automatique. |
| `crates/calendar` | Sacs MAPI, propriétés nommées, fuseaux, périodicités ([MS-OXOCAL]), vues, écriture, `.ics`/CSV. |
| `crates/contacts` | Contacts ([MS-OXOCNTC]) : liste triée comme Outlook, fiche, écriture, vCard. |
| `crates/items` | Tâches et notes ([MS-OXOTASK], [MS-OXONOTE]) : lecture, écriture, vTodo/vNote. |
| `crates/search` | Recherche globale : `thunderlook_search::query` (voir ci-dessous). |
| `app/src-tauri` | Cœur Tauri : commandes IPC, fenêtres, notifications. Windows seulement. |
| `app/ui` | Interface Leptos compilée en WebAssembly (Trunk). |
| `vendor/outlook-pst` | Lecteur PST (copie corrigée, voir `vendor/outlook-pst/PATCH.md`). |

## Flux de données

1. **Import** : `tl-import <copie.pst> <dossier-de-données>` lit une *copie* du PST, écrit une ligne `message` par
   élément (courrier, rendez-vous, contacts, tâches, notes…) et un `.eml` par élément ; ce qui n'est pas converti reste
   dans `props` et ce qui est perdu est noté dans `import_issue` (rien ne se perd en silence).
2. **Lecture** : le cœur ouvre la base ; les crates `mailbox`, `calendar`, `contacts`, `items` calculent les listes et
   fiches affichées (types de `crates/model`) ; l'interface les affiche telles quelles.
3. **Comptes** : `sync` relève les boîtes IMAP dans la même base (une `source` par compte), reporte sur le serveur les
   changements faits localement (`imap_pending`) et envoie par SMTP via la boîte d'envoi (`outbox`). Les mots de passe
   sont dans le Gestionnaire d'identifiants de Windows, pas dans la base.
4. **Écriture locale** : rendez-vous, contacts, tâches et notes se créent, modifient et suppriment directement dans la
   base (plus de passage par Outlook) : le sac `props` est réécrit, le texte va dans un `.eml`.

## Où sont les données

Dossier de données, par ordre de priorité (`app/src-tauri/src/lib.rs`) : variable d'environnement `THUNDERLOOK_DATA`,
`dataDir` dans `config.json` (dossier de configuration de l'application), sinon `<données locales>\data`.

```
<données>/
  thunderlook.db        base SQLite (WAL) ; version dans PRAGMA user_version
  eml/<source>/<lot>/<message>.eml   un fichier par élément (lot = identifiant / 1000)
                                     éléments créés dans ThunderLook : eml/<source>/tl/<message>.eml
  sources/              copies de travail des PST importés
  reports/              rapports d'import (JSON)
```
Ce dossier contient des données personnelles : il est ignoré par git (`data/`).

## Base SQLite (schéma 10)

Dates en millisecondes Unix (UTC). Tables principales (définitions complètes dans `crates/store/src/lib.rs`) :

| Table | Contenu |
|---|---|
| `source` | Un PST importé ou un compte ; `props` = table des propriétés nommées de la source (voir plus bas). |
| `folder` | Arborescence d'une source (`parent_id`, `role` : inbox, sent, deleted, junk…). |
| `message` | **Tout élément**, quelle que soit sa classe (`message_class` : `IPM.Note`, `IPM.Appointment`, `IPM.Contact`, `IPM.Task`, `IPM.StickyNote`…) : en-têtes, lu, drapeau, aperçu, `eml_path`, `props`. |
| `attachment`, `category`, `message_category` | Pièces jointes et catégories. |
| `message_fts` | Index plein texte FTS5 du courrier (sans contenu : le texte reste dans les `.eml`). |
| `fai` | Éléments de configuration associés (règles, vues, catégories…), conservés bruts. |
| `rule`, `search_folder`, `favorite` | Règles, dossiers de recherche, favoris. |
| `account`, `imap_pending`, `outbox` | Comptes IMAP/SMTP, changements à reporter, messages à envoyer. |
| `import_run`, `import_issue` | Journal et anomalies des imports. |

Les migrations ne font qu'ajouter (colonnes, tables) : la base est lue par d'autres outils (ADI Facturation).

## Format des sacs MAPI (`message.props`, `source.props`)

Sac de propriétés MAPI brut, tel qu'écrit par l'import (`crates/calendar/src/bag.rs`, lecture et écriture) : une suite
d'entrées `id u16`, `type u16`, valeur, en little-endian.

- Valeurs de taille fixe (entiers 2/4/8 octets, booléen, double, date, GUID) écrites telles quelles ; **dates** en
  FILETIME (intervalles de 100 ns depuis 1601, UTC).
- Chaînes (`PT_UNICODE`, `PT_STRING8`) et binaires précédés de leur longueur `u32` ; multivalués précédés de leur
  nombre `u32`.
- Un sac tronqué ou d'un type inconnu est une erreur de lecture, pas un silence.
- `id` < 0x8000 : propriété fixe (ex. `0x3A1C` mobile d'un contact). `id` ≥ 0x8000 : propriété *nommée* ; son sens
  (ensemble de propriétés + identifiant, ex. `PSETID_Appointment` / `0x820D` début d'un rendez-vous) se lit dans la
  table de noms de la **source** (`source.props`, `crates/calendar/src/named.rs`), car les numéros diffèrent d'un PST à
  l'autre.

Le texte (corps, description, notes) n'est pas dans le sac mais dans le `.eml` de l'élément.

## Recherche globale (`crates/search`)

```rust
thunderlook_search::query(conn, data_dir, "élodie soleil", Scope::All) -> Vec<Hit>
// Hit { kind, id, title, snippet, when }
```
- Étendue (`Scope`) : tout, contacts, rendez-vous, tâches ou notes.
- Champs cherchés : contacts (noms, société, fonction, courriels, téléphones — avec ou sans espaces —, adresses, site,
  notes) ; rendez-vous (objet, lieu, description) ; tâches (objet, texte, catégories) ; notes (texte).
- Tous les mots saisis doivent se retrouver (dans un même élément) ; insensible à la casse et aux accents
  (`thunderlook_contacts::view::sort_key`). Résultats dont le titre contient les mots d'abord ; 100 résultats au plus
  (`query_limit` pour une autre limite).
- `id` : mêmes préfixes que l'interface (`p` contact, `a` rendez-vous, `t` tâche, `n` note).
- Limites : pas d'index (parcours de tous les éléments de la base à chaque recherche) ; calendriers `.ics` importés
  non cherchés ; courrier non concerné (il a son index FTS5 dans `thunderlook-mailbox`). Le moteur n'est pas encore
  branché à l'interface.
