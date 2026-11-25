#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 2 ]; then
  echo "usage : $0 <installeur> <estompe-natif.exe>" >&2
  exit 2
fi
installeur=$1
exe=$2
TAILLE_MAX_MO=${TAILLE_MAX_MO:-125}

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
for fichier in estompe-natif.exe ffmpeg.exe libmpv-2.dll estompe.ico desinstaller.exe; do
  if ! grep -qE "(^|[[:space:]/])$fichier\$" <<< "$contenu"; then
    echo "::error::$fichier absent de l'installeur"
    exit 1
  fi
done

if grep -qiE "(^|[[:space:]/])WebView2Loader\.dll\$" <<< "$contenu"; then
  echo "::error::WebView2Loader.dll dans l'installeur : le chargeur de WebView2 doit être lié statiquement (feature youtube)"
  exit 1
fi
if LC_ALL=C grep -aqP 'CoreWebView2|E\x00m\x00b\x00e\x00d\x00d\x00e\x00d\x00B\x00r\x00o\x00w\x00s\x00e\x00r' "$exe"; then
  echo "Lecteur YouTube : vue WebView2 présente (feature youtube), créée seulement à l'ouverture d'un lecteur"
else
  echo "::warning::$exe sans vue WebView2 : l'installeur est construit sans la feature youtube, YouTube s'ouvrira dans le navigateur"
fi
echo "Contenu et dépendances de l'installeur conformes"
