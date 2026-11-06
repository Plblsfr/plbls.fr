#!/usr/bin/env bash
set -euo pipefail

sortie="${GITHUB_OUTPUT:-/dev/stdout}"
nom=modifie
if [ "${1:-}" = --nom ]; then
  [ $# -ge 2 ] || { echo "usage : $0 [--nom <sortie>] <chemin>..." >&2; exit 2; }
  nom="$2"
  shift 2
fi

case "${GITHUB_EVENT_NAME:-}:${GITHUB_REF:-}" in
  pull_request:*|push:refs/heads/*) ;;
  *)
    echo "$nom : évènement ${GITHUB_EVENT_NAME:-local} ${GITHUB_REF:-}, tout est lancé"
    echo "$nom=true" >> "$sortie"
    exit 0
    ;;
esac

[ $# -gt 0 ] || { echo "usage : $0 [--nom <sortie>] <chemin>... (préfixe ! pour exclure)" >&2; exit 2; }

specs=()
for chemin in "$@"; do
  case "$chemin" in
    !*) specs+=(":(exclude,glob)${chemin#!}") ;;
    *) specs+=(":(glob)$chemin") ;;
  esac
done

fichiers="$(git diff --name-only HEAD^1 HEAD -- "${specs[@]}")"
if [ -n "$fichiers" ]; then
  echo "$nom : fichiers concernés"
  mapfile -t lignes <<<"$fichiers"
  printf '  %s\n' "${lignes[@]}"
  echo "$nom=true" >> "$sortie"
else
  echo "$nom : aucun fichier concerné"
  echo "$nom=false" >> "$sortie"
fi
