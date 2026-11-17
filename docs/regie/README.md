# Régie

Administration des médias de plbls.fr, à part d'Entracte et de Sillon : un seul endroit pour suivre Radarr, Sonarr, Lidarr, Prowlarr et
qBittorrent, sans ouvrir leurs interfaces. Ce document décrit ce qui existe aujourd'hui : le socle du service, l'écran de santé, les téléchargements, les dépôts et la fiche technique.

| Où | Quoi |
|---|---|
| [`api/regie`](../../api/regie/README.md) | API Rust (axum), membre du workspace Cargo, sans base de données (l'état d'un dépôt est dans son dossier) |
| [`openapi.yaml`](openapi.yaml) | routes, vérifiées par un test (chaque route du code y figure, et rien d'autre) |
| [`deploy/regie/.env.example`](../../deploy/regie/.env.example) | secrets, service `regie-api` de [`deploy/docker-compose.yml`](../../deploy/docker-compose.yml) |
| `web/estompe/src/sections/regie/` | section Régie de l'app (web, PWA, Windows, Android) |

## Permission

Déclarée à aegis par manifeste (application `regie`, renvoyé toutes les 10 minutes), fermée par défaut :

| Permission | Ouvre |
|---|---|
| `regie:admin` | état des services, test d'un indexeur, retrait ou relance d'un téléchargement bloqué, suivi et pilotage des téléchargements de qBittorrent, dépôts, fiche technique |

Chaque route relève de cette permission ou d'une exception motivée (santé, `GET /me`) : la table des routes l'impose
(`api/regie/src/routes.rs`) et un test parcourt toutes les routes (401 sans session ou avec une session invalide, 403 sans la permission).
aegis injoignable : 503 `aegis_unavailable` sur les routes soumises à la permission ; `GET /me` répond quand même, `admin` faux.
Sans `AEGIS_TOKEN_REGIE`, tout est refusé.

Authentification : cookie de session du gateway (`__Secure-plbls_sid`) ou `Authorization: Bearer`, vérifié par `GET /auth/me` du gateway
(cache de 30 s). Requête modifiante avec cookie : `Origin` dans `REGIE_ALLOWED_ORIGINS`, sinon 403 `bad_origin` ; CORS avec
identifiants pour ces seules origines.

## Santé

`GET /admin/status` interroge les cinq services en parallèle et répond toujours : un service non configuré ou injoignable est
annoncé comme tel, sans bloquer les autres.

- **Services** : joignable ou non, version, alertes de santé que le service remonte (`/health` de Radarr, Sonarr, Lidarr et Prowlarr ;
  les alertes `ok` sont omises). qBittorrent : version, débits de réception et d'envoi, état de la connexion.
- **Indexeurs** (Prowlarr) : activé ou non, protocole, confidentialité, en panne (présent dans `/indexerstatus`), dernier échec et
  suspension. `POST /admin/indexers/{id}/test` relance le test de Prowlarr sur l'indexeur tel qu'il est configuré et rend sa raison en cas d'échec.
- **Disques** : dossiers racine de Radarr, Sonarr et Lidarr, espace libre, accessibilité.
- **Téléchargements bloqués** : éléments des files de Radarr, Sonarr et Lidarr en avertissement ou en erreur (import refusé,
  téléchargement échoué), avec les messages du service. `DELETE /admin/queue/{service}/{id}` retire l'élément du service et du
  client de téléchargement ; avec `?retry=true`, la release est mise en liste de blocage et le service en cherche une autre.

Régie n'expose que ces routes : pas de proxy vers les services, pas d'accès à leurs réglages. Aucune réponse ne contient d'URL ni de clé
d'un service ; une erreur de connexion est rendue sans l'adresse.

## Téléchargements

`GET /admin/downloads` rend les torrents de qBittorrent (`/api/v2/sync/maindata`), ses catégories, ses tags et ses débits.

- **En direct** : avec `?since=<version>`, la réponse attend le prochain changement (20 s au plus) ; l'app enchaîne les appels, sans
  intervalle de son côté. Régie relit qBittorrent toutes les `REGIE_SUIVI_MS` (1,5 s) tant qu'un appel attend, et s'arrête sinon.
  Une seule relecture sert tous les écrans ouverts. qBittorrent injoignable : le dernier instantané reste servi, avec `error`.
- **Rattachement aux titres** : un torrent présent dans la file de Radarr, Sonarr ou Lidarr (même identifiant de téléchargement) porte
  son film, son épisode ou son album ; les files sont relues toutes les 10 s. Un torrent terminé n'y est plus : la fiche d'un torrent
  (`GET /admin/downloads/{hash}`) cherche alors son titre dans l'historique des trois services. Les autres gardent le nom du torrent.
- **Actions** : suspendre, reprendre (routes `stop`/`start` de qBittorrent 5, `pause`/`resume` de la version 4 en repli), revérifier,
  supprimer avec ou sans les fichiers, changer la catégorie (parmi celles de qBittorrent), ajouter ou retirer des tags, limiter le débit
  d'un torrent, limites globales et mode alternatif (`PUT /admin/transfer`), priorité des fichiers d'un torrent.
  Chaque corps est validé avant tout appel à qBittorrent (identifiants de torrent, bornes, catégorie connue).

Ne sont pas exposés : les préférences de qBittorrent (dont le programme lancé en fin de téléchargement), le changement d'emplacement
d'un torrent, la création de catégories, l'ajout d'un torrent par lien.

## Dépôts

Mettre en partage un contenu que l'on a soi-même publié : le tracker donne le `.torrent`, l'app l'envoie avec ses fichiers.

1. **Le `.torrent`** (`POST /admin/deposits`, 10 Mo au plus) est lu par Régie : nom, fichiers, tailles, empreinte v1, tracker (nom d'hôte
   seul). Sont refusés un torrent illisible ou sans empreintes v1, un nom ou un chemin dangereux (`..`, `/`, caractère de contrôle),
   deux fichiers de même chemin, plus de 5000 fichiers. L'identifiant du dépôt est tiré au hasard par Régie (32 chiffres hexadécimaux) et nomme son
   dossier ; redéposer le même `.torrent` (même empreinte) rend le dépôt existant. Le total des tailles annoncées ne dépasse pas `REGIE_DEPOTS_GO`.
2. **Les fichiers** arrivent par morceaux (`PUT /admin/deposits/{id}/files/{index}?offset=`, 32 Mo au plus, 16 Mo envoyés par l'app),
   écrits directement sous le nom que le torrent attend : `<REGIE_DEPOTS>/<id>/contenu/<chemin du torrent>`. Aucun chemin n'est construit depuis une valeur d'une requête : un dépôt
   demandé par son identifiant est cherché parmi les dossiers existants, et son `.torrent` est relu du disque. Un morceau n'est accepté
   qu'à la position déjà reçue, sinon 409 avec cette position : un envoi coupé reprend où il s'est arrêté, même après fermeture de la page.
   Avant chaque écriture, le dossier réel de la cible (liens symboliques résolus) doit être sous `REGIE_DEPOTS`, et la cible ne doit pas
   être un lien : un lien posé dans le dossier par un autre service ne fait rien écrire ailleurs.
   L'app associe les fichiers choisis à ceux du torrent par la taille, puis par le chemin : leur nom sur l'appareil n'a pas à être le bon.
3. **La vérification** (`POST /admin/deposits/{id}/seed`) : le torrent est ajouté à qBittorrent à l'arrêt, sur le dossier du dépôt
   (tag `regie`), et revérifié. À 100 %, il démarre (`seeding`). Sinon il reste à l'arrêt (`mismatch`) et chaque fichier porte sa part
   valide : on remplace les fichiers en cause (`reset=true`), puis on relance. Rien n'est jamais téléchargé pour compléter.
4. **L'import** : Radarr, Sonarr ou Lidarr analyse le dossier du dépôt (`/manualimport`) ; l'app montre ce qu'il reconnaît et pourquoi il
   écarte le reste, puis confirme (`ManualImport` en mode copie : un lien physique quand le service y est autorisé, le fichier reste en
   partage). Pour un film, on peut rattacher tout le dépôt à un film de Radarr choisi à la main. Le titre doit déjà exister dans le service.

`GET /admin/deposits/{id}?wait=<état>` attend le changement d'état (20 s au plus) : l'app suit la vérification sans intervalle.
L'état d'un dépôt (`depot.json`) et son `.torrent` (`source.torrent`) sont dans son dossier : il n'y a pas de base de données.
Supprimer un dépôt retire le torrent de qBittorrent et efface son dossier ; les fichiers importés par lien physique restent en bibliothèque.

## Fiche technique

Pour préparer une publication sur un tracker : `GET /admin/library/{service}?q=` cherche un film, une série ou un album dans Radarr,
Sonarr ou Lidarr, et `GET /admin/library/{service}/{id}/sheet` rend sa fiche : titre, année, durée, genres, identifiants (TMDB, IMDb,
TVDB, MusicBrainz), et pour chaque fichier son nom, sa taille, sa qualité, son groupe, sa vidéo (codec, définition, profondeur, plage
dynamique, cadence, débit), son audio (codec, canaux, langues, débit), ses sous-titres et sa durée. Tout vient de ce que le service a
déjà analysé : Régie ne lit aucun fichier de la bibliothèque et ne rend jamais de chemin, seulement des noms de fichiers.
L'app en fait un texte à copier (`sheetText`), limité à une saison pour une série.

Clients : `X-Api-Key` pour Radarr et Sonarr (`/api/v3`), Lidarr et Prowlarr (`/api/v1`). qBittorrent : clé d'API
`REGIE_QBITTORRENT_CLE` en `Authorization: Bearer` (qBittorrent 5.2 ou plus récent), sans connexion ni cookie. Sans clé, connexion par
`POST /api/v2/auth/login` avec le compte de l'interface web, cookie de session gardé en mémoire et renouvelé sur un 401 ou un 403.
Un refus (clé ou identifiants refusés, adresse bannie) n'est pas retenté aussitôt : Régie attend 30 s, puis le double à chaque nouveau
refus, 15 minutes au plus, et l'écran affiche la raison et le délai. qBittorrent bannit en effet une adresse après quelques échecs.

## Section de l'app

Entrée « Régie » du rail de la messagerie, visible seulement avec `regie:admin` (`GET /me` de Régie, `REGIE_PUBLIC_URL`) ; `#/regie`
ramène à l'accueil sinon. La page « Santé » relit l'état à l'ouverture, au bouton « Actualiser » et après chaque action ; « Téléchargements » (`#/regie/downloads`) suit
qBittorrent en direct tant qu'elle est affichée (`Suivi`, `regie.svelte.ts`) ; « Dépôts » (`#/regie/deposits`) envoie les fichiers par
morceaux (`envoyerFichier`) tant que la page reste ouverte, sur le web et l'app Windows ; « Fiche technique » (`#/regie/sheet`) cherche un
titre et affiche sa fiche à copier.

## Actions manuelles

- `plbls.fr-infra` : sous-domaine `regie-api.plbls.fr` et sa route ; Radarr, Sonarr, Lidarr, Prowlarr et qBittorrent joignables depuis
  la stack (réseau `MEDIAS_NETWORK`), aux adresses de `plbls.env` (`REGIE_*_URL`).
- Vault, `secret/regie` : `AEGIS_TOKEN_REGIE`, `REGIE_RADARR_CLE`, `REGIE_SONARR_CLE`, `REGIE_LIDARR_CLE`, `REGIE_PROWLARR_CLE`,
  `REGIE_QBITTORRENT_CLE`.
- Dépôts : un dossier `REGIE_DEPOTS` sur le même système de fichiers que les bibliothèques, monté en écriture dans `regie-api` et visible
  de qBittorrent et de Radarr, Sonarr, Lidarr (`REGIE_DEPOTS_QBITTORRENT`, `REGIE_DEPOTS_ARR` s'ils le montent sous un autre chemin).
  `REGIE_GID` (onglet Environment) : groupe commun à ces services. Régie écrit sous ce groupe et ouvre ses fichiers au groupe (664, dossiers
  775) : sans cela, Linux refuse le lien physique d'un fichier que le service ne peut pas écrire, et l'import se rabat sur une copie.
- qBittorrent 5.2 ou plus récent : générer la clé dans Préférences > WebUI > API Key. Version plus ancienne : laisser la clé vide et
  donner `REGIE_QBITTORRENT_UTILISATEUR` et `REGIE_QBITTORRENT_MOT_DE_PASSE`.
- aegis : rien à déclarer (il lit `AEGIS_TOKEN_REGIE` comme application `regie`), puis `regie:admin` aux comptes administrateurs.
- Front : `REGIE_PUBLIC_URL` de `plbls.env`, lu au démarrage d'`estompe-web` et au build des apps natives (`VITE_REGIE_URL` pour les apps).

## Version

Composant `regie` : version du `Cargo.toml` d'`api/regie`, tag `regie-vX.Y.Z` (image `ghcr.io/plblsfr/regie`, `:edge` sur `main`).
Pas d'entrée dans `.version`, propre à Estompe. La section de l'app suit les versions de l'app (`estompe-app-v*`).
