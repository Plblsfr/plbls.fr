# Estompe — backend Rust

API d'Estompe en Rust (elle remplace l'ancien backend Go, retiré). Décisions de la migration : `../docs/migration-rust.md`.

## Stack

tokio, axum (WebSocket inclus), sqlx (rustls, sans vérification à la compilation), tower / tower-http, serde, tracing, reqwest (rustls).
Les crates sont membres du workspace Cargo racine du monorepo (`../../../Cargo.toml`, partagé avec `apps/security` et `apps/permission/backend` : un seul `Cargo.lock`).
Les dépendances s'ajoutent à `[workspace.dependencies]` de ce fichier racine avec le ticket qui s'en sert. Rust 1.99.0, épinglé par `../../../rust-toolchain.toml` (version aussi écrite dans `rust-version` du `Cargo.toml` racine et dans les `Dockerfile` Rust, vérifiés par la CI : les changer ensemble) ;
licences, sources et avis de sécurité contrôlés par `../../../deny.toml` (cargo-deny).

## Structure

- `estompe-api` : binaire, assemble tout. Écoute sur `LISTEN_ADDR` (défaut `:8080`), `GET /healthz` répond `ok`.
- `core` (`estompe-core`) : cœur. Feature `testutil` : `testutil::postgres()` crée une base par test sur `TEST_DATABASE_URL` (supprimée à la fin, avec les bases rattachées par `BasePostgres::supprimer_aussi`), ou démarre un conteneur (testcontainers) sans cette variable ; sinon `None`.
- `block` (`estompe-block`) : mécanisme de blocs.
- `blocks/<nom>` (`estompe-block-<nom>`) : un crate par bloc optionnel, mêmes noms que `../frontend/src/blocks/registry.ts` : gifs, musique, vocal,
  regarder-ensemble, draw2gether, transcription, videos, apercu-liens, notifications, emojis, avatars, badges, signalements, cairn, activite-statut.

Dépendances : `core` ne dépend d'aucun bloc ; un bloc dépend de `core`, de `block`, et d'un autre bloc seulement s'il le déclare dans `Needs`
(`regarder-ensemble`, `draw2gether` et `transcription` dépendent de `vocal`) ; `estompe-api` assemble tout ; aucune dépendance circulaire.

## Configuration et blocs

`Config::from_env()` (`core/src/config.rs`) lit les mêmes variables, avec les mêmes défauts et le même ordre de validation que le Go ; une variable vide vaut une variable absente.
Un échec s'affiche sur stderr et le processus sort avec le code 1.

`BLOCK_<NOM>` (`BLOCK_MESSAGES_PRIVES` pour `messages-prives`) vaut `true` (défaut, ou vide) ou `false`. `estompe_block::select` refuse une valeur autre et un bloc actif dont une dépendance est
désactivée ou inconnue ; `estompe_block::mount` monte les blocs actifs, dans l'ordre de `block_list()` (`estompe-api/src/main.rs`).

## PostgreSQL et migrations

`core/migrations/` est embarquée dans le binaire.
`db::migrate` reprend le suivi du Go : table `schema_migrations(name)`, noms `migrations/NNN_nom.sql` triés, un fichier par transaction sous `pg_advisory_xact_lock(7001)`,
SQL exécuté seulement si l'`INSERT` du nom a inscrit une ligne. Une base migrée par le Go n'est donc pas rejouée.

Au démarrage, l'API écoute tout de suite ; la base est ouverte en tâche de fond (nouvelle tentative toutes les 4 s (2 s d'attente de connexion, 2 s de pause), abandon et sortie en erreur après 60 s).
`GET /healthz` répond `ok` sans base ; `GET /readyz` répond `ok` si la base répond, sinon 503 `database unavailable`.
Les tests de base utilisent le PostgreSQL de `TEST_DATABASE_URL`, ou `postgres:17-alpine` (comme la production) dans un conteneur en CI.

## Identité, permissions et routes du cœur

`identity::Resolver` interroge `GET {GATEWAY_URL}/auth/me` avec le Bearer reçu (prioritaire) ou le cookie `__Secure-plbls_sid` ; cache de 30 s (1000 entrées), 401/403 → invalide, tout autre échec → indisponible.
`permission::Client` interroge `POST {PERMISSION_URL}/v1/check` avec le Bearer de service ; cache de 30 s ; en cas d'échec, l'accès est refusé (503 `permission_unavailable`).
Les extracteurs `api::Auth` (401 `unauthorized` / 503 `authentication unavailable` en texte brut) et `api::ActiveUser` (compte activé, non suspendu, permission accordée) protègent les routes ;
`GET /me` et `POST /activate` sont les premières routes du cœur. `http::{write_json, write_err, decode}` produisent les réponses JSON communes.
Les tests d'API utilisent `testutil::faux` (faux gateway et faux service de permission sur un port éphémère) et PostgreSQL.

## Serveurs, salons et invitations

