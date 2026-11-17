# regie

API de Régie, administration des médias hors d'Estompe : documentation fonctionnelle dans [`docs/regie`](../../docs/regie/README.md),
routes dans [`openapi.yaml`](../../docs/regie/openapi.yaml). Pas de base de données : l'état des dépôts est sur le disque.

| Module | Rôle |
|---|---|
| `config.rs` | variables `REGIE_*` et communes (valeurs dans [`plbls.env`](../../plbls.env), secrets dans [`deploy/regie/.env.example`](../../deploy/regie/.env.example)) ; un service sans URL ou sans clé n'est pas configuré |
| `routes.rs` | table des routes : chacune a une permission ou une exception motivée ; tests de couverture et de conformité à OpenAPI (dans les deux sens) |
| `garde.rs`, `identite.rs`, `droits.rs` | gateway (`/auth/me`, cookie ou Bearer), aegis (manifeste, `/v1/effective`), CORS et CSRF |
| `etat.rs` | état partagé (configuration, client HTTP, aegis, session qBittorrent, suivi des téléchargements) et envoi périodique du manifeste |
| `interne.rs` | santé, `GET /me` |
| `http.rs` | réponses JSON et erreurs `{error, message}` |
| `arr.rs` | client de Radarr, Sonarr (`/api/v3`), Lidarr et Prowlarr (`/api/v1`) : version, alertes, dossiers racine, éléments bloqués de la file, retrait, titres de la file et de l'historique (`titre_de`), import manuel (`import_manuel`, `importer`), liste d'une bibliothèque, indexeurs et test d'un indexeur |
| `qbittorrent.rs` | client de l'API web de qBittorrent : clé d'API en `Bearer`, ou connexion et cookie de session réutilisé ; attente croissante après un refus ; version, débits, instantané des torrents (`instantane`), actions, ajout d'un torrent (corps `multipart` écrit à la main) |
| `suivi.rs` | relecture périodique de qBittorrent tant qu'un lecteur attend (`assurer`), instantané versionné diffusé par un canal `watch`, rattachement des torrents aux titres |
| `telechargements.rs` | `GET /admin/downloads` (attente du prochain changement), actions sur les torrents, limites globales, fiche et priorités des fichiers ; validation des corps |
| `bencode.rs` | lecture d'un fichier `.torrent` (`Torrent::lire`) : nom, fichiers, empreinte v1, refus des chemins dangereux |
| `depots.rs` | dépôts : un dossier par dépôt, nommé par un identifiant tiré par Régie (`depot.json`, `source.torrent`, `contenu/`), jamais par une valeur d'une requête, réception par morceaux, ajout à qBittorrent et surveillance de la vérification (`surveiller`), import manuel dans Radarr, Sonarr ou Lidarr, recherche dans une bibliothèque |
| `fiche.rs` | fiche technique d'un titre (`composer`) à partir du film, de la série ou de l'album et de ses fichiers tels que le service les décrit ; noms de fichiers seuls |
| `sante.rs` | `GET /admin/status` (relevé des cinq services en parallèle), test d'un indexeur, retrait d'un élément de la file |

## Développement

```sh
just test-regie      # services, gateway et aegis simulés, sans base ni Docker
just regie-dev-run   # 127.0.0.1:8094, variables de deploy/dev.env
```
