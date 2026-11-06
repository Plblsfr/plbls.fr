#!/usr/bin/env bash
# Compile l'installeur Windows signé et prépare le dossier à publier (latest.json + installeur + .sig) dans web/dist-updates/.
# Usage : windows/scripts/release-windows.sh "Notes de version"   (en CI la version est calculée d'après l'historique ; en local, augmenter `version` dans tauri.conf.json et Cargo.toml)
# Prérequis : npm ci à la racine du dépôt (workspace npm : le CLI Tauri est dans son node_modules).
# Mise à jour forcée, version minimale et message : backend/core/updates.json (voir frontend/scripts/update-policy.mjs).
# La clé privée : variable TAURI_SIGNING_PRIVATE_KEY (secret de la CI) ou ~/.tauri/estompe.key. La CI l'appelle avant de construire l'image.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$PWD/../../../node_modules/.bin:$PATH"
export TAURI_SIGNING_PRIVATE_KEY="${TAURI_SIGNING_PRIVATE_KEY:-$HOME/.tauri/estompe.key}"
# NATIVE_ALPHA=1 (variable du dépôt) : build « alpha » du partage d'écran natif en Rust (EST-18). libwebrtc n'existe que pour MSVC :
# compilation croisée avec cargo-xwin et feature livekit-native. Sans, build normal inchangé (mingw).
if [ "${NATIVE_ALPHA:-}" = 1 ]; then
  TARGET=x86_64-pc-windows-msvc; RUNNER=(--runner cargo-xwin); FEATURES=(--features livekit-native)
else
  TARGET=x86_64-pc-windows-gnu; RUNNER=(); FEATURES=()
fi
# En CI, la version de .version est écrite dans les fichiers de l'app (frontend/scripts/bump-version.mjs --apply) ; en local, celle de tauri.conf.json.
if [ -n "${CI:-}" ]; then echo "Version : $(node ../frontend/scripts/bump-version.mjs --apply)"; fi
# PROFILE=ci : profil Cargo allégé (sans LTO) pour les builds de main ; par défaut (local, tags) : release complet.
PROFILE="${PROFILE:-release}"
tauri build "${RUNNER[@]}" --target "$TARGET" --bundles nsis -- --profile "$PROFILE" "${FEATURES[@]}"
VERSION=$(node -p "require('./src-tauri/tauri.conf.json').version")
SRC=src-tauri/target/$TARGET/$PROFILE/bundle/nsis
EXE="Estompe_${VERSION}_x64-setup.exe"
OUT=../web/dist-updates
mkdir -p $OUT && find $OUT -type f ! -name .gitkeep -delete
cp "$SRC/$EXE" "$SRC/$EXE.sig" $OUT/
cp "$SRC/$EXE" $OUT/Estompe-setup.exe # adresse stable du bouton « Télécharger » de l'interface web
node -e '
const [v, exe, notes, policy, out] = process.argv.slice(1)
const fs = require("fs")
fs.writeFileSync(out + "/latest.json", JSON.stringify({
  version: v, notes, ...JSON.parse(policy), pub_date: new Date().toISOString(),
  platforms: { "windows-x86_64": { signature: fs.readFileSync(out + "/" + exe + ".sig", "utf8").trim(), url: "https://estompe.plbls.fr/updates/" + exe } },
}, null, 2))' "$VERSION" "$EXE" "${1:-}" "$(node ../frontend/scripts/update-policy.mjs windows)" "$OUT"
# web.json : version servie et politique du web/PWA, relus par l'interface pour proposer ou imposer le rechargement
node -e 'console.log(JSON.stringify({ version: process.argv[1], ...JSON.parse(process.argv[2]) }))' "$VERSION" "$(node ../frontend/scripts/update-policy.mjs web)" > $OUT/web.json
echo "Prêt : web/dist-updates/ ($EXE). Embarqué dans l'image par web/Dockerfile."
