# Commandes communes du monorepo. `just` seul liste les recettes.
# Seul point d'entrée de l'outillage : les recettes appellent npm, cargo, docker et les scripts des projets.
# npm : workspace à la racine (package.json, package-lock.json, .nvmrc) ; chaque projet a les scripts lint, check, test et build.
default:
    @just --list

# Projets npm (workspace racine, Node du .nvmrc) : un seul package-lock.json
install:
    npm ci

# Lint de tout le code non Rust : Biome (JS, TS, CSS, JSON), shellcheck (scripts shell), ruff (Python), liens Markdown internes
lint: lint-js lint-shell lint-python lint-links

lint-js:
    npm run lint

lint-shell:
    scripts/lint-shell.sh

lint-python:
    ruff check .

lint-links:
    node scripts/check-links.mjs

# Types et cohérence : svelte-check (estompe, permission), tsc (pli, packages/ui), version de Node, copies de @plbls/ui
check:
    npm run check

build:
    npm run build

# Recopie packages/ui dans estompe, permission, pli et le thème Keycloak ; ui-check échoue si une copie diverge
ui-sync:
    npm run ui:sync

ui-check:
    npm run ui:check

# Tests qui ne demandent pas de base de données : vitest de tous les frontaux et de packages/ui, Rust de security et permission
test: test-js test-security test-permission

test-js:
    npm test

test-security:
    cargo test --locked -p auth-gateway

test-permission:
    cargo test --locked -p permission

test-ui:
    npm ci && npm test --workspace packages/ui

# e2e Playwright (Docker pour Postgres, ou E2E_DATABASE_URL vers une base vide)
e2e-permission:
    npm run e2e --workspace web/permission

# Ces deux-là demandent un Postgres : TEST_DATABASE_URL=postgres://… just test-pli
test-pli:
    cargo test --locked -p pli
    npm ci && npm test --workspace web/pli

test-estompe-api:
    cargo test --locked $(scripts/estompe/paquets-cargo.sh)

# Reconstruit packages/ui/dist (non versionné)
build-ui:
    npm ci && npm run build --workspace packages/ui

# security : dépendances de dev en Docker (Redis, Keycloak, Mailpit, écho), gateway en local.
# Actions : up, start (build puis lance le gateway), stop, down, reset (supprime les volumes), logs, status
security-dev action:
    scripts/security/dev.sh {{action}}

# Suites de bout en bout de security : demandent `just security-dev up` puis `just security-dev start`
security-bruno:
    scripts/security/bruno.sh

security-ws:
    python3 scripts/security/ws_test.py

security-refresh:
    python3 scripts/security/refresh_test.py

security-regressions:
    python3 scripts/security/security_test.py

# Chaque route du code est couverte par la suite Bruno
security-coverage:
    python3 scripts/security/check_coverage.py

security-test-all: test-security security-coverage (security-dev "up") (security-dev "start") security-bruno security-ws security-refresh security-regressions

# permission : Postgres de dev en Docker (éphémère), backend sur 127.0.0.1:8090
permission-dev-up:
    docker compose -f deploy/permission/docker-compose.dev.yml up -d --wait

permission-dev-down:
    docker compose -f deploy/permission/docker-compose.dev.yml down

permission-dev-run:
    cd api/permission && set -a && . ../../deploy/permission/dev.env && set +a && cargo run

# Données de départ (applications chat et radar) puis test de bout en bout de l'API : demandent permission-dev-up et permission-dev-run
permission-seed:
    set -a && . deploy/permission/dev.env && set +a && python3 scripts/permission/seed.py

permission-smoke:
    set -a && . deploy/permission/dev.env && set +a && python3 scripts/permission/smoke.py

# e2e avec le vrai auth-gateway (api/security) et Keycloak (Docker)
e2e-permission-gateway:
    E2E_GATEWAY=1 npm run e2e --workspace web/permission

# Vite sur :3000 (proxy signé vers le backend :8090)
permission-front-dev:
    npm run dev --workspace web/permission -- --host 127.0.0.1

# Image Docker complète (tests, front, backend), construite depuis la racine
permission-image:
    docker build -f api/permission/Dockerfile -t permission:local .
