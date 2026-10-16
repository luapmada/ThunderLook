# Référence d'interface (structure et parcours)

Sert de cahier des charges **structurel**. Les libellés d'Outlook cités ici sont des repères d'analyse : le clone utilisera son propre glossaire (`notes/juridique.md` §4). L'habillage suit `DESIGN.md`.

## 1. Méthode
- Arbre UI Automation (vue « contrôle ») et capture de la fenêtre principale par `tools/uia_dump.ps1`. Synthèse sans données personnelles par `tools/uia_summarize.py`.
- Bruts dans `notes/uia-raw/` et `captures/`, ignorés par git car ils contiennent des courriels.
- Relevés du 2026-10-16 sur l'Outlook installé : `ProPlus2021Retail` 16.0.20326.20232, Windows 11, interface française, thème sombre.
- Relevés faits :
  - Courrier : ruban simplifié (Accueil, Envoi/réception, Affichage, Aide) et ruban classique (Accueil, Envoi/réception, Dossier, Affichage) ;
  - Calendrier, Contacts, Tâches : ruban simplifié uniquement.
- REA n'a pas pu servir ici : `observe_native_ui` est réservé à macOS (voir `REA-LIMITES.md`).

**Effet de bord constaté.** Revenir sur le dossier « Courrier non lu » fait sélectionner le message suivant, qu'Outlook marque comme lu : 2 messages pendant les relevés. Pour les prochains relevés, il faudra un profil de test avec un PST vide, ou éviter tout retour sur un dossier.

## 2. Disposition générale (fenêtre principale)

