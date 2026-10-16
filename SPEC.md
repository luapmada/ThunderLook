# SPEC — client de messagerie de bureau (ergonomie Outlook 2021 classique)

Statut : étape 0. Priorités fixées par les réponses Q1 à Q4 du 2026-10-16 (voir la fin du fichier).
Nom du produit : à définir (jamais « Outlook » ni marque Microsoft).

**Sources de l'inventaire** : connaissance générale d'Outlook 2021, **pas encore vérifiée**. Elle sera
confrontée en étape 2 à l'arbre UI Automation et aux captures de l'Outlook installé :
Office `ProPlus2021Retail` 16.0.20326.20232 x64 (registre `HKLM\SOFTWARE\Microsoft\Office\ClickToRun\Configuration`,
VersionInfo de `OUTLOOK.EXE`).

## Contraintes
- Windows 11 uniquement ; 100 % local ; pas de compte Microsoft ni de service cloud payant.
- `A:\Fichiers Outlook` : lecture seule. On travaille sur des copies.
- Interdits : code, icônes, images, chaînes, polices, nom et branding Microsoft/Outlook. Voir `DESIGN.md` et `notes/juridique.md`.
- Interface : `DESIGN.md` s'applique (structure fidèle, habillage original).

## Données réelles constatées (listing en lecture seule, 2026-10-16)
- PST : `archive.pst` 15,0 Go ; `Outlook_300923.pst` 3,8 Go (ouvert par Outlook en ce moment) ; `Outlook_240611.pst` 2,6 Go ; `archive 2.pst` 2,2 Go ; plus des sauvegardes.
- OST : 3 comptes (un de 127 Mo, deux de 16 Mo).
- Comptes (registre `HKCU\...\Outlook\Profiles`, lecture seule ; adresses non reproduites ici) : 3 adresses, toutes chez Hostinger.
  - Profil « Outlook » : IMAP + SMTP.
  - Profil « pop-original » : POP3, option « laisser sur le serveur ».
  - Aucun compte Exchange vu.
