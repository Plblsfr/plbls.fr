#!/bin/sh
# Exécuté par l'image nginx avant le démarrage. Publie dist/ dans le volume /srv :
#   /srv/ui/<version>/  écrit une fois, jamais écrasé (une version publiée est immuable)
#   /srv/ui/<majeure>/  et /srv/ui/latest/  remplacés à chaque démarrage (sauf pré-version)
set -eu

version="$(cat /opt/cdn/VERSION)"
major="${version%%.*}"
ui=/srv/ui
mkdir -p "$ui"

if [ ! -d "$ui/$version" ]; then
  cp -r /opt/cdn/dist "$ui/$version"
  echo "publier: $version publiée"
elif ! diff -rq /opt/cdn/dist "$ui/$version" >/dev/null; then
  echo "publier: ATTENTION $version existe déjà avec un contenu différent, laissée telle quelle (changer la version dans package.json)" >&2
fi

case "$version" in
  *-*) echo "publier: pré-version, alias latest et $major inchangés" ;;
  *)
    for alias in latest "$major"; do
      rm -rf "$ui/.$alias.tmp"
      cp -r "$ui/$version" "$ui/.$alias.tmp"
      rm -rf "$ui/$alias"
      mv "$ui/.$alias.tmp" "$ui/$alias"
      echo "publier: /ui/$alias/ -> $version"
    done
    ;;
esac