```
┌ Barre de titre : [barre d'accès rapide] ······ [ Rechercher ] ······ [compte] [_][□][X] ┐
│ Onglets : Fichier | Accueil | Envoi/réception | (Dossier) | Affichage | Aide | (compléments) │
│ Ruban : classique (3 rangées + nom de groupe) ou simplifié (1 rangée + « … »)     [⌄]    │
├──────────────┬──────────────────────────────┬────────────────────────────────────────────┤
│ Volet des    │ [Tous] [Non lus]   Par Date ⌄ ↑ │ Objet (grand)                              │
│ dossiers     │ ▾ Aujourd'hui                  │ (pastille) Expéditeur   [Répondre][…][Transf.]│
│  Favoris     │  Expéditeur             heure  │ À : …                              date    │
│  Fichier…    │  Objet (accent si non lu)      │ (i) barre d'information (images bloquées…)  │
│  Dossiers de │  aperçu (1 ligne)              │ Corps du message                           │
│  recherche   │ ▾ Hier                         │                                            │
│ [✉][▦][👥][✓][…]│                              │                                            │
├──────────────┴──────────────────────────────┴────────────────────────────────────────────┤
│ Éléments : N   Non lu : N   Rappels : N      Paramètres d'affichage [Normal][Lecture] −──+ 100 % │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

- **Zone de recherche** dans la barre de titre (`SearchTextBox`, touches Alt, Y).
- **Barre d'accès rapide** : Envoyer/Recevoir tous les dossiers (Alt, 1), Annuler (Alt, 2), personnalisation.
- **Ruban** :
  - bouton « Options d'affichage du ruban » (Alt, Z R) : ruban classique, ruban simplifié, plein écran, onglets seulement, toujours afficher, masquer la barre d'accès rapide ;
  - en mode simplifié, le menu « Autres commandes, <onglet> » (KeyTip `0 0`) regroupe ce qui déborde.
- **Volet des dossiers** (repliable par `<`) : sections Favoris, puis un nœud par fichier de données, puis « Dossiers de recherche ».
  - Compteur **sans crochets, en accent** : nombre de **non lus**.
  - Compteur **entre crochets** : nombre **total**. Il s'affiche pour Brouillons, Courrier indésirable et les dossiers de recherche « Pour le suivi ».
- **Navigation basse** en mode compact : icônes Courrier, Calendrier, Contacts, Tâches, puis « … » (Navigation Menu).
- **Barre de dossier** au-dessus de la liste :
  - pivots « Tous » et « Non lus » (« Afficher tous les e-mails » / « Afficher les e-mails non lus ») ;
  - « Trier, organiser ou filtrer les messages » (« Par Date ⌄ ») ;
  - sens du tri (↑).
- **Liste** (`SuperGrid`, « Affichage Tableau ») :
  - groupes repliables par date : « Aujourd'hui », « Hier », etc. ;
  - lignes sur 3 niveaux : expéditeur, objet, aperçu ;
  - heure alignée à droite ;
  - non lu : filet vertical d'accent à gauche et objet en couleur d'accent ;
  - drapeau de suivi à droite.
- **Volet de lecture** :
  - en-tête : objet en grand, pastille à initiales, expéditeur, ligne « À », date à droite ;
  - boutons Répondre, Répondre à tous, Transférer, plus un menu « … » ;
  - barre d'information pour le suivi et les images distantes bloquées ;
  - le corps est rendu par le moteur de Word (classe `_WwB`, « Document1 »).
- **Barre d'état** :
  - Éléments, Non lu, Rappels ;
  - à droite : Paramètres d'affichage, modes Normal et Lecture, zoom (−, curseur, +, %).

## 3. Ruban classique : module Courrier

KeyTips relevés sur l'interface française (séquence complète). Raccourcis = `AcceleratorKey` exposé par UIA.

### Accueil (Alt, L)
| Groupe | Commande (id UIA) | KeyTip | Raccourci |
|---|---|---|---|
| Nouveau | Nouveau courrier (`NewItem`) | L, V | — |
| | Nouveaux éléments ▾ (`MailNewItemMenu`) | L, N | — |
| Supprimer | Ignorer (`IgnoreConversation`) | L, G | Ctrl+Suppr |
| | Nettoyer ▾ (`ThreadCompressSplitButton`) | L, E | — |
| | Courrier indésirable ▾ (`JunkMenu`) | L, C | — |
| | Supprimer (`Delete`) | L, K S | — |
| | Archiver… (`MoveToArchiveFolder`) | L, I | — |
| Répondre | Répondre (`Reply`) | L, K R | — |
| | Répondre à tous (`ReplyAll`) | L, T R | — |
| | Transférer (`Forward`) | L, F T | — |
| | Réunion (`ReplyWithMeeting`) | L, M R | — |
| | Plus ▾ (`RespondActionsMenu`) | L, É P | — |
| Actions rapides | Galerie (`QuickStepsGallery`) | L, T A | — |
| | Gérer les actions rapides… (`ManageQuickSteps`) | L, K É | — |
| Déplacer | Déplacer ▾ (`MoveToFolderGallery`) | L, Ç É | — |
| | Règles ▾ (`AlwaysMoveToFolder`) | L, Q R | — |
| | Envoyer à OneNote (`MoveToOneNote`) | L, O N | — |
| Indicateurs | Non lu/Lu (`ToggleMarkAsRead`) | L, O 1 | Ctrl+U / Ctrl+Q |
| | Classer ▾ (`CategorizeMenu`) | L, L L | — |
| | Assurer un suivi ▾ (`FollowUpMenu`) | L, X | — |
| Rechercher | Rechercher des contacts (champ) | L, Ç R | — |
| | Carnet d'adresses… (`AddressBook`) | L, L C | Ctrl+Maj+B |
| | Filtre de courrier ▾ (`MenuFilterMail`) | L, F I | — |
| Fonction vocale | Lecture à voix haute (`ReadAloud`) | L, L 1 | — |
| Langue | Traduction ▾ (`InlineTranslationMenu`) | L, T 1 | — |
| Envoyer/Recevoir | Envoyer/Recevoir tous les dossiers (`SendReceiveAll`) | L, S | — |

### Envoi/réception (Alt, J S)
| Groupe | Commande (id) | KeyTip | Raccourci |
|---|---|---|---|
| Envoyer et recevoir | Envoyer/Recevoir tous les dossiers (`SendReceiveAll`) | S | — |
| | Mettre à jour le dossier (`UpdateFolder`) | O | — |
| | Envoyer tout (`SendAll`) | E | — |
| | Groupes d'envoi/réception ▾ (`ReceiveMenu`) | G | — |
| Télécharger | Afficher la progression… (`ShowProgress`) | F | — |
| | Annuler tout (`CancelAll`) | N | — |
| Serveur | Télécharger les en-têtes (`DownloadHeadersInThisFolder`) | É | — |
| | Marquer pour téléchargement (`MarkForDownload`) | M | Ctrl+Alt+M |
| | Annuler les marques pour téléchargement (`Unmark`) | A | Ctrl+Alt+U |
| | Traiter les en-têtes marqués (`SyncThisfolderMarked`) | I | — |
| Préférences | Travailler en mode hors connexion (`ToggleOnline`) | R | — |

### Dossier (Alt, O) — absent du ruban simplifié
| Groupe | Commande (id) | KeyTip | Raccourci |
|---|---|---|---|
| Nouveau | Dossier… (`NewFolder`) | O | Ctrl+Maj+E |
| | Dossier de recherche… (`NewSearchFolder`) | U O | Ctrl+Maj+Z (vu dans UIA) |
| Actions | Personnaliser ce dossier de recherche… (`CustomizeSearchFolder`) | P P | — |
| | Renommer le dossier (`RenameThisFolder`) | W | — |
| | Copier le dossier (`CopyThisFolder`) | C C | — |
| | Déplacer le dossier (`MoveThisFolder`) | É | — |
| | Supprimer le dossier (`DeleteThisFolder`) | U S | — |
| Nettoyer | Marquer tout comme lu (`MarkAllAsRead`) | Q | — |
| | Exécuter les règles… (`RunRulesNow`) | X | — |
| | Afficher tous les dossiers de A à Z (`AlwaysSortFoldersAtoZ`) | A | — |
| | Nettoyer le dossier ▾ (`ThreadCompressFolderMenu`) | L | — |
| | Supprimer tout (`DeleteAll`) | I S | — |
| | Récupérer les éléments supprimés… (`RecoverDeletedItems`) | R R | — |
| Favoris | Ajouter aux favoris (`ToggleShowInFavorites`) | F A | — |
| Propriétés | Paramètres d'archivage automatique (`AutoArchiveSettings`) | M | — |
| | Autorisations du dossier (`SharingPermissions`) | Ù | — |
| | Propriétés du dossier (`FolderProperties`) | P R | — |

### Affichage (Alt, N)
| Groupe | Commande (id) | KeyTip |
|---|---|---|
| Affichage actuel | Changer l'affichage ▾ (`CurrentViewGallery`) | J |
| | Paramètres d'affichage… (`CurrentViewSettings`) | M |
| | Rétablir l'affichage (`ResetCurrentView`) | R |
| Messages | ☐ Afficher en tant que conversations (`ShowInConversations`) | C |
| | Paramètres de conversation ▾ (`ConversationsMenu`) | Ç |
| Réorganisation | Aperçu du message ▾ (`MessagePreview`) | A |
| | Galerie « Organiser par » (`ArrangementGallery`) | Ù |
| | Inverser le tri (`ReverseSort`) | N |
| | Ajouter des colonnes (`ShowFields`) | À |
| Disposition | Utiliser un espacement plus étroit (`DensitySetting`) | U 1 |
| | Volet des dossiers ▾ (`MenuNavigationPane`) | O |
| | Volet de lecture ▾ (`MenuPreviewPane`) | V |
| | Barre des tâches ▾ (`MenuToDoBar`) | B |
| Fenêtre | Fenêtre des rappels (`ShowRemindersWindow`) | E |
| | Ouvrir dans une nouvelle fenêtre (`WebOpenInNewWindow`) | U 2 |
| | Fermer tous les éléments (`CloseAllItems`) | F E |
| Lecteur immersif | Lecteur immersif (`ToggleLearningTools`) | L |

## 4. Ruban simplifié (une seule rangée)
- **Courrier / Accueil** : Nouveau courrier ▾, Supprimer ▾, Archiver, Déplacer ▾, Répondre, Répondre à tous, Transférer, galerie Actions rapides, Non lu/Lu, Classer ▾, Assurer un suivi ▾, Rechercher des contacts, Carnet d'adresses, Filtre ▾, Traduction ▾, Envoyer/Recevoir, « … ».
  - Les groupes sont nommés autrement : « Déplacer et supprimer » au lieu de « Supprimer » et « Déplacer ».
- **Courrier / Affichage** : Changer l'affichage, Affichage actuel ▾, Organiser par ▾, Inverser le tri, Espacement plus étroit, Disposition ▾, Lecteur immersif.
- **Calendrier / Accueil** :

| Groupe | Commandes |
|---|---|
| Nouveau | Nouveau rendez-vous ; Nouvelle réunion ▾ |
| Atteindre | Aujourd'hui (Ctrl+T relevé) ; 7 prochains jours ; Atteindre la date… (Ctrl+B relevé) |
| Organiser | Jour (Ctrl+Alt+1) ; Semaine de travail (Ctrl+Alt+2) ; Semaine (Ctrl+Alt+3) ; Mois (Ctrl+Alt+4) ; Affichage Planification (Ctrl+Alt+5) ; Options du calendrier… |
| Ajouter et partager | Ajouter ▾ ; Partager ▾ |

  - Vue : navigateur de dates sur deux mois, liste « Mes calendriers » à cases à cocher, barre « Aujourd'hui < > période » et sélecteur de vue, grille avec zone « journée entière », colonne du jour en surbrillance, couleurs de catégorie.
- **Contacts / Accueil** :

| Groupe | Commandes |
|---|---|
| Nouveau | Nouveau contact ▾ |
| Supprimer | Supprimer |
| Communiquer | Message électronique |
| Actions | Déplacer ▾ ; Envoyer à OneNote |
| Partager | Partager des contacts ▾ |
| Indicateurs | Classer ▾ ; Assurer un suivi ▾ ; Privé |
| Rechercher | Rechercher des contacts |

  - Vue liste : colonnes icône, pièce jointe, Nom complet ▾, Société, Classer sous, Pays/Région, Télécopie (bureau), Téléphone (domicile), Téléphone mobile, Adresse de courrier, Journal, Catégories.
  - Première ligne : « Cliquez ici pour ajouter un nouvel élément Contact ».
- **Tâches / Accueil** :

| Groupe | Commandes |
|---|---|
| Nouveau | Nouvelle tâche ; Nouveau courrier (Ctrl+Maj+M) |
| Supprimer | Supprimer |
| Répondre | Répondre ; Répondre à tous ; Transférer |
| Gérer | Marquer comme terminée(s) ; Supprimer de la liste ; Affecter une tâche |
| Suivi | Ajouter un indicateur à la tâche ▾ |
| Indicateurs | Classer ; Privé ; Importance haute ; Importance faible |

  - Vue « Liste des tâches » : en-tête « Réorganiser par : Indicateur : date de fin », ligne de saisie « Entrez une nouvelle tâche », groupes par échéance, colonne de drapeaux.
  - La barre d'état affiche « Filtre appliqué ».

## 5. Raccourcis clavier

Sources :
1. **UIA de ton Outlook (fr, 2021)**, la source la plus spécifique : voir les tableaux ci-dessus.
2. Page Microsoft « Raccourcis clavier pour Outlook » ([support.microsoft.com](https://support.microsoft.com/fr-fr/office/raccourcis-clavier-pour-outlook-3cdeb221-7ae5-4c1d-8c1d-9e63216c1efd), consultée le 2026-10-16). Elle décrit la version Microsoft 365 actuelle : Ctrl+3 = Copilot, Ctrl+4 = Contacts, Ctrl+Maj+5 = To Do, Ctrl+7 = Loop. **Elle ne vaut pas tel quel pour Outlook 2021.**

Socle commun retenu pour le courrier (identique dans les deux sources, ou relevé dans UIA) :

| Action | Touches |
|---|---|
| Nouveau message (dans Courrier) | Ctrl+N |
| Nouveau message (partout) | Ctrl+Maj+M |
| Répondre | Ctrl+R |
| Répondre à tous | Ctrl+Maj+R |
| Transférer | Ctrl+F |
| Transférer en pièce jointe | Ctrl+Alt+F |
| Répondre par une réunion | Ctrl+Alt+R |
| Envoyer | Alt+S ou Ctrl+Entrée |
| Marquer comme lu / non lu | Ctrl+Q / Ctrl+U |
| Ignorer la conversation | Ctrl+Suppr (UIA) ; la page indique aussi Ctrl+Maj+D |
| Supprimer | Suppr |
| Indicateur rapide | Inser |
| Boîte de dialogue d'indicateur | Ctrl+Maj+G |
| Rechercher | Ctrl+E ou F3 |
| Recherche avancée | Ctrl+Maj+F |
| Portée « dossier actuel » / « sous-dossiers » / « tous les éléments » | Ctrl+Alt+K / Ctrl+Alt+Z / Ctrl+Alt+A |
| Envoyer/Recevoir | F9 ou Ctrl+M |
| Carnet d'adresses | Ctrl+Maj+B |
| Atteindre un dossier | Ctrl+Y |
| Nouveau dossier | Ctrl+Maj+E |
| Boîte de réception / Boîte d'envoi | Ctrl+Maj+I / Ctrl+Maj+O |
| Ouvrir l'élément | Entrée ou Ctrl+O |
| Imprimer | Ctrl+P |
| Enregistrer / Enregistrer sous | Ctrl+S / F12 |
| Propriétés | Alt+Entrée |
| Passer d'un volet à l'autre | F6 / Ctrl+Tab |
| Élément suivant / précédent (ouvert) | Ctrl+. / Ctrl+, |
| Message précédent / suivant | ↑ / ↓ |
| Défiler le volet de lecture | Espace / Maj+Espace |

Navigation entre modules en 2021 (Ctrl+1 Courrier, Ctrl+2 Calendrier, Ctrl+3 Contacts, Ctrl+4 Tâches, Ctrl+5 Notes, Ctrl+6 Dossiers, Ctrl+7 Raccourcis) : **à vérifier sur ton Outlook**. Ce n'est pas exposé dans UIA, et la page Microsoft décrit une autre version.

## 6. Non relevé (à faire avec un profil de test)
- Menu Fichier (Backstage), y compris Options.
- Fenêtre de rédaction et son ruban (Message, Insertion, Options, Format du texte, Révision).
- Fenêtre de message ouvert.
- Boîtes de dialogue : Règles et alertes et l'assistant de règles, Gérer les actions rapides, Paramètres d'affichage avancés, Recherche avancée, Catégories de couleurs, Indicateur personnalisé.
- Menus contextuels et menus déroulants du ruban (Nouveaux éléments, Classer, Assurer un suivi, Organiser par…).
- Superposition des KeyTips, à-coups clavier dans la liste.
- Rubans classiques de Calendrier, Contacts et Tâches.
