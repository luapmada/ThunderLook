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
- **Dossiers** : le volet gauche affiche des noms lisibles (jamais les noms techniques d'un PST, du type `Calendar3098`) et distincts d'un dossier à l'autre. Nouveau dossier (Ctrl+Maj+E), Renommer (F2), Déplacer, Supprimer, Tout marquer comme lu, Vider, Favoris. Les dossiers spéciaux (Boîte de réception, Corbeille…) ne se renomment ni ne se déplacent.
- **Conversations** : Affichage › « Afficher en tant que conversations » ; → déplie, ← replie.
- **Rechercher** : le champ accepte du texte libre et des mots-clés (boutons de l'onglet Rechercher) : `de:`, `objet:`, `pj:oui`, `catégorie:`, `lu:non`, `suivi:oui`, `importance:haute`, `reçu:"cette semaine"`. « Enregistrer la recherche » en fait un dossier de recherche ; Échap la ferme.
- **Règles** : Règles › Gérer les règles et les alertes, ou Dossier › Exécuter les règles. Les règles se limitent à un compte au besoin.
- **Comptes** : Envoyer/Recevoir › Paramètres du compte (IMAP 993 TLS ou 143 STARTTLS, SMTP). F9 envoie et reçoit ; « Travailler hors connexion » suspend les échanges.
- **Réponses automatiques** : Envoyer/Recevoir › Réponses automatiques. Expéditeurs bloqués et approuvés : menu Courrier indésirable.
- **Impression** : Ctrl+P.

## Calendrier

- Vues : Jour (Ctrl+Alt+1), Semaine de travail (Ctrl+Alt+2), Semaine (Ctrl+Alt+3), Mois (Ctrl+Alt+4) ; « Aujourd'hui » ramène à la date du jour.
- **Nouveau rendez-vous** : Ctrl+N, ou clic droit sur un créneau vide. La boîte permet sujet, lieu, heures, journée entière, rappel, catégorie.
- **Séries** : liste « Répéter » (tous les jours, semaines, mois, ans), intervalle « Tous les N », fin « Jamais », « Après N occurrences » ou « Le… » (dernier jour). Une série porte l'icône ↻ ; le détail dit « série périodique » ou « occurrence modifiée d'une série ».
- **Modifier** : double-clic ; sur une série, « Appliquer à » choisit « Cette occurrence seulement » (une exception) ou « Toute la série » (heures et périodicité modifiables ; « Inchangée » garde la règle actuelle). « Journée entière » est grisée pour une série.
- **Supprimer** : sur une série, « Cette occurrence » ou « Toute la série ».
- **Fichiers** : « Ouvrir un calendrier » importe un `.ics` ; « Enregistrer le calendrier (.ics) » et « Exporter en CSV » exportent.
- Les rappels dus s'affichent dans la barre d'état (« Rappels : N ») ; le panneau permet Ouvrir, Reporter, Ignorer.

## Contacts

- Liste à gauche (flèches haut/bas), fiche à droite. Le champ de filtre cherche dans la liste.
- **Nouveau contact** : Ctrl+N. **Supprimer** : bouton du ruban (annulable, voir plus bas).
- Imprimer la fiche ou la liste ; Importer / Exporter en `.vcf` (vCard).
- Photo, listes de distribution et catégories de contacts : pas encore modifiables.

## Tâches

- Nouvelle tâche : Ctrl+N ; « Marquer comme terminée » ; Supprimer ; Imprimer. Dans la boîte, Ctrl+S enregistre, Échap ferme.
- **Rappel** : une tâche avec échéance peut déclencher un rappel à l'heure choisie dans Options ; la boîte a aussi son champ « Rappel ». Le rappel est écrit dans `taches.ics` (échange avec le téléphone).
- **Tâches périodiques** : « Répéter » (chaque jour, semaine, mois, année, ou N jours/semaines/mois après la fin), « Tous les », fin « Sans fin », « Après N fois », « Jusqu'au ». Supprimer une occurrence crée la suivante ; on peut aussi supprimer toute la série. Une tâche périodique porte l'icône ↻.
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

ThunderLook ne parle jamais directement au téléphone : il écrit et relit des fichiers dans un dossier, et c'est l'application de synchronisation (MyPhoneExplorer, « MPE ») qui les lit et les écrit. Procédure détaillée : [mpe-fichiers.md](mpe-fichiers.md) ; essai sur le S20 FE : [jeu-de-test-s20.md](jeu-de-test-s20.md) ; passage du S23 : [migration-s23.md](migration-s23.md).

- **Ouvrir** : Ctrl+Maj+T ou Fichier › Échange avec le téléphone…, puis « Choisir le dossier… ».
- **Exporter maintenant** : écrit `contacts.vcf`, `taches.ics`, `notes.vnt`, `calendrier.ics` et `manifest.json` (ne pas modifier ce dernier).
- **Export automatique** : case « Exporter automatiquement toutes les », de 1 à 1 440 minutes (15 en cochant).
- **Si le téléphone a modifié des fichiers** depuis le dernier export, un export les écraserait : ThunderLook prévient et propose « Exporter quand même ».
- **Vérifier les changements** : relit les fichiers et liste, par module (Contacts, Rendez-vous, Notes, Tâches), les éléments nouveaux, modifiés ou supprimés, avec ancienne et nouvelle valeur de chaque champ, et les avertissements.
- **Appliquer** : seules les cases cochées sont appliquées, après confirmation. Les suppressions ne sont jamais cochées d'office.
- **Conflits** (élément modifié des deux côtés, ou référence d'export inconnue) : jamais tranchés d'office ; choisir « Garder ThunderLook », « Garder le téléphone » ou « Ignorer » (sans choix = ignoré).
- **Annuler le dernier import** : remet les éléments dans leur état précédent (valable aussi pour un import MPE).
- **Séries** : relues du téléphone (créées, modifiées, retirées). Sont refusés, avec un avertissement et sans rien importer de travers : fréquence autre que jour/semaine/mois/an, `RDATE`, « cette occurrence et les suivantes », fuseau hors heure de Paris, jour 29-31 du mois ou 29 février non identique à l'export, etc. Liste complète : [mpe-fichiers.md § 6](mpe-fichiers.md).
- **Notes** : le téléphone est la source. Une note du fichier différente de celle de ThunderLook la remplace sans conflit, même si elle a été modifiée ici depuis l'export.
- **Rappels** : ceux des rendez-vous et des tâches sont écrits dans les fichiers ; un fichier sans rappel de tâche n'efface pas celui de ThunderLook. Les listes de distribution ne sont pas exportées.

### Importer depuis MPE

Bouton « Importer depuis MPE » en bas de la même boîte : lecture seule des fichiers de données de MyPhoneExplorer (`Phonebook.dat`, `Organizer.dat`, `Notes.dat`), sens téléphone → ThunderLook uniquement. Rien n'est écrit dans le dossier de MPE.

- **Dossier et appareil** : le dossier de MPE est exploré ; « Choisir un autre dossier… » le change, puis on choisit l'appareil dans la liste.
- **Aperçu** : « Vérifier les changements » liste nouveau / modifié / supprimé et les conflits (contacts, rendez-vous des 90 jours avant et après, notes), avec avertissements.
- **Conflits** : « Garder ThunderLook », « Garder le téléphone (MPE) » ou ignorer ; rien n'est choisi d'office.
- **Appliquer** : cases cochées seulement, après confirmation ; récapitulatif (créés, modifiés, supprimés, gardés, ignorés, avertissements).
- **Annuler le dernier import** : bouton de la même section.

## Sauvegardes et annulation

- **Annuler** (Ctrl+Z ou bouton du ruban) : dans le courrier, défait la dernière opération ; dans les autres modules, une suppression ou une modification de rendez-vous, contact, tâche ou note se défait pendant 60 jours.
- **Sauvegardes** (Fichier › Sauvegardes…) : une par jour, lancée quand l'ordinateur est au repos (14 gardées), et une « pleine » par semaine (8 gardées), pour rendez-vous, contacts, tâches et notes, jamais le courrier. S'y ajoute chaque semaine une copie cohérente de toute la base (2 gardées, sous-dossier `backups/base`), écrite puis contrôlée par `integrity_check` avant d'être mise en place ; une erreur éventuelle s'affiche sur l'écran. « Sauvegarder maintenant » lance le tout (sans effet si c'est déjà fait), « Ouvrir le dossier » montre `backups/`. Restaurer : « Fusionner les éléments manquants » ne remet que ce qui n'existe plus ; « Tout remettre » redonne à chaque élément son contenu de la sauvegarde (la version actuelle reste 60 jours dans la corbeille interne). Restaurer la base entière depuis `backups/base` n'a pas d'écran à ce jour ; il n'y a pas non plus de « test de restauration » dans l'interface.
- Une mise à jour de ThunderLook qui change le format de la base garde une copie `thunderlook.db*.bak-…` à côté de la base.

## Où sont les données

Le dossier de données est indiqué dans **Aide › À propos**. Il contient :

- `thunderlook.db` : la base SQLite (courrier, rendez-vous, contacts, tâches, notes) ;
- `eml/` : les messages ;
- `backups/` : les sauvegardes (`base/` : copies de la base entière) ;
- `exchange-undo/` : de quoi annuler les imports du téléphone.

Par défaut c'est le sous-dossier `data` du dossier de données locales de l'application (sous Windows : `%LOCALAPPDATA%\…\data`). Un autre dossier se désigne par `dataDir` dans `config.json` (dossier de configuration de l'application) ou par la variable d'environnement `THUNDERLOOK_DATA`. Les réglages (`settings.json`) sont dans le dossier de configuration, pas dans celui des données.
