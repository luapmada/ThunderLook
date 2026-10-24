# Lot AJ — recherche : configurer MPE avec des fichiers vCard / iCal / vNote (sources web)

Branche : `cloud/lot-aj-recherche-mpe`. Lis d'abord `notes/cloud/README.md`, `notes/mpe-plan.md` (§ 1 et § 8 bis) et `docs/mpe-fichiers.md`.

Périmètre de fichiers : NOUVEAU `docs/mpe-cible-fichier.md`. Rien d'autre.

Contexte constaté : MyPhoneExplorer (MPE) 2.03 / client Android 4.13. Ses fichiers de langue contiennent les libellés « vCard-File (local or network) », « iCal-File (local or network) », « vNote-File (local or network) » (`iCalSync`, `vCardSync`, `vNoteSync`) ainsi que « SyncApp », « usedSyncApp » dans l'exécutable. Aujourd'hui MPE synchronise avec Outlook (profil MAPI). On veut le synchroniser avec des FICHIERS dans un dossier, pour un appareil précis (le S20 FE) seulement.

À faire (recherche, avec sources citées par URL ; pas de spéculation) :
1. Trouver dans la documentation officielle de MPE (site de l'éditeur, FAQ, forum officiel, aide en ligne) COMMENT on choisit la cible « vCard-File / iCal-File / vNote-File » : menu et onglet exacts dans Options (F2) de MPE, nom des cases/listes, où se règle le chemin du fichier ou du dossier, si c'est un fichier unique ou un fichier par fiche, ce que « Synchroniser » écrit (contacts, rendez-vous, notes), comment sont gérées les suppressions, les conflits, la confirmation.
2. Relever les formats réellement écrits : version vCard (2.1 / 3.0), encodage, `UID`, `X-IRMC-LUID`, iCalendar 2.0 ou vCalendar 1.0, VNOTE, fichier unique ou multiple, noms de fichiers.
3. Relever les limites connues (doublons, photos, séries, fuseaux) signalées par des utilisateurs ou l'éditeur.
4. Rédiger `docs/mpe-cible-fichier.md` : procédure pas à pas EN FRANÇAIS que Paul peut suivre en deux minutes ; chaque affirmation marquée [source : URL] ou [supposé] / [non trouvé]. Si la documentation n'existe pas, le dire en une ligne plutôt que d'inventer.

Aucune action sur la machine de Paul. Fin : push, résumé de 10 lignes.
