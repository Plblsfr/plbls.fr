#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../backend"
for manifeste in */Cargo.toml blocks/*/Cargo.toml; do
  awk -F'"' '/^name = /{print "-p " $2; exit}' "$manifeste"
done | tr '\n' ' '
