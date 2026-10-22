# Couleurs et menus clic droit d'Outlook 2021 (relevés le 22/10 sur le poste de Paul, thème sombre)

Méthode : COM (affichage, sélection), WM_CONTEXTMENU envoyé à `DayViewWnd`, UI Automation limitée aux menus,
captures PrintWindow / rectangle du seul menu, pixels mesurés (`target/tl-scripts/pixels.ps1`). **Sans REA** : à
refaire ou compléter avec les outils REA (capture_native_ui_scenario, observe_native_ui…), et noter l'écart dans
REA-LIMITES.md. Rien n'a été modifié dans Outlook (Annuler / Échap partout ; couleurs des catégories vérifiées).

## Palette des 25 couleurs de catégorie (sélecteur « Couleur » de « Catégories de couleurs »)
Ordre preset0 → preset24 (OlCategoryColor COM = preset + 1) ; 15 valeurs recoupées avec les pastilles du menu Classer.
| preset | RVB | preset | RVB | preset | RVB |
|---|---|---|---|---|---|
| 0 Rouge | #B10E1C | 9 Canneberge | #CC007E | 18 Jaune foncé | #AE8E00 |
| 1 Orange | #C33400 | 10 Acier | #919DA1 | 19 Vert foncé | #0A600A |
| 2 Pêche | #E69A3E | 11 Acier foncé | #005265 | 20 Sarcelle foncé | #02767A |
| 3 Jaune | #E3CC00 | 12 Gris | #8C8E83 | 21 Olive foncé | #427505 |
| 4 Vert | #009C4E | 13 Gris foncé | #5D6C70 | 22 Bleu foncé | #00345C |
| 5 Sarcelle | #00A3AE | 14 Noir | #3E3E3E | 23 Violet foncé | #684697 |
| 6 Olive | #7EAC00 | 15 Rouge foncé | #6A0A1A | 24 Canneberge foncé | #8C0059 |
| 7 Bleu | #006CBE | 16 Orange foncé | #B5490F | | |
| 8 Violet | #756CC8 | 17 Pêche foncé | #814E29 | | |

## Calendrier (vue Semaine de travail, thème sombre)
- Rendez-vous catégorisé : rempli de la couleur de sa (première) catégorie, ~92 % sur le fond #1B1B1B
  (Olive #7EAC00 → #759E03 ; Canneberge → #BB0375 ; Violet foncé → #61428B ; Rouge → #A5131D). Texte blanc.
- Sans catégorie : couleur du calendrier, plus sombre (~83 %) : Vert #427505 → #385F08.
- Bande gauche (disponibilité) : Occupé = couleur éclaircie (~35-40 % vers le blanc : #AFD15D, #9F82C5, #D65A65,
  #7DAB46) ; Disponible = bande sombre (#393C2B).
- Pastilles « Afficher en tant que » : Disponible vide ; Travaille ailleurs pointillé sur #003B6A ; Provisoire hachuré
  #83BEEC/#003B6A ; Occupé(e) #0063B1 ; Absent(e) hachuré #BF7FBF/#800080.
- Palette des couleurs de calendrier (clic droit créneau › Couleur) : Automatique, puis Bleu #006CBE, Vert #427505,
  Orange #C33400, Bleu marine #00345C, Jaune #E3CC00, Sarcelle #00A3AE, Rose #CC007E, Violet #684697, Rouge #B10E1C
  (= couleurs de catégorie 7, 21, 1, 22, 3, 5, 9, 23, 0).
- Catégories : la liste qui compte est celle du **PST** (Store.Categories), pas celle du profil (couleurs « 0 »).
  Deux « Santé » : SantÃ© (8, bleu) et Santé (24, violet foncé, celle des rendez-vous). « Congé (Jour de) » absente
  de la base (copie du 15/10) → reprendre Store.Categories à chaque synchronisation.

## Menus clic droit (libellés exacts)
- Rendez-vous : Impression rapide ; Inviter ; Transférer ; Privé (case) ; Afficher en tant que › (Disponible, Travaille
  ailleurs, Provisoire, Occupé(e), Absent(e)) ; Classer › (Effacer toutes les catégories, 15 catégories récentes,
  Toutes les catégories…, Définir le clic rapide…) ; Envoyer à OneNote ; Supprimer.
- Réunion : Impression rapide ; Répondre ; Répondre à tous ; Transférer ; Privé ; Afficher en tant que › ; Classer › ;
  Envoyer à OneNote ; Supprimer.
- Créneau vide : Nouveau rendez-vous ; Nouvel événement d'une journée entière ; Nouvelle demande de réunion ; Nouveau
  rendez-vous périodique ; Nouvel événement périodique ; Nouvelle réunion périodique ; Couleur › (palette ci-dessus).
- Non relevés : drapeaux et indicateurs de la liste de courrier ; menu du navigateur de dates et des calendriers.
