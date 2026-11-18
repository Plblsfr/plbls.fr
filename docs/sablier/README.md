# Sablier

Service de statistiques de plbls.fr, à part d'Estompe, d'Entracte et de Sillon : il reçoit leurs évènements par une file, compte le temps
d'utilisation de l'app et garde des agrégats par jour. Les statistiques viennent par lots ; ce document décrit ce qui existe aujourd'hui :
le socle du service, la file d'évènements (format, émission, consommation, stockage, purge), le temps d'utilisation et les statistiques
d'Entracte, de Sillon et de la messagerie.

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
| `sablier:access` | ses statistiques (temps d'utilisation, Entracte, Sillon, messagerie) |
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
| `sessions_entracte` | session Entracte en cours de chaque personne : jour et instant de début, dernier évènement, secondes et épisodes cumulés |

Le jour d'un instant est celui du fuseau `SABLIER_FUSEAU` (`Europe/Paris` par défaut, vérifié au démarrage auprès de PostgreSQL), y
compris les jours de changement d'heure, qui durent 23 ou 25 heures. Les évènements bruts sont purgés une fois par jour (et au démarrage)
quand leur date et leur réception ont toutes deux plus de `SABLIER_CONSERVATION_MOIS` mois (13 par défaut) : un historique importé reste
donc reconnu, et non recompté, pendant cette durée après son import. Les agrégats ne sont jamais purgés.

Statistiques (noms figés : les trophées et les récapitulatifs s'appuient dessus ; constantes dans `agregats.rs` et dans chaque module
de `domaines/`). Une statistique est une somme du jour, sauf « maximum » : plus grande valeur du jour.

| `stat` | `cle` | Valeur |
|---|---|---|
| `utilisation.secondes` | `<plateforme>\|<section>` | secondes d'utilisation du jour |
| `entracte.secondes` | | secondes regardées (`entracte.lecture`, import compris) |
| `entracte.films_vus`, `entracte.episodes_vus` | | titres vus (`entracte.vu`, import compris) |
| `entracte.series_terminees` | | séries terminées (import compris) |
| `entracte.session_max` | | maximum : durée de la plus longue session commencée ce jour |
| `entracte.enchaines_max` | | maximum : épisodes vus dans une même session commencée ce jour |
| `entracte.abandons`, `entracte.intros_passees` | | titres abandonnés, intros passées (hors import) |
| `entracte.cinema_secondes`, `entracte.cinema_seances` | | temps en salon cinéma à plusieurs, séances à plusieurs (`estompe.cinema.*`) |
| `sillon.secondes` | | secondes écoutées, écoutes comptées ou non |
| `sillon.ecoutes` | | écoutes comptées |
| `sillon.pistes` | piste | écoutes comptées de la piste |
| `messagerie.messages` | `<serveur>/<salon>` ou `prive` | messages envoyés |
| `messagerie.reactions_donnees` | emoji | réactions données |
| `messagerie.reactions_recues` | | réactions reçues |
| `messagerie.fichiers` | | fichiers et images envoyés |
| `messagerie.vocal_secondes` | `<serveur>/<salon>` (serveur vide hors serveur) | secondes en vocal à plusieurs |
| `messagerie.appel_max` | | maximum : durée du plus long appel terminé ce jour |
| `messagerie.partage_secondes` | | secondes de partage d'écran |

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

## Statistiques par domaine

`GET /entracte`, `GET /sillon` et `GET /messaging` (`sablier:access`) prennent les mêmes `period` et `date` que `GET /usage` et rendent
toutes les statistiques du domaine pour la personne connectée, à 0 ou `null` sans activité : le front choisit ce qu'il affiche. La
personne lue est toujours celle de la session.

**Accès par domaine.** Les statistiques d'Entracte ne sont servies qu'avec `entracte:access`, celles de Sillon qu'avec `sillon:access`
(les mêmes droits que `permissions.entracte` et `permissions.sillon` de `GET /me`). Sans ce droit, la route répond `200` avec
`{"access": false}` et rien d'autre : ce n'est pas une erreur, le front affiche une carte « pas d'accès ». Avec le droit, la réponse
porte `"access": true`. Les évènements sont comptés pour tout le monde, avec ou sans ces droits.

**Import.** Un évènement importé (`importe = true`) est daté de l'historique et compte pour le temps regardé, les films et épisodes vus,
les séries terminées, les écoutes et la messagerie. Son identifiant est déterministe : rejoué, il est ignoré, les totaux ne changent
pas. Il ne compte jamais pour les façons de regarder (sessions, épisodes enchaînés, abandons, intros passées).

### Entracte

Temps regardé (somme des `secondes` des `entracte.lecture`), films vus, épisodes vus, séries terminées, plus grosse journée (jour de la
période au plus grand temps regardé), plus longue session, épisodes enchaînés, titres abandonnés, intros passées, temps en salon cinéma
et séances à plusieurs. Le temps en salon cinéma (`estompe.cinema.temps`) n'est pas ajouté au temps regardé.

- **Session** : suite d'évènements `entracte.lecture` réels d'une personne dont les dates se suivent à 30 minutes au plus. Sa durée est
  la somme de leurs `secondes`. Un `entracte.lecture` daté de plus de 30 minutes après le précédent ouvre une nouvelle session.
- **Épisodes enchaînés** : nombre d'`entracte.vu` réels d'un épisode datés dans la session en cours (30 minutes de marge avant son début
  et après son dernier évènement). Un épisode marqué vu hors de toute session compte dans les épisodes vus, pas dans les enchaînements.
- Une session appartient au jour où elle commence, même si elle passe minuit : sa durée et ses épisodes enchaînés vont à ce jour. Sur
  une période, « plus longue session » et « épisodes enchaînés » sont les maximums des jours.
- Seule la session en cours de chaque personne est gardée (`sessions_entracte`). Un évènement en retard qui tombe dans la session en
  cours s'y ajoute ; un `entracte.lecture` plus ancien que la session en cours compte comme une session à lui seul, sans la modifier.

### Sillon

Temps d'écoute (toutes les `secondes` des `sillon.ecoute`), titres écoutés (écoutes `comptee`, selon la règle de Sillon : 30 s ou la
moitié de la piste) et titres différents (pistes distinctes parmi les écoutes comptées de la période). Une même piste écoutée trois
fois compte trois titres écoutés et un titre différent.

### Messagerie et vocal

Messages envoyés (total, conversations privées, par salon), salon le plus fréquenté, journée la plus bavarde, réactions données et
reçues, emoji le plus utilisé parmi les réactions données, fichiers et images envoyés, temps en vocal, plus long appel, salon vocal
préféré, temps en partage d'écran.

- Un message dans un salon compte pour ce salon et pour le total. Les messages privés ne comptent que dans un total unique (`private`,
  clé `prive`) et dans le total général, sans détail par conversation.
- **Noms des salons** : Sablier ne connaît que des identifiants. La réponse donne `server` et `channel` ; le front lit le nom au moment
  de l'affichage dans ce que la personne peut voir. Un salon supprimé ou devenu inaccessible s'affiche « salon supprimé » et garde ses
  compteurs : Sablier ne purge rien quand un salon disparaît.

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
