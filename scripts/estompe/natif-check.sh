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
  echo "sans la feature youtube, l'app native ne doit dépendre ni de tauri, ni de wry, ni de webview2-com" >&2
  exit 1
fi
if cargo tree --locked -e normal --target all --features youtube | grep -E ' tauri v'; then
  echo "l'app native ne doit pas dépendre de tauri, même avec la feature youtube" >&2
  exit 1
fi
if ! cargo tree --locked -e normal --target x86_64-pc-windows-msvc --features youtube -i wry >/dev/null; then
  echo "la feature youtube doit apporter wry sous Windows" >&2
  exit 1
fi
cargo test --workspace --locked --no-run --lib
cargo nextest run --workspace --locked
echo "natif-check.sh : tout passe"
