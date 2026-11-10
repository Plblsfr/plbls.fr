# permission (permission.plbls.fr)

Gestion des **permissions** des comptes plbls.fr (RBAC par application). Les comptes et l'authentification sont dans [security.plbls.fr](../security/README.md) (auth-gateway + Keycloak). Rust + Axum + Postgres, frontend Svelte 5.

- `api/permission/` (ce dossier) : API REST (`/v1/...`), spec OpenAPI générée depuis le code (`/openapi.json`), sert aussi le front compilé ; membre du workspace Cargo racine (`../../Cargo.toml`), image construite depuis la racine du dépôt (`just permission-image`)
- `web/permission/` : interface d'administration Svelte 5 + plbls-ui (applications, permissions, rôles, attributions, journal)
- `docs/permission/` : modèle, intégration, architecture
- variables de production : [`plbls.env`](../../plbls.env) et les communes de `deploy/.env.example` (permission n'a aucun secret propre) ; compose et dev : [`deploy/`](../../deploy/README.md)
- `scripts/permission/smoke.py` : test de bout en bout de toute l'API

Modèle et intégration : [docs/permission/](../../docs/permission/README.md).

## Développement

Depuis la racine du dépôt (recettes du `justfile`, après `npm ci`) :

```sh
just dev-up                 # dépendances de dev (Docker), dont Postgres
just permission-dev-run     # backend sur 127.0.0.1:8090
just permission-seed        # données de départ : applications chat et radar (autre terminal)
just permission-front-dev   # front sur http://127.0.0.1:3000 (Vite)
just permission-smoke       # test de bout en bout de l'API
just e2e-permission         # e2e Playwright du front (web/permission/e2e) : démarre Postgres, backend, seed et Vite, puis arrête tout
just e2e-permission-gateway # idem avec le vrai auth-gateway (api/security) + Keycloak (connexion admin et demo)
just test-permission        # tests unitaires Rust
npm run check --workspace web/permission   # svelte-check
npm test --workspace web/permission        # tests unitaires du front (vitest + Testing Library)
```

Les e2e sont rangés comme ceux d'Estompe : `web/permission/e2e/*.spec.ts`, `web/permission/playwright.config.ts`, lancés par `npm run e2e --workspace web/permission` (`just e2e-permission`). `e2e/global-setup.ts` démarre Postgres (Docker, ou la base de `E2E_DATABASE_URL`, vide, si elle est définie), compile et lance le backend, puis `scripts/permission/seed.py` ; Playwright lance Vite. `E2E_GATEWAY=1` ne joue que `gateway.spec.ts`, avec le gateway (`api/security`) et Keycloak (Docker). Navigateur : `npx playwright install chromium` la première fois.

En dev, le proxy de Vite signe lui-même les en-têtes d'identité comme auth-gateway (secret de `deploy/dev.env`) : ni gateway ni Keycloak nécessaires. Utilisateur et rôles modifiables : `DEV_USER_ID`, `DEV_USER_ROLES` (mettre `user` pour voir l'écran « accès réservé »).

**Avec le vrai gateway et Keycloak** : `just security-dev up` puis `just security-dev start` (le realm de dev contient le rôle `permission-admin`, donné à `admin/admin`, et la route `/api/permission` vers :8090 est dans `api/security/config/dev.toml`), puis `DEV_VIA_GATEWAY=1` pour Vite. Connexion sur http://localhost:3000 : `admin/admin` administre, `demo/demo` voit « accès réservé ». `just e2e-permission-gateway` automatise ce parcours.

## Design

L'interface utilise le design system **plbls-ui** (`https://cdn.plbls.fr/ui/2/`) : classes `plbls-*`, jetons, web components (`plbls-modal`, toasts). Règle du kit : une application ne redessine pas ses composants, elle ajoute au kit ce qui manque. `web/permission/src/app.css` ne contient que de la mise en page.

Le kit est copié dans `web/permission/src/vendor/plbls-ui/` (fichiers commités, générés depuis `packages/ui` par `npm run ui:sync` à la racine du dépôt ; la CI échoue si la copie diverge). Le kit (v2) gère le thème sombre (`prefers-color-scheme`, ou `data-theme` sur `<html>`). Le frontend fait partie du workspace npm de la racine (`npm ci` à la racine) ; l'image se construit depuis la racine du dépôt (`just permission-image`, ou `docker build -f api/permission/Dockerfile .`).

## Production (Dokploy)

Le dépôt n'est **jamais cloné** en production : la CI teste en PR (`.github/workflows/ci.yml`), puis `.github/workflows/livraison.yml` construit l'image `ghcr.io/plblsfr/permission` : `edge` à chaque push sur `main`, `X.Y.Z`, `latest` et le SHA du commit sur un tag `permission-vX.Y.Z`. Dokploy tire cette image ; elle contient le backend, la config par défaut et le front compilé.

**Réglages du dépôt GitHub** : aucun secret n'est obligatoire (la CI utilise `GITHUB_TOKEN`). Le secret `DOKPLOY_WEBHOOK_URL` est optionnel : URL de déploiement de l'application Dokploy, appelée après la publication d'un tag.

**Base de données** : base `permission` du PostgreSQL partagé (`postgres-main`), appartenant à l'utilisateur commun `DATABASE_USER`. La créer une fois, en superuser :

```sql
CREATE DATABASE permission OWNER plbls;
```

Le propriétaire de la base est nécessaire : les migrations créent leurs tables dans `public` au démarrage (PostgreSQL 15+ ne l'autorise qu'au propriétaire). Aucune extension requise.

**Côté Dokploy** : service `permission` de la stack [`deploy/docker-compose.yml`](../../deploy/docker-compose.yml) (voir [`deploy/README.md`](../../deploy/README.md)), image `ghcr.io/plblsfr/permission` (tag `PERMISSION_TAG`), port `8080`, sonde `GET /health`. Variables (communes dans `plbls.env` et `deploy/.env.example`, secrets dans `secret/commun` de Vault ; voir [`deploy/README.md`](../../deploy/README.md)) :

- `DATABASE_URL`, `DATABASE_USER`, `DATABASE_PASSWORD` (communs : serveur sans base, `?sslmode=require` si l'instance l'impose ; permission rejoint la base `permission`)
- `GATEWAY_IDENTITE_SECRET`, `GATEWAY_INTERNE_URL`, `GATEWAY_INTERNE_TOKEN` (communs avec le gateway et aegis), `PERMISSION_ADMIN_ROLE` (`plbls.env`)
- `PERMISSION_TOKEN_<SERVICE>` (optionnel) : un jeton par service client, déclaré dans le `.env.example` du client (`PERMISSION_TOKEN_ESTOMPE`, `PERMISSION_TOKEN_PLI`) ; le nom du service est le suffixe en minuscules, `_` devenant `-`

Les migrations s'appliquent au démarrage. Avant le premier usage : rôle Keycloak `permission-admin`, route `/api/permission` dans le gateway ([docs/permission/integration.md](../../docs/permission/integration.md)), domaine `permission.plbls.fr`.

## Configuration

`api/permission/config/default.toml`, puis variables `PERMISSION__SECTION__CLE`, puis les variables communes ci-dessus. Secrets uniquement dans l'environnement.
