# Développement

## Prérequis (installés le 2026-10-16)
- Rust 1.99.1 (rustup, profil minimal), avec la cible `wasm32-unknown-unknown`.
- `trunk` 0.21.14 et `cargo-tauri` 2.12.3, installés par `cargo install --locked`.
- MSVC Build Tools 2019 et SDK Windows 10.0.19041. WebView2 155.

## Commandes
```powershell
# Tests du modèle partagé
cargo test -p thunderlook-model --features clock

# Développement (Trunk sur le port 1420, rechargement à chaud)
cd app; cargo tauri dev

# Binaire de publication sans installeur → target\release\thunderlook.exe
cd app; cargo tauri build --no-bundle
```

```powershell
# Tests des crates du J1
cargo test -p thunderlook-mime -p thunderlook-store -p thunderlook-import-pst

# Import PST en ligne de commande. Par défaut, copie de travail dans <données>\sources\ ;
# --in-place lit le fichier sur place (refusé sous A:\Fichiers Outlook) ;
# --replace remplace une source déjà importée (perd le classement fait depuis dans ThunderLook).
cargo build --release -p thunderlook-import-pst
target\release\tl-import.exe <copie.pst> <dossier-de-données> [--in-place] [--replace]

# Vérification indépendante : relit chaque .eml avec mail-parser et le compare à la base
cargo run --release -p thunderlook-import-pst --example verify_store -- <dossier-de-données>

# Diagnostic d'un message (propriétés nommées, pièces jointes, messages joints)
cargo run --release -p thunderlook-import-pst --example attach_debug -- <copie.pst> <nid décimal>
```
Le dossier de données contient des données personnelles : le garder sous `data/` (ignoré par git).
Un nouvel import du même fichier d'origine n'est accepté qu'avec `--replace`. Il remplace alors la source précédente (base et `.eml` de ses messages).

**Piège** : si `NO_COLOR=1` est défini (c'est le cas dans l'environnement de Claude Code), `trunk` échoue avec « invalid value '1' for '--no-color' ». Faire `Remove-Item Env:NO_COLOR` avant le build.

## Vérifications du J0
- Contrastes : `python -I tools/contrast.py` → 32/32 conformes.
- Arbre UI Automation du clone : `tools/uia_dump.ps1 -Process thunderlook -Label <nom>`.
  Structure obtenue : `Tab`/`TabItem`, `Group`, `Button` avec `AcceleratorKey`, `Tree`/`TreeItem`, `List`/`ListItem`, barre d'état.
- Clavier : `tools/uia_keys.ps1 -Process thunderlook -Keys '{F6}',…`. **Uniquement sur ThunderLook, jamais sur Outlook.**
  Vérifié :
  - F6 / Maj+F6 font le cycle recherche → onglets → dossiers → liste → lecture ;
  - flèches, Début et Fin dans la liste et l'arbre ;
  - flèches gauche/droite dans les onglets ;
  - Ctrl+1 à 4 changent de module ;
  - Ctrl+E donne le focus à la recherche.

## Import PST (J1)
- Crates : `crates/mime` (pages de code, RTF [MS-OXRTFCP]/[MS-OXRTFEX], aperçu), `crates/store` (schéma SQLite v1,
  disposition `eml/<source>/<lot>/<id>.eml`), `crates/import-pst` (parcours, conversion MAPI → MIME, rapport).
- Lecteur : `vendor/outlook-pst`, copie corrigée d'`outlook-pst` 1.2.0. Six correctifs, dont deux pertes
  silencieuses (PC tronqué au premier `PtypObject`, messages joints introuvables) : voir `vendor/outlook-pst/PATCH.md`.
- Une seule transaction par import. Les `.eml` sont écrits par un fil dédié pendant la lecture du PST.
- Rien ne se perd en silence : tout élément ou pièce jointe non importé donne une ligne `import_issue`
  (`lost` ou `degraded`). Les propriétés MAPI non converties restent dans `props` (format dans `props.rs`).
- Rôles des dossiers : EntryID du magasin et de la Boîte de réception ([MS-OXOSFLD]), puis repli sur le nom
  des dossiers de premier niveau (PST de comptes IMAP sans ces EntryID).

## Lecture (J2)
- Dossier de données de l'application, par ordre de priorité :
  1. `THUNDERLOOK_DATA` ;
  2. `dataDir` dans `%APPDATA%\fr.local.thunderlook\config.json` ;
  3. `%LOCALAPPDATA%\fr.local.thunderlook\data`.
