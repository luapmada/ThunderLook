"""Contrôle des contrastes WCAG 2.x des jetons de DESIGN.md.

Usage : python -I tools/contrast.py
Seuils : texte normal 4.5:1 (AA), grand texte 3:1, composants d'interface et focus 3:1 (1.4.11).
"""
import sys

THEMES = {
    "clair": {"bg": "#FFFFFF", "surface": "#F6F7F9", "border": "#E3E6EA", "text": "#1B1F24",
              "text2": "#5B6470", "accent": "#2F6FDE", "on_accent": "#FFFFFF",
              # Jetons dérivés (notes/design-tokens.md)
              "accent_text": "#2B67CE", "border_strong": "#899099",
              "hover": "#EBECEE", "selected": "#E6EEFB"},
    "sombre": {"bg": "#14171A", "surface": "#1C2024", "border": "#2B3036", "text": "#E8EAED",
               "text2": "#9AA3AE", "accent": "#6EA0FF", "on_accent": "#14171A",
               "accent_text": "#6EA0FF", "border_strong": "#646B73",
               "hover": "#2A2E32", "selected": "#243043"},
}

# (avant-plan, arrière-plan, seuil, usage)
PAIRS = [
    ("text", "bg", 4.5, "texte principal"),
    ("text", "surface", 4.5, "texte principal sur surface"),
    ("text2", "bg", 4.5, "texte secondaire"),
    ("text2", "surface", 4.5, "texte secondaire sur surface"),
    ("accent_text", "bg", 4.5, "texte d'accent (objet non lu, compteurs, liens)"),
    ("accent_text", "surface", 4.5, "texte d'accent sur surface"),
    ("text", "hover", 4.5, "texte sur ligne survolée"),
    ("text2", "hover", 4.5, "texte secondaire sur ligne survolée"),
    ("accent_text", "hover", 4.5, "objet non lu sur ligne survolée"),
    ("text", "selected", 4.5, "texte sur ligne sélectionnée"),
    ("text2", "selected", 4.5, "texte secondaire sur ligne sélectionnée"),
    ("accent_text", "selected", 4.5, "objet non lu sur ligne sélectionnée"),
    ("on_accent", "accent", 4.5, "texte sur bouton d'accent"),
    ("accent", "bg", 3.0, "anneau de focus / indicateur (non-texte)"),
    ("border_strong", "bg", 3.0, "contour de champ (non-texte)"),
    ("border_strong", "surface", 3.0, "contour de champ sur surface (non-texte)"),
    # « border » reste réservé aux séparateurs décoratifs, exemptés par WCAG 1.4.11
]


def lum(hex_color: str) -> float:
    rgb = [int(hex_color[i:i + 2], 16) / 255 for i in (1, 3, 5)]
    lin = [c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in rgb]
    return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2]


def ratio(a: str, b: str) -> float:
    la, lb = sorted((lum(a), lum(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    failures = 0
    for name, t in THEMES.items():
        print(f"\n## Thème {name}\n\n| Paire | Ratio | Seuil | Résultat | Usage |\n|---|---|---|---|---|")
        for fg, bg, threshold, use in PAIRS:
            r = ratio(t[fg], t[bg])
            ok = r >= threshold
            failures += not ok
            print(f"| {fg} {t[fg]} / {bg} {t[bg]} | {r:.2f}:1 | {threshold}:1 | {'OK' if ok else 'ÉCHEC'} | {use} |")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
