# Lot AC — conflit « modifié dans ThunderLook ET intact sur le téléphone » (manifeste de l'échange)

Branche : `cloud/lot-ac-conflits-echange`. Lis d'abord `notes/cloud/README.md`, `docs/mpe-fichiers.md` et le test `#[ignore]` de `crates/exchange/tests/scenarios.rs` ligne ~209 : un élément modifié dans ThunderLook mais resté intact sur le téléphone est proposé comme « modifié » (le fichier porte l'ancienne valeur) ; l'appliquer écraserait la modification faite dans ThunderLook (perte de données).

Périmètre de fichiers : `crates/exchange/src/lib.rs` (manifeste, comparaison), NOUVEAU `crates/exchange/src/conflict.rs`, `crates/exchange/src/sha256.rs` si besoin, `crates/exchange/src/import.rs` UNIQUEMENT dans les fonctions de comparaison/classement des changements (le lot AA édite la branche des séries : ne pas y toucher), `crates/exchange/tests/`, `docs/mpe-fichiers.md` (section conflits). Rien d'autre.

À faire :
1. Le manifeste doit retenir, par élément exporté, l'empreinte (SHA-256) de la valeur exportée, pas seulement l'identifiant et le SHA du fichier entier.
2. Classement à l'import : (a) fichier = valeur exportée → intact côté téléphone : ne rien proposer, la valeur ThunderLook fait foi ; (b) fichier ≠ valeur exportée ET ThunderLook inchangé → modification du téléphone, à appliquer ; (c) les deux ont changé → vrai conflit, proposé SÉPARÉMENT et jamais appliqué sans choix (garder ThunderLook / garder le téléphone), la sauvegarde/corbeille interne (crate `safety`, lecture seule ici) restant le filet.
3. Migration du manifeste existant (ancien format sans empreinte de valeur) : comportement sûr (traiter comme « inconnu » = proposer sans appliquer automatiquement), testé.
4. Retirer l'`#[ignore]` du scénario correspondant une fois corrigé ; ajouter les cas (a), (b), (c) et la migration.
5. `cargo test -p thunderlook-exchange`.

Aucune écriture dans Outlook, aucune modification de la config de MPE. Fin : push, résumé de 10 lignes (dont l'interface à prévoir pour le choix du conflit : la décrire, ne pas l'écrire).