`servers::routes()` (monté par `api::routes()`) porte `servers.go` : `POST/GET /servers`, `GET/PATCH/DELETE /servers/{id}`, `POST /servers/{id}/leave`, salons (création, renommage, ordre, suppression), invitations (liste, création, révocation) et `POST /server-invites/{code}/join`.
`roles` calcule les droits d'un membre (`Access`, exceptions de salon, salons verrouillés, exclusion, `top`, `can_grant`, `can_manage_role`, `parse` des clés de permission) et les destinataires d'un salon privé (`voyeurs_du_salon`).
`gestion_roles` (fusionné dans `servers::router()`) porte `roles.go` : création, modification, suppression et ordre des rôles, attribution et retrait à un membre, exceptions de salon par rôle et `GET /servers/{id}/audit-log`. Chaque écriture suit l'ordre du Go : base, `audit`, évènement `roles.updated`, `AppState::droits_modifies(serveur, membre)` (membre vide = tous).
`audit` écrit (`audit`, `audit_membre` : une erreur est journalisée sans faire échouer la requête) et lit (`journal` : 200 dernières actions, filtres `actor`, `target`, `action` exact ou par préfixe) le journal ; les autres modules du cœur l'appellent directement.
`AppState::sur_droits_modifies` enregistre les rappels appelés après un changement de rôle, d'attribution ou d'exception (équivalent de `syncVoice`) ; sans rappel enregistré rien ne se passe, EST-213 y branche le vocal.
Les horodatages sont formatés par PostgreSQL en UTC (`…Z`, fraction sans zéros finals) comme `encoding/json` ; `expires_at` d'une invitation créée vient de l'horloge de la base et non de celle de l'API (précision à la microseconde).
Un `POST /servers/{id}/invites` sans corps est accepté, comme en Go (`ContentLength` nul).

## Modération

`moderation` (fusionné dans `servers::router()`) porte `moderation.go` et `store/moderation.go` : `kick`, `ban` (transaction : table `server_bans`, purge `delete_messages`, retrait du membre), liste et levée des bannissements, exclusion temporaire (`timeout`), avertissement, pseudo de serveur, historique d'un membre, mode lent et verrouillage d'un salon, `bulk-delete` et transfert du serveur. La cible passe par `cible` (ni soi-même, ni le créateur, ni un rang égal ou supérieur). Ordre des effets, comme en Go : base, `messages.deleted` par salon, sortie du vocal, `audit`, évènement (`member.left`, `member.timeout`, `member.warned`, `member.nickname`, `roles.updated`).
`bulk-delete` s'appuie sur `messages::conv_du_salon` (404, puis 400 `not_text_channel`, puis 403 sans `manage_messages`). Les routes vocal `move` et `disconnect` restent au bloc vocal (EST-213).

Interface « vocal » : `state::Vocal` (`sortir_du_serveur(serveur, membre)`), enregistrée par `AppState::definir_vocal` ; `AppState::sortir_du_vocal` ne fait rien sans bloc vocal et est appelé par `kick` et `ban` (équivalent de `dropFromVoice`). L'exclusion passe toujours par `droits_modifies`.
La fin d'un ban ou d'une exclusion est calculée par PostgreSQL (`now() + n * interval '1 second'`) et non par l'horloge de l'API : précision à la microseconde, comme pour `expires_at` des invitations.

## Administration et notes de version

`administration` porte `admin.go` et `store/admin.go` (11 routes `/admin/*` du cœur) : l'extracteur `Admin` (`Auth`, puis `estompe:admin`, 403 `forbidden` sinon et si le vérificateur est injoignable ; ni compte activé ni compte non suspendu exigés), `journaliser` (`admin_log`, acteur = pseudo du compte ou `sub`), suspension (409 `admin_account` pour un administrateur, `hub().kick`), suppression de serveur (`server.deleted` aux membres calculés avant), statistiques, journaux, bot (`POST /admin/bot/messages` via `messages::envoyer_systeme`, sans notifieur ni mention). `/admin/reports*` viennent avec le bloc signalements (EST-222), l'administration des badges avec le bloc badges (EST-221).

`/admin/live` interroge le vocal par `state::Vocal::participants(salon)` (`AppState::participants_vocal` renvoie `None` sans bloc vocal : `voice: []`). `/admin/overview` : `version` = variable de compilation `ESTOMPE_REVISION` tronquée à 7 caractères (`dev` par défaut ; `ARG ESTOMPE_REVISION` du `Dockerfile`, à renseigner à la publication, EST-227).

`notes_version` embarque une copie de `changelog.json` (`notes_version/changelog.json`) et envoie chaque version une seule fois au démarrage, avant d'ouvrir le service (une transaction par version, table `release_announced` partagée avec le Go, sans évènement WebSocket).

## Messages

