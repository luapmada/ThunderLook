# Lot AH — procédure de migration du S23 et contrôles automatiques (lecture seule)

Branche : `cloud/lot-ah-migration-s23`. Lis d'abord `notes/cloud/README.md`, `notes/mpe-plan.md` (tout), `docs/mpe-fichiers.md`.

Périmètre de fichiers : NOUVEAU `docs/migration-s23.md`, NOUVEAU `tools/mpe_check.mjs` (Node, lecture seule). Rien d'autre.

À faire :
1. `docs/migration-s23.md` : procédure pas à pas, réversible, pour passer le S23 d'Outlook aux fichiers ThunderLook (plan § 6 étapes 3 et 6) : sauvegardes à refaire le jour même, ordre exact des opérations, réglages à faire dans MPE (LISTER ce qu'il faut changer sans l'affirmer constaté : marquer [supposé] tout ce qui n'a pas été vu), critères de réussite (§ 7), retour arrière à chaque étape, ce qui ne doit JAMAIS être fait (ne pas relancer MPE sans prévenir : `MultiSyncOnStart`, ne pas supprimer `pop-original` avant la fin). Appels, SMS, journaux et listes de fichiers : MPE les garde dans ses propres fichiers, indépendamment d'Outlook ; dire ce que la migration change ou ne change pas pour eux, en marquant [supposé] ce qui n'est pas constaté.
2. `tools/mpe_check.mjs` : script en LECTURE SEULE qui, sur le dossier de données de MPE (argument ; défaut `%APPDATA%\MyPhoneExplorer\<appareil>`) et le dossier d'échange (argument), compte contacts/rendez-vous/notes/tâches de chaque côté, signale les UID dupliqués, les éléments présents d'un seul côté et les différences de nombre, et sort un rapport court + code de sortie. Il ne modifie, ne crée ni ne supprime rien, ne lance aucun programme, n'affiche jamais le contenu des notes ni des fiches de contacts (compteurs et identifiants seulement).
3. Tester le script sur des échantillons synthétiques placés dans `tools/testdata/mpe_check/` (aucune donnée réelle).

Fin : push, résumé de 10 lignes.
