# Flux de règles d'Outlook : ce que montre `OUTLOOK.EXE`

Analyse REA (Ghidra 12.1.4, serveur MCP) du binaire installé, Outlook 2021 x64, sha256 `e1d9af62…dffc5`, le 2026-10-19.
Ces résultats complètent la spec communautaire d'Engelhardt (`notes/formats.md` §2). Les adresses ne valent que pour ce binaire.

## Chaîne d'appel

| Adresse | Rôle |
|---|---|
| `0x14276fe50` | `CRuntimeClass` MFC `CRuleElement` : objet de 0x58 octets, schéma 0, base `CObject` (`0x14263a1b0`) |
| `0x141dcbd08` | vtable de `CRuleElement` (12 entrées ; `CObList` commence juste après) |
| slot 2 | `Serialize` MFC : vide. Le flux ne passe pas par `CArchive` |
| slot 11, `0x14084b380` | **Lecture et écriture d'un élément** sur un `IStream` |
| `0x14084b7e8` | Recherche du descripteur par identifiant |
| `0x14084bda0` | Fabrique de l'objet valeur selon le type du descripteur (types 1 à 0x2b) |
| `0x14038c1d8` | Construction du catalogue à partir de 5 tableaux statiques (`rule_catalog.hex`) |
| `0x1405895b0` | E/S d'un dword : lecture, écriture ou calcul de taille selon le mode du flux |

## Format d'un élément (lecture, `0x14084b380`)

```
u32 id          ; 0xEA est réécrit en 0xEB à la lecture (ancienne condition remplacée)
u32 n           ; nombre de valeurs
n × valeur      ; chaque valeur se lit elle-même ; son type vient du catalogue (champ « type »)
```

- À l'écriture, `n` vaut le compte interne (`+0x54`). Il est forcé à 1 quand le 3e paramètre vaut `0xED03C`, qui est probablement la version cible du flux. C'est une piste pour le « champ compte » de la spec, non vérifiée.
- Un identifiant hors catalogue fait échouer la lecture (`E_FAIL`).
- Plages : `100` en-tête, `200-247` conditions, `300-339` actions, `400` « Appliquer cette règle », `500-539` exceptions.

## Les deux éléments mal connus

**`0x64` (100), type `0x0e`.** Valeur : deux dwords, un champ à `+8` puis des drapeaux à `+0x10` (1 par défaut).
- La fonction d'affichage (`0x141a0bc90`) affiche le libellé `0x2710` « Message » quand le bit 0 des drapeaux est à 1.
- `0x64` est donc le **filtre de type d'élément** de la règle : bit 0 = s'applique aux messages.
- Le rôle du dword à `+8` reste inconnu. Valeur par défaut : 0.

**`0x190` (400), type `0x0f`.** Valeur : deux dwords, `+8` puis des drapeaux à `+0x10`. Libellé : « Appliquer cette règle %s » (`0x0d48`).
- bit 0 : « après l'arrivée du message » (`0x2774`) ;
- bit 2 : « après avoir envoyé le message » (`0x2776`) ;
- les deux bits ensemble : les libellés sont joints par « ou » (`0xe715`).

## Catalogue complet

`notes/rea/rule_catalog.md` (104 éléments avec libellés français), généré par :

```
python -I tools/rule_catalog.py --dll "C:\Program Files\Microsoft Office\root\Office16\1036\OUTLLIBR.DLL" > notes/rea/rule_catalog.md
```

- Champs sûrs : `id` (+0), `type` de valeur (+8), identifiant de chaîne (+0x28).
- Les autres champs sont recopiés tels quels, sans interprétation.
- Les éléments masqués ou obsolètes ont pour libellé « (néant) » (`0x797e`) et `f5` = 0/0 : `0xE7`, `0xE9`, `0xEB`, `0xEC`, `0x141`, `0x143`.
- Les types `0x18` à `0x1B` déclenchent une assertion (`Ordinal_7228`, balise `0x10f222`), puis une valeur générique est utilisée.

## En-tête de règle (2026-10-19, `0x14084a418`)

