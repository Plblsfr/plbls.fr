# Sillon

Service de musique de plbls.fr, à part d'Estompe : une personne peut avoir accès à Sillon sans avoir accès à la messagerie.
Sillon écoute la bibliothèque gérée par Lidarr ; playlists, ajout autonome de musique et aléatoire avec recommandation viennent
par lots (issue chapeau [#202](https://github.com/Plblsfr/plbls.fr/issues/202)). Ce document décrit ce qui existe aujourd'hui : le socle du service, le catalogue (bibliothèque synchronisée depuis Lidarr),
la lecture audio, les playlists, les likes, la file de lecture, l'historique, l'ajout autonome de musique avec plafond du disque, l'analyse audio des pistes et l'interface web.

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

## Lecture

- `POST /tracks/{id}/playback` (`sillon:access`) reçoit ce que le client lit (`codecs`, `containers`) et la qualité voulue (`quality` :
  `originale`, `haute`, `normale`, `basse` ; sans elle, la préférence de la personne, `GET`/`PUT /preferences`, `originale` par défaut).
  Elle renvoie une URL de flux signée (`/stream/{jeton}`, valable 6 h), le format servi, la durée exacte et les gains de volume.
- Lecture directe : le fichier d'origine, avec `Range` et son `Content-Type` exact (`audio/flac`, `audio/mpeg`, `audio/ogg; codecs=…`,
  `audio/mp4`…), quand le codec et le conteneur sont annoncés lisibles et que la qualité est `originale` ; aussi pour une source avec
  perte qui ne dépasse pas le palier demandé (un MP3 à 128 kb/s en `normale`).
- Sinon, transcodage par ffmpeg du fichier entier (pas de HLS) : Opus dans Ogg si le client annonce `opus` et `ogg`, AAC (ADTS) sinon,
  à 96 (`basse`), 160 (`normale`) ou 256 kb/s (`haute` ; `originale` quand la lecture directe est impossible), stéréo au plus. Le
  résultat va dans le cache disque (`SILLON_CACHE`, plafond `SILLON_CACHE_GO`, éviction du moins récemment lu, jamais d'un fichier en
  cours de lecture ou d'encodage) et y est servi avec `Range` ; une deuxième demande ne relance pas ffmpeg. L'encodage démarre dès la
  demande de lecture (la piste suivante peut être demandée d'avance pour le préchargement) ; pendant l'encodage, le flux est servi au fil
  de l'eau (200 sans `Content-Length`), et un échec coupe la connexion plutôt que de livrer un fichier tronqué. Au-delà de
  `SILLON_MAX_TRANSCODAGES` encodages simultanés : 503 `server_busy` avec `Retry-After`.
- Jeton de flux : signé par HMAC-SHA256 (`SILLON_STREAM_SECRET`), il porte la personne, la piste, le fichier, le format et l'expiration ;
  il ne sert que sa piste (fichier changé depuis : 404, redemander la lecture). Les droits (`sillon:access`) sont revérifiés à chaque
  requête ; une session d'une autre personne présentée avec le jeton : 403. CORS ouvert (`*`, sans identifiants) sur `/stream/`, pour
  `<audio>` et Web Audio depuis toute origine.
- Sans blanc : une piste = un fichier complet. La réponse donne `samples` et `sample_rate`, durée exacte de la source en échantillons
  (comptée au décodage, la même en transcodage), `output_sample_rate` et `encoder_delay`, échantillons à retirer au début du flux décodé
  (1024 en AAC ; 0 en Opus, dont le pre-skip est retiré par le décodeur, et en direct).
- Volume : niveau intégré, plage et pic réel (EBU R128, filtre `ebur128` de ffmpeg) mesurés à la première lecture d'un fichier, puis les
  autres pistes de l'album en tâche de fond, gardés en base (table `mesures`, par piste et `contenu_le` du fichier : un fichier remplacé
  est remesuré). La réponse donne les gains piste et album (dB) pour atteindre -18 LUFS et les pics ; le client applique. Gain d'album :
  moyenne énergétique, pondérée par la durée, des pistes déjà mesurées de l'album.

## Playlists, likes, file de lecture et historique

Tout est privé et propre à la personne (`sub` du gateway), sous `sillon:access` ; la playlist, la file ou l'élément d'une autre personne
répond 404. Une piste retirée du catalogue n'est jamais supprimée (elle passe absente) : elle reste dans les playlists, la file, les likes
et l'historique, avec `available: false`.

- **Playlists** (`/playlists`) : créer, renommer, décrire, supprimer ; ajouter à la fin des pistes, un album, les pistes présentes d'un
  artiste ou une autre playlist de la personne (copie) ; retirer un élément ; déplacer une plage (`from`, `count`, `to`). Chaque élément a
  son identifiant (une piste peut y être plusieurs fois) et sa position, de 0 à n-1 sans trou ni doublon : chaque modification verrouille
  la playlist, les appels simultanés passent l'un après l'autre. Doublons : écartés et signalés (`duplicates`), ajoutés avec
  `allow_duplicates: true`. Pochette : `cover_albums`, les quatre premiers albums (avec pochette) dans l'ordre de la playlist, que le
  client compose à partir de `/albums/{id}/cover`. Plafonds : 500 playlists par personne, 10 000 pistes par playlist, nom de 1 à 100
  caractères, description de 1 000 ; dépassement : 422 `limit_reached` (ou 400 `invalid` pour une longueur), avec un message lisible.
- **Likes** (`/likes`) : piste, album ou artiste (`PUT` et `DELETE /likes/{tracks|albums|artists}/{id}`, idempotents ; 50 000 par type).
  `GET /likes` donne les identifiants, `GET /likes/tracks` « Titres likés » sous la forme d'une playlist (du plus récent au plus ancien).
- **File de lecture** (`/queue`) : une par personne, en base, rendue à l'identique sur tous ses appareils : source (`album`, `playlist`,
  `artist`, `likes`, `search`), éléments dans l'ordre de lecture, index courant, position dans la piste, aléatoire, répétition (`off`,
  `all`, `one`), réglage `suggestions` (`off`, `light`, `frequent`, gardé sans effet pour l'instant), graine du mélange (`seed`) et, par
  élément, `suggested` (toujours faux pour l'instant). « Lire ensuite » insère après la piste courante, « ajouter à la file » à la fin.
  Chaque écriture porte la `revision` de la dernière file reçue : si un autre appareil a écrit entre-temps, 409 `conflict` avec la file
  courante (`queue`), rien n'est écrit ; le client recommence sur cet état. Le client envoie sa position (`PATCH /queue`) à intervalle et
  à chaque changement de piste ; `step` passe à la piste disponible suivante ou précédente en sautant les absentes. Aléatoire : la piste
  courante passe en tête, le reste est mélangé par `ordre::melanger` (mélange uniforme, déterministe pour une graine) ; le désactiver rend
  l'ordre de la source. 10 000 éléments au plus.
- **Historique** (`/history`) : le client envoie une écoute à la fin de chaque lecture (piste, secondes écoutées, `ended_by`, source,
  élément de la file). Fin : `completed` (fin atteinte), `skipped` (sortie avant la fin vers une autre piste) ou `interrupted` (arrêt).
  Une écoute compte (`counted`) au-delà de 30 s ou de la moitié de la piste ; seules celles-là entrent dans les pistes et artistes les
  plus écoutés (`/history/top/tracks`, `/history/top/artists`, période `week`, `month`, `year` ou `all`). Écoutes récentes paginées par curseur.

## Ajout de musique

- Recherche (`GET /additions/search?q=`, `sillon:add`) : artistes et albums par la recherche de Lidarr (MusicBrainz, `/artist/lookup`,
  `/album/lookup`), 20 au plus chacun, marqués « déjà en bibliothèque » (`in_library`), « en cours » (`in_progress`) ou exclus, avec la
  taille estimée de chaque album. Identifiant : MBID MusicBrainz.
- Ajout (`POST /additions`, `{kind: album|artist, mbid}`), sans validation : envoyé tout de suite à Lidarr. Artiste créé si besoin
  (`POST /artist` : `SILLON_LIDARR_RACINE`, `SILLON_LIDARR_QUALITE`, `SILLON_LIDARR_METADONNEES`, `monitored`, `addOptions.monitor: none`),
  puis seul l'album demandé est surveillé (`PUT /album/monitor`) et sa recherche lancée (`POST /command` `AlbumSearch`). Ajout d'un artiste :
  ses albums studio seulement (type `Album`, sans type secondaire). Albums pas encore connus de Lidarr juste après la création de l'artiste :
  le suivi les surveille et lance la recherche dès qu'ils apparaissent (`RefreshArtist` demandé si l'artiste existait déjà). Lidarr en erreur
  pendant l'envoi : 502, rien n'est enregistré. Un même titre déjà en cours renvoie l'ajout existant (`existing: true`) ; un album présent, 409
  `already_available` ; un album ou un artiste supprimé par un admin, 409 `excluded`.
- Limite : `SILLON_AJOUTS_PAR_JOUR` ajouts par personne sur 24 heures glissantes (429 `daily_limit`).
- Plafond : `SILLON_MEDIAS_GO`. Occupation = taille des fichiers du catalogue (relevée à chaque synchronisation) + estimation des ajouts en
  cours (envoyés, en téléchargement, partiels), moins ce qui en est déjà arrivé. Estimation = durée de l'album (ou des albums studio
  manquants d'un artiste ; une heure si inconnue) × débit de la meilleure qualité permise par le profil de qualité (`GET /qualityprofile` :
  FLAC 1000 kbit/s, 24 bits 2304, WAV 1411, sinon le nombre du nom, MP3-320 → 320). Un ajout qui ferait dépasser le plafond est refusé (507
  `disk_full` avec `used`, `pending`, `limit`, `free`, `estimated`) et rien n'est envoyé à Lidarr. Limite et plafond sont vérifiés sous un
  verrou PostgreSQL (`pg_advisory_xact_lock`) dans la transaction qui enregistre l'ajout : deux ajouts simultanés ne dépassent pas le plafond à eux deux.
