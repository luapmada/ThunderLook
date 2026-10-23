# Performance du calendrier, des contacts, des tâches et des notes (lot F)

Banc d'essai : `cargo run --release -p thunderlook-calendar --example bench -- <dossier temporaire> [courriers de remplissage]`
(4 200 rendez-vous dont 400 séries, 10 000 contacts, 440 notes, + courrier de remplissage : sacs d'environ 1,5 Ko).
Meilleur de 3 à 5 passages, **cache disque chaud** (Linux, base fabriquée). L'empreinte (`digest`) est identique avant et après.

| Mesure (ms)                        | avant, 60 000 courriers | après | avant, 500 000 courriers (1 Go) | après |
|------------------------------------|------------------------:|------:|--------------------------------:|------:|
| `calendar::load`                   | 98                      | 22    | 527                             | 22    |
| `view::build` semaine              | 2,0                     | 2,0   | 2,0                             | 2,0   |
| `view::build` mois                 | 4,6                     | 4,5   | 8,0                             | 4,4   |
| `contacts::load` dossier (10 000)  | 91                      | 66    | 323                             | 69    |
| `contacts::load` une fiche         | 37                      | 0,0   | 264                             | 0,0   |
| `items::notes`                     | 91                      | 2,5   | 546                             | 2,5   |

## Ce qui coûtait
- **Balayage de la table `message`** : `message_class = … OR LIKE …` n'avait aucun index ; chaque ouverture de module lisait
  tout le courrier (coût proportionnel à la taille de la base, pas au nombre de rendez-vous). Après : index
  `message_class_nocase ON message(message_class COLLATE NOCASE, folder_id)` (schéma v11) + `LIKE 'IPM.X%'` seul (le `=` exact et le
  `LIKE 'IPM.X.%'` sont refiltrés en Rust, mêmes résultats). Profite aussi à `items::tasks`/`notes`/`folders_of`, qui utilisaient déjà `LIKE`.
- **Fiche de contact** : `(?1 IS NULL OR m.folder_id = ?1) AND (?2 IS NULL OR m.id = ?2)` interdit tout index → `WHERE m.id = ?`.
- **Catégories** : la jointure partait de `message_category` ; `CROSS JOIN` part désormais des seuls rendez-vous.
- **Reste du chargement du calendrier** (22 ms) : analyse des 4 200 sacs MAPI. `view::build` (2–5 ms) ne lit déjà aucun `.eml` :
  les descriptions ne sont lues que par `description()` (formulaire, export), donc rien à rendre paresseux de plus.

## Cache du calendrier (app/src-tauri, `CalendarCache`)
Une écriture ou un import ne vide plus le cache : `clear_calendar` le marque périmé, et la lecture suivante compare les empreintes
(`thunderlook_calendar::stamps` : dossier, `modified_at`, taille du sac ; sans lire les sacs) pour ne relire que les rendez-vous
modifiés, ajoutés ou supprimés (`thunderlook_calendar::refresh`, testé : résultat identique à `load`). Au-delà de la moitié de
changements, lecture complète. Les calendriers importés (`.ics`) sont relus entiers à chaque rafraîchissement.

## À vérifier chez Paul (non mesurable ici)
- Les chiffres ci-dessus sont ceux d'une base **fabriquée**, cache chaud ; la vraie base (4,3 Go) et les lectures à froid ne sont pas
  dans le dépôt. Sur disque, le gain est attendu au moins aussi grand (on ne lit plus que ~4 200 lignes au lieu de toute la table),
  mais le temps réel de « Chargement du calendrier… » reste à mesurer chez lui, avec `bench` ou à l'écran.
- **Premier lancement après mise à jour** : la création de l'index lit toute la table `message` une fois (plusieurs secondes à
  dizaines de secondes sur 4,3 Go) ; ensuite plus rien.
- Le schéma passe à la version 11 (ajout d'un index uniquement) : une version plus ancienne de ThunderLook refusera d'ouvrir la base.
- Code de `app/src-tauri` non compilé ici (bibliothèques GTK absentes) : relu seulement ; `cargo check` à faire chez lui.
- Non traité (hors périmètre de fichiers) : le rechargement complet des listes Contacts, Tâches et Notes à chaque changement
  côté interface/commandes Tauri (`items_conn`, `task_*`, `note_*`) ; `items::notes` relit les 440 `.eml` à chaque appel.
