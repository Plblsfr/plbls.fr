# Déploiement

Deux compose pour tout le monorepo, et un `.env.example` par projet.

| Fichier | Rôle |
|---|---|
| [`docker-compose.dev.yml`](docker-compose.dev.yml) | dépendances de dev de tous les projets, éphémères |
| [`dev.env`](dev.env) | variables des backends en dev (valeurs factices) |
| [`docker-compose.yml`](docker-compose.yml) | production de tous les services, sans section `environment` |
| [`.env.example`](.env.example) | début de l'onglet Environment de Dokploy : tags d'images, réseau Jellyfin, variables communes |
| `<projet>/.env.example` | suite de l'onglet, variables d'un projet (secrets en références Vault) : [`security`](security/.env.example), [`permission`](permission/.env.example), [`aegis`](aegis/.env.example), [`pli`](pli/.env.example), [`estompe`](estompe/.env.example), [`entracte`](entracte/.env.example) |

Keycloak, le PostgreSQL partagé, la stack média et Vault sont dans le dépôt privé `plbls.fr-infra`.

## Dev

```sh
just dev-up      # Postgres :5432, Redis :6379, Keycloak :8180, Mailpit :8025 (SMTP :1025), écho :8081
just dev-down
```

Postgres (`postgres`/`postgres`) crée au démarrage les bases `permission`, `aegis` et `entracte` (mot de passe = nom) ; les tests utilisent
`TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres` et `TEST_REDIS_URL=redis://localhost:6379`.
Les backends tournent en local avec `deploy/dev.env` (`just aegis-dev-run`, `just permission-dev-run`, `just security-dev start`).
Rien n'est conservé : `just dev-down` efface tout, le realm Keycloak de [`security/keycloak`](security/keycloak) est réimporté au démarrage.

## Production

Une seule stack Dokploy (type Compose) avec [`docker-compose.yml`](docker-compose.yml) : `auth-gateway` et `security-redis`, `permission`,
`aegis` et `aegis-redis`, `pli`, `entracte-api`, `estompe-web`, `estompe-api`, `estompe-postgres`, `estompe-backup`, `transcribe`, `livekit`, `cdn`.
Les services se joignent par leur nom sur le réseau de la stack ; `permission`, `aegis`, `pli` et `entracte-api` rejoignent aussi `postgres-net`
(PostgreSQL partagé, `postgres-main`), `auth-gateway` le réseau de Dokploy (Keycloak), `estompe-api` et `entracte-api` celui de la
stack média (`JELLYFIN_NETWORK` : Jellyfin, Radarr, Sonarr). `entracte-api` monte `ENTRACTE_MEDIAS` en lecture seule et garde ses
transcodages dans le volume `entracte-transcodage`.

Aucune variable n'est écrite dans le compose. L'onglet Environment de la stack contient [`.env.example`](.env.example) puis le
`.env.example` de chaque projet, mis bout à bout. Dokploy l'écrit dans `.env` à côté du compose à chaque déploiement, et tous les
services le chargent en entier (`env_file: .env`) : chaque conteneur voit donc toutes les variables de la stack.

- **Secrets** : les exemples ne contiennent que des références `${{vault.plbls.<projet>:<CLE>}}`. Dokploy les résout au déploiement
  dans Vault (`plbls.fr-infra`, `infra/vault`) : fournisseur nommé `plbls` dans Settings › Secrets, moteur KV v2 `secret`, un secret
  `secret/<projet>` par projet. Un secret repris par plusieurs projets n'est stocké qu'une fois (`GATEWAY__IDENTITY__SECRET` et
  `GATEWAY__INTERNAL__TOKEN` dans `secret/security`, référencés par permission et aegis). Une référence vers un champ absent fait
  échouer le déploiement : un secret facultatif inutilisé reste vide dans l'onglet.
- **Changer un secret** : le modifier dans Vault puis redéployer la stack ; les conteneurs ne relisent pas leur environnement.
- **Noms** : une variable appartient à un seul projet (d'où `PLI_DATABASE_URL` et `PLI_AEGIS_TOKEN`), sauf les variables communes
  de [`.env.example`](.env.example) (`RUST_LOG`, `GATEWAY_URL`, `PERMISSION_URL`, `AEGIS_URL`), de même sens partout.
- **Valeurs** : pas de `$` dans un secret (Compose l'interpréterait dans `.env`) ; `openssl rand -hex 32` convient.

Domaines Dokploy : `auth-gateway` 8080 (security.plbls.fr), `permission` 8080, `aegis` 8080, `pli` 8080, `entracte-api` 8080, `estompe-web` 80,
`estompe-api` 8080, `livekit` 7880, `cdn` 80. Ports ouverts sur l'hôte : 7881/tcp et 50000-50100/udp (WebRTC de LiveKit).

### Ajouter une variable

La lire dans le code sous un nom qu'aucun autre projet n'utilise, l'ajouter à `<projet>/.env.example` (valeur par défaut, ou référence Vault
pour un secret, dont la valeur va dans `secret/<projet>`), puis à l'onglet Environment de la stack. Le compose ne change pas.

### Reprendre les anciennes stacks

Les volumes prennent le nom du projet Compose (`name: plbls`, ou le nom d'application que Dokploy passe à `docker compose -p`) :
`<projet>_estompe-pgdata`, `_estompe-data`, `_estompe-backups`, `_security-redis-data`, `_cdn-data`, `_transcribe-models`.
Avant d'arrêter les anciennes stacks, copier leurs volumes dans les nouveaux (`docker volume ls` pour les noms), par exemple
`docker run --rm -v <ancien>:/de -v <nouveau>:/vers alpine cp -a /de/. /vers/`.
`pli` et `auth-gateway` ne publient plus `127.0.0.1:8080` : les servir par un domaine Dokploy. Variables de pli renommées :
`DATABASE_URL` devient `PLI_DATABASE_URL`, `AEGIS_TOKEN` devient `PLI_AEGIS_TOKEN`.