- Suivi : état `sent`, `downloading` (dans la file de Lidarr, `GET /queue`), `partial` (des fichiers arrivés, rien dans la file),
  `available` (tous les albums visés complets dans Lidarr et présents dans Sillon), `failed` (avec la raison : message de l'historique de
  Lidarr, `GET /history`, évènement `downloadFailed`, ou « aucun résultat après N h » passé `SILLON_AJOUTS_DELAI_HEURES`), `deleted`.
  Mis à jour après chaque webhook de Lidarr (import, grab, échecs ; après la synchronisation qu'il déclenche) et toutes les 5 minutes.
  « Mes ajouts » : `GET /additions/mine`.
- Administration (`sillon:admin`) : occupation et plafond (`GET /admin/disk`), ajouts de tous avec leur auteur (`GET /admin/additions`,
  filtre `state`), relance d'un échec (`POST /admin/additions/{id}/retry`, plafond vérifié), suppression d'un album ou d'un artiste
  (`DELETE /admin/albums/{id}`, `DELETE /admin/artists/{id}` : Lidarr supprime fichiers et entrée, `deleteFiles=true`,
  `addImportListExclusion=true` ; Sillon l'exclut de tout nouvel ajout puis synchronise : ses pistes passent absentes, sans être supprimées
  des playlists), albums présents triés par taille avec l'auteur de leur ajout (`GET /admin/albums` ; `never_played=true` : seulement ceux dont aucune piste
  n'a d'écoute comptée dans l'historique, toutes personnes confondues).
- Sillon ne supprime jamais un fichier lui-même (dossier monté en lecture seule) : tout passe par Lidarr.

## Analyse audio

- Chaque fichier (celui de la piste, le plus gros s'il y en a plusieurs) est décodé par ffmpeg en PCM mono 22 050 Hz sur sa sortie
  standard, lu au fil de l'eau (mémoire bornée, sauf l'enveloppe d'attaques : 4 octets par tranche de 23 ms), et les descripteurs sont
  calculés en Rust, sans bibliothèque native, hors du fil de l'exécuteur : tempo (autocorrélation de l'enveloppe d'attaques, de 50 à
  220 battements par minute, préférence autour de 120), énergie (niveau efficace, dB) et dynamique (écart type du niveau des trames à
  moins de 20 dB du niveau moyen), centroïde, étalement et pente du spectre, taux de passage par zéro, chroma sur 12 classes
  (tonalité et mode estimés par corrélation avec les profils de Krumhansl), moyennes et variances de 12 coefficients cepstraux (26
  bandes mel). Trames de 2048 échantillons tous les 512 ; les moyennes sont pondérées par le niveau de la trame, pour que le bruit
  d'encodage dans les silences ne compte pas.
- Le résultat est un vecteur de 43 dimensions à l'échelle nominale (table `analyses`, avec le tempo, la tonalité et le mode),
  versionné : `analyse::VERSION` monte à chaque changement d'algorithme, et les pistes analysées avec une version plus ancienne
  repassent en file.
- File de tâches en base (`taches_analyse`, une ligne par fichier : `attente`, `en_cours`, `faite`, `erreur`, tentatives, erreur) :
  alimentée à la fin de chaque synchronisation (fichier nouveau, `contenu_le` changé ou version plus ancienne), au démarrage et toutes
  les 5 minutes ; les fichiers les plus récents passent d'abord, le stock existant ensuite. `SILLON_ANALYSE_PARALLELE` ouvriers (1 par
  défaut, 0 = pas d'analyse), ffmpeg lancé par `nice -n 19` sur un seul fil. Une tâche = les descripteurs, puis la mesure du niveau
  EBU R128 si elle manque (deuxième décodage : la mesure compte les échantillons à la fréquence d'origine) : toute la bibliothèque
  est ainsi mesurée, pas seulement les pistes déjà lues. Trois tentatives, puis `erreur` (les tâches déjà essayées passent après les
  autres : un fichier illisible ne bloque pas la file). Une analyse interrompue par un arrêt du service est reprise au démarrage.
- Pendant un transcodage de lecture, aucune analyse ne démarre, et celle en cours est arrêtée puis remise en file sans compter de
  tentative.
- Voisins : les vecteurs des pistes présentes sont chargés en mémoire (`AppState::analyse().voisins()`), chaque dimension centrée et
  réduite par sa moyenne et son écart type sur la bibliothèque (écart type plancher 0,1), recalculés à chaque rechargement : au
  démarrage, toutes les 100 analyses et quand la file se vide. Distance : écart quadratique moyen par dimension. Une piste non
  analysée n'a pas de voisins (`None`), jamais d'erreur.
- Administration (`sillon:admin`) : `GET /admin/analysis` (avancement, pause, erreurs sans chemin ni nom de fichier) et
  `POST /admin/analysis/retry` (remet en file les tâches en erreur).

## Interface

Section Sillon de l'app plbls.fr ([`web/estompe/src/sections/sillon`](../../web/estompe/README.md)), affichée à qui a `sillon:access`
d'après `GET /me` ; sans messagerie, l'app s'ouvre directement sur elle. Vues : accueil (reprise, ajouts récents), artistes, fiche artiste,
fiche album, recherche, écran « pas d'accès ». Un moteur de lecture unique : deux éléments audio (la piste suivante est demandée et chargée
pendant la piste en cours), gain de normalisation appliqué par Web Audio, formats annoncés d'après `canPlayType`, qualité réglable
(`PUT /preferences`), bouton « Activer le son » si la lecture automatique est refusée, Media Session. Le mini-lecteur reste dans la
messagerie et dans Entracte ; un film d'Entracte ou un salon vocal qui démarre met la musique en pause. La file est locale au lecteur
(album ou liste en cours) ; la reprise est gardée dans le navigateur.

## Carte de lien dans Estompe

Seul lien entre Sillon et Estompe : un lien vers la section Sillon de l'app (`https://estompe.plbls.fr/#/musique/<album|artist|track>/<id>`,
une origine de `ALLOWED_ORIGINS` d'Estompe) collé dans un message devient une carte. Estompe appelle `GET /internal/preview?type=&id=&sub=`
(jeton `SILLON_SERVICE_TOKEN`, lu par les deux ; côté Estompe, Sillon est joint par `SILLON_INTERNE_URL`) avec le `sub` de l'auteur du
message : titre, artiste, type, album et image à afficher, seulement si l'auteur a `sillon:access` (sinon 403, avant toute lecture du
catalogue, et pas de carte). Une playlist, privée, ne donne jamais de carte : la route répond 404 sans lire `sub` ni la base, qu'elle
existe ou non. Le bouton « Écouter » (fiche album, ou artiste) n'est montré qu'à un lecteur qui a lui-même `sillon:access`.

