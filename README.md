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
| 3. Plan : architecture et jalons (`PLAN.md`) | Proposé, en attente d'accord |
| Code | Pas commencé (attend l'accord sur `PLAN.md`) |

## Fichiers
- `SPEC.md` : fonctions inventoriées et priorités (le mail d'abord).
- `DESIGN.md` : direction visuelle.
- `REA-LIMITES.md` : ce qui marche et ce qui échoue avec REA.
- `PLAN.md` : architecture, stack et jalons.
- `notes/` : analyses (interface, formats) et points juridiques.
- `tools/` : outils d'analyse (relevé UI Automation, en-têtes PST, sondes libpff et `outlook-pst`).
