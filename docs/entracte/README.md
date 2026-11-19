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
| `entracte:admin` | liste et traitement des demandes, pilotage de Radarr et Sonarr (recherche, releases, file, titres), fichiers écartés, état des synchronisations, détection des intros |

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
  l’en-tête d’Administration (état des services) (`GET /admin/status`).
- Identifiants publics propres à Entracte (UUID) : aucune réponse ne contient de chemin de fichier, d'identifiant Radarr/Sonarr ni d'URL TMDB.
  L'identifiant TMDB d'un titre, lui, est public (recherche et demandes). Seules les routes `/admin/arr/…`, réservées à l'administrateur,
  portent les identifiants et les dossiers racines de Radarr/Sonarr (voir « Pilotage de Radarr et Sonarr »).

Attribution : l'app affiche « Données et images fournies par TMDB » (conditions d'utilisation de l'API TMDB).

## Progression et historique

Position enregistrée par personne et par titre. Au-delà de 90 % de la durée, le titre est vu : une entrée datée par titre, que revoir le titre
met à jour. « Reprendre » liste les titres commencés et l'épisode suivant d'une série en cours. L'historique est lié au titre, pas au
fichier. Chacun ne lit et n'écrit que les siens (la personne est celle de la session, jamais un paramètre).

## Statistiques (Sablier)

Entracte émet des évènements vers [Sablier](../sablier/README.md) par la file commune (`commun::emission::Emetteur`, variables
`SABLIER_REDIS_URL`, `SABLIER_REDIS_PASSWORD`, `SABLIER_FILE_MAX`). L'émission ne bloque jamais une requête : sans `SABLIER_REDIS_URL`, ou
avec Redis arrêté, Entracte répond de la même façon et les évènements sont perdus. Les évènements ne portent que la personne et les
identifiants publics d'Entracte (titre, série), jamais de chemin de fichier ni d'identifiant Radarr, Sonarr ou TMDB.

| Évènement | Quand |
|---|---|
| `entracte.lecture` `{titre, genre, serie, secondes}` | à chaque rapport de position (`PUT /titles/{id}/progress`, toutes les 15 s en lecture, à la pause et à la fermeture) qui crédite au moins une seconde |
| `entracte.vu` `{titre, genre, serie}` | quand le titre devient vu pour la personne (première entrée dans son historique) : une seule fois par personne et par titre, même si la lecture continue, reprend ou si le titre est revu |
| `entracte.serie_terminee` `{serie}` | quand un épisode devient vu et que tous les épisodes connus de la série (présents ou absents) sont alors vus par la personne |
| `entracte.abandon` `{titre, genre}` | titre commencé, jamais vu, sans rapport de position depuis 30 jours ; daté de la fin de ces 30 jours |
| `entracte.intro_passee` `{titre}` | clic sur « Passer l'intro » (`POST /titles/{id}/intro-skipped`, titre dont l'intro est connue) |

- **Temps regardé** : mesuré par le serveur, jamais déclaré par le client. Entre deux rapports consécutifs d'une personne sur un titre,
  Entracte crédite le plus petit du temps d'horloge écoulé et de l'avancée de la position. Position inchangée (pause, mise en mémoire
  tampon) ou retour en arrière : rien pour cet intervalle, la lecture qui suit est comptée (revoir un passage compte). Saut en avant :
  seul le temps écoulé compte. Plus de 45 s entre deux rapports : rien. Le premier rapport d'une lecture ne crédite rien : la mesure
  perd au plus un intervalle de 15 s par lecture et par retour en arrière. Les fractions de seconde sont reportées sur le rapport suivant.
  Les repères sont en mémoire : un redémarrage d'Entracte perd au plus l'intervalle en cours de chaque lecture.
- **Abandon** : relevé toutes les heures (et au démarrage), seulement quand l'émission est active. La marque est gardée en base
  (`progression.abandon_le`) : un seul évènement par personne et par titre, même si la lecture reprend puis s'arrête de nouveau.
  Au premier démarrage avec l'émission active, les lectures déjà délaissées depuis plus de 30 jours sont relevées, à leur date.
- **Salon cinéma** : chaque participant envoie ses rapports de position comme en lecture seule ; son temps est donc compté ici une fois,
  comme toute lecture. Une intro passée y est comptée pour la personne qui a cliqué, pas pour les autres participants.

### Import de l'historique

`entracte importer-sablier` rejoue une fois l'historique d'Entracte (tables `progression` et `vus`) en évènements datés et marqués
`importe`, à lancer au moment où l'émission est mise en service :

| Historique | Évènements | Date |
|---|---|---|
| titre vu | `entracte.lecture` (`secondes` = durée du titre) et `entracte.vu` | date du « vu » |
| titre commencé, jamais vu | `entracte.lecture` (`secondes` = position atteinte) | dernier rapport de position |
| série dont tous les épisodes connus sont vus | `entracte.serie_terminee` | dernier épisode vu |

