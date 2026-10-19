### header (1)

| id | type | f3 | f4 | f5 | f6-f9 | chaine | libelle |
|---|---|---|---|---|---|---|---|
| 0x0064 (100) | 0x0e | 0x01 | 0x0200 | 1/5 | 1,0,0,1 | 0x0c1c |  |

### apply (1)

| id | type | f3 | f4 | f5 | f6-f9 | chaine | libelle |
|---|---|---|---|---|---|---|---|
| 0x0190 (400) | 0x0f | 0x00 | 0x0100 | 1/5 | 1,0,0,1 | 0x0d48 | Appliquer cette règle %s |

### conditions (37)

| id | type | f3 | f4 | f5 | f6-f9 | chaine | libelle |
|---|---|---|---|---|---|---|---|
| 0x00c8 (200) | 0x00 | 0x03 | 0x0300 | 1/1 | 1,0,0,1 | 0x0c80 | où j'apparais dans la zone À |
| 0x00c9 (201) | 0x00 | 0x04 | 0x0301 | 1/1 | 1,0,0,0 | 0x0c81 | dont je suis l'unique destinataire |
| 0x00ca (202) | 0x00 | 0x07 | 0x0304 | 1/1 | 1,0,0,0 | 0x0c82 | où je n'apparais pas dans la zone À |
| 0x00cb (203) | 0x01 | 0x09 | 0x0306 | 1/1 | 1,0,0,1 | 0x0c83 | de %s |
| 0x00cc (204) | 0x01 | 0x0a | 0x0307 | 1/5 | 1,0,0,1 | 0x0c84 | envoyé à %s |
| 0x00cd (205) | 0x09 | 0x0f | 0x030b | 1/5 | 1,0,0,1 | 0x0c85 | contenant %s dans l'objet |
| 0x00ce (206) | 0x09 | 0x10 | 0x030c | 1/5 | 1,0,0,1 | 0x0c86 | contenant %s dans le corps du message |
| 0x00cf (207) | 0x09 | 0x11 | 0x030d | 1/5 | 1,0,0,0 | 0x0c87 | contenant %s dans l'objet/le corps du message |
| 0x00d0 (208) | 0x11 | 0x13 | 0x030f | 1/1 | 1,0,0,0 | 0x0c88 | avec indicateur pour %s |
| 0x00d2 (210) | 0x06 | 0x15 | 0x0310 | 1/5 | 1,0,0,1 | 0x0c89 | marqué comme %s |
| 0x00d3 (211) | 0x07 | 0x16 | 0x0311 | 1/5 | 1,0,0,1 | 0x0c8a | marqué comme %s |
| 0x00d7 (215) | 0x08 | 0x1b | 0x0316 | 1/5 | 1,1,0,0 | 0x0c8e | assigné à catégorie %s |
| 0x00dc (220) | 0x00 | 0x21 | 0x031b | 1/1 | 1,0,0,0 | 0x0c91 | qui est une réponse automatique |
| 0x00de (222) | 0x00 | 0x23 | 0x031c | 1/5 | 1,0,0,1 | 0x0c93 | qui contient une pièce jointe |
| 0x00df (223) | 0x13 | 0x24 | 0x031d | 1/5 | 1,1,0,0 | 0x0c94 | qui contient %s de documents ou formulaires |
| 0x00e0 (224) | 0x14 | 0x25 | 0x031e | 1/5 | 1,0,0,1 | 0x0c95 | avec une taille %s |
| 0x00e1 (225) | 0x15 | 0x26 | 0x031f | 1/1 | 1,0,0,1 | 0x0c96 | reçu %s |
| 0x00e2 (226) | 0x00 | 0x05 | 0x0302 | 1/1 | 1,0,0,1 | 0x0c97 | lorsque mon nom est dans la zone Cc |
| 0x00e3 (227) | 0x00 | 0x06 | 0x0303 | 1/1 | 1,0,0,1 | 0x0c98 | lorsque mon nom est dans la zone À ou Cc |
| 0x00e4 (228) | 0x17 | 0x0b | 0x0320 | 1/5 | 1,0,0,0 | 0x0c99 | utilise le formulaire %s |
| 0x00e5 (229) | 0x09 | 0x27 | 0x0309 | 1/5 | 1,1,0,0 | 0x0c9a | avec %s dans l'adresse du destinataire |
| 0x00e6 (230) | 0x09 | 0x28 | 0x030a | 1/1 | 1,0,0,0 | 0x0c9b | avec %s dans l'adresse de l'expéditeur |
| 0x00e7 (231) | 0x00 | 0x29 | 0x0325 | 0/0 | 1,0,0,0 | 0x797e | (néant) |
| 0x00e8 (232) | 0x09 | 0x12 | 0x030e | 1/1 | 1,0,0,0 | 0x0c9c | avec %s dans l'en-tête du message |
| 0x00e9 (233) | 0x18 | 0x2a | 0x0326 | 0/0 | 1,1,0,0 | 0x797e | (néant) |
| 0x00eb (235) | 0x1a | 0x2c | 0x0328 | 0/0 | 1,1,0,0 | 0x797e | (néant) |
| 0x00ec (236) | 0x1b | 0x2d | 0x0329 | 0/0 | 1,1,0,0 | 0x797e | (néant) |
| 0x00ed (237) | 0x00 | 0x2e | 0x032a | 0/0 | 1,1,0,0 | 0x0ca1 | avec une pertinence %s |
| 0x00ee (238) | 0x1e | 0x08 | 0x0305 | 1/5 | 1,1,0,0 | 0xeacb | dans le compte %s |
| 0x00ef (239) | 0x21 | 0x32 | 0x032f | 1/5 | 1,1,0,0 | 0xeacd | sur cet ordinateur uniquement |
| 0x00f0 (240) | 0x22 | 0x2f | 0x032b | 1/1 | 1,1,0,0 | 0xeace | expéditeur est dans le Carnet d'adresses %s |
| 0x00f1 (241) | 0x00 | 0x30 | 0x032c | 1/5 | 1,0,0,0 | 0xead2 | qui est une mise à jour ou une invitation à une réunion |
| 0x00f3 (243) | 0x25 | 0x34 | 0x032e | 1/1 | 0,0,0,0 | 0xead4 | qui est un %s |
| 0x00f4 (244) | 0x28 | 0x0e | 0x0323 | 1/5 | 1,1,0,0 | 0xead5 | du type de formulaire %s |
| 0x00f5 (245) | 0x29 | 0x0c | 0x0321 | 1/5 | 1,0,0,0 | 0xead7 | des flux RSS avec %s dans le titre |
| 0x00f6 (246) | 0x00 | 0x1c | 0x0317 | 1/5 | 1,0,0,0 | 0x0c8f | assigné à une catégorie quelconque |
| 0x00f7 (247) | 0x2a | 0x0d | 0x0322 | 1/5 | 1,0,0,0 | 0xead9 | d'un flux RSS |

