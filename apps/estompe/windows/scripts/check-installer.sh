#!/usr/bin/env bash
# Vérifie qu'un conteneur de l'image web sert latest.json, l'installeur qu'il désigne et l'installeur à adresse stable : check-installer.sh <conteneur> ; échoue sinon.
set -euo pipefail
c=${1:?conteneur}
w() { docker exec "$c" wget -S -O "$1" "http://127.0.0.1:8080/updates/$2" 2>&1 || true; }
taille() { docker exec "$c" stat -c %s "$1"; }
echec() {
  echo "$1"
  [ -z "${2:-}" ] || echo "$2"
  docker ps -a --filter "name=^$c\$" --format 'Conteneur : {{.Status}}' || true
  docker exec "$c" ls -l /usr/share/nginx/updates/ || true
  docker logs --tail 20 "$c" 2>&1 || true
  exit 1
}
h=$(w /tmp/latest.json latest.json)
echo "$h" | grep -q ' 200 ' || echec "latest.json absent" "$h"
url=$(docker exec "$c" cat /tmp/latest.json | sed -n 's/.*"url": *"\([^"]*\)".*/\1/p')
exe=${url##*/updates/}
case "$exe" in Estompe_*_x64-setup.exe) ;; *) echec "URL inattendue dans latest.json : $url" ;; esac
for f in "$exe" "$exe.sig" Estompe-setup.exe "Estompe-setup.exe?v=0.0.0"; do
  h=$(w /tmp/check.bin "$f")
  echo "$h" | grep -q ' 200 ' || echec "$f non servi" "$h"
  echo "$h" | grep -qi 'cache-control: no-cache' || echec "$f sans Cache-Control: no-cache" "$h"
done
[ "$(taille /tmp/check.bin)" -gt 1000000 ] || echec "installeur trop petit"
h=$(w /dev/null Estompe_0.0.0_x64-setup.exe)
echo "$h" | grep -q ' 404 ' || echec "un installeur absent devrait répondre 404" "$h"
echo "Installeur ok ($exe)"
