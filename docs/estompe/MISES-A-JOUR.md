# Mettre à jour les images Docker

Les dépendances téléchargées au build sont épinglées (version + somme de contrôle ou digest) : une release amont
compromise ne peut pas s'exécuter dans le stack.

## Images (`deploy/docker-compose.yml`)

LiveKit, Redis et speaches (`transcribe`) sont épinglés par tag **et** digest (`image:tag@sha256:…`). Pour changer de version :
choisir le tag, lire son digest (`docker manifest inspect <image>:<tag>` ou l'en-tête `Docker-Content-Digest` du
registre), remplacer tag et digest ensemble, redéployer et vérifier le vocal, les sessions et la transcription. Les images Estompe
(`estompe`, `estompe-web`) sont construites par la CI ; en production, fixer `ESTOMPE_TAG`
et `ESTOMPE_WEB_TAG` (`.env` du compose) sur un tag de version plutôt que sur `latest`.

## PostgreSQL

Estompe utilise la base `estompe` du PostgreSQL partagé (`postgres-main`, `plbls.fr-infra`), joint par le réseau Docker
`postgres-net`, sans port publié ; sa version et ses mises à jour se gèrent dans `plbls.fr-infra`. Le lien n'est chiffré que si
`DATABASE_URL` (commune, `plbls.env`) porte `?sslmode=…` : si la base passe sur une autre machine, activer TLS côté PostgreSQL et
utiliser `sslmode=verify-full`.
