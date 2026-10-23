# Lot X — banc d'essai de l'échange par fichiers pour MPE (étape 5 du plan MPE)

Branche : `cloud/lot-x-echange-mpe`. Lis d'abord `notes/cloud/README.md`, `notes/mpe-plan.md` (§ 5, 6, 7) et `crates/exchange/`.

Périmètre de fichiers : `crates/exchange/` (+ tests), NOUVEAU `docs/mpe-fichiers.md`. Aucun autre fichier.

À faire :
1. Scénarios automatisés (tests) qui simulent le va-et-vient avec un téléphone par fichiers .vcf/.ics/.vnt : import → export → ré-import sans doublon ; élément modifié d'un côté ; élément supprimé d'un côté (propagé) ; UID stables ; accents, sauts de ligne repliés, photos, catégories, listes de distribution, séries et exceptions d'agenda, rappels, tâches.
2. Critères du plan § 7 (étape 5) : 0 doublon et 0 perte après chaque scénario ; rapporter chaque écart par un test qui échoue (`#[ignore]` + explication) plutôt que de le masquer.
3. Corriger les défauts trouvés DANS `crates/exchange` seulement ; ceux qui sortent du périmètre sont décrits dans le résumé.
4. `docs/mpe-fichiers.md` : procédure pour Paul (quel dossier, quels fichiers MPE lit/écrit, ordre des opérations, retour arrière). Distinguer ce qui est constaté (plan MPE) de ce qui est supposé ; ne rien inventer sur MPE.
5. `cargo test -p thunderlook-exchange` (nom réel du crate à vérifier).

Aucune écriture dans Outlook, aucune modification de la config de MPE. Fin : push, résumé de 10 lignes.
