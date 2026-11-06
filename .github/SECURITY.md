# Politique de sécurité

## Signaler une faille

Ne pas ouvrir d'issue publique, de discussion ni de PR pour une faille de sécurité.

Utiliser le signalement privé de GitHub : onglet **Security** du dépôt, puis **Report a vulnerability**
([lien direct](https://github.com/Plblsfr/plbls.fr/security/advisories/new)). Le rapport n'est visible que des mainteneurs.

Indiquer si possible :

- le composant touché (`estompe-api`, `estompe-app`, `security`, `permission`, `pli`, `ui`) et sa version ;
- la description de la faille et son impact ;
- les étapes pour la reproduire, ou une preuve de concept ;
- une piste de correction, si vous en avez une.

## Délais

- Accusé de réception sous 7 jours.
- Premier diagnostic sous 14 jours.
- Correctif publié dès que possible selon la gravité, puis avis de sécurité publié sur GitHub. La personne qui a signalé la faille
  est créditée si elle le souhaite.

## Versions prises en charge

Seule la dernière version publiée de chaque composant (tag `<composant>-vX.Y.Z`) reçoit les correctifs de sécurité.

## Périmètre

Dans le périmètre : le code de ce dépôt et les services de production qui en sont issus.

Hors périmètre : déni de service par volume, ingénierie sociale, attaques physiques, failles des dépendances tierces déjà publiées
sans exploitation propre à ce projet, rapports d'outils automatiques sans démonstration d'impact.

Merci de ne pas accéder aux données d'autres utilisateurs, de ne pas les modifier et de ne pas dégrader les services pendant vos tests.
