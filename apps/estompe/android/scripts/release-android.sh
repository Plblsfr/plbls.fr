#!/usr/bin/env bash
# Compile l'APK Android (arm64) signé et le dépose dans web/dist-updates/Estompe.apk (installation manuelle, hors Play Store).
# Prérequis : JDK 17, SDK Android + NDK (ANDROID_HOME, NDK_HOME), cibles rustup *-linux-android, keystore (voir ../README.md).
# Clé de signature : fichier ANDROID_KEYSTORE (défaut ~/.android/estompe.jks), alias « estompe », mot de passe ANDROID_KEYSTORE_PASSWORD.
# La même clé doit servir à chaque version : Android refuse de mettre à jour un APK signé avec une autre clé.
set -euo pipefail
cd "$(dirname "$0")/.."
# Le CLI Tauri est installé par `npm ci` à la racine du dépôt (workspace npm) ; Gradle l'appelle avec npx, qui le trouve dans le node_modules de la racine.
export PATH="$PWD/../../../node_modules/.bin:$PATH"
KEYSTORE="${ANDROID_KEYSTORE:-$HOME/.android/estompe.jks}"
: "${ANDROID_KEYSTORE_PASSWORD:?mot de passe du keystore absent}"
if [ -n "${CI:-}" ]; then echo "Version : $(node ../frontend/scripts/bump-version.mjs --apply)"; fi
# Android charge la bibliothèque (cdylib) ; sous Windows ce type de crate dépasse les 65 535 exports du linker mingw, d'où l'ajout ici seulement.
cp src-tauri/Cargo.toml /tmp/Cargo.toml.estompe && trap 'cp /tmp/Cargo.toml.estompe src-tauri/Cargo.toml' EXIT
sed -i 's|^crate-type = \["rlib"\]|crate-type = ["rlib", "cdylib"]|' src-tauri/Cargo.toml
tauri android build --apk --target aarch64 --ci
UNSIGNED=$(find src-tauri/gen/android/app/build/outputs/apk -name '*-unsigned.apk' | head -1)
OUT=../web/dist-updates
mkdir -p $OUT
"$(ls -d "${ANDROID_HOME:?}"/build-tools/*/apksigner | sort -V | tail -1)" sign --ks "$KEYSTORE" --ks-key-alias estompe --ks-pass env:ANDROID_KEYSTORE_PASSWORD --out $OUT/Estompe.apk "$UNSIGNED"
# android.json : version publiée, somme SHA-256 de l'APK (vérifiée par l'app avant l'installation) et politique de mise à jour (backend/core/updates.json)
SHA=$(sha256sum $OUT/Estompe.apk | cut -d' ' -f1)
node -e 'console.log(JSON.stringify({ version: require("./src-tauri/tauri.conf.json").version, notes: process.argv[1], sha256: process.argv[2], ...JSON.parse(process.argv[3]) }))' "${1:-}" "$SHA" "$(node ../frontend/scripts/update-policy.mjs android)" > $OUT/android.json
echo "Prêt : web/dist-updates/Estompe.apk, android.json"
