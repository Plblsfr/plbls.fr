# Commandes communes du monorepo. `just` seul liste les recettes.
# Chaque projet garde ses propres outils (Makefile, npm, cargo) : ces recettes ne font que les appeler.
# npm : workspace à la racine (package.json, package-lock.json, .nvmrc) ; chaque projet a les scripts lint, check, test et build.
default:
    @just --list

# Projets npm (workspace racine, Node du .nvmrc) : un seul package-lock.json
install:
    npm ci

# Lint de tout le code non Rust : Biome (JS, TS, CSS, JSON), shellcheck (scripts shell), ruff (Python)
lint: lint-js lint-shell lint-python

lint-js:
    npm run lint

lint-shell:
    scripts/lint-shell.sh

lint-python:
    ruff check .

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
    make -C apps/security test

test-permission:
    make -C apps/permission test

test-ui:
    npm ci && npm test --workspace packages/ui

# e2e Playwright (Docker pour Postgres, ou E2E_DATABASE_URL vers une base vide)
e2e-permission:
    npm run e2e --workspace apps/permission/frontend

# Ces deux-là demandent un Postgres : TEST_DATABASE_URL=postgres://… just test-pli
test-pli:
    cargo test --locked -p pli
    npm ci && npm test --workspace apps/pli

test-estompe-api:
    cargo test --locked $(apps/estompe/scripts/paquets-cargo.sh)

# Reconstruit packages/ui/dist (non versionné)
build-ui:
    npm ci && npm run build --workspace packages/ui
