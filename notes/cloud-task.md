Contexte : ThunderLook écrit désormais ses rendez-vous dans sa propre base (plus d'Outlook). Une série périodique se crée et on peut supprimer UNE occurrence (`recur.rs` : `encode`, `with_deleted_instance` ; `write.rs` : `create` avec `edit.repeat`, `delete_occurrence`). Il manque : modifier une série existante, et modifier UNE occurrence (une exception). La lecture (`parse`, `expand::occurrences`) gère déjà les exceptions.

Périmètre : crate `crates/calendar` UNIQUEMENT (`recur.rs`, `write.rs`, `expand.rs` si nécessaire, tests). Ne touche ni `app/`, ni l'interface, ni les autres crates. Branche `cloud/series-exceptions` depuis `main`, jamais de push sur `main`.

À faire :
- `recur.rs` : `Recurrence::encode()` doit aussi écrire les séries AVEC exceptions : `ExceptionInfo` (champs modifiés selon les drapeaux `OVERRIDE_*`), `ReservedBlock1`, `ExtendedException` (dont `ChangeHighlight` quand la version d'écriture est ≥ 0x3009), `ReservedBlock2`. Miroir exact de `parse`. Aujourd'hui `encode` ne sait écrire que 0 exception.
- `write.rs` :
  - `update_series(conn, data_dir, message_id, edit, rule: Option<Repeat>)` : modifie les heures et/ou la périodicité d'une série existante. Si la périodicité change, les exceptions et les occurrences supprimées sont abandonnées (le dire dans le retour : nombre perdu). Si seules les heures changent, le reste est conservé.
  - `update_occurrence(conn, data_dir, message_id, date, edit)` : crée l'exception d'UNE occurrence (`date` = jour d'origine `AAAA-MM-JJ`) : la date rejoint `deleted` ET `modified`, une `Exception` porte les champs réellement modifiés (objet, lieu, début, fin, disponibilité, rappel, journée entière). Une exception déjà présente pour cette date est remplacée.
  - NE CHANGE PAS la struct `Edit` ni les signatures existantes (`app/src-tauri/src/shared_calendar.rs` s'en sert) : ajoute des fonctions.
- Fixtures : `crates/calendar/tests/fixtures/recur_*.hex` (blobs réels de `PidLidAppointmentRecur`, un par ligne en hexadécimal, avec un commentaire). Teste `encode(parse(b)) == b` pour chacun ; ceux qui diffèrent légitimement (ancienne forme d'Outlook, `0xFFFFFFFF` pour « jamais ») sont listés dans le test avec la raison.
- Tests (`cargo test -p thunderlook-calendar`, sous Linux) : aller-retour avec exceptions ; `expand::occurrences` après `update_occurrence` (l'occurrence déplacée apparaît à sa nouvelle heure, l'ancienne a disparu, les autres sont intactes) ; `update_series` (heures changées, périodicité changée) ; `delete_occurrence` après une exception.

Points INCERTAINS : ne pas deviner en silence. Les consigner dans `REA-LIMITES.md` (section « À vérifier avec REA/Outlook ») :
- ordre et contenu exacts d'`ExtendedException`, `ChangeHighlight` et des `ReservedBlock` ;
- `StartDateTime`, `EndDateTime` et `OriginalStartDate` d'une exception (minutes locales depuis 1601) ;
- besoin éventuel d'un message intégré (pièce jointe) par exception : ThunderLook n'en écrit pas, la lecture n'en a pas besoin.

Règles (cf. `notes/handoff.md`) : réponses courtes, « je ne sais pas » plutôt qu'une invention, aucun test d'interface, aucune écriture dans Outlook. À la fin : push, puis un résumé de 10 lignes maximum (fait, reste, ce que je dois vérifier à l'écran chez moi).
