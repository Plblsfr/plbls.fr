# Déploiement

Deux compose pour tout le monorepo, et un `.env.example` par projet.

| Fichier | Rôle |
|---|---|
| [`docker-compose.dev.yml`](docker-compose.dev.yml) | dépendances de dev de tous les projets, éphémères |
| [`dev.env`](dev.env) | variables des backends en dev (valeurs factices) |
| [`docker-compose.yml`](docker-compose.yml) | production de tous les services, sans section `environment` |
| [`.env.example`](.env.example) | début de l'onglet Environment de Dokploy : tags d'images, réseau Jellyfin, variables communes |
| `<projet>/.env.example` | suite de l'onglet, variables d'un projet (secrets en références Vault) : [`security`](security/.env.example), [`aegis`](aegis/.env.example), [`pli`](pli/.env.example), [`estompe`](estompe/.env.example) |

Keycloak, le PostgreSQL partagé, la stack média et Vault sont dans le dépôt privé `plbls.fr-infra`.

## Dev

```sh
just dev-up      # Postgres :5432, Redis :6379, Keycloak :8180, Mailpit :8025 (SMTP :1025), écho :8081
just dev-down
```

Postgres (`postgres`/`postgres`) crée au démarrage la base `aegis` (mot de passe `aegis`) ; les tests utilisent
`TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres` et `TEST_REDIS_URL=redis://localhost:6379`.
Les backends tournent en local avec `deploy/dev.env` (`just aegis-dev-run`, `just security-dev start`).
Rien n'est conservé : `just dev-down` efface tout, le realm Keycloak de [`security/keycloak`](security/keycloak) est réimporté au démarrage.

## Production

Une seule stack Dokploy (type Compose) avec [`docker-compose.yml`](docker-compose.yml) : `auth-gateway` et `security-redis`,
`aegis` et `aegis-redis`, `pli`, `estompe-web`, `estompe-api`, `estompe-postgres`, `estompe-backup`, `transcribe`, `livekit`, `cdn`.
Les services se joignent par leur nom sur le réseau de la stack ; `aegis` et `pli` rejoignent aussi `postgres-net`
(PostgreSQL partagé, `postgres-main`), `auth-gateway` le réseau de Dokploy (Keycloak).

Aucune variable n'est écrite dans le compose. L'onglet Environment de la stack contient [`.env.example`](.env.example) puis le
`.env.example` de chaque projet, mis bout à bout. Dokploy l'écrit dans `.env` à côté du compose à chaque déploiement, et tous les
services le chargent en entier (`env_file: .env`) : chaque conteneur voit donc toutes les variables de la stack.

- **Secrets** : les exemples ne contiennent que des références `${{vault.plbls.<projet>:<CLE>}}`. Dokploy les résout au déploiement
  dans Vault (`plbls.fr-infra`, `infra/vault`) : fournisseur nommé `plbls` dans Settings › Secrets, moteur KV v2 `secret`, un secret
  `secret/<projet>` par projet. Un secret repris par plusieurs projets n'est stocké qu'une fois (`GATEWAY__IDENTITY__SECRET` et
  `GATEWAY__INTERNAL__TOKEN` dans `secret/security`, référencés par aegis). Une référence vers un champ absent fait
  échouer le déploiement : un secret facultatif inutilisé reste vide dans l'onglet.
- **Changer un secret** : le modifier dans Vault puis redéployer la stack ; les conteneurs ne relisent pas leur environnement.
- **Noms** : une variable appartient à un seul projet (d'où `PLI_DATABASE_URL` et `PLI_AEGIS_TOKEN`), sauf les variables communes
  de [`.env.example`](.env.example) (`RUST_LOG`, `GATEWAY_URL`, `AEGIS_URL`), de même sens partout.
- **Valeurs** : pas de `$` dans un secret (Compose l'interpréterait dans `.env`) ; `openssl rand -hex 32` convient.

Domaines Dokploy : `auth-gateway` 8080 (security.plbls.fr), `aegis` 8080, `pli` 8080, `estompe-web` 80,
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

## Exploitation d'Estompe

### Sauvegarde et restauration

`estompe-backup` fait un `pg_dump` compressé au démarrage puis chaque jour à `BACKUP_HOUR` (UTC) dans le volume `estompe-backups`,
et garde les `BACKUP_KEEP` plus récents (14). Les fichiers envoyés (`estompe-data`) ne sont archivés qu'avec `BACKUP_FILES=true` ;
sinon un fichier perdu s'affiche « expiré ». Une sauvegarde sur le même disque ne protège pas d'une panne du VPS : copier
`estompe-backups` ailleurs (`rclone`, `rsync`), et la recharger dans une base de test une fois par mois.

```sh
docker compose stop estompe-api
docker compose run --rm estompe-backup ls -lh /backups
docker compose exec estompe-postgres dropdb -U estompe estompe
docker compose exec estompe-postgres createdb -U estompe estompe
docker compose run --rm estompe-backup sh -c 'gunzip -c /backups/db-AAAA-MM-JJ-HHMM.sql.gz' | docker compose exec -T estompe-postgres psql -U estompe -d estompe -q
docker compose run --rm -v <projet>_estompe-data:/data estompe-backup sh -c 'tar xzf /backups/files-AAAA-MM-JJ.tar.gz -C /data'
docker compose start estompe-api
```

Les migrations manquantes s'appliquent au démarrage de l'API.

### Supervision

`GET /healthz` (API) et `GET /readyz` (API et PostgreSQL) sur le domaine de l'API ; `GET /` de LiveKit répond « OK ».
Avec `METRICS_TOKEN`, l'API expose `GET /metrics` (Prometheus, en Bearer ; 404 sans jeton) : `estompe_up`, `estompe_database_up`,
`estompe_storage_bytes` et `estompe_storage_max_bytes`, `estompe_ws_users`, `estompe_ws_connections`, `estompe_users`, `estompe_messages`,
`estompe_files`, `estompe_http_requests_total{code}`, `estompe_http_request_duration_seconds_{sum,count}`.

### Films (Jellyfin)

Jellyfin reste dans son propre compose, sans port ni domaine publié : seule l'API le joint, par le réseau externe `JELLYFIN_NETWORK`.
Bibliothèques « Films » et « Séries » sur les dossiers de Radarr et Sonarr montés en lecture seule, analyse des médias active (un fichier
non analysé est écarté), accès distant désactivé. Un compte de service et une clé API dédiés à Estompe (`JELLYFIN_API_KEY`), `BLOCK_FILMS=true`,
`JELLYFIN_URL=http://jellyfin:8096`, `STREAM_SECRET` (`openssl rand -hex 32`). Limiter les transcodages simultanés de Jellyfin à au moins
`FILMS_MAX_TRANSCODAGES` : au-delà, l'API répond `serveur_charge`. L'accès se donne par la permission aegis `estompe:movies`.

### Images

LiveKit, PostgreSQL et Alpine sont épinglés par tag et digest (`image:tag@sha256:…`) : changer les deux ensemble
(`docker manifest inspect <image>:<tag>`). En production, fixer `ESTOMPE_TAG` et `ESTOMPE_WEB_TAG` sur une version plutôt que `latest`.
Le lien API → PostgreSQL n'est pas chiffré (`sslmode=disable`) : il reste sur le réseau privé de la stack, sans port publié.
