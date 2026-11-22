# Commandes communes du monorepo. `just` seul liste les recettes.
# Seul point d'entrée de l'outillage : les recettes appellent npm, cargo, docker et les scripts des projets.
# npm : workspace à la racine (package.json, package-lock.json, .nvmrc) ; chaque projet a les scripts lint, check, test et build.
default:
    @just --list

# Projets npm (workspace racine, Node du .nvmrc) : un seul package-lock.json
install:
    npm ci

# Lint de tout le code non Rust : Biome (JS, TS, CSS, JSON), shellcheck (scripts shell), ruff (Python), liens Markdown internes
lint: lint-js lint-shell lint-python lint-links estompe-natif-parite

lint-js:
    npm run lint

lint-shell:
    scripts/lint-shell.sh

lint-python:
    ruff check .

lint-links:
    node scripts/check-links.mjs

# Types et cohérence : svelte-check (estompe), tsc (packages/ui), version de Node, copies de @plbls/ui
check:
    npm run check

build:
    npm run build

# Recopie packages/ui dans estompe et le thème Keycloak ; ui-check échoue si une copie diverge
ui-sync:
    npm run ui:sync

ui-check:
    npm run ui:check

# Tests qui ne demandent pas de base de données : vitest de tous les frontaux et de packages/ui, Rust de security et aegis
test: test-js test-security test-aegis

test-js:
    npm test

test-security:
    cargo test --locked -p auth-gateway

# aegis : tests unitaires ; les tests d'intégration demandent TEST_DATABASE_URL (et TEST_REDIS_URL pour le cache)
test-aegis:
    cargo test --locked -p aegis

test-ui:
    npm ci && npm test --workspace packages/ui

test-estompe-api:
    cargo test --locked $(scripts/estompe/paquets-cargo.sh)

# Estompe : chaque composant de web/estompe/src, bloc et section a sa ligne dans docs/estompe/natif-parite.md
estompe-natif-parite:
    node scripts/estompe/natif-parite.mjs

# Estompe natif (apps/estompe/natif, workspace Cargo à part, toolchain stable) : voir docs/estompe/natif.md
estompe-natif-run *ARGS:
    cd apps/estompe/natif && cargo run -- {{ARGS}}

estompe-natif-build PROFIL="release":
    cd apps/estompe/natif && cargo build --locked --profile {{PROFIL}}

estompe-natif-test:
    cd apps/estompe/natif && cargo test --workspace --locked

# Tous les contrôles de la CI Linux : portabilité, fichiers générés, fmt, clippy, cargo deny, tests, build
estompe-natif-check:
    scripts/estompe/natif-check.sh

# Régénère les jetons de couleur et les icônes Slint (depuis le CSS et Icon.svelte) et les types de l'API (depuis openapi.yaml)
estompe-natif-generer:
    node scripts/estompe/natif-da.mjs
    node scripts/estompe/gen-types-client.mjs

# Régénère le paquet d'emoji Twemoji embarqué (apps/estompe/natif/emoji/twemoji.bin) ; DOSSIER garde les SVG téléchargés
estompe-natif-emoji DOSSIER="":
    node scripts/estompe/natif-emoji.mjs {{ if DOSSIER != "" { "--cache " + DOSSIER } else { "" } }}

# Parcours long du test de volume de la messagerie (50 000 messages, 2 minutes simulées)
estompe-natif-volume:
    cd apps/estompe/natif && ESTOMPE_VOLUME=long cargo test --locked --test messagerie_volume -- --nocapture

# Test d'intégration du vocal contre le LiveKit de dev-up (docker compose -f deploy/docker-compose.dev.yml up -d --wait livekit)
estompe-natif-vocal:
    cd apps/estompe/natif && ESTOMPE_LIVEKIT_URL=ws://127.0.0.1:7880 cargo test --locked -p estompe-client --features livekit --test livekit -- --nocapture

# Mesure mémoire et CPU de l'app native (export au format de l'app Tauri) : voir docs/estompe/natif-mesures.md
estompe-natif-mesures *ARGS:
    scripts/estompe/natif-mesures.sh {{ARGS}}

# Réécrit les images de référence des captures de rendu (apps/estompe/natif/tests/references)
estompe-natif-captures:
    cd apps/estompe/natif && ESTOMPE_CAPTURES=regenerer cargo test --locked --test captures --test overlay

# Demande aussi ffmpeg et ffprobe dans le PATH
test-entracte:
    cargo test --locked -p entracte

# Sillon : une base jetable par test (TEST_DATABASE_URL, sinon testcontainers)
test-sillon:
    cargo test --locked -p sillon

# Sablier : une base jetable par test (TEST_DATABASE_URL, sinon testcontainers) ; la file d'évènements demande TEST_REDIS_URL
test-sablier:
    cargo test --locked -p sablier

test-regie:
    cargo test --locked -p regie

# Reconstruit packages/ui/dist (non versionné)
build-ui:
    npm ci && npm run build --workspace packages/ui

# Dépendances de dev de tous les projets, éphémères (deploy/docker-compose.dev.yml) : Postgres :5432 (postgres/postgres, une base
# par backend), Redis :6379, Keycloak :8180, Mailpit :8025/:1025, écho :8081, LiveKit :7880. Variables des backends : deploy/dev.env
dev-up:
    docker compose -f deploy/docker-compose.dev.yml up -d --wait

dev-down:
    docker compose -f deploy/docker-compose.dev.yml down

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

# aegis : backend sur 127.0.0.1:8091, demande dev-up (Postgres et Redis)
aegis-dev-run:
    cd api/aegis && set -a && . ../../deploy/dev.env && set +a && cargo run

aegis-image:
    docker build -f api/aegis/Dockerfile -t aegis:local .

# Entracte : backend sur 127.0.0.1:8092, demande dev-up, le gateway et aegis (AEGIS_URL=http://127.0.0.1:8091), ffmpeg
entracte-dev-run:
    set -a && . deploy/dev.env && set +a && AEGIS_URL="${AEGIS_URL:-http://127.0.0.1:8091}" cargo run -p entracte

entracte-image:
    docker build -f api/entracte/Dockerfile -t entracte:local .

# Sillon : backend sur 127.0.0.1:8093, demande dev-up, le gateway et aegis (AEGIS_URL=http://127.0.0.1:8091)
sillon-dev-run:
    set -a && . deploy/dev.env && set +a && AEGIS_URL="${AEGIS_URL:-http://127.0.0.1:8091}" cargo run -p sillon

sillon-image:
    docker build -f api/sillon/Dockerfile -t sillon:local .

# Sablier : backend sur 127.0.0.1:8095, demande dev-up, le gateway et aegis (AEGIS_URL=http://127.0.0.1:8091)
sablier-dev-run:
    set -a && . deploy/dev.env && set +a && AEGIS_URL="${AEGIS_URL:-http://127.0.0.1:8091}" cargo run -p sablier

sablier-image:
    docker build -f api/sablier/Dockerfile -t sablier:local .

# Régie : backend sur 127.0.0.1:8094, demande le gateway et aegis (AEGIS_URL=http://127.0.0.1:8091)
regie-dev-run:
    set -a && . deploy/dev.env && set +a && AEGIS_URL="${AEGIS_URL:-http://127.0.0.1:8091}" cargo run -p regie

regie-image:
    docker build -f api/regie/Dockerfile -t regie:local .
