"""Rend la fiche projet en PNG (3200 x 2262, comme les autres fiches) et en PDF (proportions A4 paysage), avec Edge
en mode headless (déjà présent sous Windows ; Playwright n'est pas installé sur BE).
usage : python docs/fiche-projet/rendre.py"""
import subprocess
from pathlib import Path

ICI = Path(__file__).resolve().parent
EDGE = r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
PAGE = (ICI / "fiche-projet-thunderlook.html").as_uri()
COMMUN = [EDGE, "--headless", "--disable-gpu", "--hide-scrollbars", "--virtual-time-budget=3000"]
subprocess.run(COMMUN + ["--window-size=1600,1131", "--force-device-scale-factor=2",
                         f"--screenshot={ICI / 'fiche-projet-thunderlook.png'}", PAGE], check=True)
subprocess.run(COMMUN + ["--no-pdf-header-footer", f"--print-to-pdf={ICI / 'fiche-projet-thunderlook.pdf'}", PAGE], check=True)
print("fiche rendue :", ICI)
