# sablier

API de Sablier, service de statistiques de plbls.fr : documentation fonctionnelle dans [`docs/sablier`](../../docs/sablier/README.md),
routes dans [`openapi.yaml`](../../docs/sablier/openapi.yaml).

| Module | Rôle |
|---|---|
| `config.rs` | variables `SABLIER_*` et communes (valeurs dans [`plbls.env`](../../plbls.env), secrets dans [`deploy/sablier/.env.example`](../../deploy/sablier/.env.example)) |
| `routes.rs` | table des routes : chacune a une permission ou une exception motivée ; tests de couverture et de conformité à OpenAPI (dans les deux sens) |
| `garde.rs`, `identite.rs`, `droits.rs` | gateway (`/auth/me`, cookie ou Bearer), aegis (manifeste, `/v1/effective`, `/v1/check` pour `entracte:access` et `sillon:access`), sections de `GET /me`, CORS et CSRF |
| `etat.rs` | état partagé (configuration, base, client HTTP, aegis) et tâches de fond (manifeste, purge quotidienne, évaluation des trophées, création des récaps, consommation de la file) |
| `interne.rs` | santé, disponibilité, `GET /me` |
| `http.rs` | réponses JSON et erreurs `{error, message}` |
| `file.rs` | consommation du Redis Stream (`consommer` : groupe, reprise des entrées non accusées, accusé après écriture) et traitement d'un évènement (`traiter`) |
| `domaines/` | un fichier par domaine (`entracte.rs`, `sillon.rs`, `messagerie.rs`) : noms des statistiques, agrégats par type d'évènement (`appliquer`), lecture (`lire`) et route (`voir`) ; `mod.rs` aiguille les évènements (`estompe.cinema.*` vers Entracte) |
| `agregats.rs` | table `agregats_jour` : écritures (`ajouter`, `maximum`), lectures génériques, jour d'un instant dans le fuseau |
| `periode.rs` | périodes de lecture (`day`, `week`, `month`, `year`, `all`), paramètres `period` et `date`, bornes, intervalle rendu (`period`, `from`, `to`) |
| `battement.rs` | `POST /heartbeat` : plateformes et sections acceptées, temps crédité (`enregistrer`, horloge en paramètre) |
| `utilisation.rs` | `GET /usage` (`lire`, `voir`) |
| `catalogue.rs` | catalogue fermé des stats (clé publique, libellé, domaine, unité, calcul, partageable), valeur sur une plage, progression vers des seuils avec la date où chacun est atteint |
| `series.rs` | séries de jours (jours comptés, série en cours et record), `GET /streaks` |
| `exploits.rs` | registre fermé des exploits codés : chacun dit s'il est atteint et depuis quel jour |
| `trophees.rs` | table `trophees`, évaluation et déblocages, personnes à réévaluer, `GET /trophies`, `GET /unseen`, `POST /seen`, routes d'administration |
| `recaps.rs` | création des récaps du mois et de l'année (`creer_les_recaps`), `GET /recaps`, `GET /recaps/{id}` |
| `partage.rs` | réglages de partage (`GET`/`PUT /sharing`), profil public (`GET /profiles/{user}`, identifiant Estompe) et demande de visibilité à Estompe |
| `effacement.rs` | `tables_personnelles()` : toutes les tables indexées par `sub` ; `effacer` (une transaction), `DELETE /data` |
| `export.rs` | `GET /export` : en-tête chargé d'un bloc (`tete`), évènements bruts envoyés par pages (`flux`), `sections()` : la section de chaque table personnelle |
| `personnes.rs` | annuaire tenu par Sablier (`noter` : nom et dernier passage, une écriture par heure au plus), alimenté par `GET /me` et le battement ; `annuaire` : comptes d'aegis (`Aegis::comptes`, `POST /v1/accounts/lookup`) ou `None` pour le repli local |
| `journal.rs` | table `consultations` : `inscrire` et `consulter` (regroupement de 10 minutes, horloge en paramètre), `GET /views`, `GET /admin/audit` |
| `admin.rs` | routes `sablier:admin` hors trophées : vue d'une personne (`/admin/people/{sub}/…`), liste des personnes, totaux, effacement |
| `purge.rs` | purge des évènements bruts (`purger`) |
| `migrations/` | schéma de la base `sablier` (`0001_sablier.sql` : `evenements`, `agregats_jour`, `battements` ; `0002_sessions_entracte.sql` : session Entracte en cours ; `0003_trophees_recaps_partage.sql` : trophées et jeu de départ, déblocages, récaps, partage ; `0004_personnes_consultations.sql` : annuaire et journal des consultations) |

