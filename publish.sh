#!/usr/bin/env bash
#
# publish.sh — release dist/LoveAroundSetup.exe (from build.sh, already signed
# for the updater: LoveAroundSetup.exe.sig) as the GitHub release v<version>
# of this repository, with latest.json for the app's updater.
#
# lovearound.me/download/LoveAroundSetup.exe and /download/windows/latest.json
# (the updater's endpoint in tauri.conf.json) redirect to the latest release
# (firebase.json in the site repo). latest.json points at this release's exe.
#
# Needs gh, logged in with write access to the repository; the release is
# tagged at HEAD, so push first.
#
#   REPO=…   the GitHub repository (default: lovearound-me/windows)
#
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
REPO="${REPO:-lovearound-me/windows}"

SETUP=dist/LoveAroundSetup.exe
for f in "$SETUP" "$SETUP.sig" dist/LoveAround.version; do
  [[ -f "$f" ]] || { echo ">> no $f — run ./build.sh first"; exit 1; }
done
VERSION="$(cat dist/LoveAround.version)"
TAG="v$VERSION"
if gh release view "$TAG" -R "$REPO" >/dev/null 2>&1; then
  echo ">> $TAG is already released"; exit 1
fi

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp "$SETUP" "$STAGE/LoveAroundSetup.exe"
cat > "$STAGE/latest.json" <<JSON
{
  "version": "$VERSION",
  "notes": "",
  "pub_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "platforms": {
    "windows-x86_64": {
      "signature": "$(cat "$SETUP.sig")",
      "url": "https://github.com/$REPO/releases/download/$TAG/LoveAroundSetup.exe"
    }
  }
}
JSON

echo ">> releasing $TAG ($(du -h "$SETUP" | cut -f1))"
gh release create "$TAG" -R "$REPO" --target "$(git rev-parse HEAD)" --latest \
  --title "Love Around $VERSION" --notes "" \
  "$STAGE/LoveAroundSetup.exe" "$STAGE/latest.json"
echo ">> done: https://github.com/$REPO/releases/tag/$TAG"