`messages` (`Conv`, résolution salon / message privé / message, message complet, liste paginée, envoi, édition, suppression, réactions, messages privés, mentions `@pseudo` / `@everyone` / `@rôle`), `sondages` (création dans l'envoi, vote, votants, clôture) et `mentions` (`GET /notifications`, `POST /notifications/read`) portent `messages.go`, `store/{messages,polls,notifications}.go` et `internal/emoji` (`emoji.rs`, `emoji/codes.json`). Ordre des effets, comme en Go : validations, base, évènement (`message.created`…), hook, réponse. Le JSON entrant est lu avec des clés en minuscules. Points d'accroche vers les blocs, no-op sans bloc : `state::PostVideo` (`apres_message`, appelé après la création d'un message d'un salon `videos` ou `music`, EST-216) et `state::Notifieur` (`publier(EvenementMessage)`, appelé après création, et après édition si des mentions sont ajoutées, EST-218), enregistrés par `AppState::definir_post_video` et `definir_notifieur`.

`messages` porte aussi les épingles (`PUT`/`DELETE /messages/{id}/pin`, `GET …/pins`) et la saisie en cours (`POST …/typing`, limiteur `saisie`).

## Regarder ensemble et Music2gether

`blocks/regarder-ensemble` (`needs: ["vocal"]`, `mount`) porte `internal/api/watch.go` : les 10 routes `/servers/{id}/…` et `/voice/{cid}/…` pour `watch` et pour `music`, soit deux jeux de sessions distincts (évènements `watch.state` et `music.state`). Modules : `lien` (`youtube_id` et `lien_video`, la partie de `YouTubeID` et `ParseVideoURL` que lit `watchVideo`), `session` (état, moteur de transitions pur `demarrer`, `agir`, `pret`, `options`, `ajouter`, `retirer`, `reordonner`, et registre `Sessions` sous verrou : une révision par commande, `pret` ne la change pas) et `lib.rs` (accès au salon, limiteur `regarder_ensemble` de 30 actions par 10 s partagé par les deux jeux, publication). Les sessions sont en mémoire (une seule instance de l'API). Music2gether a sa propre instance de `Musique` (`routes(musique)`). Le vocal passe par le trait `Vocal` du cœur : `AppState::vocal_branche()` faux donne 501 `no_voice`, `participants_vocal` donne les présents. Le départ d'un participant arrive par `AppState::sortie_salon_vocal(serveur, salon, membre)` (rappel enregistré par le bloc à sa première requête) : le bloc vocal (EST-213) doit l'appeler sur `participant_left`. `Vocal::participants` renvoie `Result<_, VocalInjoignable>` : LiveKit injoignable donne 502 `voice_unavailable` (une salle absente vaut aucun présent), comme le Go. Un troisième jeu, `cinema` (évènement `cinema.state`), sert les salons de type `cinema` : il n'accepte que les titres de la bibliothèque (`film:<id>`, hôte seul), que `watch` refuse désormais ; ses routes (`Lecteurs::routes_cinema`) sont montées par le bloc `films`, absentes quand il est désactivé. Le vocal (jeton LiveKit, présents, déplacement) traite un salon `cinema` comme un salon `voice` (`servers::est_vocal`). `Lecteurs` (`nouveau`, `partage`, `routes`, `routes_cinema`, `lancer`) porte les sessions ; `lancer` démarre ou complète la file d'un salon vocal pour le planificateur de `videos` (`session::lancer_file`, `startQueue` du Go). `mount` utilise `Lecteurs::partage()`.

## Recherche

`recherche` (`recherche::router()`) porte `store/search.go` : `analyser` reproduit `ParseSearch` (minuscules et accents comme le Go, dates strictes `AAAA-MM-JJ`), `chercher` assemble la requête fragment par fragment comme `Search` (fenêtre de classement de 2000, `LIMIT`/`OFFSET`) et réutilise la sélection et l'hydratation des messages. Les salons visibles sont recalculés à chaque recherche (`salons_visibles`). Le limiteur `recherche` (30 par minute) est partagé avec le bloc musique.

## Musique

`blocks/draw2gether` (`needs: ["vocal"]`, `mount`) porte Draw2Gether (EST-182), côté serveur seulement : `partie` (état numéroté `rev` comme `regarder-ensemble`, moteur pur sur `Etat` : phases `lobby`, `choosing`, `drawing`, `turn_end`, `ended`, tirage et masque du mot, propositions, points, indices, votes, traits vectoriels ; registre `Parties` sous verrou), `mots` (listes fr et en) et `lib.rs` (routes `/voice/{cid}/draw…`, évènements `draw.state`, `draw.private`, `draw.chat`, `draw.ops`, tâche d'une seconde qui appelle `Etat::avancer` pour les chronos). Les parties sont en mémoire. Le mot n'est jamais dans `draw.state` avant la fin du tour : le dessinateur le reçoit par `draw.private` et `GET`. Limiteurs propres au bloc (propositions, traits, actions). Aucune interface frontend.

`blocks/musique` (`needs: []`, `mount`) porte `internal/music` et les routes `/music/*` de `internal/api/music.go` : `GET /music/search`, `GET /music/resolve` (connecté et actif) et `GET /music/cover/{id}` (publique, balise `<img>`). Modules : `analyse` (`analyser_url`, `analyser_youtube_music`, `est_id_video` : `ParseMusicURL` ; chemin comparé après décodage, hôtes ASCII exacts ; refuse tout `\` et tout identifiant `user@` dans le lien, ce qui est plus strict que `url.Parse` du Go), `youtube` (API YouTube Data : `videos` ≤ 50 identifiants, `rechercher`, `liste`, `duree_iso`, `album_de`), `oembed` (repli sans clé) et `cache` (métadonnées 24 h, plafond de 500 avec vidage complet ; pochettes 500). L'API publique du crate est `Musique::pistes(cle, ids)` et `Musique::resoudre(cle, lien)`, réutilisables par `videos`, `regarder-ensemble` et `apercu-liens` (cartes de liens, qui restent à porter dans ces blocs). Les bases d'URL (Data API, oEmbed, miniatures) sont des champs de `Musique::avec_bases`, jamais lues dans l'environnement. La clé `YOUTUBE_API_KEY` est lue dans la configuration à chaque requête. `resolve` et `search` partagent le limiteur `recherche` (30 par minute) ; `resolve` valide le lien avant le limiteur, `search` après la vérification de la clé, comme le Go.

`blocks/avatars` (`needs: []`, `mount`) porte `internal/api/avatars.go` et `store/avatars.go` : avatars globaux et par serveur, icône de serveur, fond de caméra (14 routes). Comme le Go, aucun pixel n'est décodé ni recadré : `image::analyser` vérifie la signature et l'en-tête (PNG : IHDR avec CRC ; JPEG : SOF0 à SOF2), les dimensions et le poids (256 Ko et 512 px ; 1 Mo et 1920 px pour le fond), et les octets reçus sont stockés et servis tels quels (`Cache-Control: private, max-age=86400`, sans ETag comme le Go). Pas de bombe de décompression possible. L'icône est réservée à `manage_server` ou à `estompe:admin`. Les helpers `servers::acces`, `evenement_serveur`, `non_trouve`, `sans_contenu`… et `fichiers::est_admin` sont publics pour les blocs.

Écarts et limites repris du Go : deux requêtes simultanées pour un même identifiant interrogent deux fois l'amont (pas de dédoublonnage) ; le client de la Data API ne suit pas les redirections (la clé est dans l'URL), alors que le Go les suit, tandis que les clients de la pochette et d'oEmbed les suivent comme le Go (hôtes fixes) ; les identifiants vides ou invalides renvoyés par YouTube sont écartés (le Go les transmet), si bien qu'une playlist dont les vidéos sont supprimées donne « liste vide » ou une piste de moins ; si l'API échoue pour une liste de 50 titres, le repli oEmbed est séquentiel (10 s au plus par titre) ; l'oEmbed n'a pas de filtre SSRF propre car l'URL est construite depuis un identifiant validé et l'hôte est fixe.

`etat_lecture` (`etat_lecture::router()`) porte `api/readstate.go` et `store/readstate.go` : `GET /unread` (mêmes comptages SQL que le Go ; les salons sans `VIEW_CHANNEL` sont écartés par `salons_visibles`), `POST …/read` (lit les mentions de la conversation même si rien n'avance, puis avance `read_states` sans reculer ; `read.updated` et `Notifieur::retirer` seulement si le marqueur avance) et `GET …/reads`. `Notifieur::retirer(user, conv, last)` (`conv` = `c:<id>` ou `d:<id>`, implémentation par défaut vide, appelée via `AppState::retirer_notification`) est le point d'accroche de la notification Android (EST-218).

## Vocal

`blocks/vocal` (`needs: []`, `mount`, `brancher`) porte `internal/voice`, `api/voice.go`, `store/voicemod.go`, `store/voicetime.go` et les routes `move` et `disconnect` de `api/moderation.go` : `POST /voice/{cid}/token` (`?screen=1` : jeton du participant `<uid>#screen`), `GET /servers/{id}/voice`, `GET /servers/{id}/voice/stats`, `PUT /voice/{cid}/participants/{uid}/mod`, `GET /servers/{id}/voice/moderation-log`, `POST /voice/webhook` (publique, signée), `PUT .../move` et `POST .../disconnect`. Le module `livekit` contient le JWT HS256 (`hmac`, `sha2`, `base64`, mêmes claims que le Go, 1 h), le client de l'API serveur LiveKit (Twirp/JSON via `reqwest`, pas de crate `livekit-api` : protobuf et dépendances lourdes pour quatre appels : `CreateRoom`, `ListParticipants`, `UpdateParticipant`, `RemoveParticipant`) et la vérification du webhook (signature, expiration, empreinte SHA-256 du corps). La configuration LiveKit est lue dans `state.config().livekit` à chaque appel (`Lk::new`), car `mount` n'a pas accès à l'état.

`estompe_block_vocal::brancher(&state)` (appelé par `main.rs` si le bloc est actif) enregistre les accroches du cœur : `Vocal` (sortie d'un serveur, participants de l'écran d'administration), `sur_droits_modifies` (droits de publication ajustés, éjection de qui ne peut plus se connecter) et `sur_acces_retire` (éjection de tous les salons vocaux).

Écarts et limites : désactivé (`BLOCK_VOCAL=false`), le bloc retire ses routes (404) au lieu du 501 `no_voice` ; le webhook prévient le service de notifications (`EvenementMessage.vocal` : « a rejoint », « partage son écran », « a allumé sa caméra ») et `sortie_salon_vocal` (`watchLeft`) ; `Vocal::participants` signale un LiveKit injoignable (502 `voice_unavailable` dans `regarder-ensemble`, salon ignoré dans `/admin/live`) ; sans LiveKit configuré le Go n'a pas de 501 `no_voice` non plus (client toujours présent) ; le champ `at` de `voice.state` vient de l'horloge de PostgreSQL ; un évènement de webhook dont le JSON n'est pas un objet est refusé en 401 comme le Go.

## Vidéos

`blocks/videos` (`needs: []`, `mount` + `brancher`) porte `internal/api/videos.go`, `video.go` (reconnaissance des sources) et `store/videos.go` : les 10 routes `/channels/{cid}/videos*` des salons `videos` et `music` (cartes, vues par le groupe, listes ordonnées, séances et réponses), sur les tables des migrations 031 et 032 lues sans migration. Salon résolu par `messages::conv_de_tout_salon` (404, puis 400 `not_videos_channel`). Chaque écriture publie `videos.changed` aux voyeurs du salon après la base.
`brancher(&state)` (appelé par `estompe-api` si le bloc est actif) enregistre le crochet `state::PostVideo` : après un message d'un salon `videos` ou `music`, le premier lien (3 premiers liens examinés) devient une carte (titre de repli, évènement), puis titre, chaîne et miniature sont récupérés en tâche de fond (20 s) : oEmbed YouTube / Vimeo / Dailymotion, sinon aperçu de page (source `page` si la page annonce un fichier vidéo), cartes YouTube Music par le bloc `musique` (`Musique::pistes`, `Musique::carte_liste`, repli oEmbed pour les listes). Les clients HTTP réutilisent `Fetcher` d'`apercu-liens` (`image` et `oembed` désormais publics, mêmes protections SSRF).

Écarts avec le Go : une séance dont un identifiant n'est pas une carte du salon répond 400 `invalid_session` (500 en Go) ; la pochette d'une liste YouTube Music est celle de son premier titre (le Go prend d'abord la miniature de la liste) ; une date de séance doit être en RFC 3339 (validée par PostgreSQL). Le planificateur (`seances.rs`, `RunWatchSessions`) tourne toutes les 20 s depuis `brancher` : rappel aux « je viens » et « peut-être » par `Notifieur` (`EvenementMessage.rappel`, `a` = les destinataires), puis lancement d'une séance dont l'heure est passée (jusqu'à 2 h) quand une personne autre que le bot est dans le salon vocal, une seule fois (`started_at` réclamé en base) : les morceaux YouTube Music vont dans la session `music`, le reste dans `watch` (`regarder_ensemble::Lecteurs::lancer`), l'hôte est le premier présent à la séance. Il suppose le vocal branché (`vocal_branche`) ; `videos` dépend du crate `regarder-ensemble` sans le déclarer dans `needs`, comme le Go.

Écarts laissés avec le Go, constatés par les scénarios de contenu (EST-226) : l'aperçu de liens répond 400 `blocked_url` pour les écritures IPv4 non canoniques (`127.1`, `2130706433`, `0x7f.0.0.1`, `0177.0.0.1`) que la bibliothèque `url` normalise en adresse de bouclage, là où le Go les laisse à la résolution DNS (404 `no_preview`, ou 400 selon le résolveur du système) : refus plus sûr et indépendant du DNS ; une séance de vidéos dont un identifiant n'est pas une carte répond 400 au lieu du 500 du Go (déjà noté plus haut). Le JPEG d'un avatar, d'une icône ou d'un fond est validé comme `image/jpeg` du Go : un en-tête sans APP0 JFIF ni SOS, un SOF mal formé ou un sous-échantillonnage non pris en charge sont refusés.

## Middlewares transverses

`middleware::envelopper(routes, state)` (appelé par `app()` de `estompe-api`) pose, de l'extérieur vers l'intérieur, comme `Handler` du Go :
`metriques` > `journal::entrant` > `journal_erreurs` > `politique_origine` (CORS + CSRF) > `version_minimale` (426) > `normaliser_404_405` > routes.
Le 404 et le 405 vides d'axum sont réécrits au format du mux Go (texte brut, `nosniff`, `Allow` trié `GET, HEAD`) et traversent donc CORS, 426 et métriques.
Limite : un gestionnaire qui renverrait lui-même un 404/405 vide et sans `Content-Type` serait réécrit (aucune route actuelle ne le fait). Le reformatage de `Allow` (`GET, HEAD`, trié) est une couche à part, la plus externe : il s'applique à tout 405, même renvoyé par un gestionnaire.

- `updates` : `Policy`, `pour` (surcharge par plateforme, `pwa` suit `web`), `en_dessous`. `core/updates.json` est la politique embarquée ; un JSON illisible donne une politique vide. serde est strict : les clés sensibles à la casse et `null` que le Go tolérait ne le sont plus (fichier contrôlé par l'équipe).
- `ratelimit` : `RateLimiter::allow(clé, max, fenêtre)` (fenêtre glissante, clés inactives oubliées) ; `AppState::limiteurs()` expose les huit instances (`apercu`, `envoi`, `recherche`, `saisie`, `gif`, `transcription`, `signalement`, `regarder_ensemble`). Les quotas, les 429 et `Retry-After` sont aux sites d'appel.
- `metrics` : `GET /metrics` (Bearer `METRICS_TOKEN` comparé en temps constant, jeton nu accepté comme en Go ; 404 si le jeton n'est pas configuré). Les routes de recherche appellent `AppState::metrics().searched(durée)`. Les jauges `estompe_ws_users` et `estompe_ws_connections` viennent de `Hub::counts`.
- `hub` : hub temps réel en mémoire (instance unique), `AppState::hub()`. Une file de 64 évènements par connexion (client trop lent fermé), `publish(to, type, data)` (les blocs passent par `AppState::publier`), tickets à usage unique de 30 s, présence, jeu en cours, `kick`, `close_all`, `counts`, `clients`.
- `ws` : `POST /ws-ticket` et `GET /ws` (ticket vérifié avant l'upgrade, `Origin` contrôlé, lecture limitée à 1 Kio, `ping` → `pong`, inactivité `WS_IDLE` 75 s). Revérification toutes les 2 min : 4401 (session expirée), 4403 (accès retiré, ou service de permission injoignable plus de 10 min). `AppState::sur_acces_retire` enregistre les rappels appelés avant la fermeture 4403 (EST-213 y branche l'expulsion du vocal). Les durées sont réglables par `AppState::new_avec_ws` (tests).
- Journal des erreurs : une réponse 5xx (ou une panique, devenue 500 `internal`) est inscrite dans `server_errors` (purge des plus de 30 jours au passage) ; `/ws*` est ignoré ; un échec d'écriture est tracé sans toucher à la réponse.

- Journal structuré (`core/src/journal.rs`) : une ligne JSON par échange, `request_type` (`input` pour une requête reçue par l'API, `output` pour un appel sortant), `service`, `method`, `path`, `query`, `request_data`, `header_data` (`request` et `response`), `response_status`, `response_data`, `duration_ms` et `error`. Le formateur est `estompe-api/src/journal_json.rs` (logs applicatifs à plat, `RUST_LOG` filtre toujours). Tout appel sortant passe par `journal::sortant("<service>", requete)` à la place de `.send()` : actuellement Jellyfin (films) ; les autres services (GIF, LiveKit, FCM, YouTube…) restent à migrer. Les corps ne sont journalisés que s'ils sont textuels et d'au plus 16 Kio (réponses sortantes : aussi en erreur) ; flux vidéo, fichiers et `/ws*` ne le sont pas, `/metrics` et `/health` sont ignorés. Masquage : `**MASKED**` remplace les en-têtes, clés JSON, paramètres de requête et segments de chemin sensibles (`authorization`, `cookie`, `token`, `secret`, `password`, `key`…, segments de 32 caractères ou plus qui ne sont pas des UUID) ; pour une structure journalisée, `Sensible<T>` ou `#[serde(serialize_with = "journal::masquer")]` marquent un champ sensible. Niveau : `error` pour un échec de transport ou un 5xx, `warn` pour un 4xx, `info` sinon. La durée d'un appel sortant s'arrête à la réception des en-têtes, sauf quand le corps est lu pour être journalisé.

Écarts connus avec le Go : la durée des requêtes est mesurée jusqu'à l'envoi de l'en-tête de réponse (pas jusqu'à la fin du corps) ; le journal des erreurs n'est pas écrit si le client coupe avant la fin de la requête.

Les cas de succès de chaque route (corps valides, données créées) restent dans les tests de leur bloc.

## Fichiers

`fichiers::routes()` : `POST /files`, `GET /files/{id}`, `GET /files/{id}/thumb`, `POST /files/{id}/link`, ports de `files.go`, `video.go` et `blob`.
- `blob::Disque` : `DATA_DIR/<2 premiers caractères>/<clé>` pour une clé `files/<32 hexa>` (même arborescence que le Go : le volume `estompe-data` existant reste lisible) ; écriture dans `.tmp-*` puis renommage, droits 0640 (dossiers 0750), toute autre clé refusée.
- `sniff::detecter` est le port de `http.DetectContentType` (le type stocké et la vérification des extensions d'image en dépendent) ; `mp4::analyser` valide un MP4 (H.264, HEVC, AV1 avec AAC ou Opus) et lit sa durée.
- Plafonds : 10 Mo (`MAX_VIDEO_SIZE_MB` pour `kind=video`, 5 Mo pour un vocal), `MAX_USER_STORAGE_MB` (413 `quota_exceeded`), `MAX_STORAGE_GB` (507 `storage_full`), 200 Mo par heure et 1 Go par jour (429 `upload_rate_limited`), 20 fichiers en attente, 4 envois de vidéos simultanés. Le contrôle et l'écriture se font sous `AppState::verrou_stockage`.
- Visibilité : un fichier joint à un message n'est visible que si `salons::voit_message` l'autorise (membre du serveur ou du message privé, et `view_channel` après rôles et exceptions de salon, le créateur voit tout). Le calcul de `view_channel` est repris du Go ; EST-204 et EST-206 le réutiliseront. Un administrateur lit aussi les captures de signalement.
- Lien signé : `POST /files/{id}/link` renvoie `{"token": "<exp>.<hmac>"}` valable 1 h, signé par une clé tirée au démarrage (les liens ne survivent pas à un redémarrage, comme en Go). `?t=` remplace l'identité sur `GET /files/{id}` et `/thumb`.
- Purge : `fichiers::lancer_purge` supprime toutes les 10 min les fichiers sans message, sans signalement et vieux de plus d'une heure (disque d'abord, puis ligne) ; `estompe-api` l'arrête proprement après l'arrêt du serveur HTTP.

Écarts connus avec le Go : une requête `Range` à plusieurs plages reçoit le fichier entier (200) au lieu d'un `multipart/byteranges` ; `If-Match` / `If-None-Match` / `If-Modified-Since` sont ignorés (le Go n'a ni ETag ni date de modification : seul `If-Range` fait renvoyer le fichier entier).

## Blocs actifs et dépendances

| Bloc | Dépend de |
|---|---|
| `gifs`, `musique`, `vocal`, `videos`, `apercu-liens`, `notifications`, `emojis`, `avatars`, `badges`, `signalements`, `cairn`, `activite-statut` | aucun |
| `regarder-ensemble`, `draw2gether`, `transcription` | `vocal` |

La liste de référence est `block_list()` dans `estompe-api/src/main.rs` (le test `les_seize_blocs` la fige).

## Ajouter un bloc

Exemple à suivre : `blocks/gifs`. Pour un bloc `mon-bloc` :

1. **Crate** : créer `blocks/mon-bloc/Cargo.toml` (copie de celui de `blocks/gifs` : `name = "estompe-block-mon-bloc"`, `[lints] workspace = true`, dépendances `estompe-core`, `estompe-block` et `axum`, `utoipa`, `utoipa-axum`, `serde`… issues de `[workspace.dependencies]` ; `estompe-core` avec la feature `testutil` en `[dev-dependencies]`). `blocks/*` est déjà un membre du workspace. Une dépendance absente de `[workspace.dependencies]` s'y ajoute d'abord, jamais directement dans le crate.
2. **Déclaration** : ajouter `estompe-block-mon-bloc = { path = "blocks/mon-bloc" }` à `[workspace.dependencies]` de `Cargo.toml`, et `estompe-block-mon-bloc.workspace = true` aux dépendances de `estompe-api/Cargo.toml`. `core` ne doit jamais dépendre d'un bloc (la CI le vérifie).
3. **Code** : `src/lib.rs` expose `pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState>` qui fusionne les routes du bloc (`router.merge(routes())`). Les routes utilisent les extracteurs du cœur (`api::ActiveUser`, `api::Auth`), `http::{write_json, write_err, decode}` et `state::AppState`. Chaque route est décrite avec `openapi::operation` et `openapi::documenter`, avec les codes de réponse réels. Tout évènement visible par d'autres passe par le hub du cœur. Un bloc qui a besoin d'un autre le déclare dans `needs`.
4. **Enregistrement** : ajouter un `Block { name: "mon-bloc", needs: &[…], mount: estompe_block_mon_bloc::mount }` à `block_list()` dans `estompe-api/src/main.rs` (le nom est celui du dossier et de `frontend/src/blocks/mon-bloc/`), puis mettre à jour le test `les_seize_blocs` et un test « bloc désactivé : routes absentes » sur le modèle de `gifs_absent_quand_le_bloc_est_desactive`.
5. **Variable** : `BLOCK_MON_BLOC` (tirets remplacés par `_`) dans `../deploy/.env.example` et `../deploy/docker-compose.yml` (`BLOCK_MON_BLOC: ${BLOCK_MON_BLOC:-true}`), ainsi que toute variable propre au bloc, lue par `Config` si elle est commune ou par le bloc lui-même.
6. **Documentation** : `../docs/openapi.yaml` (chaque route) et `../docs/realtime.md` (chaque évènement). Le test `openapi_compatible_avec_le_contrat_go` échoue si le code et ce fichier divergent.
7. **Tests** : dans le module `#[cfg(test)]` de `src/lib.rs`, avec `testutil::postgres()`, `testutil::faux` (gateway et permission) et les migrations (`db::migrate_embarquees`). Couvrir le cas nominal, le refus de droits, les entrées invalides et l'absence du bloc quand il est désactivé.
8. **Migration** (si le bloc a des tables) : `core/migrations/NNN_nom.sql`, numéro suivant, jamais une migration déjà fusionnée modifiée.
9. **Frontend** : le même nom dans `../frontend/src/blocks/registry.ts` et un dossier `../frontend/src/blocks/mon-bloc/`.
10. **PR** : `../changelog.d/<slug-court>.md`, puis les vérifications de « Développement ». Si le bloc remplace une fonctionnalité du cœur, dire ce qui n'a pas été vérifié.

## Développement

Depuis `apps/estompe/` (cargo trouve le workspace racine en remontant les dossiers ; `scripts/paquets-cargo.sh` liste les crates d'Estompe en `-p …`) :

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test $(scripts/paquets-cargo.sh)
cargo deny check                                             # cargo install cargo-deny --locked
```

`rust-toolchain.toml` (racine du monorepo) est lu depuis n'importe quel dossier du dépôt, sauf les apps Tauri (`windows/`, `android/`, `crates/`), qui ont le leur.

Boucle rapide, du plus court au plus long (temps relevés sur 8 CPU, 25 Go, WSL2, après modification d'un corps de fonction dans `core`) :

```sh
cargo check $(scripts/paquets-cargo.sh) --all-targets       # 1,4 s (froid : 28 s)
cargo build $(scripts/paquets-cargo.sh)                     # 1,9 s (froid : 20 s)
cargo test $(scripts/paquets-cargo.sh) --no-run             # 5,4 s (froid : 47 s)
cargo test $(scripts/paquets-cargo.sh)                      # exécution : environ 50 s
```

Blaze ([indexable-inc/blaze](https://github.com/indexable-inc/blaze)), en local uniquement : un démon garde le workspace compilé en mémoire et ne recompile que ce qui change. S'installe avec `curl -fsSL https://ix.dev/blaze | sh` (macOS Apple silicon et Linux ; sous Windows, tout passe par cargo). Il remplace `cargo` pour `build`, `check`, `run`, `test` et `bench`, les autres commandes passent à cargo :

```sh
blaze check $(scripts/paquets-cargo.sh) --all-targets
blaze test $(scripts/paquets-cargo.sh)
```

Blaze compile avec sa propre toolchain, pas celle de `rust-toolchain.toml`, et réclame plusieurs Go de mémoire. Il n'est ni dans `scripts/check.sh` ni dans la CI ni dans le `Dockerfile` : ils restent sur cargo et Rust 1.99.0, qui font foi. En cas d'écart entre les deux, cargo a raison. Avant de pousser, lancer `scripts/check.sh`.

`[profile.dev] debug = "line-tables-only"` (`Cargo.toml` racine) garde fichiers et lignes dans les traces de pile, mais un débogueur n'affiche plus les variables : pour déboguer pas à pas, ajouter `--config profile.dev.debug=true`.

Linker : aucun réglage. Avec ce profil, `rust-lld` (défaut de la toolchain) et mold donnent le même temps (5,25 s contre 5,36 s) : ne pas déclarer de linker. Cargo lit `.cargo/config.toml` depuis le dossier courant et ses parents, pas depuis `--manifest-path`.

Cranelift (expérimental, jamais en CI ni dans l'image, activé nulle part) : `RUSTUP_TOOLCHAIN=nightly RUSTFLAGS=-Zcodegen-backend=cranelift cargo build …`, après `rustup +nightly component add rustc-codegen-cranelift-preview`. Le build à froid gagne 3 s (17 s contre 20 s), mais `cargo test --no-run` incrémental est plus lent (12,6 s contre 5,4 s) et les tests s'interrompent à l'exécution (`llvm.x86.sse2.pmulhu.w is not yet supported`) : à réserver à `cargo build`.

## Image et CI

`Dockerfile` (contexte de build : racine du dépôt, filtrée par `.dockerignore` racine) : compilation musl statique (`rust:1.99.0-alpine`) en deux temps avec `cargo-chef` (dépendances dans leur couche, puis le code : un changement de source ne recompile que les crates `estompe-*`), image finale `gcr.io/distroless/static-debian12:nonroot` (uid 65532, `/data` en volume,
`DATA_DIR=/data`, port 8080). `.github/workflows/estompe-backend-rs.yml` : fmt, clippy, règle « le cœur ne dépend d'aucun bloc », cargo-deny, tests, puis construction de l'image sans publication et
démarrage avec `../deploy/.env.example` et un PostgreSQL jusqu'à `GET /readyz` (base ouverte, migrations appliquées) ; le cache `:buildcache` n'est alimenté que par le tag `estompe-api-vX.Y.Z` (`.github/workflows/estompe-image.yml`) : avant le premier tag qui suit ce changement, et après chaque modification de `Cargo.lock`, le job compile les dépendances à froid. Publication par ce même tag. Déclencheurs : `docs/ci.md`.

`blocks/activite-statut` (`needs: []`, `mount`) porte `status.go` et `activity.go` : `GET /statuses`, `PUT /me/status`, `PUT /me/banner`, `PUT /me/activity`. Les voisins sont `users::co_members` plus soi-même (`Neighbors` du Go) ; `banner.updated` part aussi vers soi-même en double, comme le Go. L'activité reste dans le hub du cœur (`set_activity`, effacée à la dernière déconnexion, `409 offline` sans WebSocket) ; la présence et le snapshot restent dans `ws`.
