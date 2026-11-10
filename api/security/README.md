# auth-gateway (security.plbls.fr)

BFF et reverse proxy d'authentification pour plbls.fr, devant Keycloak. Rust + Axum + Redis.
Architecture et décisions : [docs/security/](../../docs/security/README.md). Déploiement : [`deploy/`](../../deploy/README.md) (compose de production et de dev, `dev.env`, `security/.env.example`, realm Keycloak de dev dans `security/keycloak`) ; scripts de dev et de test : `scripts/security/`.

## Développement

Depuis la racine du dépôt (recettes du `justfile`) :

```sh
just security-dev up       # Redis, Keycloak (realm plbls), Mailpit, service d'écho (Docker)
just security-dev start    # build et lance le gateway en arrière-plan (logs : just security-dev logs)
just security-dev down     # arrête tout (reset supprime aussi les volumes)
```

- Keycloak : http://localhost:8180 (admin / admin). Utilisateurs de test : `demo/demo` (rôle `user`), `admin/admin` (`user`, `admin`).
- Mails (inscription, reset) : Mailpit http://localhost:8025.
- Inscription web : http://localhost:8080/auth/register?redirect=http://localhost:3000/
- Login web : http://localhost:8080/auth/login?redirect=http://localhost:3000/
- Bearer de test : client `plbls-mobile` (direct access grants activés **en dev uniquement**).
- Routes de dev : `config/dev.toml` (`/api/whoami` public, `/api/private`, `/api/admin` rôle `admin`) vers un service qui renvoie les en-têtes reçus ; WebSocket sur `/api/whoami/echo`.
- Tests : `just test-security` (unitaires), `just security-coverage` (chaque route est couverte par la suite Bruno de `api/security/bruno/`), puis, gateway lancé, `just security-bruno`, `security-ws`, `security-refresh`, `security-regressions` ; `just security-test-all` enchaîne tout. Le crate est membre du workspace Cargo racine du monorepo (`../../Cargo.toml`, un seul `Cargo.lock`, `rust-toolchain.toml` et `deny.toml` communs) ; binaire dans `../../target/`. Image : `docker build -f api/security/Dockerfile .` depuis la racine du dépôt.

## Production

Service `auth-gateway` (et `security-redis`) de la stack [`deploy/docker-compose.yml`](../../deploy/docker-compose.yml) ; variables dans `deploy/security/.env.example` (secrets dans `secret/security` de Vault, voir [`deploy/README.md`](../../deploy/README.md)).

Les routes des micro-services sont dans `config/routes.toml` (modèle : `config/routes.example.toml`), embarqué dans l'image : modifier une route = commit, CI, redéploiement.

L'image est testée en PR (`.github/workflows/ci.yml`) et publiée par `.github/workflows/livraison.yml` : `edge` à chaque push sur `main`, puis `X.Y.Z`, `latest` et le SHA du commit sur un tag `security-vX.Y.Z` (la version du tag doit égaler celle de `Cargo.toml`). Pour figer une version : `GATEWAY_TAG=X.Y.Z` dans le `.env` du compose. Aucun secret à ajouter : la CI utilise `GITHUB_TOKEN` (secret optionnel `DOKPLOY_WEBHOOK_URL`).

Le conteneur écoute sur le port `8080` (domaine Dokploy `security.plbls.fr`) : le TLS est terminé par le reverse proxy, qui doit ajouter l'IP cliente en dernière entrée de `X-Forwarded-For` (le gateway limite `/auth/login|register|callback` par IP, `rate_limit.auth_per_minute`) et limiter aussi le débit de Keycloak (`auth.plbls.fr`).

## Configuration

`config/default.toml`, puis le fichier `GATEWAY_CONFIG`, puis les variables `GATEWAY__SECTION__CLE` (ex. `GATEWAY__OIDC__ISSUER`). Les secrets ne sont que dans l'environnement.

## Contrat avec les micro-services

Voir [docs/security/gateway.md](../../docs/security/gateway.md). Les rôles transmis dans `X-User-Roles` sont les rôles realm tels quels et les rôles client sous la forme `client:role`. `src/identity.rs` (`verify`) est la référence pour vérifier `X-Identity-Sig`.