Pochette : Estompe ne la télécharge ni ne la relaie. La carte ne porte que l'identifiant de l'image ; l'app du lecteur la demande à Sillon
(`/albums/{id}/cover`, `/artists/{id}/image`) avec sa propre session, comme dans la section. Les pochettes restent réservées à
`sillon:access` : un lecteur sans accès voit la carte (titre, artiste) avec la vignette de repli.

## Variables

| Variable | Source | Rôle |
|---|---|---|
| `SILLON_PORT` | `plbls.env` (8080) | port d'écoute |
| `SILLON_ALLOWED_ORIGINS` | `plbls.env` | origines des fronts (CORS, CSRF) |
| `AEGIS_TOKEN_SILLON` | Vault `secret/sillon` | jeton de l'application `sillon` chez aegis |
| `SILLON_LIDARR_URL` | `plbls.env` (`http://lidarr:8686`) | Lidarr, joint par le réseau de la stack média (`MEDIAS_NETWORK`) |
| `SILLON_LIDARR_CLE` | Vault `secret/sillon` | clé d'API de Lidarr ; sans elle (ou sans URL), pas de catalogue |
| `SILLON_WEBHOOK_TOKEN` | Vault `secret/sillon` | jeton du webhook Lidarr ; vide = webhook coupé (404) |
| `SILLON_SERVICE_TOKEN` | Vault `secret/sillon` | Bearer de `GET /internal/preview`, envoyé par Estompe (lu par les deux) ; vide = route coupée (404), pas de carte |
| `SILLON_INTERNE_URL` | `plbls.env` (`http://sillon-api:8080`) | Sillon joint par Estompe de serveur à serveur (cartes de liens) ; vide = pas de carte |
| `SILLON_MEDIAS` | `plbls.env` (`/srv/musique`) | dossier de la musique, monté en lecture seule au même chemin par le compose |
| `SILLON_CHEMINS` | `plbls.env` (vide) | correspondances `distant=local` séparées par des virgules, quand Lidarr voit la musique ailleurs |
| `SILLON_SYNC_MINUTES` | `plbls.env` (15) | période de synchronisation ; 0 = webhook et bouton seulement |
| `SILLON_FFPROBE` | `plbls.env` (`ffprobe`) | ffprobe de l'image |
| `SILLON_FFMPEG` | `plbls.env` (`ffmpeg`) | ffmpeg de l'image (transcodage, mesure du volume) |
| `SILLON_STREAM_SECRET` | Vault `secret/sillon` | clé de signature des URL de flux, 32 caractères au moins ; absente = clé aléatoire au démarrage |
| `SILLON_CACHE` | `plbls.env` (`/tmp/sillon`) | cache des fichiers transcodés, volume `sillon-cache` du compose |
| `SILLON_CACHE_GO` | `plbls.env` (10) | plafond du cache en Go |
| `SILLON_MAX_TRANSCODAGES` | `plbls.env` (2) | encodages simultanés au plus (1 au moins) |
| `SILLON_LIDARR_RACINE` | `plbls.env` (`/music`) | dossier racine de Lidarr des artistes ajoutés ; vide = pas d'ajout |
| `SILLON_LIDARR_QUALITE`, `SILLON_LIDARR_METADONNEES` | `plbls.env` (1) | identifiants des profils de qualité et de métadonnées (« albums studio ») de Lidarr ; 0 = pas d'ajout |
| `SILLON_AJOUTS_PAR_JOUR` | `plbls.env` (10) | ajouts par personne sur 24 heures glissantes |
| `SILLON_MEDIAS_GO` | `plbls.env` (500) | plafond du dossier de la musique, en Gio ; 0 = sans plafond |
| `SILLON_AJOUTS_DELAI_HEURES` | `plbls.env` (72) | délai sans résultat avant qu'un ajout passe en échec |
| `SILLON_ANALYSE_PARALLELE` | `plbls.env` (1) | analyses audio simultanées ; 0 = pas d'analyse |
| `SILLON_PUBLIC_URL` | `plbls.env` (`https://sillon-api.plbls.fr`) | URL publique de l'API pour le front (`estompe-web` au démarrage : `config.js` et CSP ; apps au build, `VITE_SILLON_URL` l'emporte) ; vide = pas de section |
| `DATABASE_URL`, `DATABASE_USER`, `DATABASE_PASSWORD`, `GATEWAY_URL`, `GATEWAY_INTERNE_URL`, `AEGIS_URL` | communes ([`deploy/README.md`](../../deploy/README.md)) | base `sillon` sur le serveur commun, gateway, aegis |