- `THUNDERLOOK_PERF=1` : temps d'ouverture des dossiers dans `%APPDATA%\fr.local.thunderlook\perf.log`.
- Mesures et balayage du volet de lecture : `cargo run --release -p thunderlook-mailbox --example bench -- <données> [pas]`.
- Liste virtualisée :
  - le cœur garde l'ordre des éléments de chaque vue (`open_list` → jeton) ;
  - l'interface demande les lignes visibles par tranches de 100 (`list_rows`) ;
  - hauteurs fixes : `--msg-h` = `list::row_height`, en-tête de groupe 28 px.
- Corps HTML : `iframe` `sandbox="allow-same-origin allow-popups allow-popups-to-escape-sandbox"`, **sans** `allow-scripts`, avec une CSP propre au message (`default-src 'none'`, images distantes seulement si l'utilisateur les autorise).
  - `allow-same-origin` sert à ajuster la hauteur et à relayer les raccourcis clavier.
  - Les liens partent dans le navigateur (`on_new_window` du cœur).
- Pièces jointes :
  - ouverture via un fichier temporaire (`%TEMP%\ThunderLook`) ;
  - types exécutables refusés à l'ouverture, mais « Enregistrer sous » reste possible.

## Organisation (J3)
- Écritures dans `crates/mailbox/src/ops.rs` : une transaction par opération.
  - Les `.eml` ne sont effacés qu'après validation, et seulement pour une suppression définitive.
- Annulation : pile de 50 opérations en mémoire (Ctrl+Z, bouton de la barre d'accès rapide). Elle est perdue à la fermeture.
  - Une suppression définitive ne s'annule pas : elle est toujours confirmée.
- Sélection multiple : plages de positions de la vue, envoyées telles quelles au cœur. Ctrl+A sur 22 000 éléments ne charge aucune ligne.
- Raccourcis :
  - **liste** : Suppr, Maj+Suppr (définitif), Ctrl+D, Ctrl+A, Ctrl+Maj+V, Maj+F10 ;
  - **partout dans le courrier** : Ctrl+Q, Ctrl+U, Inser, Ctrl+Z, Ctrl+Maj+E ;
  - **arborescence** : F2, Suppr, Maj+F10.
- Glisser-déposer HTML5 : il faut `dragDropEnabled: false` sur la fenêtre (`tauri.conf.json`), sinon Tauri intercepte les dépôts sous Windows. Ctrl au dépôt copie au lieu de déplacer.
- Schéma v3 : table `favorite`. Une liste vide ramène les favoris par défaut (réception et envoyés de la première source).

## Recherche (J4)
- Index : `message_fts` (FTS5 sans contenu stocké).
  - Il est rempli par `thunderlook_mailbox::index_pending` dans un fil du cœur, lancé à l'ouverture de la base et après chaque copie.
  - Pour l'indexer en ligne de commande : `cargo run --release -p thunderlook-mailbox --example index -- <données>`.
- Syntaxe : voir l'en-tête de `crates/mailbox/src/search.rs`. Mots-clés en français et en anglais (`de:`/`from:`, `objet:`, `pj:oui`, `reçu:hier`, `adresse:`…), `OU`, `-mot`, phrases entre guillemets.
- Étendues : dossier actif, sous-dossiers, fichier de données actuel (défaut), tous.
  - Changer de dossier ferme la recherche, comme dans la référence.
- Dossiers de recherche :
  - prédéfinis : « Courrier non lu », « Pour le suivi » ;
  - enregistrés : table `search_folder`, avec la requête et l'étendue ;
  - identifiants `s-…` dans l'arborescence.
- Tests d'interface sans clavier : `tools/uia_search.ps1 -Text '…'`.
  - `tools/uia_keys.ps1` s'arrête si une autre application passe au premier plan.

## Notes techniques
- CSP stricte. Le chargeur de Trunk récupère le `.wasm` par `fetch`, d'où `connect-src 'self'` et `script-src 'wasm-unsafe-eval'`. Sans eux, la fenêtre reste vide.
- `aria-level` n'existe pas dans les attributs typés de Leptos 0.8 (tachys). On passe par la propriété DOM `ariaLevel`, que Chromium reflète en attribut.
- WebView2 garde ses raccourcis de navigateur (Ctrl+R recharge, Ctrl+F cherche…). Tauri 2.12 n'a pas d'option pour les couper. L'interface les intercepte dans `keydown` (`app.rs`, `reserved`). À revérifier au J2, quand ces raccourcis seront câblés.
- Réglages d'affichage : `%APPDATA%\fr.local.thunderlook\settings.json`, écrits de façon atomique par le cœur.
