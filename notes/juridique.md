# Points juridiques (pas un avis d'avocat)

## 1. Décompilation d'OUTLOOK.EXE — litigieux
- Le droit français autorise sans accord de l'auteur :
  - **l'observation** du logiciel pendant qu'on l'utilise (« observer, étudier ou tester le fonctionnement »), CPI art. L122-6-1 III ;
  - **la décompilation**, seulement si elle est indispensable à l'interopérabilité d'un logiciel créé indépendamment, si l'information n'est pas déjà facilement accessible, et limitée aux parties nécessaires. L'information obtenue ne doit pas servir à créer un logiciel substantiellement similaire. CPI art. L122-6-1 IV.
  - Toute clause contraire de la licence est nulle (même article).
  - Source : https://www.legifrance.gouv.fr/codes/article_lc/LEGIARTI000006278920
- Conséquences :
  - Les formats publiés par Microsoft (Open Specifications : [MS-PST], [MS-OXMSG], [MS-OXRTFCP], [MS-OXPROPS]…) sont « facilement accessibles ». **Pas de décompilation sur ces sujets.**
  - « Tester les limites de REA » n'est pas une finalité d'interopérabilité. **Décompiler OUTLOOK.EXE dans ce seul but n'est pas couvert.**
  - Ce qui reste licite :
    - observation à l'exécution (UI Automation, comportement, fichiers produits) ;
    - décompilation ciblée sur un point d'interopérabilité non documenté (candidat : format des règles `.rwz`, dont je ne sais pas encore s'il est documenté) ;
    - tests « statiques » des limites de REA sur des binaires libres ou sur nos propres binaires.
- Remarque technique : la logique PST vit probablement dans `MSPST32.DLL` (2,9 Mo) et `OLMAPI32.DLL` (8,2 Mo), pas dans `OUTLOOK.EXE`. Je ne l'ai pas vérifié. La consigne actuelle limite REA à `OUTLOOK.EXE`.

## 2. Barre de commandes à onglets (« ruban »)
- En 2006, la licence Office UI de Microsoft, gratuite, excluait les applications concurrentes directes de Word, Excel, PowerPoint, Outlook et Access.
  - Sources : https://www.infoq.com/news/2006/11/Office-UI-License/ et https://itwriting.com/blog/58-legal-implications-of-microsofts-office-2007-ribbons.html
- Statut actuel de cette licence et des brevets associés : **je ne sais pas.**
- En France, l'usage privé non commercial échappe aux brevets (CPI art. L613-5 a).
- Risque jugé faible pour un usage personnel. **À réévaluer avant toute diffusion.**

## 3. Ressemblance de l'interface
- La fonctionnalité et l'ergonomie ne sont pas protégées par le droit d'auteur (CJUE, C-406/10 SAS Institute, 2012).
- Une interface graphique peut l'être si elle est originale (CJUE, C-393/09 BSA, 2010).
- On peut donc reprendre la structure et les parcours, pas l'expression visuelle. C'est ce que prévoit `DESIGN.md`.

## 4. Libellés vus dans l'arbre UI Automation
- L'arbre expose les libellés d'Outlook. Ils servent de repère de structure seulement.
- Nos libellés sont rédigés indépendamment, dans un glossaire propre (`notes/glossaire.md`, à créer).

## 5. Données personnelles
- Les captures et les copies de PST contiennent de vrais courriels. Ils restent en local dans ce dossier, ne sont jamais publiés, et sont exclus de tout dépôt git.

## 6. Nom du dépôt « thunderlook »
- Ce nom accole « Thunder » (Thunderbird, marque de Mozilla) et « look » (Outlook, marque de Microsoft). Il y a un risque de confusion de marques si le produit est diffusé sous ce nom.
- Dépôt privé : risque faible.
- Le nom du produit reste « à définir » dans SPEC.md. À trancher avant toute diffusion.

## 7. Fichiers fournis avec Office
- Jours fériés `.hol`, dictionnaires, modèles : ne pas les réutiliser.
