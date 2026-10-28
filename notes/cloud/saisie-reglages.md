# Saisie et réactivité — réglages, boîtes, raccourcis globaux (cloud, 28/10)

Périmètre : `app/ui/src/` options, signatures, dialogs, safety, exchange, mpe_import, help, menus, menu, commandbar,
app, actions, state, statusbar, print, attach_preview, splitter. Vérifié par lecture, `cargo check` (espace de travail
et `thunderlook-ui` en wasm32), `cargo test --workspace`. Rien n'a pu être essayé à l'écran.
Sous Linux, `app/src-tauri` ne compile pas (`toast.rs` utilise la crate `windows`, déclarée pour Windows seulement ;
préexistant, hors périmètre) : `cargo check -p thunderlook-ui --target wasm32-unknown-unknown` passe, et
`cargo test --workspace --exclude thunderlook` passe en entier (dont 11 tests de l'interface, 4 nouveaux).

## Défauts trouvés

| Fichier:ligne | Symptôme | Correction |
|---|---|---|
| `app.rs:588` (`Main`) | Le bloc des volets suivait TOUS les réglages (`s().reading_pane`). Un pas du séparateur liste/lecture (souris ou flèche) change `list_width` → liste, séparateur et rédaction recréés à chaque pas : le séparateur perd le focus après une flèche, le glisser s'interrompt, le corps d'un brouillon non encore enregistré (enregistrement auto toutes les N s) est perdu. Idem pour tout réglage modifié dans Options. | `Memo` `pane` sur `reading_pane` seul (`app.rs:562`). |
| `app.rs:96` (`typing`) | Seuls INPUT, TEXTAREA, SELECT comptaient : une zone `contenteditable` ou une iframe en `designMode` laissait passer Ctrl+Q, Ctrl+U, Inser, Ctrl+N/R/F, F9, Ctrl+Z, Ctrl+Maj+E et Suppr (iframe ⇒ « hors du document » ⇒ suppression du message). Aujourd'hui sans effet visible (les iframes de rédaction et de signatures ne relaient pas leurs touches), mais piège pour la suite. | `edits_text` (pure, testée) + `contenteditable` + iframe `designMode` (`app.rs:89`). |
| `app.rs:110,119` | Les boîtes modales hors `ui.dialog`/échange/sauvegardes/aide (rendez-vous, contact, tâche, note, doublons, suivi, signatures, carnet d'adresses, dossiers d'éléments, liste de distribution) ne bloquaient pas les raccourcis : Ctrl+1…5 changeait de module derrière la boîte ; dans le calendrier, Ctrl+N avec le focus sur un bouton ou une case de la boîte rendez-vous **remplaçait le rendez-vous en cours de saisie** ; Ctrl+P imprimait ; Ctrl+Z annulait derrière la boîte. | `modal_open()` : un `[aria-modal='true']` présent (toutes les boîtes sont sous `<Show>`, donc absentes fermées) ⇒ `global_key` rend la main. |
| `app.rs:40` (`reserved`) | Ctrl+Y (rétablir) bloqué dans tous les champs. | Laissé au champ pendant la saisie (testé). |
| `app.rs:424` | L'onglet « Rechercher » était reposé à chaque frappe validée dans la recherche ⇒ toute la barre de commandes redessinée. Pas de vol de focus (la barre n'a pas de champ), coût inutile. | Posé une seule fois. |
| `commandbar.rs:756,805` | Onglets redessinés à chaque frappe dans la recherche (le bloc lisait tout `search_text`). Sans vol de focus (le focus est dans la recherche). | `Memo` `searching`. |
| `dialogs.rs:157` | Entrée sur le bouton « Annuler » (atteint par Tab) lançait `submit()` : la boîte était VALIDÉE ; sur « Confirmer » (supprimer un dossier, vider un dossier) cela supprimait. | Entrée sur un bouton : son propre clic. |
| `menus.rs:327,402` (menu contextuel) | Les entrées lisaient l'arborescence, les catégories, les actions rapides… en suivi : la synchro IDLE (`refresh_tree`, `reopen_view`) redessinait le menu ouvert, le focus clavier tombait sur la page (flèches mortes). | Redessiné seulement si le contenu visible change (`menu::shape`, testé) ; entrées construites hors suivi. |
| `menu.rs:131,224` (`MenuButton`) | Même défaut pour les menus de la barre (Catégoriser, Suivi, Déplacer…, « Plus de commandes »). | Même correctif. |
| `exchange.rs:676` (`fetch_auto`) | Relevé toutes les 20 s de la synchro auto : `set` même sans changement ⇒ le bouton « Appliquer malgré le seuil… » recréé sous le focus, le champ « Toutes les (minutes) » réécrit pendant la saisie, la barre d'état redessinée. | `set` seulement si l'état change ; `Memo` `minutes` (`exchange.rs:801`). |
| `exchange.rs:433,443` (Échange avec le téléphone) | Cocher une case d'un changement (Espace) reconstruisait toute la liste ⇒ focus perdu, Tab repartait du haut. | Liste redessinée seulement si les lignes changent hors coche (`row_keys`, testé) ; coche réactive. |
| `mpe_import.rs:340,349` (Importer depuis MPE) | Même défaut. | Même correctif. |
| `signatures.rs:78` | Cliquer (ou Entrée) sur la signature déjà affichée recréait l'éditeur (focus et curseur perdus). | Sans effet si déjà affichée. |