Trouvé par les appelants du singleton du catalogue (`0x1400b9b28`) puis de la lecture d'un dword (`0x1405895b0`).
La même fonction lit, écrit ou mesure : le mode est dans le contexte du flux.
Paramètres : la règle, le flux, la **version du flux** (le `RW_Magic`), des options.

**Les versions sont des nombres, pas des « magic »** : `RW_Magic` est un u32 de version, suivi d'un second u32.

| Valeur | Octets | Version d'Outlook (spec) |
|---|---|---|
| 0xED03C (970 812) | `3C D0 0E 00` | OL98 |
| 0xE5EBD | `BD 5E 0E 00` | OL2000 |
| 0xF4240 (1 000 000) | `40 42 0F 00` | OL2002 |
| 0x140000 (1 310 720) | `00 00 14 00` | OL2016 et plus |

Les seuils lus dans le code : `< 1 000 000` (chaînes ANSI), `> 0xED03B`, `≥ 0xECFDE`, `≤ 0x140000`, `> 0x140000`, `= 1 100 000`.

Lecture d'une règle, dans l'ordre :
1. **u32 `XR_Magic`** (version ≥ 1 000 000, ou si l'appelant ne fournit pas d'options) → règle `+0x90`.
   - Les **24 bits bas** sont la version de cette règle (`& 0xFFFFFF`). Ce sont les « 3 octets » de la spec : `00 00 14` = 0x140000.
   - L'**octet haut** est le « locator » de la spec (`00` ou `06`). Il sert d'index dans une table de versions minimales (`DAT_1420af5b0[x >> 24]`, dans `0x140849260`).
2. **Nom de la règle** : chaîne au codage de longueur décrit plus bas (`0x14084b028`).
3. **u32 « active »** → bit 3 de `+0x94`.
4. **u32** → bit 4 de `+0x94` (le « unknown = 0 » de la spec, pour OL2002 et plus). À l'écriture, il est calculé par `0x140849190`.
5. **Trois u32** : `+0xAC`, puis `+0xA8` (version > 0xED03B), puis `+0x98`. C'est le bloc « magic » de 12 octets de la spec.
   - Avant 1 000 000 avec options, un chemin à part est suivi.
6. **Après 0x140000** (format plus récent qu'OL2016) : un u32 de longueur et un bloc d'octets → `+0xA0`. La spec ne le décrit pas.
   - Jusqu'à 0x140000, le u32 est à la place un booléen « éléments présents ».
7. **`bytecount`** (écriture, versions 1 000 000 à 0x140000) : Outlook réserve 4 octets, écrit la suite, puis revient y inscrire la taille de tout ce qui suit. Cela confirme la spec.
8. **Éléments** (`0x140bfba40`, ignorés si le bit `0x40` est mis et la version vaut 1 100 000) : un u16 de nombre d'éléments, puis pour chacun une **étiquette d'objet MFC (`CArchive`)** :
   - `0xFFFF` = nouvelle classe, suivie de u16 schéma, u16 longueur (au plus 255) et du nom de classe (« CRuleElement ») ;
   - `0x8001` = classe déjà vue, n° 1 (0x8000 | index). C'est le « séparateur » de la spec ;
   - puis l'élément lui-même (slot 11 de la vtable, `0x14084b380`, voir plus haut).

## Codage de longueur des chaînes (`0x1405b1b60` Unicode, `0x1405b1f48` ANSI)

- **Lecture** : un octet `n`. Si `n = 0xFF`, un u16 suit et donne la longueur. Viennent ensuite `n` caractères : UTF-16 à partir de la version 1 000 000, octets avant.
- **Écriture** : un octet si la longueur est au plus 254, sinon `0xFF` suivi de la longueur sur u16.
- **Réponse à la question de la spec** : la forme non minimale `FF 00 00` (= 0) est **acceptée** à la lecture. Il n'y a ni forme 32 bits ni marqueur Unicode à la MFC (`FF FE FF`) : longueur maximale 65 535.

## Pas encore vu

- La lecture de chaque type de valeur (slot 1 des 30 classes créées par `0x14084bda0`), sauf les types `0x0e` et `0x0f`.
- Le rôle de la règle `+0xAC` et `+0x98`, et le contenu du bloc des versions > 0x140000.
