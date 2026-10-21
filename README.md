# ThunderLook
a REA work on MSoutlook21

## Objectif
- Client de messagerie de bureau pour Windows 11, 100 % local.
- Reprend l'ergonomie d'Outlook 2021 classique : barre de commandes à onglets, trois volets, vues, calendrier, contacts, tâches, règles, recherche.
- Habillage visuel original (`DESIGN.md`). Projet sans lien avec Microsoft.
- Pas de compte Microsoft, pas de service cloud payant.
- Les données sont importées depuis des **copies** de fichiers PST, jamais depuis les originaux.
- Second objectif : éprouver l'outil de rétro-ingénierie REA. Journal dans `REA-LIMITES.md`.
- Cadre légal : `notes/juridique.md`. Aucun code, icône, image, texte, police ni marque de Microsoft.

## État d'avancement
| Étape | État |
|---|---|
| 0. Cadrage : inventaire et priorités (`SPEC.md`) | Fait (2026-10-16) |
| 1. Outillage : Ghidra, `rea setup`, `rea doctor` | Fait (2026-10-16) : Ghidra 12.1.4 et JDK 21 opérationnels pour REA |
| 2. Analyse : interface, formats, `notes/` | Fait (2026-10-16) : `notes/ui-reference.md`, `notes/formats.md` |
| 3. Plan : architecture et jalons (`PLAN.md`) | Validé (2026-10-16) ; interface passée en Rust (Leptos) |
| J0. Socle : Tauri + Leptos, 3 volets, barre de commandes, thème, clavier | Fait (2026-10-16) |
| J1. Import PST en lecture seule | Fait (2026-10-18) : 3 copies importées sans perte, 15 Go en 3 min (`PLAN.md` §5) |
| J2. Lecture : dossiers, liste, volet de lecture sur les données importées | Fait (2026-10-19) : 22 188 éléments ouverts en 145 ms, HTML isolé, pièces jointes (`PLAN.md` §5) |
| J3. Organisation : lu/non lu, déplacer, indicateurs, catégories, dossiers, favoris, annulation | Fait (2026-10-19), sans les conversations (J3b) |
| J3b. Conversations et affichages | À faire |
| J4. Recherche : index plein texte, mots-clés, étendues, dossiers de recherche | Fait (2026-10-19) : 252 846 éléments indexés en 4 min 19, recherches en moins de 300 ms |
| J9. Calendrier : vues Jour, Semaine de travail, Semaine, Mois, superposition, import/export `.ics` et CSV (lecture seule, Outlook fait foi) | Fait (2026-10-22), v0.9.0 : 4 198 rendez-vous lus, occurrences identiques à Outlook sur 2023-2026 (`PLAN.md` §5) |

## Fichiers
- `SPEC.md` : fonctions inventoriées et priorités (le mail d'abord).
- `DESIGN.md` : direction visuelle.
- `REA-LIMITES.md` : ce qui marche et ce qui échoue avec REA.
- `PLAN.md` : architecture, stack et jalons.
- `notes/` : analyses (interface, formats) et points juridiques.
- `app/` : application (cœur Tauri `app/src-tauri`, interface Leptos `app/ui`) ; `crates/model` : types partagés.
- `crates/store`, `crates/mime`, `crates/import-pst` : stockage SQLite + `.eml`, conversions de contenu, import PST (`tl-import`).
- `crates/mailbox` : lecture pour l'interface (arborescence, vues triées servies par tranches, message complet).
- `notes/integration-adi.md` : contrat d'intégration proposé avec ADI Facturation.
- `vendor/outlook-pst` : copie corrigée du lecteur PST de Microsoft (MIT), correctifs dans `PATCH.md`.
- `tools/` : outils d'analyse et de vérification (relevé et test clavier UI Automation, contrastes, en-têtes PST, sondes libpff et `outlook-pst`).
- `notes/dev.md` : construire et tester.
