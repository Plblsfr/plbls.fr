#!/usr/bin/env bash
# Compile l'installeur Windows « Estompe Test » (non signé, sans mises à jour), qui s'installe à côté de l'app de production.
# Usage : apps/estompe/windows/scripts/build-test-windows.sh <ticket>   (ex. EST-123)  → apps/estompe/windows/dist-test/ puis dossier Téléchargements de Windows
set -euo pipefail
TICKET="${1:?Usage : $0 <ticket> (ex. EST-123)}"
cd "$(dirname "$0")/.."
export PATH="$PWD/../../../node_modules/.bin:$PATH"
TARGET=x86_64-pc-windows-gnu
tauri build --config src-tauri/tauri.test.conf.json --target "$TARGET" --bundles nsis -- --profile "${PROFILE:-release}"
mkdir -p dist-test && rm -f dist-test/*.exe
cp src-tauri/target/"$TARGET"/"${PROFILE:-release}"/bundle/nsis/Estompe\ Test_*_x64-setup.exe dist-test/"Estompe-Test-$TICKET-setup.exe"
# Sous WSL : copie dans les Téléchargements de Windows (DOWNLOADS pour un autre dossier)
DL="${DOWNLOADS:-/mnt/c/Users/${WINUSER:-$USER}/Downloads}"
[ -d "$DL" ] && cp dist-test/"Estompe-Test-$TICKET-setup.exe" "$DL/" && echo "Copié dans $DL"
echo "Prêt : apps/estompe/windows/dist-test/Estompe-Test-$TICKET-setup.exe"
