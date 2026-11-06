# permission (permission.plbls.fr)

Gestion des **permissions** des comptes plbls.fr (RBAC par application). Les comptes et l'authentification sont dans [security.plbls.fr](../security) (auth-gateway + Keycloak). Rust + Axum + Postgres, frontend Svelte 5.

- `backend/` : API REST (`/v1/...`), spec OpenAPI générée depuis le code (`/openapi.json`), serveur MCP (`/mcp`), sert aussi le front compilé ; membre du workspace Cargo racine (`../../Cargo.toml`), image construite depuis la racine du dépôt (`make image`)
- `frontend/` : interface d'administration Svelte 5 + plbls-ui (applications, permissions, rôles, attributions, journal)
- `docs/` : modèle, intégration, architecture (embarquées dans le MCP)
- `scripts/smoke.py` : test de bout en bout de toute l'API

Modèle et intégration : [docs/](docs/README.md).

## Développement

```sh
make dev-up      # Postgres de dev (Docker)
make dev-run     # backend sur 127.0.0.1:8090
make seed        # données de départ : applications chat et radar (autre terminal)
make front-dev   # front sur http://127.0.0.1:3000, Vite dans Docker (pas besoin de Node)
make smoke       # test de bout en bout de l'API
make e2e         # test du front dans Chromium headless (Docker) ; démarre et arrête tout seul
make e2e-gateway # idem avec le vrai auth-gateway + Keycloak de ../security.plbls.fr (connexion admin et demo)
make test        # tests unitaires Rust
npm run check --workspace apps/permission/frontend   # svelte-check (depuis la racine du dépôt, après npm ci)
npm test --workspace apps/permission/frontend        # tests unitaires du front (vitest + Testing Library)
```

En dev, le proxy de Vite signe lui-même les en-têtes d'identité comme auth-gateway (secret de `deploy/dev.env`) : ni gateway ni Keycloak nécessaires. Utilisateur et rôles modifiables : `DEV_USER_ID`, `DEV_USER_ROLES` (mettre `user` pour voir l'écran « accès réservé »).

**Avec le vrai gateway et Keycloak** : dans `../security.plbls.fr`, `make dev-up dev-start` (le realm de dev contient le rôle `permission-admin`, donné à `admin/admin`, et la route `/api/permission` vers :8090 est dans `config/dev.toml`), puis `DEV_VIA_GATEWAY=1` pour Vite. Connexion sur http://localhost:3000 : `admin/admin` administre, `demo/demo` voit « accès réservé ». `make e2e-gateway` automatise ce parcours.

## Design

L'interface utilise le design system **plbls-ui** (`https://cdn.plbls.fr/ui/2/`) : classes `plbls-*`, jetons, web components (`plbls-modal`, toasts). Règle du kit : une application ne redessine pas ses composants, elle ajoute au kit ce qui manque. `frontend/src/app.css` ne contient que de la mise en page.

Le kit est copié dans `frontend/src/vendor/plbls-ui/` (fichiers commités, générés depuis `packages/ui` par `npm run ui:sync` à la racine du dépôt ; la CI échoue si la copie diverge). Le kit (v2) gère le thème sombre (`prefers-color-scheme`, ou `data-theme` sur `<html>`). Le frontend fait partie du workspace npm de la racine (`npm ci` à la racine) ; l'image se construit depuis la racine du dépôt (`make image`, ou `docker build -f apps/permission/Dockerfile .`).

## Production (Dokploy)

Le dépôt n'est **jamais cloné** en production : la CI (`.github/workflows/permission-ci.yml`) teste, puis construit l'image `ghcr.io/plblsfr/permission` : `edge` à chaque push sur `main`, `X.Y.Z`, `latest` et le SHA du commit sur un tag `permission-vX.Y.Z`. Dokploy tire cette image ; elle contient le backend, la config par défaut et le front compilé.

**Réglages du dépôt GitHub** : aucun secret n'est obligatoire (la CI utilise `GITHUB_TOKEN`). Le secret `DOKPLOY_WEBHOOK_URL` est optionnel : URL de déploiement de l'application Dokploy, appelée après la publication d'un tag.

**Base de données** : une instance Postgres existante (13 ou plus). Créer l'utilisateur et la base, en superuser :

```sql
CREATE ROLE permission LOGIN PASSWORD '<mot-de-passe aléatoire, sans @ : / ? # %>';
CREATE DATABASE permission OWNER permission;
```

Le propriétaire de la base est nécessaire : les migrations créent leurs tables dans `public` au démarrage (PostgreSQL 15+ ne l'autorise qu'au propriétaire). Aucune extension requise.

**Côté Dokploy** : application de type *Docker* (ou *Compose* avec `docker-compose.yml` collé tel quel), image `ghcr.io/plblsfr/permission:latest` (ou `X.Y.Z` pour figer une version), identifiants du registre `ghcr.io` (PAT avec `read:packages`), port `8080`, sonde `GET /health`. Variables d'environnement (voir `.env.example`) :

- `PERMISSION__DATABASE__URL` = `postgres://permission:<mot-de-passe>@<hôte-postgres>:5432/permission` (`?sslmode=require` si l'instance l'impose)
- `PERMISSION__IDENTITY__SECRET` (le même que `GATEWAY__IDENTITY__SECRET`)
- `PERMISSION__SERVICES__TOKENS` et `PERMISSION__MCP__TOKEN` (optionnels)

Les migrations s'appliquent au démarrage. Avant le premier usage : rôle Keycloak `permission-admin`, route `/api/permission` dans le gateway ([docs/integration.md](docs/integration.md)), domaine `permission.plbls.fr`.

## Configuration

`backend/config/default.toml`, puis variables `PERMISSION__SECTION__CLE`. Secrets uniquement dans l'environnement.

Serveur MCP : `claude mcp add --transport http permission https://permission.plbls.fr/mcp --header "Authorization: Bearer <token>"`.
