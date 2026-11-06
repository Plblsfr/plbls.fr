#!/usr/bin/env sh
set -eu
cd "$(dirname "$0")/../backend"
for manifeste in */Cargo.toml blocks/*/Cargo.toml; do
  sed -n 's/^name = "\(.*\)"/-p \1/p' "$manifeste" | head -n 1
done | tr '\n' ' '
