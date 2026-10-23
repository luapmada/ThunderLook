# Lot AA — importer les séries (RRULE) et leurs exceptions venues du téléphone

Branche : `cloud/lot-aa-series-telephone`. Lis d'abord `notes/cloud/README.md`, `docs/mpe-fichiers.md` et les tests `#[ignore]` de `crates/exchange/tests/scenarios.rs` (`serie_creee_sur_le_telephone_est_importee`, série modifiée sur le téléphone) : ils décrivent le défaut. Aujourd'hui une série créée ou modifiée sur le téléphone est ignorée (perte silencieuse) : c'est un bloquant pour la migration du téléphone (plan MPE, étape 6).

Périmètre de fichiers : NOUVEAU `crates/calendar/src/ics_series.rs` (+ ligne `pub mod` dans `crates/calendar/src/lib.rs`), `crates/calendar/src/write.rs` (ajouts seulement : API de création/remplacement d'une série à partir d'une RRULE), `crates/exchange/src/import.rs` (UNIQUEMENT la branche qui traite aujourd'hui les séries par un avertissement), `crates/exchange/tests/scenarios.rs` (retirer les `#[ignore]` des deux scénarios une fois corrigés). Le lot AC édite aussi `crates/exchange/src/{lib,import}.rs` dans d'autres fonctions : ne touche à rien d'autre.

À faire :
1. RRULE → `Repeat`/`RepeatEnd` (DAILY, WEEKLY+BYDAY, MONTHLY BYMONTHDAY et BYDAY ordinal, YEARLY ; COUNT, UNTIL, INTERVAL), EXDATE → occurrences supprimées, RECURRENCE-ID → exceptions (réutiliser l'API d'exceptions du lot U déjà fusionné : lire `crates/calendar/src/write.rs`). Ce que le modèle ne sait pas représenter : refuser PROPREMENT avec un avertissement visible et explicite (jamais de perte silencieuse), et le dire dans le résumé.
2. Série modifiée sur le téléphone : remplacer la série existante (même UID) en conservant catégorie et rappel ; série supprimée : suppression de la série.
3. Tests : aller-retour export → import d'une série avec exceptions et EXDATE, fin de mois, 29 février, changement d'heure, UNTIL en UTC.
4. `cargo test -p thunderlook-calendar -p thunderlook-exchange` (noms réels à vérifier).

Aucune écriture dans Outlook. Fin : push, résumé de 10 lignes.
