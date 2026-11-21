#!/usr/bin/env bash
set -euo pipefail
racine="$(cd "$(dirname "$0")/../.." && pwd)"
natif="$racine/apps/estompe/natif"
synthese="$racine/apps/estompe/windows/scripts/synthese-mesures.mjs"

usage() {
  cat >&2 <<'FIN'
usage :
  natif-mesures.sh app <dossier> [sessions] [scénario] [durée en s]
      lance l'app native (release) une fois par session avec --mesures, puis la synthèse ;
      défauts : 5 sessions, scénario repos, durée du scénario (repos : 300 s)
  natif-mesures.sh vocal <dossier> [participants…]
      banc du vocal contre ESTOMPE_LIVEKIT_URL : pour chaque nombre d'autres participants
      (défaut : 0 4), des robots qui parlent et un client mesuré (phases PHASES)
  natif-mesures.sh synthese <dossier|export.json>…
variables : ESTOMPE_NATIF_BINAIRE (défaut target/release/estompe-natif), ATTENTE (secondes avant
le début du scénario, défaut 0), ESTOMPE_NATIF_DOSSIER (profil, voir docs/estompe/natif.md), PHASES
(défaut repos:30,silence:60,parole:60), ESTOMPE_LIVEKIT_URL (défaut ws://127.0.0.1:7880)
FIN
  exit 2
}

app() {
  local dossier="$1" sessions="${2:-5}" scenario="${3:-repos}" duree="${4:-}"
  local binaire="${ESTOMPE_NATIF_BINAIRE:-$natif/target/release/estompe-natif}"
  if [ ! -x "$binaire" ]; then
    (cd "$natif" && cargo build --release --locked)
  fi
  mkdir -p "$dossier"
  local i options
  for i in $(seq 1 "$sessions"); do
    options=(--mesures "$dossier/natif-$scenario-$i.json" --scenario "$scenario" --attente "${ATTENTE:-0}")
    [ -z "$duree" ] || options+=(--duree "$duree")
    echo "session $i/$sessions : $scenario"
    "$binaire" "${options[@]}"
  done
  node "$synthese" "$dossier"
}

vocal() {
  local dossier="$1"
  shift
  local participants=("$@")
  [ "${#participants[@]}" -gt 0 ] || participants=(0 4)
  local url="${ESTOMPE_LIVEKIT_URL:-ws://127.0.0.1:7880}"
  local phases="${PHASES:-repos:30,silence:60,parole:60}"
  local banc="$natif/target/release/examples/mesure_vocal"
  (cd "$natif" && cargo build --release --locked -p estompe-client --features livekit --example mesure_vocal)
  mkdir -p "$dossier"
  local total=0 phase n salle i robots
  for phase in ${phases//,/ }; do total=$((total + ${phase#*:})); done
  for n in "${participants[@]}"; do
    salle="mesure-$n-$(date +%s)"
    robots=()
    for i in $(seq 1 "$n"); do
      "$banc" --url "$url" --salle "$salle" --identite "robot-$i" --parle --duree $((total + 60)) &
      robots+=("$!")
    done
    [ "$n" -eq 0 ] || sleep 5
    echo "## $n autres participants, qui parlent"
    "$banc" --url "$url" --salle "$salle" --identite mesure --phases "$phases" --sortie "$dossier/vocal-$n.json"
    for i in "${robots[@]}"; do kill "$i" 2>/dev/null || true; done
    wait || true
  done
}

[ "$#" -ge 2 ] || usage
commande="$1"
shift
case "$commande" in
  app) app "$@" ;;
  vocal) vocal "$@" ;;
  synthese) node "$synthese" "$@" ;;
  *) usage ;;
esac
