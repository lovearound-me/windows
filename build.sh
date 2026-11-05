#!/usr/bin/env bash
#
# build.sh — cross-build the Windows app on Linux (or a Mac) in Docker: Tauri
# with Rust's MSVC target through cargo-xwin, and an NSIS installer.
#
#   ./build.sh   → dist/LoveAroundSetup.exe (+ .sig, LoveAround.version)
#
# The installer is signed for the updater with the minisign key in
# ~/.config/lovearound/tauri_updater.key (TAURI_KEY=… for another path); the
# app checks lovearound.me/download/windows/latest.json against its public key.
# There is no Authenticode signing: Windows SmartScreen warns on first launch.
#
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

# 0.2.<commit count of this repository>. 0.2 because the 0.1 builds came from
# the old shared repository, whose count reached 0.1.92.
VERSION="0.2.$(git rev-list --count HEAD 2>/dev/null || echo 0)"
KEY="${TAURI_KEY:-$HOME/.config/lovearound/tauri_updater.key}"
[[ -f "$KEY" ]] || { echo ">> no updater key at $KEY"; exit 1; }

IMAGE=lovearound-windows-build
if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  echo ">> building the $IMAGE image"
  docker build -t "$IMAGE" -f Dockerfile.build .
fi

echo ">> building $VERSION"
# The whole repository: the app embeds the dictionaries in i18n/.
docker run --rm -v "$PWD":/repo \
  -v lovearound-cargo-registry:/usr/local/cargo/registry -v lovearound-xwin:/opt/xwin \
  -e TAURI_SIGNING_PRIVATE_KEY="$(cat "$KEY")" -e TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
  -w /repo/src-tauri "$IMAGE" \
  cargo tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --config "{\"version\":\"$VERSION\"}"

BUNDLE=src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis
SETUP="$(ls -t "$BUNDLE"/*"_${VERSION}_"*-setup.exe | head -1)"
rm -rf dist
mkdir -p dist
cp "$SETUP" dist/LoveAroundSetup.exe
cp "$SETUP.sig" dist/LoveAroundSetup.exe.sig
echo "$VERSION" > dist/LoveAround.version
# Docker writes as root: hand the outputs back.
docker run --rm -v "$PWD":/src "$IMAGE" chown -R "$(id -u):$(id -g)" /src/src-tauri/target /src/src-tauri/gen 2>/dev/null || true
echo ">> dist/LoveAroundSetup.exe ($VERSION)"