Le format des évènements et leur émission sont dans [`api/commun`](../commun/src/evenements.rs) (`evenements.rs`, `emission.rs`).

## Interfaces pour les autres modules

- `agregats::ajouter(exécuteur, sub, jour, stat, cle, n)` (somme) et `agregats::maximum(…)` (plus grande valeur) : écriture d'un agrégat,
  sur un `&PgPool` ou dans une transaction (`&mut *tx`). Nom de statistique `<domaine>.<mesure>`, `cle` vide sans répartition.
- `agregats::somme(pool, sub, stat, du, au) -> i64`, `par_cle(…) -> Vec<(String, i64)>` (valeur décroissante), `par_jour(…) -> Vec<(NaiveDate, i64)>`,
  `max_jour(…) -> Option<(NaiveDate, i64)>` (jour de la plus grande somme), `plus_grande(…) -> i64` (plus grande valeur d'un jour, pour
  une statistique de type maximum), `cles_distinctes(…) -> i64`, `jours_actifs(…) -> i64`, `premier_jour(pool, sub) -> Option<NaiveDate>`
  (toutes statistiques confondues) ; bornes incluses.
- `agregats::jour_de(exécuteur, fuseau, instant) -> NaiveDate` : jour d'un instant dans le fuseau (`Config::fuseau`).
- `periode::Demande` (extrait par `Query`) : `plage(&AppState) -> Result<Plage, Response>` (400 `invalid` sinon) ; `Plage { periode, date, du, au }` ;
  `Periode::bornes(date)` (`Tout` : du 0001-01-01 au 9999-12-31) ; `Plage::intervalle_de(pool, sub) -> Intervalle { period, from, to }`
  (pour `Tout` : du premier jour d'activité à `date`), à aplatir dans une réponse.
- `domaines::<domaine>::appliquer(tx, &Evenement, jour)` : appelé une seule fois par évènement, dans la transaction qui insère le brut ;
  y faire un `match` sur `e.donnees` et écrire par `agregats`.
- `domaines::<domaine>::lire(pool, sub, &Plage) -> Stats` (sérialisable : le corps de la réponse) : toutes les statistiques du domaine
  pour une personne, sans contrôle d'accès. Le gestionnaire `voir` prend le `sub` de la session et vérifie l'accès au domaine
  (`Droits.entracte`, `Droits.sillon` ; sinon `domaines::sans_acces()`, `200 {"access": false}`).
- Noms des statistiques : constantes de chaque module de domaine (`entracte::SECONDES`, `sillon::PISTES`, `messagerie::MESSAGES`…),
  listées dans [`docs/sablier`](../../docs/sablier/README.md).
- `file::traiter(pool, fuseau, json) -> Issue` (`Applique`, `Doublon`, `Ecarte(motif)`) : chemin complet d'un évènement, sans Redis.
- `utilisation::lire(pool, sub, &Plage) -> Value` : le corps de `GET /usage` pour une personne.
- `AppState::aujourd_hui()` : le jour courant dans le fuseau. Les fonctions de lecture ci-dessous le prennent en paramètre (injecté
  dans les tests) et ne contrôlent pas l'accès : l'appelant choisit le `sub` et les `Droits` qui filtrent Entracte et Sillon.
- `trophees::lire(pool, sub, &Droits, aujourd_hui) -> Trophees` (corps de `GET /trophies`), `trophees::etats(pool, sub, &Droits)`
  (un `Etat` par trophée actif et accessible : paliers débloqués, dates, vu, partagé), `trophees::evaluer(pool, sub, aujourd_hui)`
  (insère les déblocages manquants, idempotent), `trophees::marquer(exécuteur, sub)` (à réévaluer), `trophees::charger(pool, actifs)`.
- `series::lire(pool, sub, &Droits, aujourd_hui) -> Series` (corps de `GET /streaks`), `series::etat(pool, sub, Serie, aujourd_hui)`.
- `recaps::lister(pool, sub) -> Vec<Resume>`, `recaps::lire(pool, sub, id) -> Option<Value>`,
  `recaps::creer_les_recaps(&AppState, aujourd_hui) -> u64` (idempotent).
- `partage::lire(pool, sub, &Droits) -> Reglages` (corps de `GET /sharing`), `partage::profil(pool, cible, &Droits du lecteur,
  aujourd_hui) -> Profil` (ce que la cible a rendu public), `partage::profil_visible(&AppState, sub du lecteur, identifiant
  Estompe de la cible) -> Option<String>` (le `sub` de la cible quand Estompe dit le profil visible).
- `catalogue::STATS`, `catalogue::trouver(cle)`, `catalogue::valeur(pool, sub, &Stat, du, au)` ; `exploits::EXPLOITS`.
- `effacement::tables_personnelles()` : les tables à vider (`DELETE … WHERE sub = $1`) ou à exporter pour une personne. Une nouvelle
  table avec une colonne `sub` doit y être ajoutée, avec sa section dans `export::sections()` : `effacement::tests` compare la liste
  au schéma, `export::tests` compare les sections à la liste. `consultations` n'en fait pas partie (colonnes `admin_sub`, `cible_sub`).
- `effacement::effacer(pool, sub) -> u64` (lignes supprimées) et `effacement::effacer_dans(&mut tx, sub)` pour l'inclure dans une
  transaction ; `effacement::confirmer(octets)` : le corps `{"confirm": "EFFACER"}`.
- `journal::consulter(pool, &Identite de l'admin, cible, maintenant) -> bool` (ligne ajoutée ou regroupée) ;
  `journal::inscrire(&mut tx, admin, cible, Action, maintenant)` dans une transaction ; `journal::vues(pool, cible, limite)`.
- `personnes::noter(pool, &Identite, maintenant) -> bool` (écrit ou non).
- `admin::TOUT` : les `Droits` pleins passés aux fonctions de lecture pour la vue d'administration ; `admin::personnes(&AppState,
  sans_compte) -> (annuaire utilisé, fiches)`, `admin::totaux(pool, &Plage)`.
- `personnes::annuaire(&AppState, subs) -> Option<HashMap<sub, Compte { nom, present }>>` (`None` : repli sur la table locale),
  `personnes::source(&annuaire)` (`aegis` ou `local`).

## Ajouter une route

Une entrée dans `routes::table()` (méthode, chemin, garde `Garde::Droit(..)` ou exception motivée, handler) et la même route dans
`docs/sablier/openapi.yaml` : sinon `routes::tests::routes_et_openapi_identiques` échoue. Le test
`tests_api::chaque_route_protegee_refuse_sans_session_et_sans_permission` la couvre ensuite sans changement.

## Lancer

`just sablier-dev-run` (après `just dev-up`, avec le gateway et aegis), ou `cargo run -p sablier` avec les variables exportées.
Image : `docker build -f api/sablier/Dockerfile .` (contexte : racine du dépôt).

## Tests

`TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres TEST_REDIS_URL=redis://localhost:6379 just test-sablier`
(`TEST_DATABASE_URL` se termine par le nom d'une base existante, `/postgres` : les bases de test sont créées à côté) : une
base jetable par test, migrations appliquées, faux gateway et faux aegis (`Env::annuaire` : `None`, le faux aegis répond 404 à
`/v1/accounts/lookup` et Sablier se replie sur `personnes` ; `Some(comptes)` : il sert ces comptes) (`testutil.rs` : `env()`, `env_avec()`, personnes `personne`,
`alice`, `bruno` (avec `sillon:access`), `diane` (avec `entracte:access`), `admin` ; `Env::recevoir` : un évènement par `file::traiter` ;
`Env::remplir` : des lignes dans chaque table personnelle pour une personne, `Env::lignes_de` : leur nombre par table, `Env::lectures` :
la réponse de chaque route de lecture `sablier:access` ;
`attendre` : attente d'une condition). Sans `TEST_DATABASE_URL`, les tests passent par testcontainers
(ignorés sans Docker). Les tests de la consommation (`file.rs`) demandent `TEST_REDIS_URL` (ignorés sinon, avec un message) et utilisent
un flux et un groupe propres à chaque test ; la déduplication et le rejet des entrées mal formées sont aussi vérifiés sans Redis.
Les tests du battement passent l'heure en paramètre (changements d'heure du 29 mars et du 25 octobre 2026, minuit à Paris).
Les tests des trophées, des séries et des récaps passent le jour courant en paramètre ; l'environnement de test simule Estompe par un
faux serveur HTTP (`testutil::faux_estompe` : chacun n'y voit que son propre profil, identifiant `testutil::id_estompe(nom)`) ; les
tests du profil public le remplacent par le leur (`partage::tests::faux_estompe`).
