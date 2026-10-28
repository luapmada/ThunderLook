# Saisie et réactivité : calendrier, contacts, tâches, notes (audit du 28/10)

Périmètre : `app/ui/src/{calendar, appointment, contact_form, contacts, address_book, dedupe, tasks, notes, reminders,
flag_dialog, item_folders, from_mail}.rs`. Le vol de focus à l'ouverture (commit 9e1947f) n'est pas repris ici.
Vérifié par lecture, `cargo check -p thunderlook-ui --target wasm32-unknown-unknown` (aucun nouvel avertissement) et
`cargo test --workspace --exclude thunderlook` (371 tests, 0 échec, dont 3 nouveaux dans `appointment.rs`). Le cœur Tauri
(`thunderlook`) ne compile pas sous Linux, même sans ces changements : `toast.rs` utilise la crate `windows`.
Rien n'a été essayé à l'écran (pas d'application Windows en cloud).

## Défauts corrigés

| Fichier:ligne | Symptôme | Correction |
|---|---|---|
| `appointment.rs:243` (et blocs 384 à 470) | Chaque frappe dans un champ relançait les blocs `{move \|\| form()…}` : « Intervalle de répétition », « Nombre d'occurrences », « Dernier jour de la répétition », « Appliquer à », « Participants » étaient **recréés à chaque touche**. Dans « Intervalle » ou « Occurrences », le focus partait après le premier chiffre. | Les conditions d'affichage sont des `Memo` booléens (`series`, `scope_shown`, `repeating`, `end_count`…) : un bloc ne se recrée que quand sa condition change. |
| `appointment.rs:316` | Changer la date ou l'heure de début ne déplaçait pas la fin (Outlook garde la durée). Un rendez-vous avancé au lendemain gardait sa fin la veille. | Le début modifié décale la fin d'autant (`shift_with`, `appointment.rs:214`, testé). Modifier la fin seule ne bouge pas le début, comme dans Outlook. |
| `appointment.rs:444`, `:460` | Un champ « Intervalle » ou « Occurrences » vidé redevenait « 1 » tout seul (`unwrap_or(1)`). Pour taper 3, on obtenait 13. | Une valeur vide ou incomplète ne modifie pas le formulaire, et le champ reste vide pendant la frappe. |
| `appointment.rs:246` | L'effet « participants » suivait tout le formulaire. Il ne refaisait rien, mais tournait à chaque frappe. | Il suit le `Memo` `open`. |
| `tasks.rs:149` | Recherche dans les tâches : une réponse lente de `tasks_query` pouvait écraser une plus récente, et la liste ne correspondait plus au texte tapé. | Chaque requête porte un numéro (`StoredValue`), et seule la dernière s'affiche. |
| `tasks.rs:318` | Changer la date de début ne déplaçait pas l'échéance. Outlook lie les deux et garde l'écart. | L'échéance suit le début (même `shift_with`). |
| `tasks.rs:379`, `:394` | « Intervalle » et « Nombre d'occurrences » vidés redevenaient « 1 ». | Même correction que pour le rendez-vous. |
| `notes.rs:92` | Recherche dans les notes : même course que pour `tasks_query`. | Numéro de requête. |
| `notes.rs:243` | Sélecteur « Dossier » de la boîte Note : tout le formulaire, texte compris, était cloné à chaque frappe, une fois par option. | Il lit `folder_id` par `with`, sans copie. |
| `contact_form.rs:128` | Chacun des quelque 40 champs clonait **tout** le formulaire à chaque frappe, photo (data URI) comprise. La saisie ramait. | `read(get)` lit le champ par `with`, sans copie. |
| `contact_form.rs:153` | La photo `<img>` était recréée et redécodée à chaque frappe. | `Memo` `photo` : l'image ne change qu'avec la photo. |
| `dedupe.rs:87`, `:118`, `:138` | En cliquant vite deux groupes ou deux boutons radio, le détail ou l'aperçu du précédent pouvait s'afficher en dernier, avec une fusion présentée sur le mauvais groupe. | Numéros de demande `detail_seq` et `preview_seq`. |
| `flag_dialog.rs:109` | Focus à l'ouverture jamais posé : l'effet tournait avant le montage de la boîte. | `request_animation_frame`. |
| `flag_dialog.rs:160` | Entrée sur le bouton « Annuler » **validait** l'indicateur. | Entrée ne valide que hors bouton (`on_button`). |
| `flag_dialog.rs:78` | La réponse de `mail_reminder_get` d'une ouverture précédente pouvait remplir la boîte suivante. | Numéro d'ouverture, et réponse ignorée si la boîte est fermée. |
| `flag_dialog.rs:178` | Champ « Échéance » sans `aria-label`. | `aria-label="Échéance"` ajouté. |
| `item_folders.rs:401` | Boîte « Nouveau dossier » ou « Renommer » : Entrée sur « Annuler » créait ou renommait le dossier. | Même garde `on_button`. Le champ a aussi reçu `aria-label="Nom du dossier"`. |
| `contacts.rs:617` | Boîte « Liste de distribution » : l'adresse ou le nom à moitié tapés réapparaissaient à l'ouverture suivante. | Remis à vide à la fermeture. |
| `address_book.rs:289` | Carnet d'adresses : l'effet d'ouverture suivait `search.get()` et se relançait au montage du champ. Tout était remis à zéro et le carnet lu deux fois (500 lignes). | Focus par `request_animation_frame` et `get_untracked`. |
| `calendar.rs:277` | Module Calendrier, **dans une boîte ouverte** : Ctrl+N remplaçait le rendez-vous en cours de saisie par un nouveau (saisie perdue), Ctrl+P imprimait le calendrier caché, et Ctrl+Alt+1 à 4 changeait de vue. | `calendar::key` ne fait rien quand une boîte `aria-modal` est ouverte. |
| `calendar.rs:794` | Pg.Préc et Pg.Suiv dans la liste « Fuseaux horaires » changeaient de période. | La touche reste au champ. |
| `calendar.rs:32` | La vue suivait **tous** les réglages : tout réglage changé ailleurs relisait le calendrier et quittait le défilement du mois. | `Memo` `view_kind`. |
| `calendar.rs:41` | Deux relectures aux mêmes paramètres (enregistrement, puis `calendar-changed`) pouvaient répondre dans le désordre, et l'ancienne vue restait affichée. | Numéro de requête en plus des paramètres. |

## Laissés, avec la raison

- `tasks.rs:337`, « % achevé » : il est lié à `on:change` (validation à la sortie du champ), donc rien ne bouge pendant la frappe. Un champ vidé vaut 0 %, comme Outlook. Laissé.
- Tâche, échéance placée avant le début : rien n'est fait. Outlook ramène peut-être le début, mais je ne l'ai pas vérifié. Laissé.
- Le rappel de tâche ne suit pas le début. D'après msoutlook.info, Outlook le décale aussi : à vérifier avant d'imiter.
- Rendez-vous, fin placée avant le début : refus au moment d'enregistrer (cœur), comme Outlook. Laissé.
- Champs `type=date`, `type=time` et `datetime-local` (rendez-vous, tâche, anniversaire, indicateur) : `prop:value` renvoie exactement la valeur reçue, sans reformatage, donc le curseur ne saute pas. Laissé.
- `calendar.rs:956`, cases des calendriers du volet gauche : la liste est recréée après chaque bascule, et le focus clavier sur la case est perdu après Espace. Ce n'est pas une saisie de texte, et il faudrait une liste à clés (`For`). Laissé.
- `calendar.rs:109`, « Fuseaux horaires » : les deux listes sont recréées après un choix, et le focus est perdu après le changement. C'est rare et sans perte de donnée. Laissé.
- `contacts.rs:684`, membres de la liste de distribution : ils sont redessinés à chaque frappe dans « Nom de la liste ». Ce sont des boutons sans champ, donc rien n'est perdu. Laissé.
- `contacts.rs:678` : `autofocus` n'agit pas sur un élément inséré après le chargement, et le champ « Nom de la liste » ne prend pas le focus. Laissé (focus, déjà traité ailleurs).
- `address_book.rs:64` : l'autocomplétion complète toujours le **dernier** destinataire, même quand on corrige au milieu de la ligne (Outlook prend celui sous le curseur). Corriger demande la position du curseur. Laissé.
- `reminders.rs`, `from_mail.rs` : pas de champ de saisie, aucun défaut trouvé.
- **Hors périmètre (`app.rs`, `global_key`)** : avec une boîte Rendez-vous, Tâche, Note, Contact, Indicateur ou Doublons ouverte, F6, Ctrl+E/F3 (le focus part vers la recherche derrière la boîte), Ctrl+1 à 5 et Ctrl+. / Ctrl+, agissent encore. La liste des boîtes au début de `global_key` ne contient ni `appointment`, ni `task_form`, ni `note_form`, ni `contact_form`, ni `flag_dialog`, ni `dedupe_open`. Correctif proposé : y ajouter `document().query_selector("[aria-modal='true']")`, comme `calendar::modal_open`. Un `stop_propagation` dans les boîtes ne suffit pas : Leptos délègue `on:keydown` à `window`, où est aussi branché `global_key`.

## À essayer à l'écran (Paul)

Boîte, bouton qui l'ouvre → champs (`aria-label`) et essai.

1. **Rendez-vous** : Calendrier › ruban « Nouveau rendez-vous » (Ctrl+N), ou double-clic sur un rendez-vous.
   - « Objet », « Emplacement », « Participants », « Catégories », « Description » : taper une phrase ; le focus reste.
   - « Début (date) », « Début (heure) » : avancer le début d'un jour, puis d'une heure ; « Fin (date) » / « Fin (heure) » doivent suivre en gardant la durée. Modifier « Fin (heure) » seule : le début ne bouge pas.
   - « Répéter » = Toutes les semaines, puis « Intervalle de répétition » : effacer et taper 3 (le champ reste vide puis affiche 3, pas 13 ; le focus reste). « Fin de la répétition » = Après N occurrences, puis « Nombre d'occurrences » : taper 12. « Le… », puis « Dernier jour de la répétition ».
   - Dans « Description », Ctrl+N et Ctrl+P : rien ne doit se passer. Ctrl+S enregistre.
   - Série existante : « Appliquer à » reste en place pendant la saisie de l'objet.
2. **Tâche** : Tâches › « Nouvelle tâche » (Ctrl+N), ou double-clic sur une tâche.
   - « Objet », « Catégories », « Description » : saisie normale.
   - « Date de début » et « Échéance » remplies, puis « Date de début » avancée de 2 jours : « Échéance » avance de 2 jours.
   - « Pourcentage achevé » : taper 50, puis Tab.
   - « Répéter » = Chaque semaine, puis « Intervalle » effacé et tapé 2. « Fin de la répétition » = Après N fois, puis « Nombre d'occurrences ».
   - « Rechercher dans les tâches » : taper vite plusieurs lettres, puis en effacer ; la liste correspond au texte final.
3. **Note** : Notes › « Nouvelle note ». « Texte de la note » : taper un long texte, qui reste fluide. Le sélecteur « Dossier » garde son choix. « Rechercher dans les notes » : même essai que pour les tâches.
4. **Contact** : Contacts › « Nouveau contact », ou « Modifier » sur la fiche.
   - Avec une photo (« Choisir une photo »), taper dans « Prénom », « Nom », « Courrier 1 », « Notes » : saisie fluide, pas de clignotement de la photo. « Retirer la photo ».
   - « Anniversaire » : saisir une date.
5. **Liste de distribution** : Contacts › « Nouvelle liste ». « Nom de la liste », puis « Adresse du membre à ajouter » à moitié tapée, puis « Annuler », puis rouvrir : le champ est vide.
6. **Nettoyer les doublons** : Contacts › ruban « Nettoyer les doublons ». Cliquer vite deux groupes l'un après l'autre : le tableau « Fiches à comparer » et l'« Aperçu de la fiche fusionnée » sont ceux du dernier cliqué. Basculer vite des boutons « Reprendre cette valeur : … ».
7. **Indicateur personnalisé** : Courrier, clic droit sur un message › Assurer un suivi › « Personnalisé… ».
   - Le focus est sur « Échéance » à l'ouverture.
   - « Rappel » coché, puis « Date du rappel » et « Heure du rappel ».
   - Tab jusqu'à « Annuler », puis Entrée : la boîte se ferme **sans** poser d'indicateur.
8. **Nouveau dossier** : sélecteur « Dossier » (Tâches ou Notes) › « ＋ Nouveau dossier… ». Taper dans « Nom du dossier », Tab jusqu'à « Annuler », puis Entrée : rien n'est créé.
9. **Carnet d'adresses** : nouveau message › bouton « À… ». « Rechercher un nom ou une adresse » a le focus. Taper : les résultats suivent ce qui est tapé.
10. **Calendrier, en-tête** : « Fuseaux horaires », puis Pg.Suiv dans la liste « Fuseau horaire » : la période affichée ne change pas.
