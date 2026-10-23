# Lot L — tests d'interface pour tout ce qui a été ajouté (à écrire, Paul les exécutera)

Branche : `cloud/lot-l-tests-interface`. Lis d'abord `notes/cloud/README.md`.

Périmètre de fichiers : `tools/` uniquement (`tools/ui_check.mjs` et nouveaux fichiers `tools/ui_cases/*.mjs`). Aucun fichier de l'application.

Contexte : `tools/ui_check.mjs` lance ThunderLook avec le port de débogage WebView2 (CDP) et exécute des cas (J3b, J5, J6, J7, J9, J9b, J10, « Clic droit »). Les scripts `target/tl-scripts/{ctx,ct,rp,rp2,it}_test.mjs` (non versionnés) font des essais de bout en bout sur une copie de la base (`THUNDERLOOK_DATA`). Cloud ne peut PAS exécuter ces tests (Windows + WebView2) : écris-les avec soin, relis-les, vérifie leur syntaxe avec `node --check`.

À faire :
1. Découpe `ui_check.mjs` : les cas existants restent à l'identique (aucun changement de comportement) ; le moteur (lancement, CDP, `js`, `until`, `press`) va dans `tools/ui_cases/lib.mjs`, importé par `ui_check.mjs`.
2. Nouveaux cas, chacun dans son fichier, sûrs (ils créent leurs propres éléments « ZZ… » et les suppriment à la fin, même en cas d'échec ; ils n'écrivent QUE si la variable `THUNDERLOOK_DATA` pointe vers une base de test, sinon ils se sautent avec un message) :
   - contacts : créer, modifier (société), rouvrir, supprimer ; photo (si le champ fichier existe) ;
   - rendez-vous : créer ; série hebdomadaire 4 occurrences ; modifier UNE occurrence ; modifier toute la série ; supprimer une occurrence ; supprimer la série ;
   - tâches : créer avec échéance, terminer, filtre « En retard », supprimer ; notes : créer (couleur verte), modifier, supprimer ;
   - échange téléphone : écran ouvert (menu Fichier), choix de dossier simulé impossible → vérifier seulement la présence des boutons ;
   - recherche globale : taper un nom de contact créé par le cas et le retrouver ;
   - rappels : créer un rendez-vous dans 5 min avec rappel 15 min, le compteur « Rappels : N » augmente ;
   - calendrier : en vue Mois, aucun rendez-vous n'a une hauteur < 16 px (régression du 23/10) ; en vue Semaine, deux rendez-vous simultanés ne se recouvrent pas.
3. `UI_ONLY=<préfixe>` continue de fonctionner. Document la façon de lancer (dans l'en-tête du fichier).

Fin : push, résumé de 10 lignes (cas écrits, ce qui n'a pas pu être vérifié, commande exacte à lancer chez Paul).