### actions (35)

| id | type | f3 | f4 | f5 | f6-f9 | chaine | libelle |
|---|---|---|---|---|---|---|---|
| 0x012c (300) | 0x04 | 0x80 | 0x041a | 1/1 | 1,0,0,1 | 0x0ce4 | le déplacer dans le dossier %s |
| 0x012d (301) | 0x00 | 0x81 | 0x041c | 1/1 | 1,0,0,1 | 0x0ce5 | le supprimer |
| 0x012e (302) | 0x02 | 0x6f | 0x0413 | 1/1 | 1,0,0,1 | 0x0ce6 | le transférer à %s |
| 0x012f (303) | 0x05 | 0x60 | 0x0404 | 1/1 | 1,1,0,0 | 0x0ce7 | répondre en utilisant %s |
| 0x0130 (304) | 0x0a | 0x82 | 0x0401 | 1/1 | 1,1,0,1 | 0x0ce8 | afficher %s dans la fenêtre Alerte sur le nouvel élément |
| 0x0131 (305) | 0x0c | 0x62 | 0x0406 | 1/4 | 1,1,0,0 | 0x0ce9 | avec indicateur de message pour %s |
| 0x0132 (306) | 0x00 | 0x65 | 0x0409 | 1/1 | 1,1,0,0 | 0x0cea | supprimer l'indicateur de message |
| 0x0133 (307) | 0x08 | 0x67 | 0x040b | 1/5 | 1,1,0,0 | 0x0ceb | l'assigner à la %s spécifiée |
| 0x0136 (310) | 0x1f | 0x5e | 0x0400 | 1/1 | 1,1,0,1 | 0x0cee | Émettre %s |
| 0x0137 (311) | 0x26 | 0x68 | 0x040c | 1/5 | 1,1,0,0 | 0x0cef | le marquer comme %s |
| 0x0138 (312) | 0x07 | 0x69 | 0x040d | 1/4 | 1,1,0,0 | 0x0cf0 | le marquer comme %s |
| 0x0139 (313) | 0x04 | 0x7f | 0x041b | 1/5 | 1,0,0,1 | 0x0cf1 | déplacer une copie dans le dossier %s |
| 0x013a (314) | 0x00 | 0x6c | 0x0410 | 1/4 | 1,1,0,0 | 0x0cf2 | m'avertir lorsqu'il a été lu |
| 0x013b (315) | 0x00 | 0x6d | 0x0411 | 1/4 | 1,1,0,0 | 0x0cf3 | m'avertir lorsqu'il a été remis |
| 0x013c (316) | 0x10 | 0x6e | 0x0412 | 1/4 | 1,0,0,0 | 0x0cf4 | envoyer ce message en copie à %s |
| 0x013e (318) | 0x12 | 0x6b | 0x040f | 1/4 | 1,1,0,0 | 0x0cf6 | différer la remise de %s minutes |
| 0x013f (319) | 0x16 | 0x76 | 0x0419 | 1/5 | 1,1,0,1 | 0x0cf7 | effectuer %s |
| 0x0140 (320) | 0x00 | 0x77 | 0x041e | 0/0 | 1,0,0,0 | 0x0cf8 |  |
| 0x0141 (321) | 0x00 | 0x78 | 0x041f | 0/0 | 1,1,0,0 | 0x797e | (néant) |
| 0x0142 (322) | 0x00 | 0x79 | 0x0529 | 1/5 | 1,0,0,1 | 0x0cf9 | arrêter de traiter plus de règles |
| 0x0143 (323) | 0x00 | 0x7a | 0x0420 | 0/0 | 1,1,0,0 | 0x797e | (néant) |
| 0x0144 (324) | 0x02 | 0x71 | 0x0414 | 1/1 | 1,0,1,1 | 0xeac4 | le faire suivre à %s |
| 0x0145 (325) | 0x00 | 0x7b | 0x0421 | 0/0 | 1,1,0,0 | 0xeac7 | ajouter %s à la pertinence |
| 0x0146 (326) | 0x1d | 0x61 | 0x0405 | 1/1 | 1,0,1,1 | 0xeac5 | avoir la réponse du serveur en utilisant %s |
| 0x0147 (327) | 0x02 | 0x72 | 0x0415 | 1/1 | 1,0,0,1 | 0xeac6 | le transférer à %s comme pièce jointe |
| 0x0148 (328) | 0x00 | 0x5f | 0x0403 | 1/1 | 1,1,0,0 | 0xeac8 | l'imprimer |
| 0x0149 (329) | 0x20 | 0x75 | 0x0402 | 1/1 | 1,1,0,0 | 0xeac9 | démarrer %s |
| 0x014a (330) | 0x00 | 0x84 | 0x041d | 1/1 | 1,1,0,0 | 0xeaca | le supprimer définitivement |
| 0x014b (331) | 0x23 | 0x7c | 0x0422 | 1/1 | 1,1,0,0 | 0xead0 | exécuter %s |
| 0x014c (332) | 0x00 | 0x7d | 0x0423 | 1/1 | 1,1,0,0 | 0xead1 | le marquer comme lu |
| 0x014f (335) | 0x00 | 0x83 | 0x0425 | 1/1 | 1,1,0,0 | 0x0cfd | afficher une alerte sur le Bureau |
| 0x0150 (336) | 0x24 | 0x63 | 0x0407 | 1/0 | 1,1,0,0 | 0x0cfe | marquer le message avec %s |
| 0x0151 (337) | 0x27 | 0x64 | 0x0408 | 1/1 | 1,1,0,0 | 0x0cff | avec indicateur de message pour %s |
| 0x0152 (338) | 0x00 | 0x66 | 0x040a | 1/5 | 1,0,0,0 | 0x0cec | supprimer les catégories du message |
| 0x0153 (339) | 0x2b | 0x85 | 0x0426 | 1/5 | 1,0,0,0 | 0x0d00 | appliquer la stratégie de rétention : %s |

