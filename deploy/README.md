# Déploiement

Deux compose pour tout le monorepo, une configuration non secrète commune et un `.env.example` de secrets par projet.

| Fichier | Rôle |
|---|---|
| [`docker-compose.dev.yml`](docker-compose.dev.yml) | dépendances de dev de tous les projets, éphémères |
| [`dev.env`](dev.env) | variables des backends en dev (valeurs factices) |
| [`docker-compose.yml`](docker-compose.yml) | production de tous les services |
| [`../plbls.env`](../plbls.env) | valeurs non secrètes de production de tous les services, copiées dans chaque image au build (`/etc/plbls.env`) |
| [`.env.example`](.env.example) | début de l'onglet Environment de Dokploy : tags d'images, réseau de la stack média, secrets communs |
| `<projet>/.env.example` | suite de l'onglet, secrets d'un projet en références Vault : [`security`](security/.env.example), [`permission`](permission/.env.example), [`aegis`](aegis/.env.example), [`pli`](pli/.env.example), [`estompe`](estompe/.env.example), [`entracte`](entracte/.env.example) |

Keycloak, le PostgreSQL partagé, la stack média et Vault sont dans le dépôt privé `plbls.fr-infra`.

## Dev

```sh
just dev-up      # Postgres :5432, Redis :6379, Keycloak :8180, Mailpit :8025 (SMTP :1025), écho :8081
just dev-down
```

Postgres (`postgres`/`postgres`) crée au démarrage une base par backend (`permission`, `aegis`, `pli`, `entracte`, `estompe`) ; les tests utilisent
`TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres` et `TEST_REDIS_URL=redis://localhost:6379`.
Les backends tournent en local avec `deploy/dev.env` (`just aegis-dev-run`, `just permission-dev-run`, `just security-dev start`).
Rien n'est conservé : `just dev-down` efface tout, le realm Keycloak de [`security/keycloak`](security/keycloak) est réimporté au démarrage.

## Production

