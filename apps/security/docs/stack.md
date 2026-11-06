# Stack technique

Rust + Axum, un seul binaire statique d'environ 10 à 20 Mo, déployé en conteneur distroless à côté d'un Redis, via Docker Compose.

| Brique | Choix | Pourquoi |
| --- | --- | --- |
| Serveur HTTP | `axum` 0.8 + `tokio` | Standard de fait en Rust, middlewares `tower` |
| Middlewares | `tower-http` | CORS, logs, `X-Request-Id` |
| Cookies | `axum-extra` (cookie) | Lecture et écriture du cookie de session |
| Client HTTP | `reqwest` (rustls) | Appels Keycloak et proxy en streaming, sans OpenSSL |
| JWT | `jsonwebtoken` | Vérification RS256 via le JWKS |
| Sessions | `redis` (ConnectionManager) | Reconnexion automatique, TTL natif |
| Config | `config` (TOML + env) | Fichier par défaut, secrets en variables `GATEWAY__...` |
| Crypto | `sha2`, `hmac`, `rand` | PKCE, hash des sessions, signature d'identité |

## Découpage prévu

- `settings` : config
- `oidc` : Keycloak (discovery, échange, refresh, JWKS)
- `session` : Redis
- `auth` : cookie ou Bearer, refresh
- `routes/web` : login, callback, logout, me
- `routes/proxy`
- `routes/health`
