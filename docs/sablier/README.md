# Sablier

Service de statistiques de plbls.fr, à part d'Estompe, d'Entracte et de Sillon : il reçoit leurs évènements par une file, compte le temps
d'utilisation de l'app et garde des agrégats par jour. Les statistiques viennent par lots ; ce document décrit ce qui existe aujourd'hui :
le socle du service, la file d'évènements (format, émission, consommation, stockage, purge), le temps d'utilisation, les statistiques
d'Entracte, de Sillon et de la messagerie, les trophées, les séries de jours, les récaps, le partage sur le profil, l'administration
(vue par personne, totaux, journal des consultations), l'export et l'effacement.

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
| `sablier:access` | ses statistiques (temps d'utilisation, Entracte, Sillon, messagerie), ses trophées, ses séries, ses récaps, ses réglages de partage, la liste des admins qui ont consulté ses stats (`GET /views`), l'export (`GET /export`) et l'effacement (`DELETE /data`) de ses données, et les stats et trophées publics d'une autre personne (`GET /profiles/{sub}`) |
| `sablier:admin` | configuration des trophées (`/admin/trophies`, `/admin/catalogue`), statistiques, trophées et récaps de n'importe qui (`/admin/people/{sub}/…`), totaux globaux (`/admin/totals`), journal des consultations (`/admin/audit`), effacement des données d'une personne |

La personne visée n'est un paramètre que sur les routes `sablier:admin` (`/admin/people/{sub}/…`) : les routes `sablier:access` lisent
toujours la personne de la session. Seule exception, `GET /profiles/{sub}`, qui ne sert que ce que la cible a rendu public. Un test
sur la table des routes le vérifie (`routes::tests::c1_…`).

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
| `id` | identifiant unique, 1 à 200 caractères : unique et stable par fait : déterministe dès qu'une ligne d'origine existe (ex. `import:sillon:ecoute:<id>`, le même pour l'évènement réel et pour l'import de cette ligne), UUID v4 seulement sans ligne d'origine |
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
- `publier(&evenement)` attend le résultat et renvoie l'erreur : pour les commandes d'import. Un import de plus de `SABLIER_FILE_MAX`
  évènements exige que Sablier consomme pendant l'import : sinon le plafond du flux en rogne le début, perdu sans erreur.

### Consommation

Sablier lit le flux par le groupe de consommateurs `sablier` (consommateur `sablier-api`, une seule instance), créé au besoin depuis le
début du flux. Au démarrage il reprend d'abord les entrées lues mais non accusées, puis attend les nouvelles. Chaque entrée est traitée
en une transaction (évènement brut inséré, agrégats du type mis à jour) puis accusée (`XACK`) : une entrée dont l'écriture échoue reste
à reprendre. Un identifiant déjà reçu est ignoré (ni second brut, ni second comptage), quel que soit `importe` : un fait reçu en réel puis
repris par un import n'est compté qu'une fois, avec la valeur d'`importe` du premier reçu. Une entrée mal formée, d'un type inconnu ou dont
la source ne correspond pas au type est journalisée, accusée et écartée, sans arrêter la lecture. Sans `SABLIER_REDIS_URL`, Sablier
démarre sans consommer ; Redis en panne : nouvel essai toutes les 5 s, battements et lectures continuent.

### Stockage

| Table | Contenu |
|---|---|
| `evenements` | évènements bruts : `id`, `sub`, `source`, `type`, `date`, `importe`, `donnees` (JSON), `recu_le` |
| `agregats_jour` | une valeur par `(sub, jour, stat, cle)` ; tous les domaines y écrivent |
| `battements` | dernier battement compté de chaque personne |
| `sessions_entracte` | session Entracte en cours de chaque personne : jour et instant de début, dernier évènement, secondes et épisodes cumulés |
| `trophees` | catalogue des trophées : clé stable, nom, description, icône, caché, actif, genre (`paliers` ou `exploit`), stat, période et seuils, ou exploit et paramètre, ordre |
| `deblocages` | un palier débloqué par personne et par trophée : date (jour où le seuil a été atteint), `vu_le` |
| `a_evaluer` | personnes dont les trophées sont à réévaluer |
| `recaps` | récaps figés : personne, genre (`mois`, `annee`), période (`2026-10`, `2026`), contenu JSON, `vu_le` |
| `partages_stats`, `partages_trophees` | ce que chaque personne a rendu public (une ligne = partagé) |
| `personnes` | annuaire tenu par Sablier, utilisé en repli de celui d'aegis : nom et dernier passage de chaque personne vue connectée (`nom`, `vu_le`) |
| `consultations` | journal des consultations et des effacements faits par les admins : `admin_sub`, `admin_nom`, `cible_sub`, `action`, `date` |

Toute table qui porte des données d'une personne a une colonne `sub` et figure dans `effacement::tables_personnelles()` (vérifié par un
test sur le schéma) : effacer ou exporter une personne se fait table par table, par `WHERE sub = …`. `consultations` est la seule
exception voulue : le journal survit à l'effacement, ses colonnes s'appellent donc `admin_sub` et `cible_sub`, pas `sub`.

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
| `messagerie.vocal_secondes` | `<serveur>/<salon>` ou `prive` | secondes en vocal à plusieurs |
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
les séries terminées, les écoutes et la messagerie. Son identifiant est déterministe : rejoué, ou déjà reçu en réel sous le même identifiant, il est ignoré, les
totaux ne changent pas. Il ne compte jamais pour les façons de regarder (sessions, épisodes enchaînés, abandons, intros passées).

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
- Le vocal privé (sans serveur) suit la même règle : rangé sous la clé `prive`, servi en `private_voice_seconds`, compris dans
  `voice_seconds`, jamais dans `top_voice_channel`. Aucun agrégat ne porte l'identifiant d'une conversation privée ; un message ou un
  vocal sans serveur est toujours compté comme privé.
