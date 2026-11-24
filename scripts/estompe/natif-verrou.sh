#!/usr/bin/env bash
set -euo pipefail

if [ -n "${ESTOMPE_NATIF_VERROU:-}" ]; then
  exec "$@"
fi
if ! command -v flock >/dev/null; then
  echo "natif-verrou : flock absent, build lancé sans verrou" >&2
  exec "$@"
fi

verrou="${ESTOMPE_NATIF_VERROU_FICHIER:-/tmp/estompe-natif-build.lock}"
exec 9>>"$verrou"
if ! flock -n 9; then
  echo "natif-verrou : un autre build de l'app native tourne sur cette machine, attente de sa fin ($verrou)" >&2
  flock 9
fi
export ESTOMPE_NATIF_VERROU=1
exec "$@"
