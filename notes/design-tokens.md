# Jetons de design (J0)

Base : la palette de `DESIGN.md`, inchangée. Le contrôle WCAG (`python -I tools/contrast.py`) a révélé deux usages en échec. J'ai donc ajouté des **jetons dérivés**, réservés à ces usages.

| Jeton | Clair | Sombre | Origine |
|---|---|---|---|
| `--bg` | `#FFFFFF` | `#14171A` | DESIGN.md |
| `--surface` | `#F6F7F9` | `#1C2024` | DESIGN.md |
| `--border` | `#E3E6EA` | `#2B3036` | DESIGN.md (séparateurs décoratifs uniquement) |
| `--text` | `#1B1F24` | `#E8EAED` | DESIGN.md |
| `--text-2` | `#5B6470` | `#9AA3AE` | DESIGN.md |
| `--accent` | `#2F6FDE` | `#6EA0FF` | DESIGN.md (aplats, focus, indicateurs) |
| `--on-accent` | `#FFFFFF` | `#14171A` | Texte posé sur un aplat d'accent |
| `--accent-text` | **`#2B67CE`** | `#6EA0FF` | Dérivé : `#2F6FDE` ne fait que 4,40:1 sur la surface et 4,1:1 sur survol/sélection |
| `--border-strong` | **`#899099`** | **`#646B73`** | Dérivé : `--border` (1,25:1 et 1,35:1) ne suffit pas pour un contour de champ (1.4.11, 3:1) |
| `--hover` | `#EBECEE` | `#2A2E32` | Survol de ligne (surface + 5 % ou 7 % de texte) |
| `--selected` | `#E6EEFB` | `#243043` | Ligne sélectionnée (fond + 12 % ou 18 % d'accent) |

**Résultat** : 32 paires contrôlées, 32 conformes AA (texte 4,5:1, non-texte 3:1). Détail : `data/analysis/contrast.md`, non versionné, régénérable.

**Autres jetons** :
- grille 4 px, rayons 6 px, cibles cliquables ≥ 32 px ;
- police Inter (OFL, embarquée localement via `@fontsource-variable/inter`), base 14 px, interligne 1,5 ;
- densité « confortable » (défaut) ou « compacte », par variables d'espacement ;
- icônes Lucide (ISC).