- **Noms des salons** : Sablier ne connaît que des identifiants. La réponse donne `server` et `channel` ; le front lit le nom au moment
  de l'affichage dans ce que la personne peut voir. Un salon supprimé ou devenu inaccessible s'affiche « salon supprimé » et garde ses
  compteurs : Sablier ne purge rien quand un salon disparaît.

## Trophées

Le catalogue des trophées est en base (`trophees`), configurable par les routes `sablier:admin`. Le jeu de départ (32 trophées) est
inséré par la migration `0003`. Un trophée est de l'un de deux genres :

- **paliers** (`kind: tiers`) : une stat du catalogue, une période (`day`, `week`, `month`, `all`) et 1 à 6 seuils strictement
  croissants. Sur `all`, le seuil porte sur le cumul depuis le premier jour ; sur `day`, `week` ou `month`, le palier se débloque dès
  qu'une période de ce grain atteint le seuil (« Habitué » : 10 jours actifs dans un même mois). Un trophée de série de jours est un
  trophée à paliers dont la stat est une série (`streak.*`) : le seuil porte sur le nombre de jours d'affilée.
- **exploit** (`kind: exploit`) : une condition codée du registre (`exploits.rs`), que l'admin nomme, active et, si elle en prend un,
  règle par un paramètre. Un exploit n'a qu'un palier.

Un trophée peut être **caché** : tant qu'il n'est pas débloqué, il n'apparaît dans aucune réponse hors administration, ni par son nom
ni par sa condition ; il n'est que compté (`hidden_remaining`).

**Catalogue de stats** (`catalogue.rs`, liste fermée, servie par `GET /admin/catalogue`). Durées en secondes. « Partageable » : peut
paraître sur le profil ; les records datés, les titres abandonnés et les intros passées ne le peuvent pas.

| Clé | Stat | Calcul sur une plage | Partageable |
|---|---|---|---|
| `usage.active_seconds` | temps actif | somme d'`utilisation.secondes` | oui |
| `usage.longest_day` | journée la plus longue | plus grande somme d'un jour | non |
| `usage.active_days` | jours actifs | jours avec du temps actif | oui |
| `usage.first_day` | premier jour d'activité (date ; pas de palier) | premier jour d'un agrégat | oui |
| `entracte.seconds`, `entracte.movies_watched`, `entracte.episodes_watched`, `entracte.series_completed` | temps regardé, films vus, épisodes vus, séries terminées | sommes | oui |
| `entracte.longest_session`, `entracte.biggest_day` | plus longue session, plus grosse journée | maximum | non |
| `entracte.episodes_in_a_row` | épisodes enchaînés | maximum | oui |
| `entracte.abandoned`, `entracte.intros_skipped` | titres abandonnés, intros passées | sommes | non |
| `entracte.cinema_seconds`, `entracte.cinema_sessions` | temps en salon cinéma, séances à plusieurs | sommes | oui |
| `sillon.seconds`, `sillon.tracks_played` | temps d'écoute, titres écoutés | sommes | oui |
| `sillon.distinct_tracks` | titres différents | pistes distinctes | oui |
| `messaging.messages`, `messaging.reactions_given`, `messaging.reactions_received`, `messaging.files`, `messaging.voice_seconds`, `messaging.screen_share_seconds` | messages envoyés, réactions données et reçues, fichiers envoyés, temps en vocal, temps en partage d'écran | sommes | oui |
| `messaging.busiest_day`, `messaging.longest_call` | journée la plus bavarde, plus long appel | maximum | non |
| `streak.global`, `streak.entracte`, `streak.sillon`, `streak.messaging` | séries de jours | plus longue suite de jours d'affilée | oui |

