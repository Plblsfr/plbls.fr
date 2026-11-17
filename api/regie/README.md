# regie

API de Régie, administration des médias hors d'Estompe : documentation fonctionnelle dans [`docs/regie`](../../docs/regie/README.md),
routes dans [`openapi.yaml`](../../docs/regie/openapi.yaml). Pas de base de données.

| Module | Rôle |
|---|---|
| `config.rs` | variables `REGIE_*` et communes (valeurs dans [`plbls.env`](../../plbls.env), secrets dans [`deploy/regie/.env.example`](../../deploy/regie/.env.example)) ; un service sans URL ou sans clé n'est pas configuré |
| `routes.rs` | table des routes : chacune a une permission ou une exception motivée ; tests de couverture et de conformité à OpenAPI (dans les deux sens) |
| `garde.rs`, `identite.rs`, `droits.rs` | gateway (`/auth/me`, cookie ou Bearer), aegis (manifeste, `/v1/effective`), CORS et CSRF |
| `etat.rs` | état partagé (configuration, client HTTP, aegis, session qBittorrent) et envoi périodique du manifeste |
| `interne.rs` | santé, `GET /me` |
| `http.rs` | réponses JSON et erreurs `{error, message}` |
| `arr.rs` | client de Radarr, Sonarr (`/api/v3`), Lidarr et Prowlarr (`/api/v1`) : version, alertes, dossiers racine, éléments bloqués de la file, retrait, indexeurs et test d'un indexeur |
| `qbittorrent.rs` | client de l'API web de qBittorrent : connexion, cookie de session réutilisé, version, débits |
| `sante.rs` | `GET /admin/status` (relevé des cinq services en parallèle), test d'un indexeur, retrait d'un élément de la file |

## Développement

```sh
just test-regie      # services, gateway et aegis simulés, sans base ni Docker
just regie-dev-run   # 127.0.0.1:8094, variables de deploy/dev.env
```
