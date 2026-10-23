# Guide d'utilisation de ThunderLook

Ce guide décrit ce qui fonctionne aujourd'hui. Les boutons grisés du ruban (« pas encore disponible ») ne sont pas décrits.
Aide intégrée : touche **F1** ou onglet **Aide**.

## Prise en main

- Cinq modules, en bas à gauche ou au clavier : Courrier (Ctrl+1), Calendrier (Ctrl+2), Contacts (Ctrl+3), Tâches (Ctrl+4), Notes (Ctrl+5).
- Le ruban a cinq onglets : Accueil, Envoyer/Recevoir, Dossier, Affichage, Aide. Un onglet « Rechercher » apparaît pendant une recherche.
- **Fichier** ouvre les Options (thème, calendrier, courrier, tâches et notes, rappels) ; sa flèche donne « Sauvegardes… » et « Échange avec le téléphone… ».
- La recherche (Ctrl+E ou F3) peut porter sur le courrier ou, avec la portée « Tout », sur les contacts, rendez-vous, tâches et notes.

## Courrier

- **Lire** : choisir un dossier à gauche, un message dans la liste ; le volet de lecture peut être à droite, en bas ou masqué (Affichage › Volet de lecture).
- **Écrire** : Ctrl+N (nouveau), Ctrl+R (répondre), Ctrl+Maj+R (répondre à tous), Ctrl+F (transférer). Le brouillon s'enregistre ; envoi par le compte IMAP/SMTP configuré.
- **Organiser** : Suppr (corbeille), Maj+Suppr (définitif, avec confirmation), Archiver, Lu/Non lu (Ctrl+Q / Ctrl+U), Suivi (Inser), catégories, Ctrl+Maj+V (déplacer vers un dossier). Le glisser-déposer déplace ; avec Ctrl il copie.
- **Dossiers** : Nouveau dossier (Ctrl+Maj+E), Renommer (F2), Déplacer, Supprimer, Tout marquer comme lu, Vider, Favoris. Les dossiers spéciaux (Boîte de réception, Corbeille…) ne se renomment ni ne se déplacent.
- **Conversations** : Affichage › « Afficher en tant que conversations » ; → déplie, ← replie.
- **Rechercher** : le champ accepte du texte libre et des mots-clés (boutons de l'onglet Rechercher) : `de:`, `objet:`, `pj:oui`, `catégorie:`, `lu:non`, `suivi:oui`, `importance:haute`, `reçu:"cette semaine"`. « Enregistrer la recherche » en fait un dossier de recherche ; Échap la ferme.
- **Règles** : Règles › Gérer les règles et les alertes, ou Dossier › Exécuter les règles. Les règles se limitent à un compte au besoin.
- **Comptes** : Envoyer/Recevoir › Paramètres du compte (IMAP 993 TLS ou 143 STARTTLS, SMTP). F9 envoie et reçoit ; « Travailler hors connexion » suspend les échanges.
- **Réponses automatiques** : Envoyer/Recevoir › Réponses automatiques. Expéditeurs bloqués et approuvés : menu Courrier indésirable.
- **Impression** : Ctrl+P.

## Calendrier

- Vues : Jour (Ctrl+Alt+1), Semaine de travail (Ctrl+Alt+2), Semaine (Ctrl+Alt+3), Mois (Ctrl+Alt+4) ; « Aujourd'hui » ramène à la date du jour.
- **Nouveau rendez-vous** : Ctrl+N, ou clic droit sur un créneau vide. La boîte permet sujet, lieu, heures, journée entière, rappel, catégorie, et périodicité (jour, semaine, mois, an ; fin jamais, après N, à une date).
- **Modifier / supprimer** : double-clic sur un rendez-vous. Pour une série, on peut supprimer UNE occurrence. Modifier les heures ou la périodicité d'une série existante n'est pas encore possible.
- **Fichiers** : « Ouvrir un calendrier » importe un `.ics` ; « Enregistrer le calendrier (.ics) » et « Exporter en CSV » exportent.
- Les rappels dus s'affichent dans la barre d'état (« Rappels : N ») ; le panneau permet Ouvrir, Reporter, Ignorer.

## Contacts

- Liste à gauche (flèches haut/bas), fiche à droite. Le champ de filtre cherche dans la liste.
- **Nouveau contact** : Ctrl+N. **Supprimer** : bouton du ruban (annulable, voir plus bas).
- Imprimer la fiche ou la liste ; Importer / Exporter en `.vcf` (vCard).
- Photo, listes de distribution et catégories de contacts : pas encore modifiables.

## Tâches

- Nouvelle tâche : Ctrl+N ; « Marquer comme terminée » ; Supprimer ; Imprimer. Dans la boîte, Ctrl+S enregistre, Échap ferme.
- Une tâche avec échéance peut déclencher un rappel à l'heure choisie dans Options. Tâches périodiques : pas encore.
- Importer / Exporter en `.ics`.

## Notes

- Nouvelle note : Ctrl+N ; Supprimer ; Imprimer. Dans la boîte, Ctrl+S enregistre, Échap ferme. Couleur d'une nouvelle note : Options.
- Importer / Exporter en `.vnt`.

## Raccourcis clavier

| Raccourci | Action |
|---|---|
| F1 | Cette aide |
| Ctrl+1 à Ctrl+5 | Courrier, Calendrier, Contacts, Tâches, Notes |
| Ctrl+E, F3 | Rechercher |
| F6, Maj+F6 | Passer d'une zone à l'autre (recherche, onglets, dossiers, liste, lecture) |
| Ctrl+. / Ctrl+, | Élément suivant / précédent |
| Ctrl+Z | Annuler |
| Ctrl+P | Imprimer |
| Ctrl+Maj+T | Échange avec le téléphone |
| F9 | Envoyer et recevoir |
| Ctrl+N | Nouveau message, rendez-vous, contact, tâche ou note selon le module |
| Ctrl+R, Ctrl+Maj+R, Ctrl+F | Répondre, répondre à tous, transférer |
| Ctrl+Q, Ctrl+U | Marquer lu, non lu |
| Inser | Indicateur de suivi |
| Suppr, Maj+Suppr | Supprimer, supprimer définitivement |
| Ctrl+A | Tout sélectionner dans la liste |
| Ctrl+Maj+V | Déplacer vers un dossier |
| Ctrl+Maj+E, F2 | Nouveau dossier, renommer le dossier |
| Ctrl+Alt+1 à Ctrl+Alt+4 | Calendrier : Jour, Semaine de travail, Semaine, Mois |
| Échap | Fermer une boîte, un menu ou la recherche |

## Échange avec le téléphone

ThunderLook ne parle jamais directement au téléphone : il écrit et relit des fichiers dans un dossier, et c'est l'application de synchronisation du téléphone qui les lit et les écrit.

1. Ouvrir **Fichier › Échange avec le téléphone** (Ctrl+Maj+T) et choisir le **dossier d'échange**.
2. **Exporter maintenant** écrit `contacts.vcf`, `taches.ics`, `notes.vnt`, `calendrier.ics` et un `manifest.json`. Un export automatique toutes les N minutes (1 à 1 440) est facultatif.
3. **Vérifier les changements** relit les fichiers et liste ce qui diffère de la base : nouveau, modifié, supprimé. Cocher ce qu'on accepte puis **Appliquer**. Les suppressions ne sont jamais cochées d'office.
4. Si le téléphone a modifié des fichiers depuis le dernier export, un export les écraserait : ThunderLook prévient et propose « Exporter quand même ».
5. **Annuler le dernier import** remet les éléments dans leur état précédent.

## Sauvegardes et annulation

- **Annuler** (Ctrl+Z ou bouton du ruban) : dans le courrier, défait la dernière opération ; dans les autres modules, une suppression ou une modification de rendez-vous, contact, tâche ou note se défait pendant 60 jours.
- **Sauvegardes** (Fichier › Sauvegardes…) : une par jour au lancement (14 gardées) et une par semaine (8 gardées), pour rendez-vous, contacts, tâches et notes, jamais le courrier. On peut « Sauvegarder maintenant », « Ouvrir le dossier », puis restaurer : « Fusionner les éléments manquants » ne remet que ce qui n'existe plus ; « Tout remettre » redonne à chaque élément son contenu de la sauvegarde (la version actuelle reste 60 jours dans la corbeille interne).
- Une mise à jour de ThunderLook qui change le format de la base garde une copie `thunderlook.db*.bak-…` à côté de la base.

## Où sont les données

Le dossier de données est indiqué dans **Aide › À propos**. Il contient :

- `thunderlook.db` : la base SQLite (courrier, rendez-vous, contacts, tâches, notes) ;
- `eml/` : les messages ;
- `backups/` : les sauvegardes ;
- `exchange-undo/` : de quoi annuler les imports du téléphone.

Par défaut c'est le sous-dossier `data` du dossier de données locales de l'application (sous Windows : `%LOCALAPPDATA%\…\data`). Un autre dossier se désigne par `dataDir` dans `config.json` (dossier de configuration de l'application) ou par la variable d'environnement `THUNDERLOOK_DATA`. Les réglages (`settings.json`) sont dans le dossier de configuration, pas dans celui des données.
