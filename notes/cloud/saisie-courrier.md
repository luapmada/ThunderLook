# Saisie et réactivité — courrier et boîtes associées (28/10)

Périmètre : `app/ui/src/{compose,reading,list,accounts,autoreply,create_rule,rules,quicksteps,junk,topbar,global_search,folders}.rs`.
Vérifié par lecture, `cargo check -p thunderlook-ui --target wasm32-unknown-unknown` (aucun nouvel avertissement) et
`cargo test --workspace --exclude thunderlook` (Linux : 370 tests, 0 échec, dont les 2 nouveaux). `app/src-tauri` ne
compile pas sous Linux (`windows::Win32` sans `cfg`, `lib.rs:1475`) : non vérifié ici, aucun fichier touché. Rien
n'a été essayé à l'écran.

## Défauts corrigés

| Fichier:ligne | Symptôme pour l'utilisateur | Correction |
|---|---|---|
| `quicksteps.rs:395` (et `shape`, l. 143) | Éditeur d'action rapide : dans « Catégorie » ou « À » (Transférer, Nouveau message), chaque caractère tapé faisait perdre le focus (les lignes d'actions étaient recréées à chaque frappe). Même chose pour la liste « Dossier » au clavier. | Lignes recréées seulement quand la *forme* de la suite change (`Memo` sur la sorte de chaque action) ; valeurs lues sans suivi. Test `forme_inchangee_pendant_la_saisie`. |
| `quicksteps.rs` (liste « Action n ») | Changer la sorte d'une action recrée sa ligne : le focus était perdu. | Focus rendu à la liste « Action n » après le rendu. |
| `rules.rs:165` | « Exécuter les règles » / « Règles et alertes » : Tab passait par la case de chaque règle (`tabindex=-1`), et restait bloqué devant « Aperçu »/« Exécuter maintenant » grisés pendant une exécution. | Sélecteur du cycle Tab : cases `tabindex=-1` et contrôles `disabled` exclus. |
| `accounts.rs:235` | « Ajouter un compte » : Tab bloqué devant « Enregistrer » ou « Détecter les serveurs » grisés pendant la vérification. | Contrôles `disabled` exclus du cycle Tab. |
| `create_rule.rs:180` | « Créer une règle » : Tab bloqué devant « OK » grisé (chargement, enregistrement). | Idem. |
| `quicksteps.rs:324` | Même cycle Tab, mêmes contrôles grisés possibles. | Idem. |
| `accounts.rs:242`, `create_rule.rs:187`, `rules.rs:172`, `quicksteps.rs:331` | Focus hors des contrôles (après « Retirer » une ligne) : Tab sautait le premier contrôle. | Sans position : Tab va au premier, Maj+Tab au dernier. |
| `accounts.rs:220` | « Ajouter un compte » : Entrée sur le bouton « Annuler » ou « Détecter les serveurs » lançait l'enregistrement (vérification de connexion). | Entrée n'enregistre plus depuis un bouton ou une liste déroulante (comme `create_rule.rs`). |
| `rules.rs:383` | « Exécuter les règles » › « Exécuter dans le dossier » : toutes les options (tous les dossiers) recréées à chaque choix, flèches du clavier comprises. | Options construites une fois, `selected` réactif par option. |
| `global_search.rs:73, 100` | Recherche « Tout » : une réponse ancienne pouvait remplir le panneau d'une recherche plus récente de même texte (« ab », « abc », « ab » tapés vite), ou rouvrir un panneau fermé par Échap. | Numéro de recherche ; réponses périmées ignorées, `close` invalide les réponses en route. Test `reponse_perimee_ignoree`. |
| `create_rule.rs:71` | « Créer une règle » fermée puis rouverte sur un autre message : une réponse tardive pouvait préremplir la boîte avec l'ancien message (règle créée sur le mauvais expéditeur). | Réponse ignorée si la boîte est fermée ou si le message sélectionné a changé. |
| `junk.rs:23` | « Options du courrier indésirable » : une adresse tapée puis abandonnée (Annuler) reparaissait à l'ouverture suivante. | Champ vidé à l'ouverture. |
| `compose.rs:468` | Rédaction : les pastilles de pièces jointes étaient recréées à chaque frappe dans « À », « Cc », « Objet » (pas de perte de focus, travail inutile). | `Memo` sur les seules pièces jointes. |

## Examinés, laissés

