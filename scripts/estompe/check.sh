#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
test -n "${TEST_DATABASE_URL:-}" || { echo "TEST_DATABASE_URL absent : les tests Rust seraient ignorés" >&2; exit 1; }
read -ra paquets <<< "$(scripts/estompe/paquets-cargo.sh)"
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --locked "${paquets[@]}"
cargo deny check
npm --prefix web/estompe run check
npm --prefix web/estompe test
echo "check.sh : tout passe"
