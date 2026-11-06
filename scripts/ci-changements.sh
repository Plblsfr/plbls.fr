#!/usr/bin/env bash
set -euo pipefail

sortie="${GITHUB_OUTPUT:-/dev/stdout}"

if [ "${GITHUB_EVENT_NAME:-}" != pull_request ]; then
  echo "Évènement ${GITHUB_EVENT_NAME:-local} : tout est lancé"
  echo "modifie=true" >> "$sortie"
  exit 0
fi

[ $# -gt 0 ] || { echo "usage : $0 <chemin>... (préfixe ! pour exclure)" >&2; exit 2; }

specs=()
for chemin in "$@"; do
  case "$chemin" in
    !*) specs+=(":(exclude,glob)${chemin#!}") ;;
    *) specs+=(":(glob)$chemin") ;;
  esac
done

fichiers="$(git diff --name-only HEAD^1 HEAD -- "${specs[@]}")"
if [ -n "$fichiers" ]; then
  echo "Fichiers concernés :"
  mapfile -t lignes <<<"$fichiers"
  printf '  %s\n' "${lignes[@]}"
  echo "modifie=true" >> "$sortie"
else
  echo "Aucun fichier concerné : jobs ignorés"
  echo "modifie=false" >> "$sortie"
fi
