# entracte

API d'Entracte, films et séries hors d'Estompe : documentation fonctionnelle dans [`docs/entracte`](../../docs/entracte/README.md),
routes dans [`openapi.yaml`](../../docs/entracte/openapi.yaml).

| Module | Rôle |
|---|---|
| `config.rs` | variables `ENTRACTE_*` (liste dans [`deploy/entracte/.env.example`](../../deploy/entracte/.env.example)) |
| `routes.rs` | table des routes : chacune a une permission ou une exception motivée ; tests de couverture et d'OpenAPI |
| `garde.rs`, `identite.rs`, `droits.rs` | gateway (`/auth/me`, cookie ou Bearer), aegis (manifeste, `/v1/effective`), CORS et CSRF |
| `catalogue.rs`, `arr.rs`, `tmdb.rs`, `ffprobe.rs`, `filtre.rs`, `images.rs` | synchronisation Radarr/Sonarr, métadonnées, analyse, filtre, cache d'images |
| `bibliotheque.rs` | bibliothèque, fiches, saisons, épisodes, administration du catalogue |
| `lecture.rs`, `flux.rs`, `transcodage.rs`, `materiel.rs`, `sous_titres.rs` | flux signés, lecture directe, HLS, accélération, WebVTT |
| `progression.rs` | progression, reprise, historique |
| `demandes.rs` | recherche, demandes, administration, envoi à Radarr/Sonarr |
| `push/` | Web Push (VAPID), FCM, notifications |
| `interne.rs` | santé, `GET /me`, titre pour le salon cinéma d'Estompe |

## Lancer

`just entracte-dev-run` (après `just dev-up`, avec le gateway et aegis), ou `cargo run -p entracte` avec les variables exportées.
ffmpeg et ffprobe doivent être dans le `PATH`. Image : `docker build -f api/entracte/Dockerfile .` (contexte : racine du dépôt).

## Tests

`TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres just test-entracte` : une base jetable par test, de vraies vidéos
générées par ffmpeg, et de faux gateway, aegis, Radarr, Sonarr, TMDB et service push. Sans `TEST_DATABASE_URL`, les tests passent par
testcontainers (ignorés sans Docker) ; sans ffmpeg, les tests d'API sont ignorés.
