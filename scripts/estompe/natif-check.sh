#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
[ -n "${ESTOMPE_NATIF_VERROU:-}" ] || exec scripts/estompe/natif-verrou.sh scripts/estompe/natif-check.sh "$@"

scripts/estompe/natif-portabilite.sh
node scripts/estompe/natif-da.mjs --check
node scripts/estompe/gen-types-client.mjs --check

cd apps/estompe/natif
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo deny check
if cargo tree --locked -e normal --target all | grep -E ' (tauri|wry|webview2-com) v'; then
  echo "l'app native ne doit dépendre ni de tauri, ni de wry, ni de webview2-com" >&2
  exit 1
fi
cargo test --workspace --locked
cargo build --locked
echo "natif-check.sh : tout passe"