### exceptions (30)

| id | type | f3 | f4 | f5 | f6-f9 | chaine | libelle |
|---|---|---|---|---|---|---|---|
| 0x01f4 (500) | 0x00 | 0x40 | 0x0501 | 1/1 | 1,0,0,1 | 0x0dac | sauf s'il m'est directement adressé |
| 0x01f5 (501) | 0x00 | 0x41 | 0x0502 | 1/1 | 1,0,0,0 | 0x0dad | sauf si je suis l'unique destinataire |
| 0x01f6 (502) | 0x00 | 0x44 | 0x0504 | 1/1 | 1,0,0,0 | 0x0dae | sauf si mon nom n'apparaît pas dans la zone À |
| 0x01f7 (503) | 0x01 | 0x02 | 0x0506 | 1/1 | 1,0,0,1 | 0x0daf | sauf s'il vient de %s |
| 0x01f8 (504) | 0x01 | 0x49 | 0x0507 | 1/5 | 1,0,0,1 | 0x0db0 | sauf s'il est adressé à %s |
| 0x01f9 (505) | 0x09 | 0x4a | 0x050b | 1/5 | 1,0,0,1 | 0x0db1 | sauf si l'objet contient %s |
| 0x01fa (506) | 0x09 | 0x4b | 0x050c | 1/5 | 1,0,0,1 | 0x0db2 | sauf si le corps du message contient %s |
| 0x01fb (507) | 0x09 | 0x4c | 0x050d | 1/5 | 1,0,0,0 | 0x0db3 | sauf si l'objet ou le corps du texte contiennent %s |
| 0x01fc (508) | 0x11 | 0x4d | 0x050f | 1/1 | 1,0,0,0 | 0x0db4 | sauf s'il a un indicateur pour %s |
| 0x01fe (510) | 0x06 | 0x35 | 0x0510 | 1/5 | 1,0,0,1 | 0x0db5 | sauf s'il est marqué comme %s |
| 0x01ff (511) | 0x07 | 0x36 | 0x0511 | 1/5 | 1,0,0,1 | 0x0db6 | sauf s'il est marqué comme %s |
| 0x0203 (515) | 0x08 | 0x3b | 0x0516 | 1/5 | 1,1,0,0 | 0x0dba | sauf s'il est assigné à la catégorie %s |
| 0x0208 (520) | 0x00 | 0x3a | 0x051b | 1/1 | 1,0,0,0 | 0x0dbd | sauf s’il s’agit d’une réponse automatique |
| 0x020a (522) | 0x00 | 0x3e | 0x051c | 1/5 | 1,0,0,1 | 0x0dbf | sauf s'il possède une pièce jointe |
| 0x020b (523) | 0x13 | 0x54 | 0x051d | 1/5 | 1,1,0,1 | 0x0dc0 | sauf avec %s de documents ou de formulaires |
| 0x020c (524) | 0x14 | 0x55 | 0x051e | 1/5 | 1,0,0,1 | 0x0dc1 | sauf si sa taille est %s |
| 0x020d (525) | 0x15 | 0x56 | 0x051f | 1/1 | 1,0,0,0 | 0x0dc2 | sauf s'il est reçu %s |
| 0x020e (526) | 0x00 | 0x42 | 0x0503 | 1/1 | 1,0,0,1 | 0x0dc3 | sauf si mon nom apparaît dans la zone Cc |
| 0x020f (527) | 0x00 | 0x43 | 0x0505 | 1/1 | 1,0,0,1 | 0x0dc4 | sauf si mon nom apparaît dans la zone À ou Cc |
| 0x0210 (528) | 0x17 | 0x45 | 0x0520 | 1/5 | 1,0,0,0 | 0x0dc5 | sauf si le formulaire %s est utilisé |
| 0x0211 (529) | 0x09 | 0x57 | 0x0509 | 1/5 | 1,1,0,0 | 0x0dc6 | excepté avec %s dans l'adresse du destinataire |
| 0x0212 (530) | 0x09 | 0x58 | 0x050a | 1/1 | 1,0,0,0 | 0x0dc7 | excepté avec %s dans l'adresse de l'expéditeur |
| 0x0213 (531) | 0x09 | 0x59 | 0x050e | 1/1 | 1,0,0,0 | 0x0dc8 | seulement si l'en-tête du message contient %s |
| 0x0214 (532) | 0x1e | 0x3f | 0x0500 | 1/5 | 1,1,0,0 | 0xeacc | excepté dans le compte %s |
| 0x0215 (533) | 0x22 | 0x5a | 0x0525 | 1/1 | 1,1,0,0 | 0xeacf | sauf si l'expéditeur est dans le Carnet d'adresses %s |
| 0x0216 (534) | 0x00 | 0x5b | 0x0526 | 1/5 | 1,0,0,0 | 0xead3 | sauf s'il s'agit d'une mise à jour ou d'une invitation à une réunion |
| 0x0218 (536) | 0x28 | 0x48 | 0x0523 | 1/5 | 1,1,0,0 | 0xead6 | sauf s'il s'agit d'un formulaire de type %s |
| 0x0219 (537) | 0x29 | 0x46 | 0x0521 | 1/5 | 1,0,0,0 | 0xead8 | sauf s'il provient des flux RSS avec %s dans le titre |
| 0x021a (538) | 0x00 | 0x3c | 0x0517 | 1/5 | 1,0,0,0 | 0x0dbb | sauf s'il est assigné à une catégorie quelconque |
| 0x021b (539) | 0x2a | 0x47 | 0x0522 | 1/5 | 1,0,0,0 | 0xeada | sauf s'il provient d'un flux RSS |

