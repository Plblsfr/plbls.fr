# aegis

Permissions des comptes plbls.fr. aegis décide qui peut utiliser quelle fonctionnalité d'Estompe et de pli : les
applications déclarent leurs permissions, un administrateur règle les droits de chaque compte depuis Estompe
(Administration › accès), et les applications lisent les droits calculés, mis en cache et invalidés en direct.

| Document | Contenu |
| --- | --- |
| [fonctionnement.md](fonctionnement.md) | Modèle (rôles, groupes, règles, portée, expiration), règle de calcul, cache Redis et propagation, décisions |
| [integration.md](integration.md) | Brancher une application : manifeste, droits effectifs, invalidation, ressources ; configuration et bascule depuis permission |
| [fonctionnalites.md](fonctionnalites.md) | Catalogue des permissions d'Estompe et de pli, routes protégées, plafond des rôles de serveur |
| [interface.md](interface.md) | Écrans d'administration dans Estompe |

Code : `api/aegis` (Rust, Axum, sqlx, Redis). Déploiement : `deploy/aegis`. Tag de livraison : `aegis-vX.Y.Z`.
Spec OpenAPI générée depuis le code : `GET /openapi.json`.

## En dev

```sh
just dev-up            # Postgres (5432) et Redis (6379) éphémères, avec les autres dépendances de dev
just aegis-dev-run     # backend sur 127.0.0.1:8091 (deploy/dev.env)
TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres TEST_REDIS_URL=redis://localhost:6379 cargo test -p aegis
```
