#!/bin/sh
# Test de bout en bout du front dans un Chromium headless (Docker) : Postgres + backend + Vite + seed, puis e2e/ui.test.mjs.
# Rien n'est affiché sur le bureau ; captures dans $SHOTS (défaut /tmp/e2e-shots). Usage : make e2e
set -eu
cd "$(dirname "$0")/.."
PW=mcr.microsoft.com/playwright:v1.50.0-noble
cleanup() {
  docker rm -f perm-e2e-vite >/dev/null 2>&1 || true
  [ -n "${BACK_PID:-}" ] && kill "$BACK_PID" 2>/dev/null || true
  docker compose -f deploy/docker-compose.dev.yml down >/dev/null 2>&1 || true
}
trap cleanup EXIT

docker compose -f deploy/docker-compose.dev.yml up -d --wait >/dev/null
(cd backend && cargo build -q)
set -a; . deploy/dev.env; set +a
(cd backend && exec "${CARGO_TARGET_DIR:-../../../target}/debug/permission") >/tmp/e2e-backend.log 2>&1 & BACK_PID=$!
for i in $(seq 1 30); do curl -sf localhost:8090/health >/dev/null && break; sleep 1; done
python3 scripts/seed.py >/dev/null

docker run -d --name perm-e2e-vite --network host -u "$(id -u):$(id -g)" -e HOME=/tmp -e npm_config_cache=/tmp/npm \
  -v "$(cd ../.. && pwd):/repo" -w /repo node:26-slim sh -c "npm ci --no-audit --no-fund && npm run dev --workspace apps/permission/frontend -- --host 127.0.0.1" >/dev/null
for i in $(seq 1 60); do curl -sf localhost:3000 >/dev/null && break; sleep 2; done

mkdir -p "${SHOTS:-/tmp/e2e-shots}"
docker run --rm --network host -u "$(id -u):$(id -g)" -e HOME=/tmp -e SHOTS="${SHOTS:-/tmp/e2e-shots}" \
  -v "$PWD/e2e:/w" -v "${SHOTS:-/tmp/e2e-shots}:${SHOTS:-/tmp/e2e-shots}" -w /w "$PW" \
  sh -c "npm install --no-audit --no-fund >/dev/null 2>&1 && node ui.test.mjs && node errors.test.mjs"
