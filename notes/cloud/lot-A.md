# Lot A — modifier une série ou une occurrence depuis la boîte « Rendez-vous »

Branche : `cloud/lot-a-series-ui`. Lis d'abord `notes/cloud/README.md`.

Contexte : `crates/calendar/src/write.rs` sait maintenant `update_series(conn, data_dir, message_id, edit, rule: Option<Repeat>) -> SeriesChange` et `update_occurrence(conn, data_dir, message_id, date, edit)`. L'interface ne les appelle pas encore : la boîte « Rendez-vous » traite une série comme « heures et périodicité non modifiables ».

Périmètre de fichiers : `crates/model/src/calendar.rs`, `app/src-tauri/src/shared_calendar.rs`, `app/ui/src/appointment.rs`. Rien d'autre.

À faire :
1. `AppointmentForm` (model) : ajouter, avec `#[serde(default)]`, `occurrence: String` (jour `AAAA-MM-JJ` de l'occurrence ouverte, vide si ouvert depuis la série), `scope: String` (`"occurrence"` ou `"series"`), `series_date: String` (premier jour de la série).
2. `appointment_form` (Tauri) : pour un rendez-vous périodique (`form.recurring`), renseigner `series_date` (jour de `form.start`), `repeat_every = 1`, `repeat_count = 10`. Si l'identifiant porte `@AAAA-MM-JJ` : `occurrence = ce jour`, `scope = "occurrence"`, et `start`/`end` recalculés = jour de l'occurrence + heure de la série, même durée. Sinon `scope = "series"`.
3. `appointment_save` (Tauri), pour un rendez-vous existant `form.recurring` :
   - `scope == "occurrence"` → `update_occurrence(.., &form.occurrence, &edit_of(&form))` ;
   - sinon (série) → le premier jour reste `series_date` ; seules l'heure de début et la durée du formulaire comptent ; appeler `update_series(.., repeat_of(&form))` où `repeat_of` factorise la lecture des champs `repeat*` (aujourd'hui dans `edit_of`, qui garde `repeat: None` pour un rendez-vous existant) ; message de retour qui mentionne `lost_exceptions + lost_deleted` s'ils sont non nuls.
4. `appointment.rs` (interface) : pour un rendez-vous périodique existant, une liste « Appliquer à : Cette occurrence seulement / Toute la série » (« Cette occurrence » seulement si `occurrence` non vide) ; en portée « série » : champs date désactivés, heures actives, ligne « Répéter » affichée avec l'option vide libellée « Inchangée » ; en portée « occurrence » : tout est modifiable sauf la répétition. Retirer l'ancienne note « ne se modifient pas encore ».
5. Ne change aucune signature publique de `crates/calendar`.

Vérification : `cargo check -p thunderlook` (si les bibliothèques système de Tauri manquent sous Linux, le dire) et `cargo check -p thunderlook-ui --target wasm32-unknown-unknown`. Décris dans le résumé les 4 scénarios que Paul doit essayer à l'écran (modifier l'heure d'une occurrence ; changer l'objet de toute la série ; passer d'hebdomadaire à mensuel ; supprimer l'exception).
