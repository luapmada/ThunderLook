# pst-probe

Sonde de lecture PST (lecture seule) sur le crate `outlook-pst` 1.2.0 (Microsoft, MIT), avec un correctif local.

## Préparer le crate corrigé (non versionné)
```bash
cargo fetch                      # télécharge outlook-pst 1.2.0
mkdir -p vendor && cp -r ~/.cargo/registry/src/*/outlook-pst-1.2.0 vendor/outlook-pst
cd vendor/outlook-pst && patch -p1 < ../../outlook-pst-1.2.0-missing-subnode.patch
```
`Cargo.toml` pointe vers `vendor/outlook-pst` via `[patch.crates-io]`.

## Correctif
`outlook-pst-1.2.0-missing-subnode.patch` : un message sans sous-arbre de nœuds (contacts, rendez-vous, notes écrits par Outlook) est lu avec une liste de sous-nœuds vide, au lieu d'échouer avec `MessageSubNodeTreeNotFound`.

Sur une copie de 3,9 Go : 6 248 échecs → 0. À proposer en amont.

## Usage
```bash
cargo build --release
target/release/pst-probe <copie.pst> <dossier-de-sortie-dans-data/> [--open-all]
```
La sortie contient des données personnelles : la garder sous `data/` (ignoré par git).
