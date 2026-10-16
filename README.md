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
| 1. Outillage : Ghidra, `rea setup`, `rea doctor` | En cours : téléchargements en attente de confirmation |
| 2. Analyse : interface, formats, `notes/` | À faire |
| 3. Plan : architecture et jalons (`PLAN.md`) | À faire |
| Code | Pas commencé (attend l'accord sur `PLAN.md`) |

## Fichiers
- `SPEC.md` : fonctions inventoriées et priorités (le mail d'abord).
- `DESIGN.md` : direction visuelle.
- `REA-LIMITES.md` : ce qui marche et ce qui échoue avec REA.
- `notes/` : analyses et points juridiques.
