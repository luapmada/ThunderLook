# Lot AK — audit de parité avec Outlook (ce qui manque encore à ThunderLook)

Branche : `cloud/lot-ak-parite`. Lis d'abord `notes/cloud/README.md`, `notes/handoff.md`, `notes/outlook-couleurs-menus.md`, `REA-LIMITES.md`, `docs/utilisation.md` et le code des modules (`app/ui/src/`, `crates/`).

Périmètre de fichiers : NOUVEAU `docs/parite-outlook.md`. Rien d'autre (aucune modification de code).

À faire :
1. Pour Courrier, Calendrier, Contacts, Tâches, Notes, Recherche, Rappels, Options, Impression, Échange téléphone, Sauvegardes : tableau « fonction d'Outlook » / « état dans ThunderLook » (fait, partiel, absent) basé sur le CODE LU (citer le fichier), jamais sur une supposition. Ne rien affirmer sur Outlook que les notes du dépôt ne documentent pas : marquer [à vérifier].
2. Classer les manques par priorité d'usage pour un utilisateur qui veut se passer d'Outlook au quotidien (courrier IMAP multi-comptes, calendrier, contacts, synchronisation téléphone par MPE) : bloquant / gênant / secondaire.
3. Pour chaque manque bloquant ou gênant : taille estimée (petit / moyen / gros), fichiers concernés, et un découpage en lots cloud indépendants (périmètres de fichiers DISJOINTS) prêts à être écrits dans `notes/cloud/`.
4. Terminer par la liste des 10 premiers lots à lancer, dans l'ordre.

Aucune écriture dans Outlook. Fin : push, résumé de 10 lignes.
