# Lot U — modifier une série existante et UNE occurrence (exceptions)

Branche : `cloud/lot-u-series`. Lis d'abord `notes/cloud/README.md`, `notes/handoff.md` (§ 23/10) et `crates/calendar/src/{recur,write}.rs`.

Périmètre de fichiers : `crates/calendar/src/{recur,write,view}.rs` (+ tests de `crates/calendar/tests/`), `app/ui/src/appointment.rs`, `app/ui/src/calendar.rs` (branchements seulement), NOUVEAU `app/src-tauri/src/series_cmds.rs` (une ligne `mod`, enregistrements dans `invoke_handler`). Rien d'autre.

À faire :
1. Modifier une série existante : heures de début/fin, périodicité (jour/semaine/mois/an, fin jamais/N/date), sujet, lieu, catégorie ; ré-encoder `PidLidAppointmentRecur` sans perdre les exceptions et suppressions déjà présentes.
2. Modifier UNE occurrence (exception) : encoder une exception (date de l'occurrence d'origine, nouvelle date/heure, sujet, lieu), la relire dans `view::build` (l'occurrence modifiée remplace l'originale, les autres restent).
3. Boîte de dialogue à la manière d'Outlook : « Modifier cette occurrence / toute la série ».
4. Tests : aller-retour encode → décode, séries avec exceptions + suppressions, bornes (fin de mois, 29 février, fuseau).
5. `cargo test -p thunderlook-calendar` ; `cargo check -p thunderlook-ui --target wasm32-unknown-unknown`.

Aucune écriture dans Outlook. Fin : push, résumé de 10 lignes (dont ce que Paul doit vérifier à l'écran).
