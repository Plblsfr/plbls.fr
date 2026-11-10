# Déploiement

Deux compose pour tout le monorepo, et un `.env.example` par projet.

| Fichier | Rôle |
|---|---|
| [`docker-compose.dev.yml`](docker-compose.dev.yml) | dépendances de dev de tous les projets, éphémères |
| [`dev.env`](dev.env) | variables des backends en dev (valeurs factices) |
| [`docker-compose.yml`](docker-compose.yml) | production de tous les services, sans section `environment` |
| [`.env.example`](.env.example) | variables du compose de production lui-même (tags d'images, dossier des `.env`, réseau Jellyfin) |
| `<projet>/.env.example` | variables d'un projet en production : [`security`](security/.env.example), [`permission`](permission/.env.example), [`aegis`](aegis/.env.example), [`pli`](pli/.env.example), [`estompe`](estompe/.env.example) |

Keycloak, le PostgreSQL partagé, la stack média et Vault sont dans le dépôt privé `plbls.fr-infra`.

## Dev

```sh
just dev-up      # Postgres :5432, Redis :6379, Keycloak :8180, Mailpit :8025 (SMTP :1025), écho :8081
just dev-down
```

Postgres (`postgres`/`postgres`) crée au démarrage les bases `permission` et `aegis` (mot de passe = nom) ; les tests utilisent
`TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres` et `TEST_REDIS_URL=redis://localhost:6379`.
Les backends tournent en local avec `deploy/dev.env` (`just aegis-dev-run`, `just permission-dev-run`, `just security-dev start`).
Rien n'est conservé : `just dev-down` efface tout, le realm Keycloak de [`security/keycloak`](security/keycloak) est réimporté au démarrage.

## Production

Une seule stack Dokploy (type Compose) avec [`docker-compose.yml`](docker-compose.yml) : `auth-gateway` et `security-redis`, `permission`,
`aegis` et `aegis-redis`, `pli`, `estompe-web`, `estompe-api`, `estompe-postgres`, `estompe-backup`, `transcribe`, `livekit`, `cdn`.
Les services se joignent par leur nom sur le réseau de la stack ; `permission`, `aegis` et `pli` rejoignent aussi `postgres-net`
(PostgreSQL partagé, `postgres-main`), `auth-gateway` le réseau de Dokploy (Keycloak).

Aucune variable n'est écrite dans le compose : chaque service lit `${PLBLS_ENV_DIR}/<projet>/.env` (`env_file`), le même fichier pour
tous les services d'un projet. Ces fichiers sont rendus par l'agent Vault de `plbls.fr-infra` (`infra/vault`) depuis les secrets
`secret/plbls/<projet>`, dont les clés sont celles de `<projet>/.env.example`. Sans Vault, les écrire à la main depuis les exemples.

Domaines Dokploy : `auth-gateway` 8080 (security.plbls.fr), `permission` 8080, `aegis` 8080, `pli` 8080, `estompe-web` 80,
`estompe-api` 8080, `livekit` 7880, `cdn` 80. Ports ouverts sur l'hôte : 7881/tcp et 50000-50100/udp (WebRTC de LiveKit).

### Ajouter une variable

La lire dans le code, l'ajouter à `<projet>/.env.example` avec sa valeur par défaut, puis au secret Vault du projet. Le compose ne change pas.

### Reprendre les anciennes stacks

Les volumes prennent le nom de la stack (`plbls_estompe-pgdata`, `plbls_estompe-data`, `plbls_estompe-backups`,
`plbls_security-redis-data`, `plbls_cdn-data`, `plbls_transcribe-models`) : avant d'arrêter les anciennes stacks, copier leurs volumes
dans les nouveaux, par exemple `docker run --rm -v <ancien>:/de -v plbls_estompe-pgdata:/vers alpine cp -a /de/. /vers/`.
`pli` et `auth-gateway` ne publient plus `127.0.0.1:8080` : les servir par un domaine Dokploy.
