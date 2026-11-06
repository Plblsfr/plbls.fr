#!/usr/bin/env bash
# Vérifie qu'un conteneur de l'image web sert bien l'APK (type de contenu, nom, taille) : check-apk.sh <conteneur> ; échoue sinon.
set -euo pipefail
c=${1:?conteneur}
w() { docker exec "$c" wget -S -O "$1" "http://127.0.0.1/updates/$2" 2>&1; }
h=$(w /tmp/check-apk.bin Estompe.apk || true)
echo "$h" | grep -qi 'content-type: application/vnd.android.package-archive' || { echo "Type de contenu inattendu"; echo "$h"; exit 1; }
echo "$h" | grep -qi 'filename="Estompe.apk"' || { echo "Content-Disposition absent"; echo "$h"; exit 1; }
[ "$(docker exec "$c" stat -c %s /tmp/check-apk.bin)" -gt 1000000 ] || { echo "APK trop petit"; exit 1; }
{ w /dev/null absent.apk || true; } | grep -q ' 404 ' || { echo "absent.apk devrait répondre 404"; exit 1; }
echo "APK ok"
