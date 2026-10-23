# Lot F — vitesse au lancement et fluidité (calendrier, contacts, tâches, notes)

Branche : `cloud/lot-f-performance`. Lis d'abord `notes/cloud/README.md`.

Problème mesuré : à l'ouverture du module Calendrier, « Chargement du calendrier… » reste affiché plus de 15 secondes (base SQLite de 4,3 Go, ~4 200 rendez-vous, ~10 600 contacts, ~440 notes). Les modules Contacts, Tâches et Notes rechargent aussi tout à chaque changement.

Périmètre de fichiers : `crates/calendar/` (`lib.rs` `load`, `view.rs`), `crates/contacts/src/lib.rs` (`load`), `crates/items/src/lib.rs`, `crates/store/src/lib.rs` (index uniquement), `app/src-tauri/src/lib.rs` (uniquement `with_calendar`, `CalendarCache`, `calendar_view`, `contact_list`, `contact_card`). Rien d'autre.

À faire :
1. Banc d'essai reproductible : un exemple `crates/calendar/examples/bench.rs` qui fabrique une base SQLite temporaire réaliste (4 200 rendez-vous dont 400 séries, 10 000 contacts, 400 notes ; sacs MAPI construits avec `thunderlook_calendar::bag::merge`) et chronomètre `load`, `view::build` pour une semaine et un mois, `contacts::load` d'un dossier. Consigne les temps dans `notes/perf.md` (avant/après).
2. Trouver ce qui coûte : lectures de `.eml` (description des rendez-vous), analyse des sacs, requêtes sans index (`message_class LIKE` : essayer `CREATE INDEX IF NOT EXISTS message_class_nocase ON message(message_class COLLATE NOCASE, folder_id)` ; mesurer), `SELECT *` inutiles. Corriger ce que le banc d'essai montre, sans changer les signatures publiques.
3. Chargement paresseux : `view::build` ne doit lire que ce que la période affichée demande (descriptions seulement pour le rendez-vous sélectionné) ; le cache du calendrier ne se recalcule pas en entier à la moindre écriture (invalider par dossier ou par rendez-vous modifié).
4. Contacts : `load(conn, Some(folder), Some(id))` ne doit pas balayer toute la table pour une fiche (requête `WHERE m.id = ?`).
5. Tests sous Linux : les résultats sont identiques avant/après (mêmes occurrences, mêmes contacts) sur la base fabriquée ; le banc d'essai affiche un gain d'au moins 3× sur le chargement ou explique pourquoi non.

Points INCERTAINS : tout gain que tu n'as pu mesurer que sur la base fabriquée (la vraie base de Paul n'est pas dans le dépôt) — le dire, Paul mesurera chez lui.