- Autres : règles `.rwz` (2010, 2013), modèles `.oft` (2), signatures (HTM/RTF/TXT), calendrier `.ics` et `.csv`, export registre des comptes `.reg`.
- Conséquence : le moteur doit tenir des PST de plusieurs Go (jusqu'à 15 Go).

## Légende
| Code | Sens |
|---|---|
| P0 | Socle du premier jalon mail |
| P1 | Mail complet |
| P2 | Modules hors mail, dans l'ordre Calendrier, puis Contacts, puis Tâches (Q3) |
| P3 | Secondaire / confort |
| HP | Hors périmètre : exige Exchange / Microsoft 365, un service cloud, ou un composant Microsoft (Word, Windows Search…) qu'il faudrait remplacer |

## 1. Courrier

### 1.1 Comptes et stockage
| Fonction | Prio |
|---|---|
| Importer un PST (ANSI et Unicode) en lecture seule, depuis une copie | P0 |
| Stockage propre local (base + messages `.eml`) ; toute modification se fait là, jamais dans un PST | P0 |
| Écrire ou exporter du PST | non retenu (Q2) |
| Comptes IMAP + SMTP (Hostinger) | P1, jalon suivant le PST hors ligne (Q1) |
| Compte POP3 | P1, ordre par rapport à IMAP non tranché |
| Lire un OST | P3 |
| Exchange, Microsoft 365, Outlook.com | HP |
| Gestion des fichiers de données (ajouter, fermer, propriétés, compacter) | P1 |
| Importer / exporter (PST, CSV) | P1 |
| Archivage automatique | P3 |
| Profils multiples | P3 |

### 1.2 Volet Dossiers
| Fonction | Prio |
|---|---|
| Arborescence multi-fichiers, dossiers spéciaux (Réception, Brouillons, Envoyés, Supprimés, Indésirable, Boîte d'envoi, Archive, RSS), compteurs non lus | P0 |
| Créer / renommer / déplacer / copier / supprimer un dossier, glisser-déposer | P1 |
| Favoris | P1 |
| Dossiers de recherche | P1 (Q4) |
| Propriétés, taille des dossiers, nettoyage | P3 |
| Volet réductible | P1 |

### 1.3 Liste des messages
| Fonction | Prio |
|---|---|
| Colonnes (De, Objet, Reçu, Taille, Catégories, Indicateur, Pièce jointe, Importance), tri, non lus en gras | P0 |
| Liste virtualisée (dossiers de plus de 100 000 éléments) | P0 |
| Sélection multiple, navigation clavier | P0 |
| « Organiser par » et groupes repliables | P1 |
| Affichage en conversations | P1 |
| Filtres rapides (Tous, Non lus) | P1 |
| Aperçu du message (0 à 3 lignes), dispositions compacte / simple / aperçu | P1 |
| Modifier la vue, colonnes personnalisées, vues enregistrées | P1 |
| Mise en forme conditionnelle | P3 |

### 1.4 Volet de lecture et ouverture
| Fonction | Prio |
|---|---|
| Position droite / bas / masqué | P0 |
| Rendu HTML isolé, images distantes bloquées par défaut | P0 |
| Texte brut, RTF compressé, HTML encapsulé dans le RTF | P0 |
| Liste des pièces jointes, enregistrer, ouvrir | P0 |
| Pièces jointes TNEF (`winmail.dat`) | P1 |
| Marquer comme lu après un délai | P1 |
| Ouvrir le message dans sa propre fenêtre | P1 |
| En-têtes Internet / propriétés du message | P1 |
| Ouvrir des fichiers `.msg` / `.eml` | P1 |
| Zoom | P1 |
| Aperçu des pièces jointes (PDF, images) | P3 |
| Invitations de réunion affichées dans le message | P2 |
| S/MIME (signé, chiffré) | P3 (non retenu en Q4) |
| Info-courrier (MailTips) | HP |

### 1.5 Rédaction
| Fonction | Prio |
|---|---|
| Nouveau, Répondre, Répondre à tous, Transférer : en brouillon au jalon PST, envoi avec SMTP | P1 |
| Éditeur HTML (mise en forme, listes, tableaux, images incorporées), texte brut | P1 |
| À / Cc / Cci, carnet d'adresses, saisie semi-automatique | P1 |
| Signatures (plusieurs, par compte, nouveau / réponse) | P1 |
| Pièces jointes (fichiers, éléments), glisser-déposer | P1 |
| Brouillons enregistrés automatiquement | P1 |
| Vérification orthographique (moteur local Windows) | P1 |
| Modèles `.oft` | P1 (Q4) |
| Importance, sensibilité, accusés de réception / lecture, remise différée, « Réponses à » | P3 |
| Boutons de vote, papier à lettres, QuickParts, mentions @ | P3 |
| Fonctions propres à Word dans l'éditeur (styles, SmartArt…) | HP |
| Traduction, Éditeur Microsoft, dictée | HP |

### 1.6 Organisation et actions
| Fonction | Prio |
|---|---|
| Supprimer, Maj+Suppr, déplacer, copier | P0 |
| Marquer comme lu / non lu | P0 |
| Indicateurs de suivi (Aujourd'hui, Demain, Cette semaine…, personnalisé), rappels | P1 |
| Catégories de couleur (liste principale, raccourcis) | P1 |
| Archiver | P1 |
| Imprimer, Enregistrer sous (`.msg`, `.txt`, `.html`) | P1 |
| Règles (assistant, exécuter maintenant, import `.rwz`) | P1 (Q4) |
| Actions rapides | P1 (Q4) |
| Réponse automatique locale (règle + modèle) | P1 (Q4), nécessite SMTP |
| Courrier indésirable (listes d'expéditeurs bloqués / approuvés) | P3 |
| Ignorer / nettoyer une conversation | P3 |
| Notifications de bureau, son | P1, avec IMAP |
| Filtre anti-spam de Microsoft, réponses automatiques serveur | HP |

### 1.7 Recherche
| Fonction | Prio |
|---|---|
| Recherche instantanée, étendue (dossier, sous-dossiers, fichier, tout) | P1 |
| Onglet Recherche : filtres De, Objet, Pièces jointes, Catégorie, Non lu, Indicateur, Important, Cette semaine | P1 |
| Syntaxe par mots-clés (expéditeur, objet, pièce jointe, date) | P1 |
| Index local (pas Windows Search) | P1 |
| Recherche avancée (Ctrl+Maj+F), recherches récentes | P1 (Q4) |
| Recherche dans le contenu des pièces jointes | P3 |

### 1.8 Envoi / réception
| Fonction | Prio |
|---|---|
| Envoyer / recevoir (F9), boîte d'envoi, travailler hors connexion, groupes d'envoi/réception, planification, en-têtes seuls | P1, avec IMAP + SMTP |

### 1.9 Flux RSS
| Fonction | Prio |
|---|---|
| Abonnement et lecture de flux | P3 |

## 2. Calendrier (P2, premier module après le mail)
- Vues Jour, Semaine de travail, Semaine, Mois, Planification ; navigateur de dates.
- Rendez-vous, événements d'une journée, périodicité (exceptions comprises), rappels.
- Réunions par courrier (iCalendar / iTIP / iMIP : RFC 5545, 5546, 6047), suivi des réponses.
- Calendriers multiples, superposition, groupes de calendriers.
- Import / export `.ics` et CSV ; deux fuseaux horaires ; catégories.
- P3 : jours fériés (données publiques, pas le fichier de Microsoft), styles d'impression.
- HP : disponibilité Exchange, réunions Teams, météo, publication en ligne.

## 3. Contacts (P2, deuxième)
- Vues Carte de visite, Carte, Téléphone, Liste ; fiche complète, photo.
- Groupes de contacts (listes de distribution), carnet d'adresses (Ctrl+Maj+B).
- Import / export vCard (RFC 6350) et CSV ; recherche.
- HP : publipostage (dépend de Word), connecteur réseaux sociaux.

## 4. Tâches (P2, troisième)
- Tâches et éléments marqués d'un indicateur (liste des tâches à faire).
- Vues (Liste simple, Liste détaillée, Actives, 7 prochains jours, En retard, Par catégorie).
- Périodicité, rappels, état, % achevé, priorité.
- Barre des tâches à faire (calendrier, personnes, tâches).
- P3 : affecter une tâche par courrier. HP : synchronisation Microsoft To Do.

## 5. Notes, Journal, Raccourcis
- P3 pour les trois.

## 6. Interface transverse
| Fonction | Prio |
|---|---|
| Barre de commandes à onglets repliable (Accueil, Envoi/réception, Dossier, Affichage), mêmes regroupements | P0 |
| Navigation basse Courrier / Calendrier / Contacts / Tâches | P0 |
| Barre d'état (nombre d'éléments, non lus, état) | P0 |
| Raccourcis clavier du courrier, identiques à Outlook | P0 |
| Tout au clavier, focus visible, accessibilité UI Automation | P0 |
| Thème clair / sombre selon Windows | P0 |
| Interface en français (libellés rédigés par nous) | P0 |
| Touches d'accès Alt (infobulles de touches), F6 entre volets | P1 |
| Menu Fichier (Info, Ouvrir et exporter, Enregistrer sous, Imprimer, Options) | P1 |
| Options (sous-ensemble utile) | P1 |
| Densité confortable / compacte | P1 |
| Aperçus au survol de la navigation | P2 |
| Barre d'outils Accès rapide, personnalisation des onglets | P3 |
| Client de messagerie par défaut Windows, `mailto:`, MAPI simple | P3 |
| Compléments COM / Web, macros VBA | HP |

## Réponses de cadrage (2026-10-16)
- Q1 : le premier jalon mail porte sur des copies de PST, hors ligne. IMAP et SMTP viennent ensuite.
- Q2 : stockage propre plus import PST. Pas d'écriture PST.
- Q3 : ordre Calendrier, puis Contacts, puis Tâches.
- Q4 : fonctions utilisées : règles (avec import `.rwz`), actions rapides, modèles `.oft` et réponse automatique, dossiers de recherche.

## Points encore ouverts
- Ordre entre IMAP et POP3.
- Nom du produit.
