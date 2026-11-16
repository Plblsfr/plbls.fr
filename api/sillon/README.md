# sillon

API de Sillon, service de musique hors d'Estompe : documentation fonctionnelle dans [`docs/sillon`](../../docs/sillon/README.md),
routes dans [`openapi.yaml`](../../docs/sillon/openapi.yaml).

| Module | Rôle |
|---|---|
| `config.rs` | variables `SILLON_*` et communes (valeurs dans [`plbls.env`](../../plbls.env), secrets dans [`deploy/sillon/.env.example`](../../deploy/sillon/.env.example)) |
| `routes.rs` | table des routes : chacune a une permission ou une exception motivée ; tests de couverture et de conformité à OpenAPI (dans les deux sens) |
| `garde.rs`, `identite.rs`, `droits.rs` | gateway (`/auth/me`, cookie ou Bearer), aegis (manifeste, `/v1/effective`), sections de `GET /me`, CORS et CSRF |
| `etat.rs` | état partagé (configuration, base, client HTTP, aegis, verrou de synchronisation) et tâches de fond (manifeste, synchronisation périodique) |
| `interne.rs` | santé, disponibilité, `GET /me` |
| `http.rs` | réponses JSON et erreurs `{error, message}`, comparaison de jetons à temps constant |
| `lidarr.rs` | client de l'API v1 de Lidarr (`Client` : artistes, albums, pistes, fichiers, images ; `requete`, `envoyer`, `get` pour les autres appels) |
| `catalogue.rs` | synchronisation (une à la fois, relance), présence des pistes, albums et artistes, accès aux fichiers pour les lots suivants |
| `filtre.rs`, `ffprobe.rs` | filtre des fichiers (extension, double extension, nom suspect, conteneur, flux audio) et analyse ffprobe |
| `images.rs` | pochettes et images d'artistes : téléchargées depuis Lidarr, cache en base, trois tailles |
| `bibliotheque.rs` | routes de la bibliothèque, recherche, administration du catalogue, webhook ; JSON publics réutilisables |
| `migrations/` | schéma de la base `sillon` (`0001_sillon.sql` vide ; `0002_catalogue.sql` : catalogue) |

## Schéma du catalogue

| Table | Contenu |
|---|---|
| `artistes`, `albums`, `pistes` | identifiant public `id` (UUID), MBID (`mbid` ; `mbid_enregistrement` et `mbid_piste` pour une piste), `lidarr_id` (jamais exposé), `present`, `present_le` (dernière arrivée), colonnes générées `cle` (titre sans accents, minuscules) et `recherche` (`tsvector`) |
| `pistes` | `album_id`, `artiste_id`, `disque`, `numero`, `duree`, `explicite` ; unique par `(album_id, mbid_enregistrement)` ; jamais supprimée (absente quand plus aucun fichier) |
| `albums` | `type`, `types_secondaires`, `sortie`, `annee` |
| `fichiers` | `piste_id`, `chemin` réel (sous `SILLON_MEDIAS`), `taille`, `modifie_le`, `conteneur`, `codec`, `debit`, `frequence`, `canaux`, `bits`, `duree`, `contenu_le` (date du dernier changement de contenu : nouveau fichier ou fichier modifié), `vu_le` ; supprimé quand il disparaît |
| `genres`, `artistes_genres`, `albums_genres` | genres de Lidarr par artiste et par album |
| `fichiers_ecartes` | chemin, nom, raison |
| `synchronisations` | par source (`lidarr`) : dernière réussite, dernier essai, erreur |
| `images` | cache des images (clé `<type>/<uuid>/<taille>`, type `cover`, `poster` ou `fanart` ; version = URL Lidarr) |

Fonction SQL `sans_accents(text)` et configuration de recherche `sillon` (`unaccent` puis `simple`).

## Interfaces pour les autres modules

- `catalogue::synchroniser(&AppState)` : lance une synchronisation, ou en demande une à la fin de celle en cours.
- `catalogue::fichier_de_piste(&PgPool, Uuid) -> Option<FichierPiste>` : fichier d'une piste (chemin réel, conteneur, codec, débit,
  fréquence, canaux, bits, durée, taille, `contenu_le`) ; le plus gros si plusieurs. `None` = piste absente.
- `catalogue::fichiers_modifies_depuis(&PgPool, depuis, limite)` : fichiers nouveaux ou changés après `depuis` (par `contenu_le`, puis `id`).
- `catalogue::taille_totale(&PgPool)` : octets de tous les fichiers du catalogue.
- `catalogue::recalculer_presence(&PgPool)` : recalcule `present` des pistes, albums et artistes.
- `lidarr::Client::new(&config::Lidarr, &reqwest::Client)` : `requete(méthode, "/chemin")` (préfixe `/api/v1`, en-tête `X-Api-Key`), `envoyer`,
  `get`, `liste`, et les lectures typées (`artistes`, `albums`, `pistes`, `fichiers`, `image`, `version`). Erreurs en texte, sans URL.
- `bibliotheque::{COLONNES_PISTE, JOINTURES_PISTE, piste_publique, pistes_par_ids}` (et leurs équivalents artiste et album) : JSON public
  d'une piste, dans l'ordre des identifiants donnés ; `curseur`, `lire_curseur` : pagination.
- `config::chemin_local(...)` : traduction et vérification d'un chemin Lidarr (sous `SILLON_MEDIAS`, liens résolus).

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
Les tests du catalogue (`tests_api.rs`, `bibliotheque()`) montent un faux Lidarr (`Monde`, modifiable pendant le test) et de vrais fichiers
flac, mp3 et opus générés par ffmpeg dans un dossier temporaire ; ils demandent ffmpeg et ffprobe (sinon ignorés, avec un message).
Sans `TEST_DATABASE_URL`, les tests passent par testcontainers (ignorés sans Docker).