Une seule stack Dokploy (type Compose) avec [`docker-compose.yml`](docker-compose.yml) : `auth-gateway`, `redis`, `permission`, `aegis`,
`pli`, `entracte-api`, `estompe-web`, `estompe-api`, `transcribe`, `livekit`, `cdn`.
Les services se joignent par leur nom sur le réseau de la stack ; `permission`, `aegis`, `pli`, `entracte-api` et `estompe-api` rejoignent
aussi `postgres-net` (PostgreSQL partagé, `postgres-main`, une base par service), `auth-gateway` le réseau de Dokploy (Keycloak),
`entracte-api` celui de la stack média (`MEDIAS_NETWORK` : Radarr, Sonarr ; obligatoire, le déploiement échoue sans lui plutôt que de joindre
`dokploy-network`). Le Redis partagé (`redis` : sessions du gateway, cache et invalidations d'aegis, persistance AOF dans `redis-data`) est
seul sur son réseau `internal: true`, joint par `auth-gateway`, `aegis` et `estompe-api`, et exige `REDIS_PASSWORD` (`requirepass`).
`entracte-api` monte `ENTRACTE_MEDIAS` en lecture seule et garde ses transcodages dans le volume `entracte-transcodage`.
Les sauvegardes de `postgres-main` sont faites hors de cette stack.

### Variables

Trois sources, de la moins prioritaire à la plus prioritaire :

1. **Défauts du code** (`config/default.toml` de security, permission et aegis ; valeurs par défaut des autres).
2. **[`plbls.env`](../plbls.env)** (racine du dépôt) : toutes les valeurs non secrètes de production, copiées dans chaque image au build
   (`/etc/plbls.env`, autre chemin par `PLBLS_ENV`). Format `CLE=valeur`, sans guillemets ni interpolation. Changer une valeur = une PR,
   puis les images reconstruites par la CI. `estompe-web` en lit `API_URL`, `GATEWAY_URL`, `VOICE_URL`, `KEYCLOAK_ISSUER` et
   `ENTRACTE_PUBLIC_URL` au démarrage.
3. **Environnement du conteneur** : l'onglet Environment de la stack, qui contient [`.env.example`](.env.example) puis le `.env.example`
   de chaque projet, mis bout à bout. Dokploy l'écrit dans `.env` à côté du compose à chaque déploiement. Les backends Rust
   (`auth-gateway`, `permission`, `aegis`, `pli`, `entracte-api`, `estompe-api`) le chargent en entier (`env_file: .env`) ; les autres
   services ne reçoivent que ce qu'ils lisent, par interpolation (`${VAR}` dans `environment:` ou `command:`, `${VAR:?}` si obligatoire) :
   `redis` `REDIS_PASSWORD`, `livekit` `LIVEKIT_API_SECRET`, `estompe-web` ses URL publiques si l'onglet les redéfinit ; `transcribe` et
   `cdn` n'en reçoivent aucune. Une valeur de l'onglet l'emporte sur `plbls.env` (ex. `RUST_LOG=debug`, que tracing ne lit que là).

Variables communes, de même nom et de même sens partout :

| Variable | Source | Lue par |
|---|---|---|
| `DATABASE_URL`, `DATABASE_USER` / `DATABASE_PASSWORD` | `plbls.env` / Vault `secret/commun` | permission, aegis, pli, entracte, estompe : chacun ajoute sa base (son nom) au serveur `DATABASE_URL` |
| `REDIS_URL` / `REDIS_PASSWORD` | `plbls.env` / Vault `secret/commun` | `redis` (mot de passe), auth-gateway, aegis, estompe (URL avec le mot de passe) |
| `GATEWAY_URL`, `GATEWAY_INTERNE_URL` | `plbls.env` | gateway public (navigateurs) ; le même par le réseau Docker pour permission, aegis, pli, entracte, estompe (aucun appel entre services ne sort du VPS) |
| `GATEWAY_IDENTITE_SECRET`, `GATEWAY_INTERNE_TOKEN` | Vault `secret/commun` | en-têtes d'identité signés (gateway, permission, aegis) ; Bearer des routes `/internal/*` du gateway (gateway, permission, aegis) |
| `PERMISSION_URL`, `AEGIS_URL`, `PERMISSION_ADMIN_ROLE`, `AEGIS_ADMIN_ROLE`, `AEGIS_REDIS_PREFIX` | `plbls.env` | services de droits et leurs clients |
| `PERMISSION_TOKEN_<SERVICE>`, `AEGIS_TOKEN_<APPLICATION>` | Vault `secret/<client>` | le client (son jeton) et permission ou aegis, qui accepte chaque variable du préfixe comme service `<nom>` en minuscules |
| `ENTRACTE_INTERNE_URL`, `ENTRACTE_SERVICE_TOKEN` | `plbls.env` / Vault `secret/entracte` | Entracte et le salon cinéma d'Estompe |
| `VAPID_*`, `FCM_SERVICE_ACCOUNT` | `plbls.env` / Vault `secret/commun` | notifications push d'Estompe et d'Entracte |

- **Secrets** : les exemples ne contiennent que des références `${{vault.plbls.<secret>:<CLE>}}`. Dokploy les résout au déploiement
  dans Vault (`plbls.fr-infra`, `infra/vault`) : fournisseur nommé `plbls` dans Settings › Secrets, moteur KV v2 `secret`, un secret
  `secret/commun` pour les secrets communs et un `secret/<projet>` par projet ; le champ porte le nom de la variable. Une référence vers
  un champ absent fait échouer le déploiement : un secret facultatif inutilisé reste vide dans l'onglet.
- **Changer un secret** : le modifier dans Vault puis redéployer la stack ; les conteneurs ne relisent pas leur environnement.
- **Noms** : une variable appartient à un seul projet, sauf les communes ci-dessus ; un jeton partagé par deux services n'a qu'un nom.
- **Valeurs** : pas de `$` dans un secret (Compose l'interpréterait dans `.env`) ; `openssl rand -hex 32` convient.

Domaines Dokploy : `auth-gateway` 8080 (security.plbls.fr), `permission` 8080, `aegis` 8080, `pli` 8080, `entracte-api` 8080, `estompe-web` 8080,
`estompe-api` 8080, `livekit` 7880, `cdn` 8080. Ports ouverts sur l'hôte : 7881/tcp et 50000-50100/udp (WebRTC de LiveKit).

Durcissement : images tierces épinglées par `tag@sha256` (Dependabot met les digests à jour). `estompe-web` et `cdn` sont construits
sur `nginxinc/nginx-unprivileged` (utilisateur 101, port 8080) ; le volume `cdn-data` doit appartenir à cet utilisateur
(`docker run --rm -v <projet>_cdn-data:/srv alpine chown -R 101:0 /srv`, une fois, pour un volume écrit par l'ancienne image).
Redis a un healthcheck, et ses clients attendent `service_healthy`. Tous les services ont
`no-new-privileges`. `cap_drop: [ALL]` sur ceux qui tournent sans root ou n'ont besoin d'aucune capacité (backends, nginx, LiveKit) ;
pas sur Redis (son entrypoint change de propriétaire puis d'utilisateur) ni `transcribe`.
`read_only` avec un `tmpfs` sur `/tmp` pour les backends distroless (`auth-gateway`, `permission`, `aegis`, `pli`, `estompe-api`) ;
pas pour ce qui écrit ailleurs au démarrage (nginx, `entracte-api` et ffmpeg, Redis, LiveKit, `transcribe`).

### Ajouter une variable

La lire dans le code sous un nom qu'aucun autre projet n'utilise (ou un nom commun s'il en existe un). Valeur non secrète : l'écrire
dans [`plbls.env`](../plbls.env), sous la section du projet ; l'image la porte au prochain build. Secret : une référence Vault dans
`<projet>/.env.example` (valeur dans `secret/<projet>`, champ du nom de la variable), puis dans l'onglet Environment de la stack.
Pour un backend Rust le compose ne change pas ; pour un autre service, ajouter `NOM: ${NOM}` (ou `${NOM:-défaut}`, `${NOM:?message}`)
à sa section `environment`.

### Volumes

Les volumes prennent le nom du projet Compose (`name: plbls`, ou le nom d'application que Dokploy passe à `docker compose -p`) :
`<projet>_redis-data`, `_estompe-data`, `_cdn-data`, `_transcribe-models`, `_entracte-transcodage` (`docker volume ls` pour les noms).
Pour reprendre le contenu d'un autre volume : `docker run --rm -v <ancien>:/de -v <nouveau>:/vers alpine cp -a /de/. /vers/`.
