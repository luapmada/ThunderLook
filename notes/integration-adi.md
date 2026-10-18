# Intégration avec ADI Facturation

Statut : **proposition** (2026-10-19), à valider par Paul. ThunderLook et ADI Facturation vont travailler en relation étroite. ADI est presque en production.

## Ce que l'on sait d'ADI Facturation
- Application de bureau Electron, installée dans `%LOCALAPPDATA%\Programs\ADI Facturation`.
- Installeurs versionnés de 1.0.0 à 2.6.1.
- Base SQLite `adi-facturation.sqlite`, avec des sauvegardes horodatées et des migrations de schéma (v5 → v8).
  - Elle est rangée dans le dossier d'entreprise.
- **Pas encore vu** :
  - le code source (emplacement inconnu) ;
  - le schéma de la base ;
  - la façon dont ADI envoie aujourd'hui les factures par mail.

  L'outil a refusé la lecture du dossier d'entreprise et du paquet installé (données d'entreprise). Il faut l'accord explicite de Paul, ou le chemin du dépôt de code.

## Indice relevé au J4 dans tes données
Dans « Éléments envoyés », des factures d'ADI (pièces jointes `ADI_2026_FACT….pdf`, catégorie « ADI ») partent avec
l'objet **« Envoi d'un message : ADI_2026… »**. C'est l'objet que Windows met quand une application envoie un fichier par
**Simple MAPI** (`MAPISendMail`) au client de messagerie par défaut, et ce client est Outlook.
- Hypothèse forte : ADI envoie ses factures par MAPI. Seule la lecture du code d'ADI la confirmera.
- Conséquence :
  - pour remplacer Outlook **sans toucher à ADI**, ThunderLook doit s'enregistrer comme client MAPI de Windows. Cela
    demande une DLL qui exporte `MAPISendMail`/`MAPISendMailW` et la déclaration sous `HKLM\Software\Clients\Mail`. La
    DLL transmet ensuite le message à la fenêtre de rédaction de ThunderLook ;
  - l'autre voie : qu'ADI appelle directement la ligne de commande de rédaction, ce qui est plus simple et plus robuste.
  - À trancher avec Paul au J5.

## Principes
1. **Chaque application reste maître de ses données.**
   - ThunderLook n'écrit jamais dans la base d'ADI, et ADI n'écrit jamais dans `thunderlook.db`.
   - Les échanges passent par des lectures seules, ou par des commandes explicites.
2. **Tout reste local.** Ni serveur ni service cloud entre les deux.
3. **Contrats versionnés.**
   - Pour la base de ThunderLook, `thunderlook.db` (`PRAGMA user_version`) : les migrations ne font qu'ajouter, jamais renommer ni supprimer une colonne lue par ADI.
   - Pour celle d'ADI, ThunderLook vérifie la version de schéma avant de la lire. Si la version est inconnue, l'intégration se coupe proprement.

## Échanges prévus

| Sens | Besoin | Mécanisme proposé | Jalon |
|---|---|---|---|
| ADI → ThunderLook | Envoyer une facture ou un devis PDF à un client | Ligne de commande de rédaction : `thunderlook.exe --compose --to <adresse> --subject <objet> --body-file <html> --attach <pdf>`, plus des commutateurs compatibles Outlook (`/c ipm.note /m "mailto:…" /a <fichier>`). ThunderLook peut ainsi remplacer Outlook sans modifier ADI, si ADI pilote Outlook de cette façon | J5 (fenêtre de rédaction), J7 (envoi) |
| ADI → ThunderLook | Afficher l'historique des mails d'un client depuis sa fiche | Lecture seule de `thunderlook.db` (`thunderlook_store::open_read_only`, ou SQLite en lecture seule côté Node). Depuis le J4 : index `message_from_addr` et index plein texte des destinataires ; dans ThunderLook, la recherche `adresse:client@exemple.fr` donne cet historique (218 ms sur 252 846 éléments). Plus tard : `thunderlook.exe --search "adresse:…"` | J4 fait pour la recherche ; ligne de commande au J5 |
| ThunderLook → ADI | Voir, dans le volet de lecture, que l'expéditeur est un client d'ADI : nom, factures impayées, lien vers la fiche | Lecture seule de la base d'ADI, chemin configuré dans `config.json` (`adiDatabase`), connexion SQLite en lecture seule et version de schéma contrôlée | Après J4, quand le schéma d'ADI sera connu |
| ThunderLook → ADI | Classer un mail ou une pièce jointe (bon de commande, règlement) dans le dossier du client | Commande d'ADI à définir (ligne de commande ou protocole `adi://`). ThunderLook ne touche pas aux fichiers d'ADI | À définir avec Paul |

## Déjà en place au J2
- `thunderlook_store::open_read_only` ouvre la base de ThunderLook en lecture seule (`query_only`). C'est la porte d'entrée prévue pour ADI.
- Le schéma est versionné (v3 au J3). Les migrations v1 → v2 → v3 ne font qu'ajouter un index puis une table.
- Depuis le J3, ThunderLook écrit dans sa base : lu, indicateurs, catégories, classement.
  - La base est en mode WAL : un lecteur comme ADI peut lire pendant que ThunderLook écrit, sans être bloqué.
  - Il voit l'état validé de la dernière transaction.

## Questions pour Paul
1. Où est le code source d'ADI Facturation (dépôt, dossier) ?
2. Comment ADI envoie-t-il les factures aujourd'hui : Outlook piloté (ligne de commande, COM), `mailto:`, SMTP direct ?
3. Quelles données d'ADI veux-tu voir dans ThunderLook, et dans l'autre sens ?
