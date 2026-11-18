# Sablier

Service de statistiques de plbls.fr, à part d'Estompe, d'Entracte et de Sillon : il reçoit leurs évènements par une file, compte le temps
d'utilisation de l'app et garde des agrégats par jour. Les statistiques viennent par lots ; ce document décrit ce qui existe aujourd'hui :
le socle du service, la file d'évènements (format, émission, consommation, stockage, purge) et le temps d'utilisation.

| Où | Quoi |
|---|---|
| [`api/sablier`](../../api/sablier/README.md) | API Rust (axum, sqlx), membre du workspace Cargo |
| [`api/commun`](../../api/commun/src/evenements.rs) | format des évènements (`evenements.rs`) et émission (`emission.rs`), partagés avec les services qui émettent |
| [`openapi.yaml`](openapi.yaml) | routes, vérifiées par un test (chaque route du code y figure, et rien d'autre) |
| [`deploy/sablier/.env.example`](../../deploy/sablier/.env.example) | secrets, service `sablier-api` de [`deploy/docker-compose.yml`](../../deploy/docker-compose.yml) |

## Permissions

Déclarées à aegis par manifeste (application `sablier`, renvoyé toutes les 10 minutes), toutes fermées par défaut :

| Permission | Ouvre |
|---|---|
| `sablier:access` | ses statistiques (temps d'utilisation) |
| `sablier:admin` | administration de Sablier (aucune route aujourd'hui) |

Chaque route relève d'une permission ou d'une exception motivée (santé, disponibilité, `GET /me`, `POST /heartbeat`) : la table des
routes l'impose (`api/sablier/src/routes.rs`) et un test parcourt toutes les routes (401 sans session ou avec une session invalide, 403
sans la permission). aegis injoignable : 503 `aegis_unavailable` sur les routes soumises à une permission. Sans `AEGIS_TOKEN_SABLIER`,
tout est refusé.

`GET /me` répond à toute personne connectée, sans permission Sablier : identité, permissions, `sections` (ce qu'elle peut ouvrir :
`stats` avec `sablier:access`, `admin` avec `sablier:admin` ; vide = section à ne pas afficher) et `heartbeat_seconds`, l'intervalle des
battements. `permissions.entracte` et `permissions.sillon` disent si la personne a `entracte:access` et `sillon:access` : aegis ne rend
à Sablier, dans `POST /v1/effective`, que les permissions de l'application `sablier` ; Sablier demande donc ces deux-là par
`POST /v1/check` (même cache, 30 s au plus). aegis injoignable : `GET /me` répond quand même, permissions toutes fausses.

Authentification : cookie de session du gateway (`__Secure-plbls_sid`) ou `Authorization: Bearer`, vérifié par `GET /auth/me` du gateway
(cache de 30 s). Requête modifiante avec cookie : `Origin` dans `SABLIER_ALLOWED_ORIGINS`, sinon 403 `bad_origin` ; CORS avec
identifiants pour ces seules origines.

## Évènements

Un évènement (`commun::evenements::Evenement`) est un objet JSON :

| Champ | Contenu |
|---|---|
| `id` | identifiant unique, 1 à 200 caractères : UUID v4 pour un évènement réel, identifiant déterministe pour un import (ex. `import:sillon:ecoute:<id>`) |
| `sub` | la personne |
| `source` | `entracte`, `sillon` ou `estompe` ; doit correspondre au type |
| `date` | date de l'évènement (RFC 3339) |
| `importe` | vrai pour un évènement repris d'un historique (faux par défaut) |
| `type`, `donnees` | type et données propres au type |

Les identifiants des données sont des identifiants publics opaques (chaînes), les durées des secondes entières.

| `type` | `donnees` |
|---|---|
| `entracte.lecture` | `titre`, `genre` (`film`, `episode`), `serie` (ou `null`), `secondes` : tranche de temps réellement regardé |
| `entracte.vu` | `titre`, `genre`, `serie` |
| `entracte.serie_terminee` | `serie` |
| `entracte.abandon` | `titre`, `genre` |
| `entracte.intro_passee` | `titre` |
| `sillon.ecoute` | `piste`, `secondes`, `comptee` |
| `estompe.message` | `serveur`, `salon` (ou `null`), `prive` |
| `estompe.fichier` | `serveur`, `salon`, `prive`, `nombre` |
| `estompe.reaction` | `sens` (`donnee`, `recue`), `emoji` |
| `estompe.vocal.temps` | `serveur`, `salon`, `secondes` |
| `estompe.vocal.appel` | `serveur`, `salon`, `secondes` : un par appel terminé, durée passée avec au moins une autre personne |
| `estompe.partage_ecran` | `secondes` |
| `estompe.cinema.temps` | `secondes` |
| `estompe.cinema.seance` | aucune : une séance à plusieurs |

### Émission

`commun::emission::Emetteur` publie dans le Redis Stream `sablier:evenements` (champ `e` : le JSON de l'évènement), plafonné par
`XADD … MAXLEN ~ SABLIER_FILE_MAX` (500 000 par défaut).

- `Emetteur::depuis_env` lit `SABLIER_REDIS_URL` (et `SABLIER_REDIS_PASSWORD`, ajouté à l'URL si elle n'en porte pas) et
  `SABLIER_FILE_MAX`. Sans adresse, ou avec une adresse illisible, l'émetteur est inactif.
- `emettre(evenement)` ne bloque jamais et ne renvoie rien : l'évènement entre dans un tampon borné (10 000), vidé vers Redis par une
  tâche de fond (connexion à la demande, 2 s au plus par opération). Tampon plein, Redis injoignable ou en erreur : l'évènement est
  abandonné. Une seule ligne de journal par panne, une au retour ; pendant une panne, un essai toutes les 5 s au plus.
- `publier(&evenement)` attend le résultat et renvoie l'erreur : pour les commandes d'import.

### Consommation

Sablier lit le flux par le groupe de consommateurs `sablier` (consommateur `sablier-api`, une seule instance), créé au besoin depuis le
début du flux. Au démarrage il reprend d'abord les entrées lues mais non accusées, puis attend les nouvelles. Chaque entrée est traitée
en une transaction (évènement brut inséré, agrégats du type mis à jour) puis accusée (`XACK`) : une entrée dont l'écriture échoue reste
à reprendre. Un identifiant déjà reçu est ignoré (ni second brut, ni second comptage). Une entrée mal formée, d'un type inconnu ou dont
la source ne correspond pas au type est journalisée, accusée et écartée, sans arrêter la lecture. Sans `SABLIER_REDIS_URL`, Sablier
démarre sans consommer ; Redis en panne : nouvel essai toutes les 5 s, battements et lectures continuent.

### Stockage

| Table | Contenu |
|---|---|
| `evenements` | évènements bruts : `id`, `sub`, `source`, `type`, `date`, `importe`, `donnees` (JSON), `recu_le` |
| `agregats_jour` | une valeur par `(sub, jour, stat, cle)` ; tous les domaines y écrivent |
| `battements` | dernier battement compté de chaque personne |

Le jour d'un instant est celui du fuseau `SABLIER_FUSEAU` (`Europe/Paris` par défaut, vérifié au démarrage auprès de PostgreSQL), y
compris les jours de changement d'heure, qui durent 23 ou 25 heures. Les évènements bruts sont purgés une fois par jour (et au démarrage)
quand leur date et leur réception ont toutes deux plus de `SABLIER_CONSERVATION_MOIS` mois (13 par défaut) : un historique importé reste
donc reconnu, et non recompté, pendant cette durée après son import. Les agrégats ne sont jamais purgés.

Statistiques d'aujourd'hui :

| `stat` | `cle` | Valeur |
|---|---|---|
| `utilisation.secondes` | `<plateforme>\|<section>` | secondes d'utilisation du jour |

## Temps d'utilisation

L'app envoie `POST /heartbeat` (`platform` : `web`, `pwa`, `windows`, `android` ; `section` : `messagerie`, `entracte`, `sillon`,
`regie`, `sablier`, `admin`, `autre`) toutes les `heartbeat_seconds` secondes tant qu'elle est au premier plan, et à chaque retour au
premier plan. La route n'exige que la session : le temps est collecté pour tout le monde, avec ou sans `sablier:access`.

Le serveur ne crédite jamais une durée fixe : il ajoute le temps écoulé depuis le battement précédent de la même personne, plafonné à
`SABLIER_BATTEMENT_SECONDES` (30 par défaut). Le premier battement ne compte rien ; un battement après une heure de silence compte un
intervalle au plus ; deux onglets ou deux appareils actifs en même temps se partagent le même temps écoulé (verrou sur la ligne de la
personne) et ne comptent pas double. Le temps va au jour du battement, à sa plateforme et à sa section.

`GET /usage?period=day|week|month|year|all&date=AAAA-MM-JJ` (`sablier:access`) rend le temps de la personne sur la période qui contient
`date` (aujourd'hui par défaut ; semaine du lundi au dimanche) : total, par jour, par section, par plateforme, journée la plus longue,
nombre de jours actifs et premier jour d'activité.

## Configuration

Valeurs non secrètes dans [`plbls.env`](../../plbls.env), secrets dans Vault ([`deploy/README.md`](../../deploy/README.md)).

| Variable | Rôle |
|---|---|
| `SABLIER_PORT` | port d'écoute (8080) |
| `SABLIER_ALLOWED_ORIGINS` | origines des fronts (CORS, CSRF) |
| `SABLIER_FUSEAU` | fuseau des journées (`Europe/Paris`) |
| `SABLIER_CONSERVATION_MOIS` | conservation des évènements bruts (13) |
| `SABLIER_BATTEMENT_SECONDES` | intervalle et plafond d'un battement (30, entre 5 et 3600) |
| `SABLIER_REDIS_URL`, `SABLIER_REDIS_PASSWORD`, `SABLIER_FILE_MAX` | file d'évènements, communes à Sablier et aux services qui émettent |
| `AEGIS_TOKEN_SABLIER` | jeton de l'application `sablier` chez aegis (Vault `secret/sablier`) |
| `SABLIER_PUBLIC_URL` | adresse publique de l'API, lue par le front d'Estompe |
| `DATABASE_URL`, `DATABASE_USER`, `DATABASE_PASSWORD`, `GATEWAY_URL`, `GATEWAY_INTERNE_URL`, `AEGIS_URL` | communes ; base `sablier` |

## Mise en service

À faire hors de ce dépôt (`plbls.fr-infra`, Vault, Dokploy) avant le premier déploiement :

1. Base `sablier` sur `postgres-main`, accessible à `DATABASE_USER`.
2. Vault : `AEGIS_TOKEN_SABLIER` dans `secret/sablier` et `SABLIER_REDIS_PASSWORD` dans `secret/commun` (sans `$`), puis les lignes de
   [`deploy/.env.example`](../../deploy/.env.example) et de [`deploy/sablier/.env.example`](../../deploy/sablier/.env.example) dans
   l'onglet Environment de la stack. Sans `SABLIER_REDIS_PASSWORD`, la stack ne se déploie pas.
3. Utilisateur Redis `sablier` : créé par la commande du service `redis` du compose (ACL : clés `sablier:*`, commandes `XADD`,
   `XREADGROUP`, `XACK`, `XGROUP CREATE` et de connexion) ; rien à faire à la main, le redéploiement de la stack le crée.
4. aegis : `AEGIS_TOKEN_SABLIER` dans son environnement (même onglet) ; l'application `sablier` y apparaît au premier manifeste. Donner
   `sablier:access` aux comptes voulus.
5. Reverse proxy : domaine Dokploy `sablier-api.plbls.fr` vers `sablier-api` port 8080 (`SABLIER_PUBLIC_URL`).

## Développement

`just dev-up` puis `just sablier-dev-run` (127.0.0.1:8095, avec le gateway et aegis). Tests : `just test-sablier`, voir
[`api/sablier`](../../api/sablier/README.md).
