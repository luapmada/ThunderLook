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

**`OUTLOOK.EXE` par le serveur MCP (2026-10-19, session de Claude Code redémarrée, Ghidra visible)**
- `open_binary` (fournisseur `ghidra`) : immédiat, avec l'identité du binaire et son architecture x86_64. L'analyse est paresseuse.
- `search_strings 'CRuleElement'` : **échec deux fois**, `provider_timeout` au bout de **330 000 ms** (`retryable: true`).
  - Le deuxième essai repart de zéro : l'analyse partielle n'est pas conservée entre deux requêtes.
- Cause probable : le serveur MCP garde son délai de démarrage de Ghidra par défaut (330 s). La CLI n'a réussi qu'avec `REA_GHIDRA_STARTUP_TIMEOUT_MS=14400000`, pour 20 min d'analyse.
- **Limite** : avec la configuration MCP par défaut, REA ne peut pas analyser un binaire de la taille d'`OUTLOOK.EXE` (46,6 Mo).
  - Le message d'erreur conseille une « requête plus petite ». Mais toute requête déclenche l'analyse complète du programme.
- **Contournement proposé, non appliqué** :
  - ajouter `REA_GHIDRA_STARTUP_TIMEOUT_MS=14400000` dans la section `env` du serveur `rea` (`~/.claude.json`) ;
  - redémarrer Claude Code ;
  - relancer `search_strings`, puis `xrefs 0x142216ad8`.

  C'est une modification de la configuration de Claude Code : elle est laissée à Paul.

**`OUTLOOK.EXE` par le serveur MCP, délai relevé (2026-10-19, `REA_GHIDRA_STARTUP_TIMEOUT_MS=14400000` appliqué par Paul)**
- `search_strings 'CRuleElement'` : **réussi**, `0x142216ad8`, même résultat que la CLI. La requête dépasse 120 s et Claude Code la passe en tâche de fond. Durée exacte non mesurée.
- Les requêtes suivantes (`xrefs`, `read_bytes`, `procedure_pseudo_code`) répondent **immédiatement** : la session MCP garde la base Ghidra. La limite des 330 s venait donc de la configuration, pas de REA.
- Ce qui a été trouvé :
  - `0x14276fe50` est le `CRuntimeClass` MFC de `CRuleElement` : nom, taille d'objet `0x58`, schéma 0, `CreateObject`/constructeur `0x141a195d0`, classe de base `0x14263a1b0`.
  - La vtable est en `0x141dcbd08`. `GetRuntimeClass` = `0x140ded740` (8 octets).
  - `Serialize` (slot 2) pointe sur un bouchon vide (`_guard_check_icall`). La (dé)sérialisation des règles ne passe donc **pas** par `CObject::Serialize`. Il faudra la chercher ailleurs : slot 4 `0x140dd3090`, qui itère une liste d'enfants en `+0x30`, ou les appelants du constructeur.
- **Limite** : `xrefs` sur `0x14276fe50` remonte aussi `FUN_140b4f200`. Cette fonction parcourt une table de fonctionnalités (`FIsFeatureEnabled`) qui se termine juste avant le `CRuntimeClass`, et l'adresse lui sert de borne de fin. Ce faux positif n'est pas signalé, car `xrefs` ne donne pas le type de référence. `procedure_references` permettrait de le distinguer.
- La suite de l'investigation reste au jalon J6, si l'import des règles bute sur des champs inconnus.

**Investigation du flux de règles par MCP (2026-10-19, même session ; résultat dans `notes/rea/regles-outlook.md`)**
- Environ 25 requêtes (`xrefs`, `read_bytes`, `procedure_pseudo_code`, `batch_decompile`) en quelques minutes, toutes instantanées. La session gardée ouverte change tout par rapport à la CLI.
- `batch_decompile` est très utile : 9 fonctions de vtable décompilées d'un coup, sans échec.
- **Erreur de ma part, corrigée** : j'avais lu la vtable de `CRuleElement` sur 16 entrées. Les entrées 12 et suivantes appartiennent à `CObList`. Le slot qui compte est le 11 (`0x14084b380`).
  - REA ne donne pas les bornes d'une vtable. Il faut les déduire à partir d'un `GetRuntimeClass` voisin ou d'un GUID.
- `procedure_pseudo_code` nomme `_guard_check_icall` une fonction qui ne contient qu'un `ret`, partagée par repli COMDAT. Cela peut faire croire à tort à un appel de garde CFG.
- **Limite** : REA ne résout pas les chaînes de ressources (`LoadString` sur `OUTLLIBR.DLL`), alors que les libellés ont été décisifs pour donner un sens à `0x64` et à `0x190`.
  - Contournement hors REA : `tools/rule_catalog.py --dll`, qui charge la DLL en données avec `LoadLibraryExW`.
- Les tableaux statiques ont été lus avec `read_bytes`, puis recopiés dans un fichier `.hex` pour un décodage reproductible. REA n'a pas d'outil pour extraire une structure répétée vers un fichier.

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

**En-tête du flux de règles (2026-10-19, nouvelle session MCP)**
- Le serveur MCP s'est reconnecté en cours de session et la session d'analyse a été perdue. Il a fallu tout réanalyser : la première requête est passée en tâche de fond pendant plusieurs minutes.
  - **Limite** : rien n'est gardé entre deux sessions du serveur. Un instantané (`snapshot_path`) l'éviterait, mais il faut l'avoir exporté avant.
- La méthode qui a payé : lister les **appelants** d'une primitive connue (singleton du catalogue, lecture d'un dword), décompiler en lot les voisins d'adresse, puis repérer les constantes de version dans le pseudo-code.
- `batch_decompile` sur 7 fonctions dépasse la taille de réponse autorisée. REA enregistre alors le résultat dans un fichier, découpé ensuite en un fichier `.c` par fonction (`data/rea/`).
- Résultats dans `notes/rea/regles-outlook.md` : versions du flux, `XR_Magic`, `bytecount`, étiquettes d'objet MFC et codage des longueurs.
