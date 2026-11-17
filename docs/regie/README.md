# Régie

Administration des médias de plbls.fr, à part d'Entracte et de Sillon : un seul endroit pour suivre Radarr, Sonarr, Lidarr, Prowlarr et
qBittorrent, sans ouvrir leurs interfaces. Ce document décrit ce qui existe aujourd'hui : le socle du service et l'écran de santé.

| Où | Quoi |
|---|---|
| [`api/regie`](../../api/regie/README.md) | API Rust (axum), membre du workspace Cargo, sans base de données |
| [`openapi.yaml`](openapi.yaml) | routes, vérifiées par un test (chaque route du code y figure, et rien d'autre) |
| [`deploy/regie/.env.example`](../../deploy/regie/.env.example) | secrets, service `regie-api` de [`deploy/docker-compose.yml`](../../deploy/docker-compose.yml) |
| `web/estompe/src/sections/regie/` | section Régie de l'app (web, PWA, Windows, Android) |

## Permission

Déclarée à aegis par manifeste (application `regie`, renvoyé toutes les 10 minutes), fermée par défaut :

| Permission | Ouvre |
|---|---|
| `regie:admin` | état des services, test d'un indexeur, retrait ou relance d'un téléchargement bloqué |

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

Clients : `X-Api-Key` pour Radarr et Sonarr (`/api/v3`), Lidarr et Prowlarr (`/api/v1`). qBittorrent : connexion par
`POST /api/v2/auth/login`, cookie de session gardé en mémoire et renouvelé sur un 401 ou un 403.

## Section de l'app

Entrée « Régie » du rail de la messagerie, visible seulement avec `regie:admin` (`GET /me` de Régie, `REGIE_PUBLIC_URL`) ; `#/regie`
ramène à l'accueil sinon. La page relit l'état à l'ouverture, au bouton « Actualiser » et après chaque action.

## Actions manuelles

- `plbls.fr-infra` : sous-domaine `regie-api.plbls.fr` et sa route ; Radarr, Sonarr, Lidarr, Prowlarr et qBittorrent joignables depuis
  la stack (réseau `MEDIAS_NETWORK`), aux adresses de `plbls.env` (`REGIE_*_URL`).
- Vault, `secret/regie` : `AEGIS_TOKEN_REGIE`, `REGIE_RADARR_CLE`, `REGIE_SONARR_CLE`, `REGIE_LIDARR_CLE`, `REGIE_PROWLARR_CLE`,
  `REGIE_QBITTORRENT_MOT_DE_PASSE`.
- qBittorrent : compte de l'interface web (`REGIE_QBITTORRENT_UTILISATEUR`), sans contournement d'authentification nécessaire.
- aegis : rien à déclarer (il lit `AEGIS_TOKEN_REGIE` comme application `regie`), puis `regie:admin` aux comptes administrateurs.
- Front : `REGIE_PUBLIC_URL` de `plbls.env`, lu au démarrage d'`estompe-web` et au build des apps natives (`VITE_REGIE_URL` pour les apps).

## Version

Composant `regie` : version du `Cargo.toml` d'`api/regie`, tag `regie-vX.Y.Z` (image `ghcr.io/plblsfr/regie`, `:edge` sur `main`).
Pas d'entrée dans `.version`, propre à Estompe. La section de l'app suit les versions de l'app (`estompe-app-v*`).
