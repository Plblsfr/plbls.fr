# Mettre à jour les images Docker

Les dépendances téléchargées au build sont épinglées (version + somme de contrôle ou digest) : une release amont
compromise ne peut pas s'exécuter dans le stack.

## Images (`deploy/docker-compose.yml`)

LiveKit, PostgreSQL et Alpine sont épinglés par tag **et** digest (`image:tag@sha256:…`). Pour changer de version :
choisir le tag, lire son digest (`docker manifest inspect <image>:<tag>` ou l'en-tête `Docker-Content-Digest` du
registre), remplacer tag et digest ensemble, redéployer et vérifier le vocal / la base. Les images Estompe
(`estompe`, `estompe-web`) sont construites par la CI ; en production, fixer `ESTOMPE_TAG`
et `ESTOMPE_WEB_TAG` (`.env` du compose) sur un tag de version plutôt que sur `latest`.

## PostgreSQL

Le lien API → PostgreSQL (`sslmode=disable` dans `DATABASE_URL`) n'est pas chiffré : le choix retenu est de s'appuyer sur
le réseau Docker privé du stack, où seul le service `estompe-postgres` est joignable et sans port publié. Si la base passe sur une
autre machine, activer TLS côté PostgreSQL et utiliser `sslmode=verify-full`.
