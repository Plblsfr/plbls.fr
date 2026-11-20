#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 2 ]; then
  echo "usage : $0 <installeur> <estompe-natif.exe>" >&2
  exit 2
fi
installeur=$1
exe=$2
TAILLE_MAX_MO=${TAILLE_MAX_MO:-45}

octets=$(wc -c < "$installeur" | tr -d ' ')
taille=$(awk -v o="$octets" 'BEGIN { printf "%.1f", o / 1048576 }')
echo "Installeur : $(basename "$installeur"), $taille Mo ($octets octets), plafond $TAILLE_MAX_MO Mo"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  echo "Installeur $(basename "$installeur") : $taille Mo (plafond $TAILLE_MAX_MO Mo)" >> "$GITHUB_STEP_SUMMARY"
fi
if [ "$octets" -gt $((TAILLE_MAX_MO * 1048576)) ]; then
  echo "::error::installeur trop gros : $taille Mo > $TAILLE_MAX_MO Mo"
  exit 1
fi

contenu=$(7z l -ba "$installeur" | tr -d '\r')
echo "$contenu"
for fichier in estompe-natif.exe estompe.ico desinstaller.exe; do
  if ! grep -qE "(^|[[:space:]/])$fichier\$" <<< "$contenu"; then
    echo "::error::$fichier absent de l'installeur"
    exit 1
  fi
done

for motif in WebView2Loader EmbeddedBrowserWebView; do
  if grep -aq "$motif" "$exe"; then
    echo "::error::$exe référence $motif"
    exit 1
  fi
done
echo "Contenu et dépendances de l'installeur conformes"
