# Lot W — retirer le code COM mort (`crates/outlook`)

Branche : `cloud/lot-w-retrait-com`. Lis d'abord `notes/cloud/README.md` et `notes/handoff.md` (§ 23/10 : ThunderLook n'appelle plus Outlook).

Périmètre de fichiers : `crates/outlook/` (suppression), les `Cargo.toml` (racine, `app/src-tauri`, autres crates qui en dépendent), et la suppression des modules/fonctions devenus inutiles qui l'appellent (`app/src-tauri/src/shared_calendar.rs`, `contacts.rs` s'il existe, `lib.rs` : retraits seulement, aucun ajout). Ne touche pas aux autres lots (U calendrier/séries, V tâches, X exchange, Y safety, Z folders).

À faire :
1. Lister précisément (grep) tout ce qui dépend de `crates/outlook` ; vérifier qu'aucun chemin d'exécution ne l'appelle encore. Si un appel est vivant, ne pas le supprimer et le signaler.
2. Retirer le crate, ses dépendances (`windows`, COM) devenues inutiles, les commandes Tauri mortes et leur enregistrement dans `invoke_handler`, les appels côté interface (`app/ui/src/ipc.rs`) UNIQUEMENT s'ils sont morts.
3. `cargo check --workspace` (cibles Linux possibles ; pour src-tauri, dire ce qui n'a pas compilé), `cargo test --workspace`, `cargo check -p thunderlook-ui --target wasm32-unknown-unknown`.
4. Mettre à jour `notes/handoff.md` n'est PAS à faire (Paul s'en charge) ; donner la liste des retraits dans le résumé.

Fin : push, résumé de 10 lignes.
