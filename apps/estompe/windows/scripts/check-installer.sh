#!/usr/bin/env bash
# Vérifie qu'un conteneur de l'image web sert latest.json, l'installeur qu'il désigne et l'installeur à adresse stable : check-installer.sh <conteneur> ; échoue sinon.
set -euo pipefail
c=${1:?conteneur}
w() { docker exec "$c" wget -S -O "$1" "http://127.0.0.1/updates/$2" 2>&1; }
taille() { docker exec "$c" stat -c %s "$1"; }
w /tmp/latest.json latest.json | grep -q ' 200 ' || { echo "latest.json absent"; exit 1; }
url=$(docker exec "$c" cat /tmp/latest.json | sed -n 's/.*"url": *"\([^"]*\)".*/\1/p')
exe=${url##*/updates/}
case "$exe" in Estompe_*_x64-setup.exe) ;; *) echo "URL inattendue dans latest.json : $url"; exit 1 ;; esac
for f in "$exe" "$exe.sig" Estompe-setup.exe "Estompe-setup.exe?v=0.0.0"; do
  h=$(w /tmp/check.bin "$f" || true)
  echo "$h" | grep -q ' 200 ' || { echo "$f non servi"; echo "$h"; exit 1; }
  echo "$h" | grep -qi 'cache-control: no-cache' || { echo "$f sans Cache-Control: no-cache"; exit 1; }
done
[ "$(taille /tmp/check.bin)" -gt 1000000 ] || { echo "installeur trop petit"; exit 1; }
h=$({ w /dev/null Estompe_0.0.0_x64-setup.exe || true; })
echo "$h" | grep -q ' 404 ' || { echo "un installeur absent devrait répondre 404"; exit 1; }
echo "$h" | grep -qi 'cache-control: no-cache' || { echo "le 404 de /updates/ doit porter Cache-Control: no-cache"; exit 1; }
echo "Installeur ok ($exe)"
