"""Rend la fiche projet en PNG (3200 x 2262, comme les autres fiches) et en PDF (A4 paysage, pour l'impression).
usage : python docs/fiche-projet/rendre.py"""
from pathlib import Path
from playwright.sync_api import sync_playwright

ICI = Path(__file__).resolve().parent
with sync_playwright() as p:
    b = p.chromium.launch()
    pg = b.new_page(viewport={"width": 1600, "height": 1131}, device_scale_factor=2)
    pg.goto((ICI / "fiche-projet-thunderlook.html").as_uri())
    pg.wait_for_load_state("networkidle"); pg.evaluate("document.fonts.ready")
    pg.screenshot(path=str(ICI / "fiche-projet-thunderlook.png"))
    pg.pdf(path=str(ICI / "fiche-projet-thunderlook.pdf"), width="1600px", height="1131px", print_background=True)
    b.close()
print("fiche rendue :", ICI)
