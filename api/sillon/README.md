# sillon

API de Sillon, service de musique hors d'Estompe : documentation fonctionnelle dans [`docs/sillon`](../../docs/sillon/README.md),
routes dans [`openapi.yaml`](../../docs/sillon/openapi.yaml).

| Module | Rôle |
|---|---|
| `config.rs` | variables `SILLON_*` et communes (valeurs dans [`plbls.env`](../../plbls.env), secrets dans [`deploy/sillon/.env.example`](../../deploy/sillon/.env.example)) |
| `routes.rs` | table des routes : chacune a une permission ou une exception motivée ; tests de couverture et de conformité à OpenAPI (dans les deux sens) |
| `garde.rs`, `identite.rs`, `droits.rs` | gateway (`/auth/me`, cookie ou Bearer), aegis (manifeste, `/v1/effective`), sections de `GET /me`, CORS et CSRF |
| `etat.rs` | état partagé (configuration, base, client HTTP, aegis) et tâches de fond (manifeste) |
| `interne.rs` | santé, disponibilité, `GET /me` |
| `http.rs` | réponses JSON et erreurs `{error, message}` |
| `migrations/` | schéma de la base `sillon` (`0001_sillon.sql` vide : le socle n'a pas de table) |

## Ajouter une route

Une entrée dans `routes::table()` (méthode, chemin, garde `Garde::Droit(..)` ou exception motivée, handler) et la même route dans
`docs/sillon/openapi.yaml` : sinon `routes::tests::routes_et_openapi_identiques` échoue. Le test
`tests_api::chaque_route_protegee_refuse_sans_session_et_sans_permission` la couvre ensuite sans changement ; un paramètre de chemin y
reçoit un UUID d'exemple (`exemple()` dans `tests_api.rs`).

## Lancer

`just sillon-dev-run` (après `just dev-up`, avec le gateway et aegis), ou `cargo run -p sillon` avec les variables exportées.
Image : `docker build -f api/sillon/Dockerfile .` (contexte : racine du dépôt), avec ffmpeg et ffprobe.

## Tests

`TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres just test-sillon` : une base jetable par test, migrations
appliquées, faux gateway et faux aegis (`testutil.rs` : `env()`, `env_avec()`, personnes `personne`, `alice`, `bruno`, `admin`).
Sans `TEST_DATABASE_URL`, les tests passent par testcontainers (ignorés sans Docker).
