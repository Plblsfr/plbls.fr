# aegis

Permissions des comptes plbls.fr : rôles multi-applications, groupes, droits directs, refus explicites, expiration, portée sur
une ressource ; manifestes déclarés par les applications ; cache Redis et invalidation par pub/sub. Rust + Axum + PostgreSQL + Redis.

- `api/aegis/` (ce dossier) : API REST (`/v1/...`), spec OpenAPI générée depuis le code (`/openapi.json`) ;
  membre du workspace Cargo racine, image construite depuis la racine du dépôt (`just aegis-image`)
- `docs/aegis/` : fonctionnement, intégration, fonctionnalités, interface
- `deploy/aegis/` : compose de production (avec Redis) et de dev, `dev.env`

L'interface d'administration est dans Estompe (Administration › accès). Documentation : [docs/aegis/](../../docs/aegis/README.md).

```sh
just aegis-dev-up && just aegis-dev-run
TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres TEST_REDIS_URL=redis://localhost:6379 cargo test -p aegis
```
