#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 2 ]; then
  echo "usage : $0 <dossier du profil cargo, ex. target/x86_64-pc-windows-msvc/release> <dossier des archives>" >&2
  exit 2
fi
profil=$1
archives=$2

sortie=""
skia=""
for fichier in "$profil"/build/skia-bindings-*/output "$profil"/build/skia-bindings/*/run/stdout; do
  [ -f "$fichier" ] || continue
  if [ -z "$sortie" ] || [ "$fichier" -nt "$sortie" ]; then
    sortie=$fichier
    case "$fichier" in
      */run/stdout) skia=$(dirname "$(dirname "$fichier")")/out/skia ;;
      *) skia=$(dirname "$fichier")/out/skia ;;
    esac
  fi
done
if [ -z "$sortie" ]; then
  echo "::error::sortie du script de build de skia-bindings introuvable sous $profil/build"
  exit 1
fi

nouvelle=false
if grep -q '^STARTING A FULL BUILD' "$sortie"; then
  cle=$(tr -d '\r' < "$sortie" | sed -n 's|^TRYING TO DOWNLOAD AND INSTALL SKIA BINARIES: [^/]*/||p' | head -1)
  if [ -z "$cle" ]; then
    echo "::error::clé des binaires de Skia absente de $sortie"
    exit 1
  fi
  temp=$(mktemp -d)
  mkdir "$temp/skia-binaries"
  find "$skia" -maxdepth 1 -type f \( -name '*.lib' -o -name '*.a' -o -name 'bindings.rs' -o -name '*.dat' \) -exec cp {} "$temp/skia-binaries/" \;
  test -s "$temp/skia-binaries/bindings.rs"
  mkdir -p "$archives"
  tar -czf "$archives/skia-binaries-$cle.tar.gz" -C "$temp" skia-binaries
  rm -rf "$temp"
  echo "Binaires de Skia archivés : $archives/skia-binaries-$cle.tar.gz ($(du -h "$archives/skia-binaries-$cle.tar.gz" | cut -f1))"
  nouvelle=true
else
  echo "Binaires de Skia repris de l'archive en cache : rien de nouveau à archiver"
fi

find "$skia" -mindepth 1 -maxdepth 1 -type d -exec rm -rf {} +

if [ -n "${GITHUB_OUTPUT:-}" ]; then
  echo "nouvelle=$nouvelle" >> "$GITHUB_OUTPUT"
fi
