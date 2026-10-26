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
## À vérifier avec REA/Outlook — doublons de contacts (`crates/contacts/src/dedupe.rs`, lot S)

Écrit et testé sous Linux sur des bases de test (`cargo test -p thunderlook-contacts dedupe`) ; **aucune écriture dans Outlook, jamais lancé sur la vraie base**.

- **Périmètre** : l'analyse porte sur le dossier de contacts affiché (jamais « Contacts » contre « doublons2 » : deux dossiers = deux analyses). Pour dédoublonner entre les deux, il faudrait un choix explicite de portée (non fait).
- **Informations que le formulaire ne porte pas** (deuxième prénom, surnom, adresse « autre », messagerie, conjoint, téléphones hors Mobile/Bureau/Domicile…) : celles d'une fiche supprimée sont recopiées en fin de notes (« Reporté de la fiche fusionnée… ») si le contact conservé ne les a pas ; elles ne sont pas remises dans leurs propres champs. Photo : une seule est gardée (la plus grande par défaut).
- **Courriel seul = groupe** (score 70, jamais « sûr » si les noms diffèrent) : une adresse partagée (contact@société, boîte familiale) regroupera des personnes différentes ; à regarder à l'écran.
- **Annulation** : une fusion = une modification + N suppressions, chacune dans la corbeille interne (60 jours). Ctrl+Z n'annule que la dernière suppression ; les autres se remettent via `thunderlook_safety::restore` (pas d'écran).
- **Mobile « équivalent »** : seuls +33 / 0033 / 0 suivis de 9 chiffres sont ramenés à `0…` ; tout autre indicatif est comparé tel quel en chiffres.
- **Non compilé ici** : `app/src-tauri` (GTK absent). `dedupe_cmds.rs` a été contrôlé par un crate de substitution (types seulement, `tauri` simulé), pas lancé. L'interface compile pour `wasm32-unknown-unknown` ; mise en page non vue (styles en ligne, aucun CSS ajouté).

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

## À vérifier avec REA/Outlook — impression (`app/ui/src/print.rs`, lot N)

Non compilé au-delà de `cargo check` (wasm) ; **jamais vu à l'écran ni à l'aperçu**.

- Rendez-vous **privés** : imprimés « Rendez-vous privé » (objet et lieu masqués). Comportement d'Outlook non vérifié.
- Impression Tâches/Notes : la liste imprimée est celle affichée (vue, tri, recherche). L'intitulé de la vue n'est pas imprimé.
- Ctrl+P : câblé seulement dans le Calendrier (ailleurs, bouton « Imprimer » du ruban).
- `@page` paysage injecté à l'impression ; WebView2 doit le respecter dans l'aperçu (à confirmer).
## À vérifier — annulation et sauvegardes (`crates/safety`, lot O)

