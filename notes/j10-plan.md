# J10 — Contacts

## Décision
- 22/10 : Paul choisit « contacts d'abord » ; comme le calendrier, **lecture seule, Outlook fait foi** (MPE synchronise
  le téléphone avec les contacts du PST). L'écriture (contacts et calendrier) se décidera ensemble : piste
  recommandée, écrire dans Outlook par COM (un seul carnet, MPE inchangé).

## Données
- Les contacts étaient déjà importés avec le PST (sac MAPI dans `message.props`, notes et photo dans le `.eml`) :
  10 649 `IPM.Contact` en tout ; Contacts 2 138, doublons2 2 779, Suggested Contacts 1 069, OXALIS 57, Éléments
  supprimés 3 (+ archive : Deleted Items 4 533, Éléments supprimés 70) ; 3 listes de distribution (`IPM.DistList`).
- Propriétés d'après [MS-OXOCNTC] (PDF v20250520) : nom (0x3001, 0x3A06, 0x3A11, 0x3A44, 0x3A45, 0x3A05), Classer
  sous (`PidLidFileUnder` 0x8005, `PidLidFileUnderId` 0x8006), Courrier 1-3 (PSETID_Address 0x8080/0x8090/0x80A0 + 3),
  19 téléphones, adresses bureau (0x8045-0x8049), domicile (0x3A5D…), autre (0x3A63…), anniversaire 0x3A42, photo
  (pièce jointe `ContactPicture.jpg`).

## Avancement
- Étape 1 faite le 22/10 : crate `crates/contacts` ; `examples/read_all` : 10 649 lus, 0 échec.
- Comparaison avec Outlook (COM, lecture seule : `target/tl-scripts/outlook_contacts.ps1`, `compare_contacts.py`) sur
  le dossier Contacts, 14 champs (nom, Classer sous, société, 3 courriers, mobile, bureau, domicile, villes,
  catégories, anniversaire, photo) : 2 137 fiches identiques ; 1 écart (Vandé, modifiée après la copie du 15/10) ;
  2 contacts créés dans Outlook les 20 et 21/10 absents de la copie.
- Classer sous : Outlook affiche la **valeur enregistrée** (2 137/2 138, tous codes confondus) ; le code
  `PidLidFileUnderId` ne sert qu'à la reconstituer quand elle manque (`fileas_codes.py`).
- Étape 2 faite le 22/10 : module Contacts (Ctrl+3) : dossiers « Mes contacts » à gauche, liste triée par Classer
  sous (sans casse ni accents) avec lettres d'index, filtre, ↑/↓, fiche (photo, Contact, Adresses, Dates, Autres,
  Notes ; clic sur un courrier = nouveau message). Deleted Items (4 533) s'affiche en 0,8 s. Cas `ui_check` J10.

## Reste
- Rechargement depuis une copie fraîche du PST (comme `tl-import --calendars`) : les 2 contacts récents.
- Listes de distribution (3) ; export vCard ; recherche des contacts dans la recherche globale.
- Écriture : à décider avec Paul (voir Décision).