### Laissé
- `options.rs:180,193,372` : `prop:value` des champs nombre/heure suit tout `settings` ; une autre modification de réglage pendant la frappe réécrirait le champ. Rien ne modifie les réglages pendant que la boîte Options (modale) est ouverte ; validation sur `change` (pas à chaque frappe) : pas de curseur qui saute.
- `options.rs:148`, `dialogs.rs:54`, `signatures.rs:85`, `safety.rs:114`, `exchange.rs:147`, `help.rs:317`, `menu.rs:150`, `menus.rs:346` : effets d'ouverture — ils ne suivent que le signal d'ouverture (ou un `NodeRef`), pas ce que la frappe modifie. Sains.
- `dialogs.rs:215…287`, `help.rs:387` : `prop:value` lié au signal brut (pas de transformation) ; blocs réactifs qui ne suivent que `ui.dialog` / `tab` : le champ n'est pas recréé.
- `statusbar.rs` : aucun élément focalisable recréé (textes seulement) ; la barre ne vole pas le focus pendant la synchro IDLE.
- `actions.rs`, `state.rs`, `print.rs`, `attach_preview.rs`, `splitter.rs` : rien (le séparateur était victime du défaut de `Main`).
- **Hors périmètre, à signaler** : changer de module (Ctrl+2…5) pendant une rédaction démonte `ComposePane` ; le corps tapé depuis le dernier enregistrement auto est perdu (`compose.rs` n'enregistre pas au démontage).
- **À vérifier** : les raccourcis lisent `ev.key()`. Sur AZERTY, Ctrl+& donne-t-il `"1"` ou `"&"` dans WebView2 ? Si c'est `"&"`, Ctrl+1…5 et Ctrl+Alt+1…4 ne marchent qu'avec Maj (exclu) — utiliser `ev.code()` (`Digit1`…). Le cas `ui_check` envoie `"2"` par CDP et ne le verrait pas.

## Inventaire des raccourcis globaux (`app.rs`, écouteur `keydown` sur `window`, plus l'iframe du message lu relayée par `reading.rs`)

Toutes ces touches sont coupées quand une boîte modale, un menu contextuel, l'échange, les sauvegardes ou l'aide est ouvert.
« Saisie » = focus dans un champ, une liste, une zone `contenteditable` ou une iframe en `designMode`. Les iframes de
rédaction et de signatures ne relaient pas leurs touches : rien de ce tableau ne s'y déclenche (Ctrl+S, Ctrl+Entrée,
Ctrl+K de la rédaction sont dans `compose.rs`).

