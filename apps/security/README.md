# auth-gateway (security.plbls.fr)

BFF et reverse proxy d'authentification pour plbls.fr, devant Keycloak. Rust + Axum + Redis.
Architecture et décisions : [docs/](docs/README.md).

## Développement

```sh
docker compose -f deploy/docker-compose.dev.yml up -d   # Redis, Keycloak (realm plbls), service d'écho
set -a; . deploy/dev.env; set +a
cargo run
```

- Keycloak : http://localhost:8180 (admin / admin). Utilisateurs de test : `demo/demo` (rôle `user`), `admin/admin` (`user`, `admin`).
- Mails (inscription, reset) : Mailpit http://localhost:8025.
- Inscription web : http://localhost:8080/auth/register?redirect=http://localhost:3000/
- Login web : http://localhost:8080/auth/login?redirect=http://localhost:3000/
- Bearer de test : client `plbls-mobile` (direct access grants activés **en dev uniquement**).
- Routes de dev : `config/dev.toml` (`/api/whoami` public, `/api/private`, `/api/admin` rôle `admin`) vers un service qui renvoie les en-têtes reçus ; WebSocket sur `/api/whoami/echo`.
- Tests : `cargo test`. Le crate est membre du workspace Cargo racine du monorepo (`../../Cargo.toml`, un seul `Cargo.lock`, `rust-toolchain.toml` et `deny.toml` communs) ; binaire dans `../../target/`. Image : `docker build -f apps/security/Dockerfile .` depuis la racine du dépôt.

## Production

```sh
cp .env.example .env                              # issuer Keycloak, secrets
docker login ghcr.io                              # une fois, PAT avec read:packages
docker compose pull && docker compose up -d
```

Les routes des micro-services sont dans `config/routes.toml` (modèle : `config/routes.example.toml`), embarqué dans l'image : modifier une route = commit, CI, redéploiement.

L'image est construite par la CI (`.github/workflows/security-ci.yml`) : `edge` à chaque push sur `main`, puis `X.Y.Z`, `latest` et le SHA du commit sur un tag `security-vX.Y.Z` (la version du tag doit égaler celle de `Cargo.toml`). Pour figer une version : `GATEWAY_TAG=X.Y.Z docker compose up -d`. Aucun secret à ajouter : la CI utilise `GITHUB_TOKEN` (secret optionnel `DOKPLOY_WEBHOOK_URL`).

Le conteneur écoute sur `127.0.0.1:8080` : le TLS est terminé par le reverse proxy de `security.plbls.fr`, qui doit ajouter l'IP cliente en dernière entrée de `X-Forwarded-For` (le gateway limite `/auth/login|register|callback` par IP, `rate_limit.auth_per_minute`) et limiter aussi le débit de Keycloak (`auth.plbls.fr`).

## Configuration

`config/default.toml`, puis le fichier `GATEWAY_CONFIG`, puis les variables `GATEWAY__SECTION__CLE` (ex. `GATEWAY__OIDC__ISSUER`). Les secrets ne sont que dans l'environnement.

Serveur MCP (documentation pour les assistants) : `POST https://security.plbls.fr/mcp` avec `Authorization: Bearer <GATEWAY__MCP__TOKEN>`. Désactivé si le token est vide. Exemple : `claude mcp add --transport http auth-gateway https://security.plbls.fr/mcp --header "Authorization: Bearer <token>"`.

## Contrat avec les micro-services

Voir [docs/gateway.md](docs/gateway.md). Les rôles transmis dans `X-User-Roles` sont les rôles realm tels quels et les rôles client sous la forme `client:role`. `src/identity.rs` (`verify`) est la référence pour vérifier `X-Identity-Sig`.
