# Déclencheur horaire de la multi-synchro MyPhoneExplorer (lot AN)

MyPhoneExplorer (MPE) n'a pas de minuterie de synchronisation. `tools/mpe_trigger.ps1` envoie toutes les heures à MPE la commande de son menu **Outils › Commencer la multi-synchronisation** (Maj+F5), via une tâche planifiée.

> **⚠ Non testé.** Ce lot a été écrit depuis le cloud, sans Windows et sans MPE : ni le script, ni le XML de la tâche n'ont été exécutés. Il repose sur les constats faits sur le PC de Paul : MPE est une appli VB6, fenêtre principale de classe `ThunderRT6FormDC` au titre commençant par `MyPhoneExplorer - `, commande de menu d'identifiant **44**, et `WM_COMMAND` (0x111) avec `wParam` 44 posté à cette fenêtre lance la multi-synchro. Premier essai : **uniquement `-DryRun`**, puis un essai réel surveillé.

## Ce que fait le script

Il ne fait **rien** (et l'écrit dans le journal) si :

1. `MyPhoneExplorer.exe` ne tourne pas ;
2. aucune fenêtre `ThunderRT6FormDC` visible avec un titre `MyPhoneExplorer - …` (MPE réduit dans la zone de notification) ;
3. une autre fenêtre visible de MPE existe, ou la fenêtre principale est désactivée : synchro déjà en cours ou confirmation en attente ;
4. la dernière synchro date de moins de **50 minutes** (`-MinMinutes` pour changer). Elle est lue dans `LastContactSync` (format `jjMMaaaaHHmmss`) du `settings.dat` de l'appareil donné par `LastUser` dans `general.ini`, tous deux sous `%APPDATA%\MyPhoneExplorer\`, **en lecture seule**. Valeur absente, illisible ou dans le futur : on ne fait rien.

Sinon : `PostMessage(WM_COMMAND, 44)` sur la fenêtre principale. Pas de souris, pas de clavier, pas de mise au premier plan. Une seule instance à la fois (mutex).

Journal : `%LOCALAPPDATA%\ThunderLook\mpe_trigger.log` (roulé à 512 Ko, `-LogPath` pour changer). Les messages sont en ASCII ; tout accent éventuel passerait par `[char]`, pour éviter les soucis d'encodage de PowerShell 5.1.

## Installer

Dans PowerShell (pas besoin d'être administrateur), depuis la racine du dépôt :

```powershell
$script = (Resolve-Path .\tools\mpe_trigger.ps1).Path
$xml = (Get-Content .\tools\mpe_trigger_task.xml -Raw -Encoding UTF8) `
  -replace 'REMPLACER_UTILISATEUR', "$env:USERDOMAIN\$env:USERNAME" `
  -replace 'REMPLACER_CHEMIN_SCRIPT', $script
Register-ScheduledTask -TaskName 'ThunderLook - MPE multi-synchro' -Xml $xml
```

La tâche : toutes les heures, pour l'utilisateur courant, **session ouverte uniquement** (jeton interactif), sans privilèges élevés, pas de rattrapage des exécutions manquées, une seule instance. Le premier déclenchement a lieu à l'heure pile suivante. Garder le dépôt à son emplacement (le chemin du script est figé dans la tâche) ; sinon réinstaller.

La fenêtre PowerShell est cachée mais peut clignoter une fraction de seconde à chaque passage (limite connue de `powershell.exe` en tâche interactive).

## Tester (sans rien envoyer)

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\tools\mpe_trigger.ps1 -DryRun
Get-Content $env:LOCALAPPDATA\ThunderLook\mpe_trigger.log -Tail 10
```

`-DryRun` fait tous les contrôles et écrit « Enverrait WM_COMMAND 44 … » sans rien poster. À vérifier : la fenêtre trouvée porte bien le titre attendu, l'appareil lu est le bon, l'âge de la dernière synchro est plausible. Tester MPE ouvert au repos, MPE fermé, MPE réduit, et avec un dialogue ouvert (chaque cas doit donner un « Rien : … »). Ensuite seulement : un essai réel (sans `-DryRun`) MPE ouvert, téléphone connecté, en regardant MPE.

Lancer la tâche à la main : `Start-ScheduledTask -TaskName 'ThunderLook - MPE multi-synchro'`.

## Désinstaller

```powershell
Unregister-ScheduledTask -TaskName 'ThunderLook - MPE multi-synchro' -Confirm:$false
```

Le journal peut être supprimé à la main. Le script ne modifie rien dans MPE.

## Limites

- **MPE doit être ouvert** (fenêtre visible, pas dans la zone de notification) et le **téléphone connecté en Wi-Fi** : la commande ne se connecte pas, elle lance seulement la multi-synchro. Téléphone absent : MPE affichera probablement une erreur ou une fenêtre, qui bloquera les passages suivants (voir ci-dessous) **[non vérifié]**.
- **Session Windows ouverte et déverrouillée** requise ; pas d'exécution sinon.
- **Fenêtre de confirmation de MPE** : avec `ContactConfirmation=1` / `EventConfirmation=1` / `NoteConfirmation=1` (cas actuel, `notes/mpe-plan.md` § 1.2), MPE ouvre une fenêtre de confirmation avant d'appliquer les changements. Elle **reste tant que Paul ne la ferme pas**, et le script, voyant un dialogue, n'envoie plus rien : la synchro horaire est alors gelée. **Réglage à faire pour les cibles fichier** (vCard/iCal/vNote, `docs/mpe-cible-fichier.md`) : dans les réglages de synchro de l'appareil, désactiver les confirmations (ou relever leurs seuils `…ConfirmationLimit`) une fois la cible validée en test, sinon la boucle horaire ne tournera que quand Paul valide à la main. Nom exact des cases dans l'interface : à relever sur place **[non vérifié]**.
- Le critère « dernière synchro » ne regarde que `LastContactSync`. Si la synchro des contacts est désactivée pour l'appareil, ou si MPE n'écrit `settings.dat` qu'à la fermeture **[non vérifié]**, la valeur peut être périmée : le script enverrait alors la commande à chaque passage (le garde-fou « dialogue ouvert » et la durée d'une synchro limitent les doublons, sans les exclure). À contrôler pendant les premiers essais en regardant si `LastContactSync` avance après une synchro.
- Un dialogue d'une autre appli n'empêche rien ; seules les fenêtres du processus MPE sont examinées.
- L'identifiant 44 peut changer avec une version de MPE (constaté sur la version installée chez Paul, 2.03) : à re-vérifier après mise à jour.