**Exploits codés** (clé : condition, paramètre par défaut) : `premier_jour` (le premier jour d'activité existe ; daté de ce jour),
`episodes_enchaines` (épisodes vus dans une même session, 5), `longue_session` (secondes d'une session Entracte, 14 400),
`journee_entracte` (secondes regardées sur une journée, 28 800), `journee_bavarde` (messages sur une journée, 200), `long_appel`
(secondes d'un appel, 10 800), `longue_journee` (secondes de temps actif sur une journée, 21 600), `meme_emoji` (un même emoji donné
en réaction, 500 fois), `salon_habite` (messages dans un même salon, privés exclus, 1 000), `mois_parfait` (du temps actif tous les
jours d'un mois ; daté du dernier jour du mois).

**Déblocage.** Sablier calcule les déblocages sur les agrégats (`trophees::evaluer`, idempotent) et les enregistre dans `deblocages`,
un par palier. La date d'un déblocage est le jour où le seuil a été atteint d'après les agrégats (cumul des jours dans l'ordre), pas le
jour du calcul : un trophée débloqué par un historique importé porte la date de l'historique. Un palier débloqué garde sa date, même
si des évènements plus anciens arrivent ensuite.

L'évaluation ne ralentit pas la consommation : chaque évènement traité et chaque battement compté marque la personne dans `a_evaluer`
(dans la même transaction) ; une tâche de fond évalue les personnes marquées toutes les 10 secondes, et `GET /trophies` et
`GET /unseen` évaluent d'abord la personne lue si elle est marquée.

**Règles de modification.** L'évaluation n'enlève jamais un déblocage.

- Un seuil relevé ne retire rien : la personne garde son palier.
- Un trophée créé, modifié ou réactivé est recalculé sur l'historique de tout le monde dans la requête d'administration : un nouveau
  trophée ou un seuil abaissé se débloque aussitôt pour les personnes qui l'atteignent déjà, à la date où elles l'ont atteint. Le
  recalcul parcourt toutes les personnes ayant des agrégats ; s'il échoue, la tâche de fond le termine.
- Un trophée désactivé n'est plus évalué ni servi ; ses déblocages restent en base et reviennent, avec leur date, leur état « vu » et
  leur partage, quand il est réactivé. Il n'y a pas de suppression.
- La clé (`key`) et le genre (`kind`) d'un trophée ne se modifient pas.

**Lecture.** `GET /trophies` rend les trophées débloqués (palier, date du dernier palier, vu, partagé), les trophées en cours
(progression vers le palier suivant ; pour une série, la série en cours) et le nombre de trophées cachés restants. Un trophée dont la
stat ou l'exploit relève d'Entracte ou de Sillon n'est servi (et compté) qu'avec `entracte:access` ou `sillon:access`, comme les
statistiques du domaine. `GET /unseen` donne le nombre de trophées et de récaps pas encore vus, `POST /seen` les marque vus.

**Icônes.** `icon` est un nom d'icône du front d'Estompe (`web/estompe/src/components/Icon.svelte`) ; le serveur n'en vérifie que la
forme (`[a-z0-9-]{1,40}`).

## Séries de jours

Une journée (fuseau `SABLIER_FUSEAU`) compte s'il y a au moins une action réelle : `entracte.secondes` pour Entracte, `sillon.ecoutes`
(écoutes comptées) pour Sillon, `messagerie.messages` ou `messagerie.vocal_secondes` pour la messagerie ; la série globale compte une
journée dès qu'un des trois domaines la compte. Le temps d'utilisation seul ne compte jamais.

`GET /streaks` rend pour la série globale et pour chaque domaine la série en cours et le record. La série en cours est le nombre de
jours d'affilée jusqu'à aujourd'hui, ou jusqu'à hier : elle n'est cassée que lorsqu'une journée entière s'est écoulée sans action.
`entracte` et `sillon` valent `null` sans l'accès au service.

## Récaps

Une tâche de fond (au démarrage, puis toutes les heures) crée le récap du mois précédent et celui de l'année précédente pour chaque
personne qui a assez d'activité sur la période et pas encore de récap : le récap du mois apparaît donc le 1er du mois suivant, celui de
l'année le 1er janvier. Au premier lancement, seuls le mois précédent et l'année précédente sont créés, pas tout l'historique.

- **Figé.** Le contenu est calculé une fois et enregistré tel quel (`recaps.contenu`) : des évènements en retard, un import ou une
  modification des trophées ne le changent plus.
- **Seuil.** Pas de récap sous `SABLIER_RECAP_JOURS_MIN` jours avec une action réelle sur le mois (3 par défaut), ou
  `SABLIER_RECAP_JOURS_MIN_ANNEE` sur l'année (10). Une seule requête par passe cherche les personnes à servir : une période sous le
  seuil ne coûte rien de plus. Tant que le mois suivant (ou l'année suivante) n'est pas fini, une personne qui passe le seuil grâce à
  des évènements arrivés en retard reçoit son récap à la passe suivante.
- **Domaines.** Le récap contient les totaux d'utilisation et de messagerie, et ceux d'Entracte et de Sillon seulement si la personne
  a `entracte:access` et `sillon:access` au moment de la création (demandés à aegis par `POST /v1/check` avec son seul `sub`). aegis
  injoignable ou non configuré : la création est reportée à la passe suivante plutôt que de figer un récap amputé.
- **Contenu.** Totaux (formes de `GET /usage`, `/entracte`, `/sillon`, `/messaging` sur la période), faits marquants (`highlights` :
  records non nuls sous forme `{kind, value, …}`, sans texte, le front rédige), trophées débloqués pendant la période et plus longue
  série de jours de la période.

`GET /recaps` liste ses récaps, `GET /recaps/{id}` rend le contenu figé d'un des siens.

## Partage et profil

Tout est privé par défaut. `GET /sharing` rend les réglages de la personne, stat par stat (stats partageables du catalogue, pour les
domaines auxquels elle a accès) et trophée par trophée (trophées débloqués et actifs) ; `PUT /sharing` les modifie (envoi partiel ; une
stat non partageable ou un trophée non débloqué est refusé, et rien n'est modifié).

`GET /profiles/{sub}` est la seule route `sablier:access` qui vise une autre personne. Elle ne sert que ce que la cible a partagé :
ses stats sur le mois en cours et au total (valeurs nulles omises), son premier jour si `usage.first_day` est partagée, ses trophées
partagés. Trois contrôles s'appliquent :

1. une stat ou un trophée non partagé n'est jamais servi ;
2. une stat ou un trophée d'Entracte ou de Sillon n'est servi que si le **lecteur** a `entracte:access` ou `sillon:access` ;
3. le lecteur doit pouvoir ouvrir le profil de la cible dans Estompe : Sablier le demande de serveur à serveur par
   `GET {SABLIER_ESTOMPE_URL}/internal/profile-visible?viewer=<sub>&target=<sub>` avec `Authorization: Bearer <SABLIER_ESTOMPE_TOKEN>`,
   réponse attendue `200 {"visible": true}` (3 secondes au plus). Toute autre réponse, Estompe injoignable ou variables non
   renseignées : la route répond comme s'il n'y avait rien de public (`200`, listes vides), jamais les données.

Lire son propre profil ne demande pas l'appel à Estompe et rend ce que les autres verraient.

## Administration

Routes `sablier:admin`. La configuration des trophées est décrite plus haut ; le reste :

**Vue d'une personne.** `GET /admin/people/{sub}/usage`, `/entracte`, `/sillon`, `/messaging` (mêmes `period` et `date`), `/trophies`,
`/streaks`, `/recaps` et `/recaps/{id}` rendent pour n'importe quel `sub` ce que la personne lit elle-même par les routes du même nom,
par les mêmes fonctions de lecture. Trois différences :

- l'admin voit tout : les statistiques, les trophées et les séries d'Entracte et de Sillon sont toujours servis, sans regarder
  `entracte:access` ni `sillon:access` (ceux de l'admin comme ceux de la personne) ;
- ces routes ne modifient rien d'autre que le journal : rien n'est marqué vu, et les déblocages ne sont pas recalculés avant la
  réponse (la tâche de fond s'en charge, 10 secondes au plus) ;
- un trophée caché non débloqué reste seulement compté (`hidden_remaining`) ; le catalogue complet est dans `GET /admin/trophies`.

Un `sub` inconnu répond comme une personne sans activité.

**Personnes.** `GET /admin/people` rend `{directory, people}` : les personnes qui ont des agrégats, avec `sub`, `name`, `known`,
`last_seen`, premier et dernier jour d'activité, temps d'utilisation cumulé ; triées par nom, les personnes sans nom à la fin.
`GET /admin/people?orphans=true` ne rend que les **personnes sans compte** (`known: false`). Cette liste n'est pas journalisée.

**Annuaire.** `name` et `known` (et `target_name` dans `GET /admin/audit`) viennent de l'un de deux annuaires, nommé par `directory` :

- `aegis` : Sablier demande les `sub` à aegis par `POST /v1/accounts/lookup` avec son jeton de service (`{"subs": […]}`, 500 par
  appel, réponse `{"accounts": [{sub, display_name, in_keycloak}]}`, comptes connus seulement). `known` vaut vrai si le compte est
  dans la réponse avec `in_keycloak` vrai : un compte absent, ou supprimé de Keycloak, est « sans compte ».
- `local` : aegis n'est pas configuré, ne répond pas, ou ne sert pas cette route (toute réponse autre qu'un succès). Sablier se replie
  sur sa table `personnes` : à chaque `GET /me` et à chaque battement, il note le nom de la session et l'instant, une écriture par heure
  et par personne au plus. `known` veut alors dire « Sablier a déjà vu cette personne connectée », pas « le compte existe encore » :
  un compte supprimé après avoir utilisé l'app reste connu, avec un `last_seen` qui ne bouge plus. L'écran doit prévenir que la liste
  des personnes sans compte est approximative.

`POST /v1/accounts/lookup` n'existe pas encore dans aegis : tant qu'elle n'y est pas, `directory` vaut toujours `local`. Les autres
routes de service d'aegis ne conviennent pas (`/v1/effective` répond pour n'importe quel `sub` et l'inscrit dans l'annuaire,
`/v1/check` aussi sans l'inscrire ; `/v1/admin/accounts` demande une session d'administrateur d'aegis). `last_seen` vient toujours
de la table locale. L'effacement est le même pour toutes les personnes listées.

**Totaux.** `GET /admin/totals?period=&date=` rend pour la période les personnes actives (personnes distinctes avec du temps
d'utilisation ou une action ; les réactions reçues ne comptent pas), le temps d'utilisation, le temps regardé (`entracte.secondes`),
le temps d'écoute (`sillon.secondes`) et les messages, au total et par jour. Aucune personne n'est visée : pas de ligne au journal.

**Effacement.** `DELETE /admin/people/{sub}/data` a le même effet que l'effacement fait par la personne (voir plus bas), pour
n'importe quel `sub`, avec ou sans compte.

## Journal des consultations

Chaque lecture d'une route `/admin/people/{sub}/…` par un admin ajoute une ligne à `consultations` : l'admin (`sub` et nom, figé au
moment de la consultation), la personne consultée, la date. La ligne est écrite avant la lecture : si le journal ne peut pas être
écrit, la route répond 500 et ne sert rien.

- **Regroupement.** L'écran d'une personne charge une dizaine de routes d'un coup : pas de nouvelle ligne tant que le même admin a
  une ligne `consultation` pour la même personne depuis moins de 10 minutes (`journal::REGROUPEMENT`). Un verrou par couple admin et
  personne garantit une seule ligne même quand les requêtes arrivent ensemble. Le journal dit donc « cet admin a consulté cette
  personne à cette heure », pas quelle statistique.
- Un admin qui consulte ses propres stats, par ses routes ou par `/admin/people/<son sub>/…`, n'est pas journalisé.
- Un effacement fait par un admin ajoute toujours une ligne, d'action `effacement`, dans la transaction de l'effacement.
- `GET /admin/audit?before=<id>&limit=` (`sablier:admin`) rend le journal complet, du plus récent au plus ancien, par pages (50 par
  défaut, 200 au plus ; `next_before` donne la suite), avec le nom connu de chaque personne consultée (annuaire d'aegis, ou local en
  repli : champ `directory`).
- `GET /views` (`sablier:access`) rend à chacun « qui a consulté mes stats » : date, nom de l'admin et action, 500 lignes au plus,
  sans le `sub` de l'admin.

Le journal n'est pas une donnée de la personne consultée : il n'est ni purgé ni effacé avec elle.

## Export

`GET /export` (`sablier:access`) rend un fichier JSON (`Content-Disposition: attachment; filename="sablier-export-AAAA-MM-JJ.json"`)
avec tout ce que Sablier garde de la personne de la session : agrégats par jour (`aggregates`, sous leurs noms de stockage), trophées
débloqués (`trophies`, un par palier), récaps figés (`recaps`), réglages de partage (`sharing`), évènements bruts encore conservés
(`events`), consultations par les admins (`views`) et état de la collecte (`name`, `last_seen`, `last_heartbeat`, `entracte_session`,
`pending_evaluation`). Chaque table de `tables_personnelles()` a sa section (`export::sections()`, vérifié par un test), et l'export ne
contient aucune donnée d'une autre personne.

Les évènements bruts sont lus par pages de 1 000 (`export::PAGE`) et envoyés au fil de la lecture : leur nombre ne pèse pas sur la
mémoire. Le reste (agrégats compris) est chargé d'un bloc. Une panne de base pendant l'envoi coupe la réponse, que le front doit alors
refuser (JSON incomplet).

## Effacement

`DELETE /data` (`sablier:access`) avec le corps `{"confirm": "EFFACER"}` supprime définitivement, en une transaction
(`effacement::effacer`), toutes les lignes de la personne dans les tables de `tables_personnelles()` : évènements bruts, agrégats,
trophées débloqués, récaps, réglages de partage, session Entracte en cours, dernier battement, ligne d'annuaire. Toute autre valeur de
`confirm` : 400, rien n'est effacé. `DELETE /admin/people/{sub}/data` (`sablier:admin`, même corps) fait la même chose pour une autre
personne.

- Après l'effacement, toutes les routes de lecture répondent comme pour une personne sans activité, et le profil ne montre plus rien.
- La collecte reprend à zéro : le battement suivant ne compte rien (il sert de point de départ), un évènement arrivé ensuite est
  compté normalement, le premier jour d'activité est celui de la reprise.
- L'historique d'Entracte, de Sillon et les messages ne sont pas touchés. Le journal des consultations est conservé.
- **Import après effacement.** La déduplication repose sur les évènements bruts, qui sont supprimés : un import d'historique rejoué
  après l'effacement est compté de nouveau pour cette personne et lui rend ses anciennes statistiques. Ne pas relancer d'import
  d'Entracte, de Sillon ou d'Estompe après un effacement sans le vouloir.
- Un évènement ou une évaluation des trophées en cours de traitement au moment exact de l'effacement peut laisser une ligne ; un
  second effacement la supprime.

## Configuration

Valeurs non secrètes dans [`plbls.env`](../../plbls.env), secrets dans Vault ([`deploy/README.md`](../../deploy/README.md)).

| Variable | Rôle |
|---|---|
| `SABLIER_PORT` | port d'écoute (8080) |
| `SABLIER_ALLOWED_ORIGINS` | origines des fronts (CORS, CSRF) |
| `SABLIER_FUSEAU` | fuseau des journées (`Europe/Paris`) |
| `SABLIER_CONSERVATION_MOIS` | conservation des évènements bruts (13) |
| `SABLIER_BATTEMENT_SECONDES` | intervalle et plafond d'un battement (30, entre 5 et 3600) |
| `SABLIER_RECAP_JOURS_MIN`, `SABLIER_RECAP_JOURS_MIN_ANNEE` | jours avec une action réelle en dessous desquels il n'y a pas de récap du mois (3) ou de l'année (10) |
| `SABLIER_ESTOMPE_URL`, `SABLIER_ESTOMPE_TOKEN` | adresse interne de l'API d'Estompe et jeton de service pour la visibilité des profils ; sans les deux, `GET /profiles/{sub}` ne sert que son propre profil |
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
5. Profil public : `SABLIER_ESTOMPE_URL` (adresse interne de l'API d'Estompe) et `SABLIER_ESTOMPE_TOKEN` (secret partagé avec Estompe,
   qui l'attend sur `GET /internal/profile-visible`).
6. Reverse proxy : domaine Dokploy `sablier-api.plbls.fr` vers `sablier-api` port 8080 (`SABLIER_PUBLIC_URL`).

## Développement

`just dev-up` puis `just sablier-dev-run` (127.0.0.1:8095, avec le gateway et aegis). Tests : `just test-sablier`, voir
[`api/sablier`](../../api/sablier/README.md).
