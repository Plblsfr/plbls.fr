# Sillon

Service de musique de plbls.fr, à part d'Estompe : une personne peut avoir accès à Sillon sans avoir accès à la messagerie.
Sillon écoute la bibliothèque gérée par Lidarr ; playlists, ajout autonome de musique et aléatoire avec recommandation viennent
par lots (issue chapeau [#202](https://github.com/Plblsfr/plbls.fr/issues/202)). Ce document décrit ce qui existe aujourd'hui : le socle du service.

| Où | Quoi |
|---|---|
| [`api/sillon`](../../api/sillon/README.md) | API Rust (axum, sqlx), membre du workspace Cargo |
| [`openapi.yaml`](openapi.yaml) | routes, vérifiées par un test (chaque route du code y figure, et rien d'autre) |
| [`deploy/sillon/.env.example`](../../deploy/sillon/.env.example) | secrets, service `sillon-api` de [`deploy/docker-compose.yml`](../../deploy/docker-compose.yml) |

## Permissions

Déclarées à aegis par manifeste (application `sillon`, renvoyé toutes les 10 minutes), toutes fermées par défaut :

| Permission | Ouvre |
|---|---|
| `sillon:access` | bibliothèque, recherche, lecture, playlists, likes, file, historique |
| `sillon:add` | recherche d'un album ou d'un artiste absent et ajout ; découverte automatique |
| `sillon:admin` | occupation du disque, échecs, suppression, état des synchronisations et de l'analyse |

Chaque route relève d'une permission ou d'une exception motivée (santé, disponibilité, `GET /me`) : la table des routes l'impose
(`api/sillon/src/routes.rs`) et un test parcourt toutes les routes (401 sans session ou avec une session invalide, 403 sans la permission,
même avec les deux autres). aegis injoignable : 503 `aegis_unavailable` sur les routes soumises à une permission. Sans `AEGIS_TOKEN_SILLON`,
tout est refusé.

`GET /me` répond à toute personne connectée, sans permission Sillon : identité, permissions et `sections`, la liste de ce qu'elle peut
ouvrir (`library` avec `sillon:access`, `add` avec `sillon:add`, `admin` avec `sillon:admin`). aegis injoignable : `GET /me` répond quand
même, permissions toutes fausses et aucune section.

Authentification : cookie de session du gateway (`__Secure-plbls_sid`) ou `Authorization: Bearer`, vérifié par `GET /auth/me` du gateway
(cache de 30 s). Requête modifiante avec cookie : `Origin` dans `SILLON_ALLOWED_ORIGINS`, sinon 403 `bad_origin` ; CORS avec
identifiants pour ces seules origines.

## Variables

| Variable | Source | Rôle |
|---|---|---|
| `SILLON_PORT` | `plbls.env` (8080) | port d'écoute |
| `SILLON_ALLOWED_ORIGINS` | `plbls.env` | origines des fronts (CORS, CSRF) |
| `AEGIS_TOKEN_SILLON` | Vault `secret/sillon` | jeton de l'application `sillon` chez aegis |
| `DATABASE_URL`, `DATABASE_USER`, `DATABASE_PASSWORD`, `GATEWAY_URL`, `GATEWAY_INTERNE_URL`, `AEGIS_URL` | communes ([`deploy/README.md`](../../deploy/README.md)) | base `sillon` sur le serveur commun, gateway, aegis |

## Actions manuelles

- `plbls.fr-infra` : base `sillon` sur `postgres-main` (`CREATE DATABASE sillon OWNER plbls;`), sous-domaine `sillon-api.plbls.fr` et sa route.
- Vault, `secret/sillon` : `AEGIS_TOKEN_SILLON`.
- aegis : rien à déclarer (il lit `AEGIS_TOKEN_SILLON` comme application `sillon`), puis les permissions aux comptes.

## Version

Composant `sillon` : version du `Cargo.toml` d'`api/sillon`, tag `sillon-vX.Y.Z` (image `ghcr.io/plblsfr/sillon`, `:edge` sur `main`).
Pas d'entrée dans `.version`, propre à Estompe.
