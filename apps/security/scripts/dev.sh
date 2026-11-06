#!/usr/bin/env bash
# Environnement de dev : dépendances en Docker, gateway en processus local (logs dans .dev/).
set -euo pipefail
cd "$(dirname "$0")/.."
COMPOSE="docker compose -f deploy/docker-compose.dev.yml"
PID=.dev/gateway.pid
LOG=.dev/gateway.log

running() { [ -f "$PID" ] && kill -0 "$(cat "$PID")" 2>/dev/null; }

case "${1:-}" in
  up)    $COMPOSE up -d --wait ;;
  down)  "$0" stop; $COMPOSE down ;;
  reset) "$0" stop; $COMPOSE down -v ;;
  start) # build puis lance le gateway en arrière-plan, attend /readyz
    running && { echo "gateway déjà lancé (pid $(cat $PID))"; exit 0; }
    mkdir -p .dev
    cargo build --quiet
    set -a; . deploy/dev.env; set +a
    nohup "${CARGO_TARGET_DIR:-../../target}/debug/auth-gateway" > "$LOG" 2>&1 &
    echo $! > "$PID"
    for _ in $(seq 1 30); do
      curl -sf localhost:8080/readyz >/dev/null && { echo "gateway prêt (pid $(cat $PID)), logs: $LOG"; exit 0; }
      sleep 1
    done
    echo "le gateway ne répond pas, voir $LOG" >&2; tail -20 "$LOG" >&2; exit 1 ;;
  stop)  running && kill "$(cat "$PID")" && echo "gateway arrêté" || true; rm -f "$PID" ;;
  logs)  tail -f "$LOG" ;;
  status)
    $COMPOSE ps
    running && echo "gateway: pid $(cat $PID)" || echo "gateway: arrêté" ;;
  *) echo "usage: $0 {up|start|stop|down|reset|logs|status}" >&2; exit 2 ;;
esac