| Touche | Action | Pendant la saisie | Voulu ? |
|---|---|---|---|
| F1 | Aide | actif | voulu (Outlook) |
| Ctrl+Maj+T | Échange avec le téléphone | actif | voulu (aucun sens dans un champ) |
| Ctrl+Q / Ctrl+U | Lu / non lu (Courrier) | inactif | voulu |
| Inser | Indicateur (Courrier) | inactif | voulu |
| Ctrl+N, Ctrl+R, Ctrl+Maj+R, Ctrl+F | Nouveau, Répondre, Répondre à tous, Transférer (Courrier) | inactif (et défaut du navigateur bloqué) | sûr ; Outlook accepte Ctrl+N depuis la recherche, écart mineur |
| F9 | Envoyer/recevoir (Courrier) | inactif | sûr |
| Ctrl+Z | Annuler l'opération (Courrier : `actions::undo` ; autres modules : `safety::undo`) | inactif, Ctrl+Z reste au texte | voulu |
| Ctrl+Maj+E | Nouveau dossier (Courrier) | inactif | voulu |
| Suppr | Supprimer (depuis le volet de lecture ou son iframe) | inactif | voulu (liste et arborescence ont leur Suppr) |
| Ctrl+Alt+1…4 | Vue Jour / Semaine de travail / Semaine / Mois (Calendrier, `calendar::key`) | actif | voulu (aucun sens dans un champ) |
| Ctrl+N | Nouveau rendez-vous (Calendrier) | actif hors boîte (ex. recherche) | voulu ; dans la boîte rendez-vous, c'était le défaut corrigé |
| Ctrl+P | Imprimer (Calendrier) | actif | voulu |
| Ctrl+1…5 | Courrier, Calendrier, Contacts, Tâches, Notes | actif | voulu (Outlook) ; voir « hors périmètre » pour la rédaction |
| F6 / Maj+F6 | Volet suivant / précédent | actif | voulu |
| Ctrl+E, F3 | Aller à la recherche | actif | voulu |
| Ctrl+. / Ctrl+, | Élément suivant / précédent | actif | voulu (aucun sens dans un champ ; la rédaction reste affichée) |
| Échap | Ferme la barre déroulée (mode « Onglets seuls ») | actif, seulement si la barre est déroulée | voulu |
| F5, F7, F12, Ctrl+R/F/N/P/U/Q/S/M/G/J/H/O/T/K/E/Y/D/L/B/+/−/=/0 | Défaut de WebView2 bloqué (rechargement, recherche dans la page…) | bloqué, **sauf Ctrl+Y** en saisie (rétablir) | voulu |
| Lettres seules, Retour arrière, Ctrl+A, flèches, Entrée | aucun raccourci global | — | — |

Autres écouteurs : `calendar.rs:605` (Échap annule un glisser en cours, seulement pendant le glisser) ; `menu.rs`,
`menus.rs` : écouteurs `pointerdown` sur `window` (fermeture au clic dehors) ; touches locales des menus, onglets,
séparateurs, boîtes (sur leur élément, pas globales). `commandbar.rs` n'a pas d'écouteur global.

## À essayer à l'écran
1. Courrier, volet à droite : rédiger une réponse, taper dans le corps, puis glisser le séparateur liste/lecture : le texte reste, le glisser est continu. Au clavier : focus sur le séparateur, plusieurs flèches de suite.
2. Calendrier : ouvrir un nouveau rendez-vous, taper un objet, Tab jusqu'à une case, Ctrl+N : rien ne change. Ctrl+2 / Ctrl+3 dans la fiche contact : on reste dans la boîte.
3. Boîte « Confirmer » (supprimer un dossier) : Tab sur « Annuler », Entrée : rien n'est supprimé. Idem « Nouveau dossier » : Tab, Entrée sur « Annuler ».
4. Clic droit sur un message, naviguer aux flèches pendant qu'une relève IDLE arrive (s'envoyer un message) : le focus reste dans le menu. Idem menu Catégoriser de la barre.
5. Fichier › Échange avec le téléphone : « Vérifier les changements », cocher/décocher au clavier (Espace) puis Tab : le focus avance normalement. Idem « Importer depuis MPE ». Taper dans « Toutes les (minutes) » et attendre 20 s : le texte reste.
6. Fichier › Signatures : cliquer la signature déjà sélectionnée pendant la frappe dans l'éditeur : rien ne bouge.
7. Champ « Nom » de « Nouveau dossier » : Ctrl+Z puis Ctrl+Y rétablit.
8. Recherche : taper ; la barre « Rechercher » apparaît une fois, sans clignotement.
