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

## Interface native d'Outlook (23/10)
- `observe_native_ui`, `capture_native_ui_scenario` et `trace_native_ui_action` sont décrits comme liés à une cible **Mach-O** (accessibilité macOS, `AXValue`). **Non testés sur `OUTLOOK.EXE`** : déduit de leur description seulement.
- Pistes REA non macOS à essayer pour les menus et les couleurs : `inspect_pe_resources` (menus et dialogues dans les ressources PE), `search_strings` / `xrefs` (palette des catégories), `observe_native_calls`.

## Menus d'Outlook par les ressources PE (23/10)
- `inspect_pe_resources` : `OUTLOOK.EXE` n'a **aucun** RT_MENU (icônes, bitmaps, 268 UIFILE DirectUI). `1036\OUTLLIBR.DLL` en a 87, mais le résultat est une arborescence d'offsets (non décodée) : menus et chaînes (RT_STRING) lus par script Python à partir des offsets.
- Les menus décodés sont des menus Win32 hérités (dossiers, pièces jointes, zone de notification, etc.) : **les menus contextuels du calendrier (rendez-vous, créneau) n'y sont pas** ; ils viennent de la barre de commandes Office (hors RT_MENU). Pour eux, seule l'observation de l'interface (UIA) donne les libellés : relevés déjà faits dans `notes/outlook-couleurs-menus.md`.

## À vérifier avec REA/Outlook — séries avec exceptions (`crates/calendar`, 23/10)

`Recurrence::encode` écrit maintenant les exceptions (miroir de `parse`) ; `write::update_series` et
`write::update_occurrence` s'en servent. Vérifié : `encode(parse(b)) == b` sur les 25 blobs réels de
`tests/fixtures/recur_reels.hex` (dont 10 avec exception, **sans objet ni lieu**), sauf 2 « jamais » à l'ancienne
(`0xFFFFFFFF`, réécrit `0x2023`). **Jamais testé dans Outlook** : aucune écriture dans Outlook, aucune ouverture d'une
série écrite par ThunderLook.

Non vérifié sur un blob réel (pas de fixture avec objet ou lieu d'exception) :
- **Chaînes 8 bits d'`ExceptionInfo`** : écrites `longueur+1, longueur, octets` (sans zéro final), d'après ma lecture de
  [MS-OXOCAL] 2.2.1.44.2 ; `parse` ignore la première longueur, donc rien ne la confirme ici.
- **`ExtendedException`** : ordre écrit = `ChangeHighlight` (si version d'écriture ≥ 0x3009 : taille 4 + valeur),
  `ReservedBlockEE1`, puis, si objet ou lieu modifié, `StartDateTime`/`EndDateTime`/`OriginalStartDate`, chaînes
  Unicode (`longueur u16` + UTF-16, objet puis lieu), `ReservedBlockEE2`. Confirmé seulement pour le cas sans chaîne
  (10 fixtures, `ChangeHighlight` = 0, blocs vides).
- **`ChangeHighlight`** : toujours 0 à l'écriture (jamais le masque de champs modifiés d'Outlook) ; sens des bits non vérifié.
- **`ReservedBlock1` / `ReservedBlock2`** : vides à l'écriture ; relus et réécrits tels quels sinon. Les octets
  **après** `ReservedBlock2` (4 à 20 octets vus sur 2 fixtures, ex. `2a92090d 00000000 …`) ne sont pas décrits : conservés
  à l'identique, jamais produits par ThunderLook.
- **Drapeau `0x200` d'`OverrideFlags`** (vu sur 1 fixture, sans donnée associée) : conservé, jamais produit.
- **`StartDateTime`/`EndDateTime`** d'une exception : minutes locales depuis 1601 du nouveau début/fin (conforme aux
  fixtures). **`OriginalStartDate`** : écrit = minuit du jour d'origine + heure de début de la série (conforme aux
  fixtures) ; je ne sais pas si Outlook exige exactement cette forme pour retrouver l'occurrence.