Identifiants déterministes : `import:entracte:lecture:<sub>:<titre>`, `import:entracte:vu:<sub>:<titre>`,
`import:entracte:serie_terminee:<sub>:<série>`. La commande peut être relancée : Sablier ignore un identifiant déjà reçu. Elle n'émet ni
abandon ni intro passée, attend l'accusé de Redis pour chaque évènement et s'arrête à la première erreur (`SABLIER_REDIS_URL` absent,
Redis injoignable) avec un code de sortie 1. Une lecture faite après la mise en service de l'émission est déjà comptée en direct :
relancer l'import plus tard ne la recompte pas si son titre était déjà dans l'import précédent, mais un titre commencé depuis y
entrerait une seconde fois.

```
docker exec <conteneur entracte-api> /app/entracte importer-sablier
```

En local (variables de `deploy/dev.env` chargées) : `cargo run -p entracte -- importer-sablier`. Code et tests : `api/entracte/src/sablier.rs`.

## Intros

Le lecteur propose « Passer l'intro » (bouton, ou touche S) tant que la position est dans `[debut, fin)` ; un clic saute à `fin`
et le signale à Entracte (`POST /titles/{id}/intro-skipped`, échec ignoré) pour les [statistiques](#statistiques-sablier).
Les bornes viennent d'une comparaison audio entre les épisodes d'une même saison : les fichiers n'ont pas besoin de chapitres.
Seules les séries sont concernées (ni films, ni génériques de fin, ni saisie manuelle). Dans le salon cinéma d'Estompe, le bouton
(ou la touche S) est proposé à qui peut piloter la lecture et déplace la séance de tous les participants à `fin`.

- **Mode** (Administration › Intros, gardé en base, `manuel` par défaut). `manuel` : rien ne démarre seul, l'administrateur lance
  « Analyser » série par série, puis « Réanalyser » ou « Effacer ». `automatique` : après chaque synchronisation du catalogue (et au
  moment où le mode est activé), les saisons jamais analysées sont mises en file.
- **Empreinte** : pour chaque épisode, les 10 premières minutes de la première piste audio (mono, 11 025 Hz, un seul fil, `nice`),
  passées à Chromaprint par le muxer `chromaprint` de ffmpeg, à défaut par `fpcalc` (les deux sont dans l'image). L'empreinte est
  gardée en base et recalculée seulement si la taille ou la date du fichier change.
- **Comparaison** : les épisodes sont comparés deux à deux, à tous les décalages ; deux points se ressemblent s'ils diffèrent d'au
  plus `ENTRACTE_INTRO_BITS` bits (distance de Hamming) ; on garde la plus longue suite contiguë de 15 à 120 s, les trous d'au plus
  3 s étant tolérés. Une suite trop uniforme (silence) est ignorée.
- **Consensus** : pour un épisode, l'intro est retenue si au moins 60 % de ses paires donnent les mêmes bornes (à 5 s près, médiane
  du groupe) ; un segment commun à deux épisodes seulement est rejeté dès que la saison en compte trois. Avec deux épisodes, leur
  unique paire suffit.
- **File** : un seul travailleur, une saison à la fois ; une analyse interrompue par un redémarrage reprend. États d'une saison :
  jamais analysée, en file, en cours, intro trouvée, rien trouvé, erreur (message sans chemin de fichier). Moins de deux épisodes ou
  aucun consensus donne « rien trouvé ». Une saison « rien trouvé » ou « erreur » n'est pas réanalysée seule : elle le sera si un de
  ses fichiers change (épisode ajouté, retiré ou remplacé), ce qui la ramène à « jamais analysée », ou sur « Réanalyser ».
- **Fichier remplacé** : ses bornes et son empreinte sont effacées à la synchronisation qui le constate ; les autres épisodes gardent
  les leurs jusqu'à la nouvelle analyse.
- **Réglages** : `ENTRACTE_INTRO_*` de [`plbls.env`](../../plbls.env) (durée analysée, durées minimale et maximale, bits, trou,
  consensus, tolérance, priorité `nice`, chemin de `fpcalc`).
- **Valider sur le serveur** : `entracte analyser-intro <dossier d'une saison>` calcule les empreintes des vidéos du dossier, les
  compare et affiche les bornes par épisode, sans base de données ni écriture. Dans le conteneur :
  `docker exec <conteneur entracte-api> /app/entracte analyser-intro "/srv/medias/<série>/<saison>"`.

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
- Accepter envoie le titre à Radarr (film surveillé, recherche lancée) ou à Sonarr (saisons demandées surveillées, recherche lancée).
  L'administrateur choisit l'emplacement (dossier racine), le profil de qualité et les tags parmi ceux de Radarr/Sonarr
  (`GET /admin/requests/options`). Les valeurs proposées sont celles que Radarr/Sonarr utilise déjà : la configuration d'Entracte
  (`ENTRACTE_RADARR_ROOT_FOLDER`, `ENTRACTE_RADARR_QUALITY_PROFILE`, idem `SONARR`) si elle y existe, sinon le dossier et le profil les plus
  utilisés par les titres de sa bibliothèque, les tags que porte plus de la moitié d'entre eux et, pour Radarr, la disponibilité minimale
  la plus utilisée (`released` à défaut) ; sans bibliothèque, les premiers déclarés. Un titre ajouté sans les tags habituels peut n'être
  cherché par aucun indexeur : dans Radarr/Sonarr, un indexeur qui porte des tags ne sert qu'aux titres qui en partagent un.
  Un envoi en échec laisse la demande « acceptée » avec l'erreur, et « Renvoyer » rouvre le choix.
- « Disponible » : quand la synchronisation voit arriver le fichier (film), ou tous les épisodes annoncés par TMDB des saisons demandées.
  Une série demandée entière se ferme quand toutes les saisons dont TMDB annonce des épisodes sont complètes ; une saison annoncée plus
  tard sans épisode ne la bloque pas, une saison en cours de diffusion la garde ouverte jusqu'à son dernier épisode annoncé.

## Pilotage de Radarr et Sonarr

L'administration d'Entracte (`entracte:admin`, routes `/admin/arr/…`) fait le travail quotidien de Radarr et Sonarr ; leurs propres
interfaces ne servent plus qu'aux réglages (indexeurs, profils de qualité, clients de téléchargement, dossiers racines, listes d'import).

