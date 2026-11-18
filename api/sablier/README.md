# sablier

API de Sablier, service de statistiques de plbls.fr : documentation fonctionnelle dans [`docs/sablier`](../../docs/sablier/README.md),
routes dans [`openapi.yaml`](../../docs/sablier/openapi.yaml).

| Module | Rôle |
|---|---|
| `config.rs` | variables `SABLIER_*` et communes (valeurs dans [`plbls.env`](../../plbls.env), secrets dans [`deploy/sablier/.env.example`](../../deploy/sablier/.env.example)) |
| `routes.rs` | table des routes : chacune a une permission ou une exception motivée ; tests de couverture et de conformité à OpenAPI (dans les deux sens) |
| `garde.rs`, `identite.rs`, `droits.rs` | gateway (`/auth/me`, cookie ou Bearer), aegis (manifeste, `/v1/effective`, `/v1/check` pour `entracte:access` et `sillon:access`), sections de `GET /me`, CORS et CSRF |
| `etat.rs` | état partagé (configuration, base, client HTTP, aegis) et tâches de fond (manifeste, purge quotidienne, consommation de la file) |
| `interne.rs` | santé, disponibilité, `GET /me` |
| `http.rs` | réponses JSON et erreurs `{error, message}` |
| `file.rs` | consommation du Redis Stream (`consommer` : groupe, reprise des entrées non accusées, accusé après écriture) et traitement d'un évènement (`traiter`) |
| `domaines/` | agrégats par type d'évènement : `mod.rs` (`appliquer`, aiguillage par source), `entracte.rs`, `sillon.rs`, `messagerie.rs` (un fichier par domaine, vides aujourd'hui : le brut seul est gardé) |
| `agregats.rs` | table `agregats_jour` : écritures (`ajouter`, `maximum`), lectures génériques, jour d'un instant dans le fuseau |
| `periode.rs` | périodes de lecture (`day`, `week`, `month`, `year`, `all`), paramètres `period` et `date`, bornes |
| `battement.rs` | `POST /heartbeat` : plateformes et sections acceptées, temps crédité (`enregistrer`, horloge en paramètre) |
| `utilisation.rs` | `GET /usage` |
| `purge.rs` | purge des évènements bruts (`purger`) |
| `migrations/` | schéma de la base `sablier` (`0001_sablier.sql` : `evenements`, `agregats_jour`, `battements`) |

Le format des évènements et leur émission sont dans [`api/commun`](../commun/src/evenements.rs) (`evenements.rs`, `emission.rs`).

## Interfaces pour les autres modules

- `agregats::ajouter(exécuteur, sub, jour, stat, cle, n)` (somme) et `agregats::maximum(…)` (plus grande valeur) : écriture d'un agrégat,
  sur un `&PgPool` ou dans une transaction (`&mut *tx`). Nom de statistique `<domaine>.<mesure>`, `cle` vide sans répartition.
- `agregats::somme(pool, sub, stat, du, au) -> i64`, `par_cle(…) -> Vec<(String, i64)>` (valeur décroissante), `par_jour(…) -> Vec<(NaiveDate, i64)>`,
  `max_jour(…) -> Option<(NaiveDate, i64)>` (jour de la plus grande somme), `jours_actifs(…) -> i64`, `premier_jour(pool, sub) -> Option<NaiveDate>`
  (toutes statistiques confondues) ; bornes incluses.
- `agregats::jour_de(exécuteur, fuseau, instant) -> NaiveDate` : jour d'un instant dans le fuseau (`Config::fuseau`).
- `periode::Demande` (extrait par `Query`) : `plage(&AppState) -> Result<Plage, Response>` (400 `invalid` sinon) ; `Plage { periode, date, du, au }` ;
  `Periode::bornes(date)` (`Tout` : du 0001-01-01 au 9999-12-31).
- `domaines::<domaine>::appliquer(tx, &Evenement, jour)` : appelé une seule fois par évènement, dans la transaction qui insère le brut ;
  y faire un `match` sur `e.donnees` et écrire par `agregats`.
- `file::traiter(pool, fuseau, json) -> Issue` (`Applique`, `Doublon`, `Ecarte(motif)`) : chemin complet d'un évènement, sans Redis.

## Ajouter une route

Une entrée dans `routes::table()` (méthode, chemin, garde `Garde::Droit(..)` ou exception motivée, handler) et la même route dans
`docs/sablier/openapi.yaml` : sinon `routes::tests::routes_et_openapi_identiques` échoue. Le test
`tests_api::chaque_route_protegee_refuse_sans_session_et_sans_permission` la couvre ensuite sans changement.

## Lancer

`just sablier-dev-run` (après `just dev-up`, avec le gateway et aegis), ou `cargo run -p sablier` avec les variables exportées.
Image : `docker build -f api/sablier/Dockerfile .` (contexte : racine du dépôt).

## Tests

`TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres TEST_REDIS_URL=redis://localhost:6379 just test-sablier` : une
base jetable par test, migrations appliquées, faux gateway et faux aegis (`testutil.rs` : `env()`, `env_avec()`, personnes `personne`,
`alice`, `bruno`, `admin` ; `attendre` : attente d'une condition). Sans `TEST_DATABASE_URL`, les tests passent par testcontainers
(ignorés sans Docker). Les tests de la consommation (`file.rs`) demandent `TEST_REDIS_URL` (ignorés sinon, avec un message) et utilisent
un flux et un groupe propres à chaque test ; la déduplication et le rejet des entrées mal formées sont aussi vérifiés sans Redis.
Les tests du battement passent l'heure en paramètre (changements d'heure du 29 mars et du 25 octobre 2026, minuit à Paris).
