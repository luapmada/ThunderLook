# Lot Q — créer un rendez-vous, une tâche, une note ou un contact à partir d'un message

Branche : `cloud/lot-q-depuis-courrier`. Lis d'abord `notes/cloud/README.md`.

Contexte : dans Outlook, glisser un message vers Calendrier ou Tâches crée un élément pré-rempli. ThunderLook ne le fait pas.

Périmètre de fichiers : nouveau fichier Tauri `app/src-tauri/src/from_mail_cmds.rs`, nouveau fichier d'interface `app/ui/src/from_mail.rs`, et UNIQUEMENT des lignes de branchement dans `app/ui/src/{main,menus,commandbar,ipc,state}.rs` et `app/src-tauri/src/lib.rs`. Aucune modification des crates existants (lecture seule de `thunderlook_mailbox`, `thunderlook_calendar::write`, `thunderlook_items`).

À faire :
1. Clic droit sur un message (et « Actions rapides ») : « Créer un rendez-vous », « Créer une tâche », « Créer une note », « Créer un contact à partir de l'expéditeur ».
2. Rendez-vous : objet = objet du message, description = corps en texte (4 000 caractères au plus) + « De : … », début = prochaine heure pleine, boîte « Rendez-vous » ouverte pré-remplie. Tâche : objet et description. Note : objet + corps. Contact : nom et adresse de l'expéditeur (analyse « Nom <adresse> ») ; si un contact de même adresse existe, ouvrir CELUI-CI.
3. Les formulaires se remplissent côté Tauri (`from_mail_cmds.rs`) et renvoient `AppointmentForm` / `TaskForm` / `NoteForm` / `ContactForm` ; l'interface les ouvre dans les boîtes existantes. Rien n'est écrit en base avant l'enregistrement par l'utilisateur.
4. Tests sous Linux pour l'analyse de l'expéditeur et le remplissage des formulaires : isole ces fonctions pures dans un module sans dépendance Tauri.

Fin : push, résumé de 10 lignes (gestes à essayer à l'écran).
