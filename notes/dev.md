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

## Notes techniques
- CSP stricte. Le chargeur de Trunk récupère le `.wasm` par `fetch`, d'où `connect-src 'self'` et `script-src 'wasm-unsafe-eval'`. Sans eux, la fenêtre reste vide.
- `aria-level` n'existe pas dans les attributs typés de Leptos 0.8 (tachys). On passe par la propriété DOM `ariaLevel`, que Chromium reflète en attribut.
- WebView2 garde ses raccourcis de navigateur (Ctrl+R recharge, Ctrl+F cherche…). Tauri 2.12 n'a pas d'option pour les couper. L'interface les intercepte dans `keydown` (`app.rs`, `reserved`). À revérifier au J2, quand ces raccourcis seront câblés.
- Réglages d'affichage : `%APPDATA%\fr.local.thunderlook\settings.json`, écrits de façon atomique par le cœur.
