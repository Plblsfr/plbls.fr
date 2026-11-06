#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
test -n "${TEST_DATABASE_URL:-}" || { echo "TEST_DATABASE_URL absent : les tests Rust seraient ignorés" >&2; exit 1; }
paquets="$(scripts/paquets-cargo.sh)"
(
  cd ../..
  cargo fmt --all --check
  cargo clippy --workspace --all-targets --locked -- -D warnings
  cargo test --locked $paquets
  cargo deny check
)
npm --prefix frontend run check
npm --prefix frontend test
echo "check.sh : tout passe"