- **Dates d'`ExtendedException`** : recopiées de celles d'`ExceptionInfo` (aucune fixture avec chaîne pour confirmer).
- **`ModifiedInstanceDates`** : minuit du jour du nouveau début ; triées par date, comme `DeletedInstanceDates`. Le
  lien d'ordre avec le tableau d'exceptions (trié par date d'origine) n'est pas confirmé quand une occurrence est
  déplacée avant ou après sa voisine.
- **Message intégré par exception** : Outlook en met un (pièce jointe, `has_attachment`) par exception ; ThunderLook
  n'en écrit pas, la lecture n'en a pas besoin. Ignoré : description, catégories et caractère privé d'une occurrence
  (seuls objet, lieu, début, fin, disponibilité, rappel, journée entière vont dans l'exception). Outlook pourrait
  recréer ou réclamer ce message à l'ouverture : à voir.
- **Changement de périodicité** (`update_series`) : exceptions et suppressions abandonnées (comptées dans le retour).
  « Même périodicité » = fréquence, motif, période, fin et dates de début/fin identiques.

## À vérifier avec REA/Outlook — contacts : photo, catégories, listes de distribution (`crates/contacts`, 23/10)

Écrit et testé sous Linux sur des bases de test uniquement ; **aucune écriture dans Outlook, aucun PST réel relu**.

- **`PidLidHasPicture` (named 0x8015) après suppression de la photo** : ThunderLook écrit « faux » (propriété conservée,
  pas retirée). Je ne sais pas ce qu'Outlook écrit (faux, ou propriété absente). Sans effet à la lecture ici (absente =
  faux ; la photo se lit dans le `.eml`). Si la source n'a pas la propriété dans sa table de noms, rien n'est écrit.
- **Photo** : seul le JPEG est accepté (magic `FF D8 FF`), en formulaire comme en vCard (une PHOTO PNG ou une URL est
  ignorée avec avertissement). Pas vérifié : qu'Outlook relise `ContactPicture.jpg` d'un `.eml` écrit par ThunderLook.
- **Catégories** : écrites dans `Keywords` (jeu `PS_PUBLIC_STRINGS`), comme le calendrier ; couleurs/catégories maîtresses
  d'Outlook non touchées. Vide → propriété retirée.
- **Listes de distribution** (lecture seule, `distlist.rs`). Lu : `PidLidDistributionListName` (0x8053, sinon objet),
  `…OneOffMembers` (0x8054) puis, à défaut, `…Members` (0x8055), tableaux `PtypMultipleBinary` (0x1102) ; seuls les
  EntryID **one-off** ([MS-OXCDATA] 2.2.5.1 : fournisseur `812B1FA4-BEA3-1019-9D6E-00DD010F5402`, drapeau Unicode `0x8000`
  après la version, puis nom, type, adresse) sont décodés. Vérifié seulement sur des EntryID fabriqués d'après le
  document, **pas sur les 3 listes réelles**. Incertain :
  - un EntryID de membre renvoyant à un contact ou une liste (autre fournisseur) n'est **pas résolu** : compté, affiché
    « n non résolu(s) », jamais deviné ;
  - si 0x8054 liste bien **tous** les membres (contacts compris) comme je le lis : non confirmé ;
  - si l'import PST conserve les propriétés `PtypMultipleBinary` dans `message.props` (sinon la liste s'affiche vide) ;
  - `PidLidDistributionListStream` (0x8064, ordre et noms de repli) non lu.
## À vérifier avec REA/Outlook — tâches et notes (`crates/items`, lot C, 23/10)

Tout est écrit dans la base de ThunderLook, jamais dans Outlook. Tests sous Linux : `cargo test -p thunderlook-items` (23 tests, dont
écriture/relecture en base pour rappel, périodicité, occurrence suivante, couleur de note).

**`PidLidTaskRecurrence` (0x8116) — format NON confirmé sur une tâche réelle.**
- Écrit = `RecurrencePattern` seul : l'encodeur de `calendar::recur` privé des 26 derniers octets (`AppointmentRecurrencePattern`
  vide). Lu = le même blob + ces 26 octets. Aucune fixture de tâche périodique réelle dans le dépôt : seul l'aller-retour
  `encode`/`decode` est testé, pas la conformité à ce qu'écrit Outlook.
- Je n'ai pas lu [MS-OXOTASK] lui-même (la recherche ne rend que les pages MAPI « canonical property »). À vérifier : fixture
  d'une vraie tâche périodique (octets de 0x8116) comparée à `trecur::encode`.
- Périodicités relues qui n'entrent pas dans le formulaire (plusieurs jours de semaine, n-ième jour du mois, fin de mois) :
  affichées « Personnalisée », blob conservé tel quel à l'enregistrement.
- `PidLidTaskFRecurring` (0x8126) écrit avec 0x8116 ; la doc MAPI demande aussi `PidLidTaskDeadOccurrence` (0x8109) — écrit
  seulement sur la tâche terminée dont on a créé la suite. Autres propriétés de [MS-OXOTASK] pour une série (ex. `TaskOwner`,
  `TaskAccepted`, `TaskMode`) : non écrites.

