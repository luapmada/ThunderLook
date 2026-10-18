# pst-probe

Sonde de lecture PST (lecture seule) sur le crate `outlook-pst` 1.2.0 (Microsoft, MIT), dans sa copie corrigée
`vendor/outlook-pst` (versionnée depuis le J1, partagée avec `crates/import-pst`).

`Cargo.toml` pointe vers `../../vendor/outlook-pst` via `[patch.crates-io]`.

## Correctifs
Détail dans `vendor/outlook-pst/PATCH.md`. Diff complet par rapport à la version publiée :
`vendor/outlook-pst-1.2.0-thunderlook.patch` (à proposer en amont).

## Usage
```bash
cargo build --release
target/release/pst-probe <copie.pst> <dossier-de-sortie-dans-data/> [--open-all]
```
La sortie contient des données personnelles : la garder sous `data/` (ignoré par git).
