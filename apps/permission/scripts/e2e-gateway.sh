#!/bin/sh
# Test de bout en bout avec le vrai auth-gateway + Keycloak de ../security.plbls.fr (realm de dev) : connexion admin et demo
# dans un Chromium headless (Docker). Démarre et arrête tout. Usage : make e2e-gateway
set -eu
cd "$(dirname "$0")/.."
SEC="${SECURITY_DIR:-$PWD/../security.plbls.fr}"
[ -x "$SEC/scripts/dev.sh" ] || { echo "security.plbls.fr introuvable : $SEC (SECURITY_DIR=...)" >&2; exit 2; }
PW=mcr.microsoft.com/playwright:v1.50.0-noble
SHOTS="${SHOTS:-/tmp/e2e-shots}"
cleanup() {
  docker rm -f perm-e2e-vite >/dev/null 2>&1 || true
  [ -n "${BACK_PID:-}" ] && kill "$BACK_PID" 2>/dev/null || true
  docker compose -f deploy/docker-compose.dev.yml down >/dev/null 2>&1 || true
  "$SEC/scripts/dev.sh" down >/dev/null 2>&1 || true
}
trap cleanup EXIT

# Keycloak est recréé pour réimporter le realm (rôle permission-admin).
(cd "$SEC" && docker compose -f deploy/docker-compose.dev.yml down >/dev/null 2>&1 || true)
"$SEC/scripts/dev.sh" up >/dev/null
"$SEC/scripts/dev.sh" start

docker compose -f deploy/docker-compose.dev.yml up -d --wait >/dev/null
(cd backend && cargo build -q)
set -a; . deploy/dev.env; set +a
(cd backend && exec "${CARGO_TARGET_DIR:-../../../target}/debug/permission") >/tmp/e2e-backend.log 2>&1 & BACK_PID=$!
for i in $(seq 1 30); do curl -sf localhost:8090/health >/dev/null && break; sleep 1; done
python3 scripts/seed.py >/dev/null

docker run -d --name perm-e2e-vite --network host -u "$(id -u):$(id -g)" -e HOME=/tmp -e npm_config_cache=/tmp/npm -e DEV_VIA_GATEWAY=1 \
  -v "$(cd ../.. && pwd):/repo" -w /repo node:26-slim sh -c "npm ci --no-audit --no-fund && npm run dev --workspace apps/permission/frontend -- --host 127.0.0.1" >/dev/null
for i in $(seq 1 60); do curl -sf localhost:3000 >/dev/null && break; sleep 2; done

mkdir -p "$SHOTS"
docker run --rm --network host -u "$(id -u):$(id -g)" -e HOME=/tmp -e SHOTS="$SHOTS" -v "$PWD/e2e:/w" -v "$SHOTS:$SHOTS" -w /w "$PW" \
  sh -c "npm install --no-audit --no-fund >/dev/null 2>&1 && node gateway.test.mjs"
