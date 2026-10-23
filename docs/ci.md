# Intégration continue

Fichier : `.github/workflows/ci.yml` (Linux, `ubuntu-latest`, cache de `~/.cargo` et de `target`).

| Étape | Commande | État au 2026-10-23 |
|---|---|---|
| Tests | `cargo test --workspace --exclude thunderlook` | Passent (34 binaires de test). Le crate Tauri `thunderlook` est exclu (WebView2, chaîne Windows). |
| Clippy | `cargo clippy --workspace --exclude thunderlook -- -W warnings` | **53 avertissements, aucun échec** : `-D warnings` ferait échouer la CI. |
| rustfmt | non lancé | Le dépôt n'est pas formaté avec rustfmt : `cargo fmt --check` signale 1 694 différences. |

## Avertissements clippy existants (par crate)
`thunderlook-ui` 29, `thunderlook-mailbox` 8, `thunderlook-contacts` 4, `thunderlook-items` 4, `thunderlook-import-pst` 3,
`thunderlook-sync` 3, `thunderlook-model` 1, `thunderlook-calendar` 1. Surtout `collapsible_if` (37), types complexes (5),
`is_multiple_of` (2). Passer à `-D warnings` suppose de les corriger d'abord.

## Tests réservés à Windows
`crates/sync/tests/account.rs` : deux tests (`account_lifecycle_keeps_password_out_of_the_database`,
`password_is_shared_between_data_dirs`) sont en `#[cfg(windows)]`, car le mot de passe vit dans le Gestionnaire
d'identifiants de Windows (`keyring` avec `windows-native`), qui n'existe pas sous Linux.

## Non couvert
- Compilation de l'application Tauri et de l'interface WebAssembly (`trunk`, cible `wasm32`) ; `thunderlook-ui` est
  seulement compilé et vérifié en natif par `cargo test` / `cargo clippy`.
- `tools/ui_check.mjs` (pilotage de l'application par CDP) : Windows, à la main.
