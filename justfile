# Commandes communes du monorepo. `just` seul liste les recettes.
# Chaque projet garde ses propres outils (Makefile, npm, cargo) : ces recettes ne font que les appeler.
# npm : workspace à la racine (package.json, package-lock.json, .nvmrc) ; chaque projet a les scripts lint, check, test et build.
default:
    @just --list

# Projets npm (workspace racine, Node du .nvmrc) : un seul package-lock.json
install:
    npm ci

lint:
    npm run lint

check:
    npm run check

build:
    npm run build

# Recopie packages/ui dans estompe, permission, pli et le thème Keycloak ; ui-check échoue si une copie diverge
ui-sync:
    npm run ui:sync

ui-check:
    npm run ui:check

# Tests qui ne demandent pas de base de données
test: test-security test-permission test-ui

test-security:
    make -C apps/security test

test-permission:
    make -C apps/permission test

test-ui:
    npm ci && npm test --workspace packages/ui

# Ces deux-là demandent un Postgres : TEST_DATABASE_URL=postgres://… just test-pli
test-pli:
    npm ci && npm test --workspace apps/pli

test-estompe-api:
    cargo test --locked $(apps/estompe/scripts/paquets-cargo.sh)

# Reconstruit packages/ui/dist (non versionné)
build-ui:
    npm ci && npm run build --workspace packages/ui
