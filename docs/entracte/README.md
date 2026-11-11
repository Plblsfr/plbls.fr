# Entracte

Films et séries, à part d'Estompe : une personne peut avoir accès à Entracte sans avoir accès à la messagerie. Entracte lit son
catalogue dans Radarr et Sonarr, décrit les titres avec TMDB, lit et transcode les fichiers lui-même (ffmpeg), garde la
progression et l'historique de chacun, reçoit les demandes de titres absents et prévient les demandeurs par notification push.
On regarde à plusieurs dans le salon cinéma d'Estompe, qui lit les titres d'Entracte.

| Où | Quoi |
|---|---|
| [`api/entracte`](../../api/entracte/README.md) | API Rust (axum, sqlx), membre du workspace Cargo |
| [`openapi.yaml`](openapi.yaml) | routes, vérifiées par un test (chaque route du code y figure, et rien d'autre) |
| [`transcodage.md`](transcodage.md) | lecture directe, HLS, paliers, accélération matérielle |
| [`import-estompe.md`](import-estompe.md) | import unique des progressions et « déjà vus » de l'ancien bloc films d'Estompe (`entracte importer-estompe`), à lancer avant le déploiement du retrait de ce bloc |
| [`deploy/entracte/.env.example`](../../deploy/entracte/.env.example) | variables, service `entracte-api` de [`deploy/docker-compose.yml`](../../deploy/docker-compose.yml) |
| `web/estompe/src/sections/entracte/` | section Entracte de l'app (web, PWA, Windows, Android) |

## Permissions

Déclarées à aegis par manifeste (application `entracte`), toutes fermées par défaut :

| Permission | Ouvre |
|---|---|
| `entracte:access` | bibliothèque, lecture, reprise, historique, recherche, notifications |
| `entracte:request` | demander un titre absent, « Mes demandes » |
| `entracte:admin` | liste et traitement des demandes, fichiers écartés, état des synchronisations |

Chaque route relève d'une permission ou d'une exception motivée (santé, `GET /me`, flux signés, webhooks, appel interne d'Estompe) :
la table des routes l'impose (`api/entracte/src/routes.rs`) et un test parcourt toutes les routes (401 sans session, 403 sans la permission).
`GET /me` répond sans permission Entracte : l'app y lit les sections à afficher.

## Catalogue

- Synchronisation périodique (`ENTRACTE_SYNC_MINUTES`, 15 par défaut), par les webhooks Radarr/Sonarr (`POST /webhooks/radarr`, jeton `ENTRACTE_WEBHOOK_TOKEN` en mot de passe
  du Basic auth (champs Username/Password du webhook, nom libre), en `Authorization: Bearer` ou, à éviter car journalisé, en `?token=…` ;
  type « Webhook », évènements import, renommage et suppression) et par le bouton d'administration. Une seule à la fois ; une demande reçue pendant une synchronisation en relance une à la fin.
- Seuls les fichiers connus de Radarr (films) et Sonarr (épisodes) entrent dans le catalogue ; un fichier présent sur le disque mais inconnu
  de Radarr/Sonarr est ignoré. Une série sans identifiant TMDB dans Sonarr est ignorée.
  Le chemin d'un fichier (traduit par `ENTRACTE_CHEMINS`) doit rester sous `ENTRACTE_MEDIAS` une fois les liens symboliques résolus,
  sinon il est ignoré et compté comme inaccessible.
- Chaque fichier passe le filtre (extensions autorisées, double extension, conteneur incohérent, absence de flux vidéo, nom suspect) puis
  ffprobe ; un fichier refusé va dans « fichiers écartés » avec sa raison (nom seul, jamais le chemin).
- Métadonnées et affiches TMDB gardées en base : un titre dont le fichier disparaît passe « absent » sans perdre son titre, son affiche ni
  son historique. Les images sont servies par Entracte depuis son cache, jamais par une URL tierce.
- Radarr, Sonarr ou TMDB injoignable, ou dossier des médias inaccessible (aucun fichier lisible) : rien n'est retiré, la bibliothèque déjà synchronisée reste servie et l'erreur s'affiche dans
  Administration › état (`GET /admin/status`).
- Identifiants publics propres à Entracte (UUID) : aucune réponse ne contient de chemin de fichier, d'identifiant Radarr/Sonarr ni d'URL TMDB.
  L'identifiant TMDB d'un titre, lui, est public (recherche et demandes).

Attribution : l'app affiche « Données et images fournies par TMDB » (conditions d'utilisation de l'API TMDB).

## Progression et historique

Position enregistrée par personne et par titre. Au-delà de 90 % de la durée, le titre est vu : une entrée datée par titre, que revoir le titre
met à jour. « Reprendre » liste les titres commencés et l'épisode suivant d'une série en cours. L'historique est lié au titre, pas au
fichier. Chacun ne lit et n'écrit que les siens (la personne est celle de la session, jamais un paramètre).

## Demandes

- Recherche unique : titres présents et résultats TMDB absents, distingués, avec l'état d'une éventuelle demande.
  Les résultats TMDB dont le titre (ou le titre original) contient les mots cherchés passent devant, puis les plus populaires.
  TMDB ne tolère pas les fautes de frappe : quand aucun titre ne correspond (« dardevil »), Entracte demande à TVmaze
  (recherche floue de séries, sans clé, `ENTRACTE_TVMAZE_URL`, `https://api.tvmaze.com` par défaut) les noms proches, relance TMDB
  avec les deux premiers et met ces résultats devant (« Daredevil », série et film). TVmaze ne reçoit que le texte cherché ;
  injoignable, il est ignoré et la recherche reste celle de TMDB.
- Demande (`entracte:request`) : un film, une série entière ou des saisons précises (y compris une saison manquante d'une série présente).
  Une même demande en cours n'est pas dupliquée : la personne y est rattachée et sera prévenue comme le premier demandeur.
- États : `en_attente`, `acceptee`, `refusee` (motif obligatoire), `disponible`.
- Accepter envoie le titre à Radarr (film surveillé, recherche lancée) ou à Sonarr (saisons demandées surveillées, recherche lancée),
  avec le profil de qualité et le dossier racine de la configuration (à défaut, les premiers déclarés). Un envoi en échec laisse la demande
  « acceptée » avec l'erreur, et « Accepter » renvoie.
- « Disponible » : quand la synchronisation voit arriver le fichier (film), ou tous les épisodes annoncés par TMDB des saisons demandées.
  Une série demandée entière se ferme quand toutes les saisons dont TMDB annonce des épisodes sont complètes ; une saison annoncée plus
  tard sans épisode ne la bloque pas, une saison en cours de diffusion la garde ouverte jusqu'à son dernier épisode annoncé.

## Notifications

Refus et disponibilité d'une demande : notification à chaque appareil abonné de chaque demandeur, et à personne d'autre.
Abonnements propres à Entracte (`/push/subscription`, `/push/fcm`) : une personne sans accès à Estompe est prévenue. Un abonnement refusé
par le service push (404, 410) est retiré. Seuls les services push connus sont acceptés (`fcm.googleapis.com`, `updates.push.services.mozilla.com`,
`*.notify.windows.com`, `web.push.apple.com`, en https sans port ni IP littérale) ; un abonnement enregistré hors de cette liste est retiré au
premier envoi sans être contacté. Au plus 10 abonnements par personne : le onzième remplace le plus ancien (réabonner un appareil le
rajeunit), pour qu'un changement d'appareil ne soit jamais refusé. Les envois partent en tâche de fond, 8 à la fois au plus : refuser une
demande ou la synchronisation du catalogue n'attendent pas les services push, dont le corps des réponses n'est jamais journalisé. Les clés VAPID et le compte Firebase sont ceux d'Estompe, variables communes
(`VAPID_*`, `FCM_SERVICE_ACCOUNT`) : un navigateur n'a qu'un abonnement push par application, que l'app enregistre
auprès des deux API, et l'app Android réutilise son jeton FCM. L'app Windows n'a pas de push : elle lit `GET /notifications` et affiche
elle-même les nouvelles.

## Salon cinéma d'Estompe

Estompe garde la séance (salon `cinema`, pilotée par l'hôte, évènement `cinema.state`) et vérifie le titre par
`GET /internal/titles/{id}?sub=` (jeton `ENTRACTE_SERVICE_TOKEN`, lu par les deux ; côté Estompe, Entracte est joint par `ENTRACTE_INTERNE_URL`) : titre
jouable, durée, épisodes suivants et droit d'accès de l'hôte. Chaque participant ouvre son propre flux auprès d'Entracte (qualité, piste et
sous-titres à lui) ; seule la position est synchronisée, et sa progression est enregistrée comme en lecture seule.

## Actions manuelles

- `plbls.fr-infra` : base `entracte` sur `postgres-main` (`CREATE DATABASE entracte OWNER plbls;`, propriétaire : l'utilisateur commun `DATABASE_USER`), sous-domaine
  `entracte-api.plbls.fr` et sa route, Radarr et Sonarr joignables depuis la stack (réseau `MEDIAS_NETWORK`), dossier des médias monté en
  lecture seule (`ENTRACTE_MEDIAS`, mêmes chemins que Radarr/Sonarr ou `ENTRACTE_CHEMINS`), périphérique de transcodage ([transcodage.md](transcodage.md)).
- Vault, `secret/entracte` : `AEGIS_TOKEN_ENTRACTE`, `ENTRACTE_STREAM_SECRET`, `ENTRACTE_SERVICE_TOKEN`,
  `ENTRACTE_RADARR_API_KEY`, `ENTRACTE_SONARR_API_KEY`, `ENTRACTE_TMDB_API_KEY`.
- aegis : rien à déclarer (il lit `AEGIS_TOKEN_ENTRACTE` comme application `entracte`), puis les permissions aux comptes.
- Radarr et Sonarr : webhook vers `https://entracte-api.plbls.fr/webhooks/radarr` (ou `/webhooks/sonarr`), jeton en mot de passe (facultatif).
- Front : `ENTRACTE_PUBLIC_URL` (web, image `estompe-web`) et `VITE_ENTRACTE_URL` au build des apps natives.

## Version

Composant `entracte` : version du `Cargo.toml` d'`api/entracte`, tag `entracte-vX.Y.Z` (image `ghcr.io/plblsfr/entracte`, `:edge` sur `main`).
Pas d'entrée dans `.version`, propre à Estompe. La section de l'app suit les versions de l'app (`estompe-app-v*`).