- **Suivi d'une demande** : une demande acceptée affiche l'avancement de ce que la file télécharge pour elle (même titre, et pour une
  demande de saisons, épisodes de ces saisons seulement), « bloqué » avec le motif, ou « sans téléchargement » quand le titre est
  surveillé, sans fichier et absent de la file. La file est relue toutes les 15 secondes tant que l'onglet est affiché.
- **Recherche automatique** (`POST /admin/arr/{service}/search`) : un film, une série, une saison, des épisodes ou tous les manquants ;
  le service choisit et récupère lui-même.
- **Recherche interactive** (`GET` puis `POST /admin/arr/{service}/releases`) : le service interroge tous ses indexeurs (de dix secondes
  à plus d'une minute, délai de 150 s côté Entracte et de 180 s dans l'app) et rend les releases avec qualité, langues, taille, sources,
  score et motifs de rejet ; l'administrateur en envoie une au client de téléchargement. Une release rejetée se force après
  confirmation. Le service ne garde une recherche qu'un temps : une release ancienne se récupère après une nouvelle recherche.
- **Téléchargements** : file complète des deux services, « Autre version » (retrait, liste noire et nouvelle recherche), « Retirer »
  (sans rien bloquer), import manuel d'un téléchargement bloqué (seuls les fichiers que le service liste pour ce téléchargement et
  rattache à un titre), historique des récupérations, liste noire.
- **Manquants** : titres surveillés, sortis, sans fichier (200 au plus par service, le total réel est affiché).
- **Catalogue et fiche** : tous les titres de Radarr et Sonarr ; par titre, surveillance, profil de qualité, nouvelles saisons,
  surveillance par saison et par épisode, historique, suppression (fichiers et exclusion des listes d'import au choix : les demandes
  en cours pour ce titre sont refusées avec le motif saisi et leurs demandeurs prévenus). Ajout d'un titre sans demande, avec
  l'emplacement, le profil, les tags et les saisons surveillées.
- Un service injoignable n'empêche pas l'autre de répondre : il est signalé dans la vue. Sans `ENTRACTE_RADARR_*` ni
  `ENTRACTE_SONARR_*`, ces rubriques ne s'affichent pas.
- Non couvert : changer le dossier d'un titre (déplacement de fichiers), renommer des fichiers, les réglages de Radarr/Sonarr,
  l'état des indexeurs et le client de téléchargement lui-même (voir [Régie](../regie/README.md)).

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
- Front : `ENTRACTE_PUBLIC_URL` de `plbls.env`, lu au démarrage d'`estompe-web` et au build des apps natives (une variable de Dokploy l'emporte pour le web, `VITE_ENTRACTE_URL` pour les apps).

## Version

Composant `entracte` : version du `Cargo.toml` d'`api/entracte`, tag `entracte-vX.Y.Z` (image `ghcr.io/plblsfr/entracte`, `:edge` sur `main`).
Pas d'entrée dans `.version`, propre à Estompe. La section de l'app suit les versions de l'app (`estompe-app-v*`).
