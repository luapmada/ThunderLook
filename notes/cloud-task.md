Contexte : l'échange avec le téléphone passera par des fichiers `.vcf`, `.ics`, `.vnt` (voir `notes/mpe-plan.md`, option B). L'export/import iCal des rendez-vous existe déjà (`crates/calendar/src/ics.rs`). Il manque les convertisseurs des contacts, des tâches et des notes.

Périmètre : NOUVEAUX fichiers seulement, fonctions pures texte ↔ formulaires. Aucune connexion SQLite, aucune écriture en base.

À écrire :
- `crates/contacts/src/vcard.rs` (+ `pub mod vcard;` dans `lib.rs`) :
  - `export(forms: &[ContactForm], version: Version) -> String`, avec `enum Version { V21, V30 }`.
  - `parse(text: &str) -> Result<Vec<Parsed>, String>`, avec `Parsed { form: ContactForm, uid: Option<String>, warnings: Vec<String> }`.
  - Champs de `ContactForm` : lire `crates/model/src/contacts.rs`. La photo n'existe pas dans le formulaire : l'ignorer avec un avertissement.
- `crates/items/src/vnote.rs` : `NoteForm` ↔ VNOTE 1.1 (`BEGIN:VNOTE`, `BODY` avec `ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8`, `X-OUTLOOK-UID` conservé en lecture et en écriture, dates si présentes). La couleur n'a pas d'équivalent : ne pas l'inventer.
- `crates/items/src/vtodo.rs` : `TaskForm` ↔ VTODO (`UID`, `SUMMARY`, `DTSTART`, `DUE`, `STATUS`, `PRIORITY`, `DESCRIPTION`, `CATEGORIES`, `PERCENT-COMPLETE`). Lire `task_save` dans `crates/items/src/lib.rs` pour la convention exacte de `status` et `importance`. Ne rien supposer.

Contraintes de lecture (le format exact qu'écrit MPE n'est pas établi) :
- accepter vCard 2.1, 3.0 et 4.0 ; lignes repliées ; fins de ligne CRLF ou LF ;
- `QUOTED-PRINTABLE` avec `CHARSET=UTF-8` ou `CHARSET=WINDOWS-1252` ; échappements `\n` `\,` `\;` ;
- types de téléphone et d'adresse : `CELL`, `WORK`, `HOME`, `VOICE`, `PREF` ;
- un enregistrement illisible ne fait pas échouer le fichier : avertissement, on passe au suivant.

Ne PAS toucher : `crates/calendar/src/recur.rs`, `write.rs`, `expand.rs`, `ics.rs`, `app/`, l'interface, MPE, les réglages. Pas de nouvelle dépendance sans nécessité (le quoted-printable s'écrit à la main).

Tests (`cargo test -p thunderlook-contacts -p thunderlook-items`, sous Linux) :
- aller-retour `parse(export(x)) == x` pour chaque type et chaque version ;
- lecture d'une vCard 2.1 avec accents en quoted-printable, lignes repliées, notes sur plusieurs lignes ;
- cas dégradés (champ vide, enregistrement tronqué).

Points INCERTAINS : ne pas deviner en silence. Les consigner dans `notes/mpe-plan.md` (section « À valider sur le S20 FE avec Paul ») :
- version de vCard que MPE accepte en écriture (2.1 ou 3.0) ;
- prise en charge des VTODO par MPE ;
- usage des `X-` (UID) pour retrouver un élément.

Règles (cf. `notes/handoff.md`) : réponses courtes, « je ne sais pas » plutôt qu'une invention, aucun test d'interface. Branche `cloud/convertisseurs-mpe` depuis `main`, jamais de push sur `main`. À la fin : push, puis un résumé de 10 lignes maximum (fait, reste, ce que je dois vérifier chez moi).
