#!/usr/bin/env bash
# Lance la suite Bruno avec le CLI officiel dans Docker (pas besoin de Node sur la machine).
# Réseau host : le conteneur joint localhost:8080 (gateway) et localhost:8180 (Keycloak).
set -euo pipefail
cd "$(dirname "$0")/../../api/security"
mkdir -p .dev
exec docker run --rm --network host -v "$PWD/bruno:/work" -v "$PWD/.dev:/out" -w /work \
  node:26-alpine npx --yes @usebruno/cli@4.2.1 run --env dev --disable-cookies --reporter-junit /out/bruno-junit.xml "$@"
