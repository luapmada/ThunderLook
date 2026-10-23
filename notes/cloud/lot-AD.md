# Lot AD — jeu de données de test et banc de charge de l'échange (S20 FE)

Branche : `cloud/lot-ad-jeu-de-test`. Lis d'abord `notes/cloud/README.md`, `docs/mpe-fichiers.md`, `notes/mpe-plan.md` § 6 (étape 5) et § 7.

Périmètre de fichiers : NOUVEAU `crates/exchange/examples/gen_testset.rs`, NOUVEAU `crates/exchange/tests/bulk.rs`, NOUVEAU `docs/jeu-de-test-s20.md`. Rien d'autre.

À faire :
1. `gen_testset` : écrit dans un dossier donné en argument un jeu SYNTHÉTIQUE (aucune donnée réelle) lisible par MPE : 20 contacts (accents, 2 téléphones, e-mails, adresse, certains avec photo minuscule), 20 rendez-vous dont 4 séries (jour/semaine/mois/an) avec 2 exceptions et 1 suppression, rendez-vous journée entière, 5 notes, 2 tâches (dont une avec rappel), au format que ThunderLook sait ré-importer (vCard 2.1 ET 3.0 en deux variantes de dossier, iCal 2.0, vNote). Noms reconnaissables (préfixe « ZZ-TEST »).
2. `tests/bulk.rs` : 2 000 contacts synthétiques → export → import → ré-export : 0 doublon, 0 perte, durée mesurée et affichée (seuil raisonnable en échec) ; même chose avec 3 passes de modification alternée côté « téléphone » et côté ThunderLook.
3. `docs/jeu-de-test-s20.md` : mode d'emploi pour Paul en 10 lignes (générer, charger sur le S20 via MPE, quoi regarder après la synchro), sans rien affirmer sur MPE qui ne soit constaté (marquer [supposé]).
4. `cargo test -p thunderlook-exchange` et `cargo run -p thunderlook-exchange --example gen_testset -- <dossier>`.

Aucune écriture dans Outlook, aucune config de MPE. Fin : push, résumé de 10 lignes.
