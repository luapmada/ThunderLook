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

**Installation (autorisée le 2026-10-16)**
- Ghidra 12.1.4 : 570 Mo, SHA-256 vérifié (`ddac49f9…`). Extrait dans `A:\PROJETS\IA-LAB\ghidra\ghidra_12.1.4_PUBLIC`.
- JDK Temurin 21.0.12.1 : zip de 205 Mo, SHA-256 vérifié (`f9d6e191…`). Extrait dans `A:\PROJETS\IA-LAB\ghidra\jdk-21.0.12.1`.
  - Dossier renommé sans le `+`, par prudence : la doc REA refuse les métacaractères dans les chemins.
- Variables utilisateur : `GHIDRA_INSTALL_DIR`, `JAVA_HOME`, et `%JAVA_HOME%\bin` ajouté en tête du `Path` utilisateur.
- `rea setup --client claude_code --yes` : a ajouté `GHIDRA_INSTALL_DIR` et `JAVA_HOME` dans la section `env` de l'enregistrement MCP (`~/.claude.json`), avec une sauvegarde `.claude.json.rea.backup`.

**`rea doctor` après installation**
- Ghidra entièrement vert : installation, version 12.1.4, headless, décompilateur natif win_x86_64, Java 21 (JDK 64 bits), « native authority » (Job Object, DACL privée, NTFS local).
- Toujours KO :
  - `hopper` : non supporté sous win32, d'après `binary_session` ;
  - `ida` : non configuré ;
  - `skill:identity` reste « stale ».
    - `rea setup --skill-client claude_code --dry-run` ne prévoit aucune action.
    - `identity.skill` n'indique que `installed_version: 37`, sans le motif.
    - Limite : diagnostic non actionnable.
- `healthy` reste `false` à cause d'éléments hors périmètre (Hopper, IDA, autres agents). L'indicateur global n'est donc pas exploitable : il faut lire chaque vérification.

**Limites constatées sur le serveur MCP**
- Le serveur REA lancé avant l'installation garde son environnement de départ : `binary_session` → Ghidra `not_configured`.
  - Il faut redémarrer le client.
  - Contournement : la CLI `rea … --provider ghidra --snapshot <fichier>`, avec les variables posées dans le shell. Le snapshot évite de réanalyser le binaire à chaque commande.
- `binary_session` renvoie 243 000 caractères (inventaire complet de 170 outils), au-delà de la limite de sortie de Claude Code.
  - Contournement : sauvegarde dans un fichier, puis filtrage par script.
- `observe_native_ui` et `capture_native_ui_scenario` sont « bound to the active Mach-O target » : macOS uniquement.
  - Pas d'observation de l'interface Windows par REA.
  - Contournement : script UI Automation maison (étape 2).

## 2026-10-16 — Étape 2 : analyse

**Native `MSPST32.DLL` (copie, 3 Mo, x64) par la CLI**
- `rea inspect --provider ghidra --snapshot …` : 87 s, puis `binary_overview` OK.
  - 9 segments, 8 110 procédures, 2 594 chaînes.
  - Profil d'analyse exposé : `no_return_repair: disabled-windows-p0`, `analyzer_preset: ghidra-default`.
- `rea search … 'pst'` (chaînes) : **fonctionne**, contrairement à ce que la doc Windows P0 laissait craindre.
  - Résultats utiles : `TardisPST::DiskFileOpenW`, `LargePST::DiskFileOpenW`, chemin source `src\outlook\mspst\ndb\fcache.cpp`, clés de registre `…\Outlook\PST`.
- `rea search … --kind procedures 'Crypt'` : seules les importations sont nommées (binaire sans symboles). Les fonctions internes ne sont que des adresses.
- **Limite majeure en CLI : le snapshot ne garde pas la base Ghidra.**
  - Chaque commande réimporte et réanalyse le binaire : 85 à 87 s pour 3 Mo.
  - Le fichier `.snapshot` fait 7,9 Ko, ce n'est pas une base d'analyse.
  - Pour `OUTLOOK.EXE` (46,6 Mo), une investigation à plusieurs requêtes en CLI est irréaliste.
  - Contournement : le serveur MCP, qui garde la session ouverte. Il faut redémarrer Claude Code pour qu'il voie Ghidra.
- Erreur d'usage, de ma part : un chemin contenant `..` est refusé (« Ambiguous Windows path component is unsupported »). C'est conforme à la doc. Toujours passer des chemins absolus normalisés.

**Native `OUTLOOK.EXE` (copie, 46,6 Mo, x64) par la CLI**
- `rea search OUTLOOK.EXE 'CRuleElement' --provider ghidra`, avec `REA_GHIDRA_STARTUP_TIMEOUT_MS=14400000` : **1 210 s (20 min)** pour l'import, l'analyse automatique Ghidra par défaut et la recherche. Sortie 0, pas d'erreur.
  - Les essais de la doc REA dépassaient 900 et 1 800 s. Ici ça passe, sur Ryzen 7 7700 avec 63 Go de RAM.
- Résultat : la chaîne `CRuleElement` a été trouvée à l'adresse `0x142216ad8`.
  - C'est le nom de classe sérialisé dans le flux de règles et les `.rwz`.
  - C'est le point d'entrée pour remonter aux fonctions de (dé)sérialisation des règles via `xrefs`.
- Limite pratique : chaque requête CLI suivante (xrefs, décompilation) coûterait encore environ 20 min, puisque le snapshot ne garde pas la base.
  - L'investigation est reportée au jalon J6, via le serveur MCP après redémarrage de Claude Code, et seulement si l'import des règles bute sur des champs inconnus.

**Observation d'interface**
- `observe_native_ui` et `capture_native_ui_scenario` sont macOS uniquement.
- Contournement : `tools/uia_dump.ps1` (UI Automation + `PrintWindow`), `tools/uia_menu.ps1` et `tools/uia_summarize.py`.
- Ce n'est pas un test de REA, mais l'analyse d'interface Windows passe entièrement hors REA.

**Décompilation devenue inutile pour les formats du jalon 1**
- Ce qu'il faut importer est documenté : [MS-PST], [MS-OXOCFG], [MS-OXORULE], et la spec communautaire des règles `.rwz`.
- Vérifié sur les FAI de la copie avec `tools/pst-probe`.
- REA sur `OUTLOOK.EXE` reste justifié pour les quelques champs inconnus du flux de règles. C'est un test des limites de REA sur un très gros binaire.

**Plan B, si la voie Windows échoue**
1. REA sous WSL2 (Linux), avec Ghidra Linux, sur une copie du binaire. C'est la voie la plus documentée.
2. Relever `REA_GHIDRA_STARTUP_TIMEOUT_MS` (3 600 000 ms ou plus) et analyser par fonctions ciblées.
3. Pour tester REA en statique sans enjeu juridique : binaires libres, ou nos propres binaires (voir `notes/juridique.md`).
