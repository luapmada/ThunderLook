# REA-LIMITES — journal

Chaque entrée : action → résultat → cause → contournement → source.

## 2026-10-16 — Étape 1 : outillage

**Environnement**
- REA 6.4.0, installé par npm (`rea --version`).
- Node 24.18.0, Windows 11 Pro x64.
- Pas de JDK, pas de Ghidra.
- Python 3.12.10 présent.

**`rea doctor`**
- Action : `rea doctor`.
- Résultat : `healthy: false`.
  - OK : `node` 24.18.0, `host` win32 x64, enregistrement `claude_code` aligné.
  - KO : `ghidra` (« GHIDRA_INSTALL_DIR is not set »), `hopper` absent, `ida-registration` non configuré.
  - KO aussi : `skill:identity` « stale », alors que la version installée et la version attendue valent toutes deux 37. Cause : je ne sais pas.
  - KO hors sujet : `registration:codex` et `registration:hermes` (autres agents de la machine).
- Limite : la sortie dépasse 30 000 caractères (elle liste les 170 outils), donc elle est tronquée.
- Contournement : `--filter-output checks`, ou `--format json` puis filtrage.

**`rea setup`**
- Action : `rea setup --dry-run`.
- Résultat : `plannedActions: []`.
- Cause : sans prompt interactif, aucun client n'est sélectionné.
- Il faudra `rea setup --client claude_code`. Cette commande écrit dans `~/.claude`, hors du dossier projet.
- Non exécuté : j'attends la confirmation.

**Support Windows natif : README de github.com/morluto/rea, docs `windows-ghidra-p0.md` et `installation.md`**
- Statut : « experimental Windows Ghidra support ».
- Cibles admises : PE natif non managé x86, x86-64, ARMNT, ARM64.
- Prérequis : Ghidra 12.1.x (« verified with 12.1.4 ») et un JDK 64 bits complet, version 21 ou plus.
- Python et PyGhidra ne sont pas requis.
- Limites citées :
  - « Windows P0 remains read-only ».
  - `read_bytes` et `search_strings` ne renvoient rien sur cet hôte.
  - L'inspection de l'image chargée d'un PE renvoie « unsupported ».
  - L'état GUI de Ghidra et les annotations ne sont pas disponibles.
  - Chemins SUBST, UNC, ADS et points de reparse sont refusés.
  - Taille maximale de binaire : non documentée.
  - Délai d'import à froid : 330 s par défaut (`REA_GHIDRA_STARTUP_TIMEOUT_MS`). Dans leurs essais, l'analyse par défaut a dépassé des budgets de 900 et 1 800 s.
- Sources :
  - https://github.com/morluto/rea
  - https://raw.githubusercontent.com/morluto/rea/main/docs/windows-ghidra-p0.md
  - https://raw.githubusercontent.com/morluto/rea/main/docs/installation.md
- Verdict pour `OUTLOOK.EXE` (46,6 Mo, PE x64) : admis en principe. Risques :
  - analyse Ghidra très longue ;
  - pas de recherche de chaînes ;
  - lecture seule.

**Plan B, si la voie Windows échoue**
1. REA sous WSL2 (Linux), avec Ghidra Linux, sur une copie du binaire. C'est la voie la plus documentée.
2. Relever `REA_GHIDRA_STARTUP_TIMEOUT_MS` (3 600 000 ms ou plus) et analyser par fonctions ciblées.
3. Pour tester REA en statique sans enjeu juridique : binaires libres, ou nos propres binaires (voir `notes/juridique.md`).