| Fichier:ligne | Constat | Raison |
|---|---|---|
| `compose.rs:628` « Objet », `compose.rs:597` « De » | `prop:value` suit tout le brouillon. | Valeur brute (ni trim ni format) : réécrire la même valeur ne déplace pas le curseur (spécification HTML). Pas de saut. |
| `compose.rs:611-623` champs « À », « Cc », « Cci » | Composant `AddressField`. | Dans `address_book.rs`, hors périmètre. |
| `compose.rs:588` « Ignorer » | Ferme sans demander, même modifié. | Comportement d'Outlook pour la réponse intégrée non vérifié ; Échap ne ferme pas la rédaction. À vérifier avec Outlook. |
| `autoreply.rs`, `junk.rs` | Pas de cycle Tab : Tab peut sortir de la boîte vers l'arrière-plan. | Pas un piège ; à harmoniser avec les autres boîtes si Paul le souhaite. |
| Toutes les boîtes | Un clic sur le fond (`modal-backdrop`) ferme sans demander : texte saisi perdu. | Comportement commun à toute l'application (fichiers hors périmètre aussi) ; Outlook ignore ce clic. À décider globalement. |
| `accounts.rs:211` | Échap en « Ajouter un compte » revient à la liste sans demander. | Assistant d'Outlook non vérifié. |
| `quicksteps.rs:313` | Échap dans l'éditeur annule sans demander. | Idem, non vérifié. |
| `topbar.rs` | Recherche à la frappe : temporisation par génération, déjà correcte ; champ non lié à un signal. | Rien à faire. |
| `reading.rs:44` | Chargement du message : réponse tardive déjà ignorée. `reading.rs:467` commentaire d'invitation : bloc qui ne suit que `pending`. | Rien à faire. |
| `list.rs`, `folders.rs` | Raccourcis (Suppr, Ctrl+A, F2, lettres) posés sur la liste et l'arbre eux-mêmes, pas sur `window`. | Ne se déclenchent pas pendant la saisie. |
| `app.rs:86` `typing()` (hors périmètre) | Ne reconnaît pas un élément `contenteditable`. | Aucun dans ces fichiers (le corps de rédaction est un `iframe` dont les touches ne remontent pas à `window`). À reprendre si un champ éditable apparaît dans la page. |

## À essayer à l'écran

| Bouton qui ouvre | Champ (`aria-label` ou libellé) | Ce qu'il faut voir |
|---|---|---|
| Groupe « Actions rapides » › « Gérer les actions rapides » › « Nouveau… », ou galerie › « Créer une action rapide… » | liste `Action 1` → choisir « Classer le message », puis champ `Catégorie` | Taper un mot entier sans perdre le focus. |
| idem | liste `Action 1` → « Transférer » ou « Nouveau message », champ `À` | Idem. |
| idem | liste `Action 1` changée aux flèches du clavier | Le focus reste sur la liste. |
| idem | liste `Dossier` (Déplacer vers le dossier) | Choix au clavier sans perdre le focus. |
| Bouton « Exécuter les règles » du ruban, ou menu Règles › « Exécuter les règles… » | Tab depuis la liste `Règles` | Tab ne passe plus par les cases ; va à « Exécuter dans le dossier », etc. |
| idem | liste « Exécuter dans le dossier » | Flèches haut/bas : le dossier choisi suit, sans saut. |
| Groupe « Comptes » › « Paramètres du compte » › « Ajouter… » | Tab jusqu'à « Annuler » puis Entrée | Retour à la liste, pas de vérification lancée. |
| idem | `Adresse`, `Mot de passe`, `Port IMAP`, `Chiffrement SMTP`, puis Entrée dans un champ | Enregistrement ; pendant « Vérification de la connexion… », Tab continue de tourner. |
| Clic droit sur un message › Règles › « Créer une règle… » | `Texte de l'objet`, `Dossier de destination` | Tab tourne dans la boîte même pendant le chargement. |
| Clic droit › Courrier indésirable › « Options du courrier indésirable… » | `Adresse ou domaine` : taper, Annuler, rouvrir | Champ vide. |
| Zone `Rechercher`, liste `Portée de la recherche` = « Tout » | taper vite « ab », « abc », retour à « ab » ; puis Échap | Le panneau montre la dernière recherche ; rien ne se rouvre après Échap. |
| « Nouveau message » (Ctrl+N), « Joindre » | `À`, `Objet` avec une pièce jointe | Saisie fluide, pastilles stables. |
| « Réponses automatiques » | `Du (facultatif)`, `Au (facultatif)`, `Message` | Contrôle seulement (aucun changement). |