**Fin d'une tâche périodique** (choix de ThunderLook, pas un comportement observé d'Outlook) : la tâche est marquée terminée
(`DateCompleted` = minuit UTC du jour, comme avant) ET une nouvelle tâche est créée (début, échéance, rappel décalés au jour
de l'occurrence suivante, état « pas commencée »). « N fois » devient « N-1 fois » sur la suite ; série personnalisée : blob
recopié. La terminée garde sa périodicité et reçoit `DeadOccurrence` = vrai (pas de doublon si on la rouvre puis la termine).
- **INCERTAIN : sens de `PidLidTaskDateCompleted` pour une tâche périodique** (date de l'occurrence terminée ou de la série ?
  La doc MAPI exige seulement minuit local converti en UTC ; ThunderLook écrit minuit UTC, comme avant le lot).
- Si la série n'a plus d'occurrence, rien n'est créé.

**Rappel** : `PidLidReminderSet` 0x8503 + `PidLidReminderTime` 0x8502 (PSETID_Common, FILETIME UTC), heure saisie = heure de
Paris. Si la source n'a pas ces noms dans sa table, le rappel n'est pas écrit (silencieusement, comme les autres champs).
`PidLidReminderSignalTime` (0x8560) non écrit : Outlook pourrait ne pas déclencher le rappel sans lui. À vérifier à l'écran.

**Couleur de note** : `PidLidNoteColor` 0x8B00 / PSETID_Note `0006200E-…` confirmé par la doc (PT_LONG, 0 bleu, 1 vert, 2 rose,
3 jaune, 4 blanc). Pas de base réelle en cloud : je n'ai PAS pu vérifier sur les notes importées que la propriété y est. Repli
explicite : jaune (3) si la source n'a pas la propriété, si la note ne la porte pas, ou si la valeur est hors 0-4
(`NOTE_COLOR_FALLBACK`, testé). Non essayé : `PidTagIconIndex` (0x1080) comme source de secours — correspondance non vérifiée.
## À vérifier avec REA/Outlook — rappels (`crates/reminders`, lot D)

- **Rappels manqués (ThunderLook fermé plusieurs jours)** : comportement d'Outlook non vérifié. Plafond **proposé** :
  `Window::default()` = 24 h en arrière ; au-delà, un rappel manqué n'est plus rendu (ni notifié). À confirmer : faut-il
  plutôt tout lister jusqu'à ce que l'utilisateur l'ignore (Outlook affichait « Rappels : 171 »), ou plafonner à N jours ?
- **Rappel reporté** : revient à l'heure du report, même si l'heure d'origine est plus ancienne que la plage arrière
  (14 jours au plus, plafond de report que je suppose proche d'Outlook, non vérifié).
- **Ignorer** : définitif pour l'identifiant. Une tâche ignorée (`t<id>`) le reste même si son heure de rappel change
  ensuite ; une occurrence de série est identifiée par son jour local d'origine (`a<id>@AAAA-MM-JJ`).
- **Tâche sans heure de rappel** : échéance à 8 h (heure de Paris) ; une tâche sans `PidLidReminderSet` n'a pas de rappel
  (hypothèse : Outlook n'en crée pas sans cette case cochée). Non vérifié : tâche récurrente (non traitée : une seule échéance).
- **Rappel de rendez-vous** : `Appointment.reminder_minutes` tel que lu (15 min si `PidLidReminderDelta` absent) ; l'heure du
  rappel d'une exception de série (rappel propre à l'occurrence) n'est pas relue, c'est celui de la série. Rendez-vous
  d'une journée entière : rappel compté depuis minuit.
- **Notification** : le clic ramène seulement ThunderLook au premier plan (pas de lien d'ouverture dédié). Le thread Tauri
  (`app/src-tauri/src/reminders_cmds.rs`) n'a pas pu être compilé en cloud (gdk absent, Windows seulement).

## À vérifier avec REA/Outlook — courrier : tri À/Importance, impression, tout enregistrer (lot J, 23/10)

Écrit et testé sous Linux (`cargo test -p thunderlook-mailbox` : tris À et Importance, noms de fichiers uniques). Le code
Tauri (`mail_cmds.rs`) et l'interface n'ont pas pu être compilés ici pour Windows/Tauri (gdk absent) ; l'interface compile en wasm32.

- **Tri « À »** : clé = premier destinataire de `to_line` (nom, sinon adresse), insensible à la casse et aux accents. Je ne sais
  pas si Outlook trie sur le premier destinataire ou sur la ligne « À » entière.
- **Tri « Importance »** : ordre croissant = haute, normale, faible (hypothèse : celui d'Outlook ; non confirmé).
- **Doublons de noms** à l'enregistrement groupé : « nom (2).ext ». Format choisi par ThunderLook ; Outlook n'enregistre
  pas de façon groupée avec ce schéma à ma connaissance — non vérifié.
- **Impression** : `window.print()` du cadre du message (HTML) ou du volet de lecture (texte, règles `@media print`). Pas d'en-têtes
  Outlook (De/À/Date/Objet) dans l'impression du corps HTML seul : à voir à l'écran.
