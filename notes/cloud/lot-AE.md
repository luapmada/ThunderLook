# Lot AE — lire le vCalendar 1.0 et le vCard 2.1 que MPE écrit

Branche : `cloud/lot-ae-vcal10`. Lis d'abord `notes/cloud/README.md`, `docs/mpe-fichiers.md` et `notes/mpe-plan.md` § 1.5 : le fichier de MPE pour le calendrier (`Organizer.dat`) est du **vCalendar 1.0** (369 `VEVENT`, `X-IRMC-LUID`, `X-OUTLOOK-UID`, dates en UTC `20261023T230000Z`), les contacts du **vCard 2.1** (`ENCODING=QUOTED-PRINTABLE`, `CHARSET=UTF-8`, `X-IRMC-LUID`). Il est possible que la cible « fichier » de MPE écrive ce dialecte plutôt que de l'iCal 2.0 : l'import de ThunderLook doit le lire sans perte.

Périmètre de fichiers : NOUVEAU `crates/exchange/src/vcal10.rs` (+ ligne `mod` dans `crates/exchange/src/lib.rs`), `crates/exchange/src/import.rs` UNIQUEMENT pour la détection de version (`VERSION:1.0` → `vcal10`) dans la branche calendrier, `crates/contacts/src/vcard.rs` seulement si le 2.1 QP échoue, NOUVEAU `crates/exchange/tests/vcal10.rs`. Le lot AG édite la branche des notes de `import.rs` : n'y touche pas.

À faire :
1. Lecteur vCalendar 1.0 : `VEVENT` (DTSTART/DTEND UTC ou locaux, journée entière, SUMMARY/LOCATION/DESCRIPTION en QP, CATEGORIES, `AALARM`/`DALARM`), règles de récurrence 1.0 (`D1 #0`, `W1 MO TU #10`, `MD1 15 #0`, `MP1 1+ MO #0`, `YM1 10 #0`, `YD1`, avec fin `#N` ou date), `EXDATE`, `X-IRMC-LUID`/`X-OUTLOOK-UID` comme identifiants. Convertir vers les structures déjà utilisées par l'import des séries (lot AA, `crates/calendar/src/ics_series.rs`) ; ce que le modèle ne sait pas représenter est refusé avec un avertissement visible, jamais ignoré en silence.
2. Fabriquer des échantillons SYNTHÉTIQUES fidèles au format décrit ci-dessus pour les tests (aucune donnée réelle).
3. Vérifier la lecture de vCard 2.1 QP multi-lignes UTF-8 (accents, `=\r\n`), `TEL;CELL`, `TEL;HOME;VOICE`, `PHOTO;ENCODING=BASE64`.
4. `cargo test -p thunderlook-exchange -p thunderlook-contacts`.

Aucune écriture dans Outlook. Fin : push, résumé de 10 lignes.