- **Taille du JSON de sauvegarde, mesurée** sur 10 000 contacts fabriqués (nom, société, courriel, téléphones, adresse, note) : **15 Mo sans photo** (écriture 0,6 s) ; **383 Mo avec une photo de 20 Ko chacun**, car la photo est stockée deux fois (sacs MAPI + `.eml`) puis encodée en base64. Au pire, 14 quotidiennes + 8 hebdomadaires = plusieurs Go. Si les vrais contacts de Paul ont beaucoup de photos : à vérifier dans `data_dir/backups` après quelques jours ; piste = ne pas sauvegarder le `.eml` des contacts (reconstructible).
- Relecture « fusionner » de 10 000 éléments : ~8 s (non optimisé).
- Annulation : pile en mémoire (perdue à la fermeture) ; les instantanés restent 60 jours dans `data_dir/trash/` et se remettent via `thunderlook_safety::restore` (pas d'écran pour l'instant).
- Non vérifié : la création d'un élément n'est pas annulable (périmètre : suppressions et modifications). Une suppression faite par « Échange avec le téléphone » (annuler l'import) passe aussi par la corbeille interne.
- Non compilé ici : `app/src-tauri` (bibliothèques GTK absentes sous Linux cloud). L'interface (`app/ui`) compile pour `wasm32-unknown-unknown`.

## À vérifier avec REA/Outlook — dossiers de calendriers, contacts, tâches, notes (`crates/folders`, 23/10)

Testé sous Linux sur une base fabriquée uniquement ; **aucune écriture dans Outlook, aucun PST réel touché**.
- Les nouveaux dossiers (NID ≥ 0x7000_0000, classe `IPF.Appointment` / `IPF.Contact` / `IPF.Task` / `IPF.StickyNote`) vivent
  dans la base ThunderLook : on ne sait pas comment Outlook réagit s'il relit un jour ce fichier (non vérifié).
- Menu clic droit d'un calendrier : libellés (Nouveau calendrier, Renommer, Couleur ›, Supprimer) repris du cahier des charges ;
  le menu réel d'Outlook sur le navigateur de calendriers n'a pas été relevé.
- Couleurs : les 25 couleurs de la palette des catégories ; Outlook n'en propose que 9 + Automatique pour un calendrier.
- Nom unique comparé sans casse sur tout le fichier de données, pas seulement parmi les dossiers de même parent : règle plus
  stricte que celle d'Outlook (non vérifiée).

## À vérifier avec REA/Outlook — tâches périodiques (`crates/items`, lot V)
- « Recommencer N jours après la fin » : écrit comme `PidLidTaskRecurrence` (RecurrencePattern) avec `SlidingFlag` = 1 (octets 18-21,
  [MS-OXOCAL] 2.2.1.44.1), sans fin, motif Jour/Semaine/Mois/An. Le drapeau est établi par la spécification, PAS relevé sur une
  tâche créée par Outlook : à comparer avec une tâche « Régénérer » réelle (blob identique ? `PatternType`/`Period` pour mois et années).
- Série « régénérée » : prochaine échéance = jour de la fin (Paris) + N unités, écart début/échéance et rappel conservés ; fin de
  mois ramenée au dernier jour (31 janv. + 1 mois = 28/29 févr.). Comportement d'Outlook sur ces deux points non relevé.
- Supprimer UNE occurrence crée la suivante puis supprime celle-ci (Outlook : même effet supposé, non relevé). Ctrl+Z ne restaure que
  l'occurrence supprimée : la suivante créée reste.
- Export `.ics` : `RRULE` (FREQ/INTERVAL/COUNT/UNTIL) pour les séries fixes ; « après la fin » → `X-THUNDERLOOK-REGEN` (aucun équivalent iCalendar).
- Rappels : aucune modification de `crates/reminders` ; chaque occurrence suivante est une vraie tâche avec son propre rappel décalé.

## À vérifier avec REA/Outlook — séries venues du téléphone (`crates/calendar/src/ics_series.rs`, lot AA)

Une série importée du téléphone est écrite par `write::create_series` / `write::replace_series` avec la périodicité
complète (exceptions et dates supprimées) : même format que `update_series`, **donc mêmes réserves que la section
« séries avec exceptions » ci-dessus** (jamais ouverte dans Outlook, aucune écriture dans Outlook).
- **`FirstDateTime`** : calculé comme `write::recurrence_of` (jour, semaine selon `WKST`, mois, an) ; l'expansion de
  ThunderLook ne s'en sert pas, Outlook peut s'en servir : non vérifié.
- **`OccurrenceCount`** d'une série à `UNTIL` : nombre d'occurrences réellement générées ; pour `COUNT`, la valeur du fichier.
- **Heure fixe en UTC** (`DTSTART:…Z` + RRULE) : acceptée seulement si le décalage de Paris ne change pas sur la
  période (sinon refus signalé). Le fuseau des `TZID` reconnus (Europe/Paris, Brussels, Berlin… « Romance Standard Time »)
  est supposé identique à Paris. **Je ne sais pas ce qu'écrit MPE** (TZID ou UTC, `RANGE`, `EXDATE` en date seule) : à relever.
- Occurrence modifiée dont l'objet, le lieu, les heures, la disponibilité, le rappel ou « journée entière » changent : gardée.
  Description / catégories / confidentialité différentes : série refusée. Un champ absent de l'occurrence est lu comme
  « comme la série » (la RFC dirait « absent »), pour ne pas refuser les téléphones qui n'écrivent que les différences.
- **Rappel et catégories** d'une série remplacée : ceux du téléphone s'il en envoie, sinon ceux de ThunderLook ; un
  rappel ou des catégories **retirés** sur le téléphone ne sont donc pas retirés dans ThunderLook.
- Annuler un import qui a modifié une série la rétablit depuis son export `.ics` : les champs que l'export ne porte
  pas (disponibilité / rappel propres à une occurrence modifiée) ne sont pas rétablis.

## À vérifier avec le S20 FE / MPE — vCalendar 1.0 (`crates/exchange/src/vcal10.rs`, lot AE)
Lu d'après la description de `Organizer.dat` (docs/mpe-fichiers.md § 1.5) et des échantillons synthétiques ; **aucun fichier réel de MPE n'a été lu**.
- **Journée entière** : lue si `DTSTART`/`DTEND` sont des dates seules (`DTEND` égal ou antérieur au début = un jour). Si MPE écrit une journée entière en UTC (`…T220000Z`→`…T220000Z`), elle est lue comme un rendez-vous de 24 h ou de 0 h, pas comme une journée entière. `DTEND` de plusieurs jours : lu exclusif (norme iCal 2.0) ; en 1.0 il est peut-être inclusif (un jour de trop ou de moins).
- **Heures sans `Z`** : lues en heure de Paris ; `TZ:`/`DAYLIGHT:` du fichier ne sont pas appliqués (avertissement).
- **Séries en UTC sans fin** (`W1 TU #0`) : refusées par le lecteur de séries (répétition à heure locale fixe, changement d'heure) ; avec fin avant le changement d'heure, acceptées.
- **Règles** : `D`, `W`, `MD`, `MP`, `YM`, `YD` (jour du début seulement). Refusés avec avertissement : plusieurs règles, `RDATE`, `EXRULE`, `MP` avec rang 2- à 4-/plusieurs jours, `MD` à plusieurs jours, jours 29-31. Sans fin explicite, 1.0 compte 2 occurrences (appliqué).
- **Rappel** : seul le premier `AALARM`/`DALARM`/`PALARM`/`MALARM` est lu, en minutes avant le début ; répétition/sonnerie ignorées. Rappel en repère différent du début (UTC/local) ou après le début : non importé, avec avertissement.
- **Écartés** : `ATTENDEE` (signalé), tâches `VTODO` du calendrier (signalées ; elles se lisent dans `taches.ics`), propriétés `X-*` autres que les identifiants. Identifiant retenu : `UID`, sinon `X-OUTLOOK-UID`, sinon `X-IRMC-LUID`.
- **Photos de contacts** : le lecteur de vCard 2.1 lit `PHOTO;ENCODING=BASE64` (JPEG) mais l'import d'échange ne reprend jamais les photos (comportement existant, `norm_contact`).

## À vérifier avec REA/Outlook — fidélité des contacts (lot BB, `crates/contacts`, 26/10)

Testé sous Linux (`cargo test -p thunderlook-contacts -p thunderlook-exchange`) ; **aucune écriture dans Outlook, jamais lancé sur la vraie base ni avec MPE**.

- **Types vCard inventés** : « autre téléphone » s'écrit `TEL;OTHER` et « autre adresse » `ADR;OTHER` (pas de type standard) ; messagerie `IMPP`, conjoint `X-SPOUSE`. Ce que MPE/Samsung écrit et relit pour ces champs n'est pas établi : si le téléphone les supprime, l'import les propose en modification (jamais appliquée sans accord).
- **Propriétés MAPI utilisées** (identifiants de [MS-OXPROPS], non vus dans une vraie fiche Outlook ici) : 0x3A45 titre, 0x3A44 second prénom, 0x3A05 suffixe, 0x3A4F surnom, 0x3A24/0x3A25 télécopies, 0x3A1B/0x3A2F deuxièmes numéros, 0x3A1E voiture, 0x3A21 récepteur d'appels, 0x3A1F autre, 0x3A63… adresse « autre », 0x3A48 conjoint, `PidLidInstantMessagingAddress`. À comparer à l'écran avec une fiche Outlook remplie.
- **Région/État des adresses** : toujours pas dans le formulaire, mais une modification ne l'efface plus (avant : effacée à chaque enregistrement).
- **Autres téléphones d'Outlook** (Assistant, Rappel, Société, RNIS, Principal, Radio, Télex, TTY) : toujours non portés ; ils restent intacts en base à la modification, absents du `.vcf`.
## À vérifier avec REA/Outlook — autocomplétion des destinataires et Carnet d'adresses (`crates/contacts/src/suggest.rs`, lot BC)

Lecture seule, testé sous Linux sur des bases de test (`cargo test -p thunderlook-contacts suggest`) ; **jamais lancé sur la vraie base, interface non vue à l'écran**.

- **Destinataires du courrier envoyé** : lus dans les `.eml` des 300 derniers messages du dossier de rôle `sent` (la base ne garde que les noms affichés dans `to_line`). Les destinataires plus anciens ne sont proposés que s'ils ont écrit eux-mêmes.
- **Expéditeurs** : `from_addr` hors dossiers Envoyés, Brouillons, Boîte d'envoi, Éléments supprimés, Courrier indésirable ; seules les adresses SMTP (avec `@`) comptent, les adresses Exchange X.500 sont ignorées.
- **Contacts** : adresses des Courriers 1 à 3 ; pour une adresse Exchange, l'adresse SMTP du « nom d'origine » (comme la fiche). Listes de distribution : insère tous les membres résolus ; les membres non résolus sont omis sans avertissement.
- **Performance** : chaque requête relit tous les contacts et ouvre jusqu'à 300 `.eml` (délai de frappe 150 ms) ; sur 2 138 fiches, non mesuré. Pas de cache.
- **Ordre** : contacts, listes, puis courrier ; début de mot d'abord. Outlook classe autrement (historique « saisie semi-automatique » `.nk2`/cache RoamCache, non lu ici) : [à vérifier] ce qu'il propose pour la même frappe.
## À vérifier avec MPE / le S23 — relecture automatique d'un dossier écrit par MPE (`crates/exchange/src/phone.rs`, lot BK)
Testé sur des fichiers **reconstitués d'après le relevé statistique** de MPE 2.3.0 (`docs/mpe-cible-fichier.md` § 10), jamais sur les vrais fichiers.
- **Première lecture** : bloquée par le seuil (2 140 fiches / 372 rendez-vous) jusqu'à « Appliquer malgré le seuil ». Les contacts déjà dans ThunderLook sont reconnus par nom + société + e-mail seulement : un contact connu de MPE sous un autre nom/e-mail donne un **doublon** ; un contact reconnu mais différent donne un **conflit « inconnu »**. À regarder à l'écran sur le S23 (nombre de conflits, doublons) avant d'accepter.
- **`UID` de MPE stable ?** Tout repose sur l'`UID` (GUID) identique d'une écriture à l'autre. Si MPE le régénère à chaque synchro, chaque passage verra N éléments « nouveaux » (le seuil bloquera) : à vérifier en comparant deux relevés d'`UID` à une heure d'écart.
- **Écriture de MPE en plusieurs temps** : les trois fichiers ne s'écrivent pas au même instant ; chacun est traité seul (stabilité 5 s + 10 s d'âge, fichier tronqué ignoré). Une écriture interrompue **après** `END:VCARD` mais incomplète ne se voit pas.
- **Suppression sur le téléphone** : toujours mise de côté (compteur dans la barre d'état), jamais appliquée seule, et pour l'instant sans bouton « supprimer » dédié : passer par « Vérifier les changements » (cocher la suppression).
- **Notes** : pas de format relevé ; une note sans identifiant stable modifiée sur le téléphone devient une nouvelle note (l'ancienne reste).
- **Champs ignorés à la lecture** (FAX, téléphone en trop, `X-ANNIVERSARY`…) : si le téléphone les modifie seuls, ThunderLook ne le voit pas (rien de perdu côté téléphone, ThunderLook n'y écrit pas).
- **Sens ThunderLook → téléphone** : n'existe pas dans ce mode (aucune écriture dans le dossier de MPE) ; un rendez-vous créé dans ThunderLook ne va pas sur le téléphone par ce dossier.


## À vérifier avec REA/Outlook — fuseaux horaires, jours fériés, « 7 prochains jours » (lot AZ, 26/10)
- Fuseau de l'utilisateur (`crates/calendar/src/tz.rs`, `TimeZone::local`, Paris par défaut, réglé par « Fuseaux horaires »
  dans l'en-tête du calendrier, gardé dans `fuseaux.json`) : sert à l'affichage, à la saisie, aux rappels, aux tâches et aux
  textes d'échange MPE. Les FORMATS restent en Paris (`TimeZone::PARIS`) : `ics.rs` (VTIMEZONE « Romance Standard Time »),
  `ics_series.rs`, `itip.rs`, `csv.rs`, `items/src/vtodo.rs`, export CSV de `lib.rs`. Un autre fuseau que Paris n'a jamais été
  essayé à l'écran ; une série Outlook portant son propre `PidLidTimeZoneStruct` reste dans ce fuseau-là, et une série créée
  par ThunderLook (sans ce struct) suit le fuseau de l'utilisateur : à vérifier dans Outlook après changement de fuseau.
- Règles de la table `ZONES` : règle actuelle seulement (pas d'historique) ; Casablanca sans le changement du ramadan.
- Jours fériés (`holidays.rs`) : 11 jours de métropole, ajoutés au bandeau « Journée » (non enregistrés, rien n'est écrit).
  Doublon évité si un rendez-vous journée entière du même nom existe ce jour-là ; les noms qu'Outlook écrit avec « Ajouter des
  jours fériés » (France) n'ont pas été vus : un nom différent donnerait deux entrées. Pas d'option pour les masquer.
- « 7 prochains jours » : 7 colonnes à partir d'aujourd'hui, flèches de 7 jours ; comportement exact d'Outlook non vu.
