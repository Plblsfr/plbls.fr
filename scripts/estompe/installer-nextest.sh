#!/usr/bin/env bash
set -euo pipefail

version=0.9.152
case "$(uname -s)" in
  Linux) cible=x86_64-unknown-linux-gnu; somme=d039b43c3e0958441b6ad0a39c8884d018b3d2e8fb9928f209074b1100b5e17a; exe=cargo-nextest ;;
  MINGW*|MSYS*|CYGWIN*) cible=x86_64-pc-windows-msvc; somme=0384fc89a5a547d4fd5e183fcc101d31dba2837feafd8e64d5ea003a0ddb1941; exe=cargo-nextest.exe ;;
  *) echo "installer-nextest : système non pris en charge, installer avec cargo install cargo-nextest --version $version --locked" >&2; exit 1 ;;
esac

bin="${CARGO_HOME:-$HOME/.cargo}/bin"
if command -v cygpath >/dev/null; then bin="$(cygpath -u "$bin")"; fi
if [ -x "$bin/$exe" ] && "$bin/$exe" nextest --version 2>/dev/null | grep -q " $version "; then
  echo "installer-nextest : cargo-nextest $version déjà installé"
  exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
archive="cargo-nextest-$version-$cible.tar.gz"
curl -fsSL --retry 3 -o "$tmp/$archive" "https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-$version/$archive"
echo "$somme  $tmp/$archive" | sha256sum -c --quiet
mkdir -p "$bin"
tar -xzf "$tmp/$archive" -C "$bin" "$exe"
"$bin/$exe" nextest --version