## Actions manuelles

- `plbls.fr-infra` : base `sillon` sur `postgres-main` (`CREATE DATABASE sillon OWNER plbls;`), sous-domaine `sillon-api.plbls.fr` et sa route.
- Vault, `secret/sillon` : `AEGIS_TOKEN_SILLON`, `SILLON_LIDARR_CLE`, `SILLON_WEBHOOK_TOKEN`, `SILLON_STREAM_SECRET`, `SILLON_SERVICE_TOKEN` (`openssl rand -hex 32`).
- `plbls.fr-infra` : dossier de la musique monté en lecture seule (`SILLON_MEDIAS`, mêmes chemins que Lidarr ou `SILLON_CHEMINS`),
  Lidarr joignable depuis la stack (réseau `MEDIAS_NETWORK`). La migration crée l'extension `unaccent` (extension de confiance : le
  propriétaire de la base suffit).
- Lidarr : connexion « Webhook » vers `https://sillon-api.plbls.fr/webhooks/lidarr`, évènements import, renommage et suppression, jeton en mot de passe.
- Lidarr : profil de métadonnées « albums studio », profil de qualité et dossier racine, leurs valeurs dans `SILLON_LIDARR_*` ;
  connexion « Webhook » avec aussi les évènements grab et échecs (téléchargement, import) pour le suivi des ajouts.
- aegis : rien à déclarer (il lit `AEGIS_TOKEN_SILLON` comme application `sillon`), puis les permissions aux comptes.

## Version

Composant `sillon` : version du `Cargo.toml` d'`api/sillon`, tag `sillon-vX.Y.Z` (image `ghcr.io/plblsfr/sillon`, `:edge` sur `main`).
Pas d'entrée dans `.version`, propre à Estompe.
