# Lot E — intégration continue, documentation, recherche globale (moteur)

Branche : `cloud/lot-e-ci-doc-recherche`. Lis d'abord `notes/cloud/README.md`.

Périmètre de fichiers : `.github/workflows/`, `README.md`, `docs/` (nouveau), et le nouveau crate `crates/search/` (+ ajout dans `[workspace] members` du `Cargo.toml` racine). Rien d'autre.

À faire :
1. CI : `.github/workflows/ci.yml` : sous Linux, `cargo test --workspace --exclude thunderlook` (le crate Tauri exclu) ; `cargo clippy --workspace --exclude thunderlook -- -D warnings` si le dépôt passe (sinon `-W warnings`, et lister ce qui ne passe pas) ; `cargo fmt --check` seulement si le dépôt est déjà formaté. Cache `~/.cargo` et `target`. Ne rien casser : si un test échoue sous Linux pour une raison de chemin ou de plateforme, corrige le TEST (ou marque-le `#[cfg(windows)]` avec la raison), pas le code.
2. Documentation : `README.md` (but du projet : clone autonome d'Outlook ; fonctionnalités actuelles par module ; compilation ; où sont les données) et `docs/architecture.md` (crates, flux de données, base SQLite, format des sacs MAPI ; schéma lisible). Pas de promesses : décrire seulement ce qui existe (lire le code et `notes/handoff.md`).
3. Recherche globale — moteur : `thunderlook_search::query(conn, data_dir, text, scope) -> Vec<Hit>` sur contacts, rendez-vous (objet, lieu, description), tâches, notes ; `Hit { kind, id, title, snippet, when }` ; insensible à la casse et aux accents (réutiliser `thunderlook_contacts::view::sort_key`) ; limite de résultats. Tests sous Linux sur une base SQLite de test construite en mémoire (voir comment les autres crates font leurs tests de base, `thunderlook_store`).
4. Dire dans le résumé comment brancher `query` dans la barre de recherche de l'interface (sans le faire).
