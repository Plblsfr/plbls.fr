# Sillon

Service de musique de plbls.fr, à part d'Estompe : une personne peut avoir accès à Sillon sans avoir accès à la messagerie.
Sillon écoute la bibliothèque gérée par Lidarr ; playlists, ajout autonome de musique et aléatoire avec recommandation viennent
par lots (issue chapeau [#202](https://github.com/Plblsfr/plbls.fr/issues/202)). Ce document décrit ce qui existe aujourd'hui : le socle du service et le catalogue (bibliothèque synchronisée depuis Lidarr).

| Où | Quoi |
|---|---|
| [`api/sillon`](../../api/sillon/README.md) | API Rust (axum, sqlx), membre du workspace Cargo |
| [`openapi.yaml`](openapi.yaml) | routes, vérifiées par un test (chaque route du code y figure, et rien d'autre) |
| [`deploy/sillon/.env.example`](../../deploy/sillon/.env.example) | secrets, service `sillon-api` de [`deploy/docker-compose.yml`](../../deploy/docker-compose.yml) |

## Permissions

Déclarées à aegis par manifeste (application `sillon`, renvoyé toutes les 10 minutes), toutes fermées par défaut :

| Permission | Ouvre |
|---|---|
| `sillon:access` | bibliothèque, recherche, lecture, playlists, likes, file, historique |
| `sillon:add` | recherche d'un album ou d'un artiste absent et ajout ; découverte automatique |
| `sillon:admin` | occupation du disque, échecs, suppression, état des synchronisations et de l'analyse |

Chaque route relève d'une permission ou d'une exception motivée (santé, disponibilité, `GET /me`) : la table des routes l'impose
(`api/sillon/src/routes.rs`) et un test parcourt toutes les routes (401 sans session ou avec une session invalide, 403 sans la permission,
même avec les deux autres). aegis injoignable : 503 `aegis_unavailable` sur les routes soumises à une permission. Sans `AEGIS_TOKEN_SILLON`,
tout est refusé.

`GET /me` répond à toute personne connectée, sans permission Sillon : identité, permissions et `sections`, la liste de ce qu'elle peut
ouvrir (`library` avec `sillon:access`, `add` avec `sillon:add`, `admin` avec `sillon:admin`). aegis injoignable : `GET /me` répond quand
même, permissions toutes fausses et aucune section.

Authentification : cookie de session du gateway (`__Secure-plbls_sid`) ou `Authorization: Bearer`, vérifié par `GET /auth/me` du gateway
(cache de 30 s). Requête modifiante avec cookie : `Origin` dans `SILLON_ALLOWED_ORIGINS`, sinon 403 `bad_origin` ; CORS avec
identifiants pour ces seules origines.

## Catalogue

- Synchronisation périodique (`SILLON_SYNC_MINUTES`, 15 par défaut ; 0 = pas de synchronisation périodique), par le webhook Lidarr
  (`POST /webhooks/lidarr`, jeton `SILLON_WEBHOOK_TOKEN` en mot de passe du Basic auth (champs Username/Password du webhook, nom libre),
  en `Authorization: Bearer` ou, à éviter car journalisé, en `?token=…` ; type « Webhook », évènements import, renommage et suppression ;
  les évènements Test, Grab, santé, échecs et mise à jour sont ignorés) et par le bouton d'administration (`POST /admin/sync`).
  Une seule à la fois ; une demande reçue pendant une synchronisation en relance une à la fin. Sans `SILLON_WEBHOOK_TOKEN`, le webhook répond 404.
- Seuls les fichiers connus de Lidarr entrent (`/api/v1/artist`, `/album`, `/track`, `/trackfile`, clé `SILLON_LIDARR_CLE` dans `X-Api-Key`) :
  un fichier présent sur le disque mais inconnu de Lidarr est ignoré, une piste sans MBID d'enregistrement aussi. Artistes, albums et pistes
  gardent leurs MBID (artiste, album, enregistrement et piste), numéro de disque et de piste, durée, année et date de sortie, type et types
  secondaires d'album, genres (artiste et album).
- Le chemin donné par Lidarr est traduit par `SILLON_CHEMINS` et doit rester sous `SILLON_MEDIAS` une fois les liens symboliques résolus,
  sinon le fichier est écarté (`hors_medias`). Chaque fichier passe ensuite le filtre (extension audio autorisée : flac, mp3, ogg, oga, opus,
  m4a, aac, wav, aif, aiff, wv, ape ; double extension ; nom suspect ; conteneur incohérent avec l'extension ; absence de flux audio) puis
  ffprobe (conteneur, codec, débit, fréquence, canaux, bits, durée), relancé seulement quand la taille ou la date de modification changent.
  Un fichier refusé va dans les fichiers écartés (`GET /admin/rejected`) avec sa raison, nom seul, jamais le chemin.
- Un fichier qui disparaît rend sa piste « absente » (`available: false`) sans la supprimer : playlists et historique la gardent. Elle se
  rattache, avec le même identifiant public, quand un fichier revient pour le même enregistrement (MBID) dans le même album. Un album ou un
  artiste sans piste présente passe absent de la même façon.
- Pochettes et images d'artistes (portrait, fond) : téléchargées depuis Lidarr (`/api/v1/mediacover/…`, 10 Mo au plus, JPEG, PNG, GIF ou
  WebP), gardées en base (table `images`, renouvelées quand Lidarr change l'image) et servies par Sillon en trois tailles (`small`, `medium`,
  `large`), jamais par une URL tierce. Lidarr injoignable : l'image en cache est servie, sinon 502.
- Lidarr injoignable ou en erreur, ou dossier de la musique inaccessible (aucun fichier lisible) : rien n'est retiré, la bibliothèque déjà
  synchronisée reste servie et l'erreur s'affiche dans l'état (`GET /admin/status`).
- Identifiants publics propres à Sillon (UUID) : aucune réponse ne contient de chemin de fichier ni d'identifiant Lidarr.

Routes (`sillon:access`) : artistes présents (`GET /artists`, par nom sans accents, filtre `genre`), fiche artiste et tous ses albums,
fiche album et ses pistes (absentes comprises), piste, ajouts récents (`GET /albums/recent`, albums par date d'arrivée), genres, images ;
pagination par curseur (`limit`, `cursor` = `next_cursor` de la page précédente). Recherche (`GET /search?q=`) : artistes, albums et pistes
présents, séparément, plein texte PostgreSQL sans accents ni casse (configuration `sillon` : `unaccent` puis `simple`) ou sous-chaîne
(`LIKE` échappé par `commun::sql`) : « beyonce » trouve « Beyoncé ». Administration (`sillon:admin`) : état des synchronisations et
compteurs de la bibliothèque, fichiers écartés, lancement manuel.

## Variables

| Variable | Source | Rôle |
|---|---|---|
| `SILLON_PORT` | `plbls.env` (8080) | port d'écoute |
| `SILLON_ALLOWED_ORIGINS` | `plbls.env` | origines des fronts (CORS, CSRF) |
| `AEGIS_TOKEN_SILLON` | Vault `secret/sillon` | jeton de l'application `sillon` chez aegis |
| `SILLON_LIDARR_URL` | `plbls.env` (`http://lidarr:8686`) | Lidarr, joint par le réseau de la stack média (`MEDIAS_NETWORK`) |
| `SILLON_LIDARR_CLE` | Vault `secret/sillon` | clé d'API de Lidarr ; sans elle (ou sans URL), pas de catalogue |
| `SILLON_WEBHOOK_TOKEN` | Vault `secret/sillon` | jeton du webhook Lidarr ; vide = webhook coupé (404) |
| `SILLON_MEDIAS` | `plbls.env` (`/srv/musique`) | dossier de la musique, monté en lecture seule au même chemin par le compose |
| `SILLON_CHEMINS` | `plbls.env` (vide) | correspondances `distant=local` séparées par des virgules, quand Lidarr voit la musique ailleurs |
| `SILLON_SYNC_MINUTES` | `plbls.env` (15) | période de synchronisation ; 0 = webhook et bouton seulement |
| `SILLON_FFPROBE` | `plbls.env` (`ffprobe`) | ffprobe de l'image |
| `DATABASE_URL`, `DATABASE_USER`, `DATABASE_PASSWORD`, `GATEWAY_URL`, `GATEWAY_INTERNE_URL`, `AEGIS_URL` | communes ([`deploy/README.md`](../../deploy/README.md)) | base `sillon` sur le serveur commun, gateway, aegis |

## Actions manuelles

- `plbls.fr-infra` : base `sillon` sur `postgres-main` (`CREATE DATABASE sillon OWNER plbls;`), sous-domaine `sillon-api.plbls.fr` et sa route.
- Vault, `secret/sillon` : `AEGIS_TOKEN_SILLON`, `SILLON_LIDARR_CLE`, `SILLON_WEBHOOK_TOKEN`.
- `plbls.fr-infra` : dossier de la musique monté en lecture seule (`SILLON_MEDIAS`, mêmes chemins que Lidarr ou `SILLON_CHEMINS`),
  Lidarr joignable depuis la stack (réseau `MEDIAS_NETWORK`). La migration crée l'extension `unaccent` (extension de confiance : le
  propriétaire de la base suffit).
- Lidarr : connexion « Webhook » vers `https://sillon-api.plbls.fr/webhooks/lidarr`, évènements import, renommage et suppression, jeton en mot de passe.
- aegis : rien à déclarer (il lit `AEGIS_TOKEN_SILLON` comme application `sillon`), puis les permissions aux comptes.

## Version

Composant `sillon` : version du `Cargo.toml` d'`api/sillon`, tag `sillon-vX.Y.Z` (image `ghcr.io/plblsfr/sillon`, `:edge` sur `main`).
Pas d'entrée dans `.version`, propre à Estompe.
