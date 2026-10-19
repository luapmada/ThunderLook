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

## Pas encore vu

- Les bits de `XR_Magic` et le codage de longueur sur 3 octets : ils appartiennent à l'en-tête du flux et de chaque règle, en amont de `CRuleElement`. Il faudra trouver l'appelant de `0x14084b380`, une méthode virtuelle appelée par la liste d'éléments d'une règle.
- La lecture de chaque type de valeur (slot 1 des 30 classes créées par `0x14084bda0`), sauf les types `0x0e` et `0x0f`.
