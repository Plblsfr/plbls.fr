#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

dossiers=("$@")
[ "${#dossiers[@]}" -gt 0 ] || dossiers=(apps/estompe/natif apps/estompe/crates/client)

motif='cfg(_attr)?!?[[:space:]]*\(.*\b(target_os|target_family|target_env|target_vendor|windows|unix)\b|CARGO_CFG_(TARGET_|WINDOWS|UNIX)'
trouve="$(grep -rnE "$motif" --include='*.rs' --include='*.toml' --exclude-dir=target "${dossiers[@]}" \
  | sed 's/windows_subsystem = "windows"/sous-systeme/' | grep -E "$motif" || true)"
if [ -n "$trouve" ]; then
  echo "Code propre à un OS hors de apps/estompe/crates/plateforme :" >&2
  echo "$trouve" >&2
  echo "Le déplacer dans la crate plateforme, derrière un trait (docs/estompe/natif.md, « Portabilité »)." >&2
  exit 1
fi
echo "natif-portabilite : aucun code propre à un OS dans ${dossiers[*]}"
