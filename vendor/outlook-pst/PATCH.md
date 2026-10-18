# Correctif local (ThunderLook)

Copie de `outlook-pst` 1.2.0 (crates.io, licence MIT, © Microsoft — voir `LICENSE`).

Seule modification : `src/messaging/message.rs`, `read_embedded`.
Un message dont l'entrée NBT n'a pas de sous-arbre (`nidSub` = 0) était rejeté avec
`MessageSubNodeTreeNotFound`. Outlook écrit pourtant de tels messages (contacts, rendez-vous,
notes sans destinataires ni pièces jointes). Le correctif les lit avec une liste de sous-nœuds vide.

Effet mesuré sur une copie de PST de 3,9 Go : 6 248 échecs → 0 (35 589 éléments).
À proposer en amont : https://github.com/microsoft/outlook-pst-rs

Deuxième modification (J1) : `src/messaging/message.rs`, méthode `Message::into_any`.
`AttachmentData::Message` rend un message joint en `Rc<dyn Message>` ; pour ouvrir ses propres pièces
jointes, `UnicodeAttachment::read` exige le type concret `Rc<UnicodeMessage>`. `into_any` permet le
`downcast` (même chose pour `AnsiMessage`).

Troisième modification (J1) : `src/messaging/attachment.rs`, lecture d'une pièce jointe.
- `PidTagAttachMethod` absent : la pièce jointe était rejetée (« Missing PidTagAttachMethod »). Elle est
  maintenant lue par valeur si `PidTagAttachDataBinary` est présent, sinon avec la méthode `afNone`.
  Sur la copie de 3,9 Go, les 298 cas observés venaient en fait du correctif n° 4 (PC tronqué) ; une fois
  celui-ci appliqué, aucun n'a de méthode absente. Le repli est conservé par prudence.
- `PidTagAttachDataBinary` de HNID nul : rejeté avec un message trompeur (« Invalid PidTagAttachDataObject
  on afEmbeddedMessage attachment: Null », alors que la branche est celle des pièces jointes par valeur).
  C'est une pièce jointe de 0 octet : elle est maintenant rendue vide. Mesuré : 48 pièces jointes.

Quatrième modification (J1) : `src/ltp/prop_type.rs`, `TryFrom<u16> for PropertyType`.
`0x000D` (PtypObject) manquait. Comme la lecture des feuilles de BTH s'arrêtait à la première entrée
illisible, **tout PC contenant un objet était tronqué en silence** à partir de cette propriété : une pièce
jointe de type message joint ne montrait que `0x0E20` et `0x3001`, sans méthode ni données.
Mesuré : 298 pièces jointes concernées sur la copie de 3,9 Go (vérifié contre libpff).

Cinquième modification (J1) : `src/ltp/tree.rs`, `HeapTree::entries`.
La boucle `while let Ok(row) = …read()` est remplacée : seule la fin des données arrête la lecture, toute
autre erreur remonte. Une entrée illisible ne peut plus tronquer une liste de propriétés en silence.

Sixième modification (J1) : `src/messaging/attachment.rs`, messages joints et objets OLE.
- Le sous-nœud de `PidTagAttachDataObject` est cherché d'abord dans l'arbre de sous-nœuds de la pièce
  jointe ([MS-PST] 2.4.6.2), puis dans celui du message parent (seul endroit cherché auparavant).
- `read_embedded` est appelé après la libération du verrou du fichier : il le reprenait alors qu'il était
  encore tenu, d'où un interblocage. Ce chemin n'était jamais atteint avant le correctif n° 4.
