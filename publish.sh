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
# tagged at HEAD, so push first. Needs the history and tags (for the notes).
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

# What changed, for the release page: each `Release-note: …` line in the
# messages of the commits since the last release, oldest first. One line per
# change, anywhere in the message; a commit nobody would notice has none.
PREV="$(git describe --tags --abbrev=0 --match 'v*' HEAD 2>/dev/null || true)"
NOTES="$(git log --reverse --format=%B "${PREV:+$PREV..}HEAD" \
  | sed -n -e 's/^Release-note:[[:space:]]*\(.*[^[:space:]]\)[[:space:]]*$/- \1/p')"
if [[ -z "$NOTES" ]]; then
  echo "::warning::no Release-note lines since ${PREV:-the first commit}"
  NOTES="Small fixes under the hood."
fi
printf '>> notes:\n%s\n' "$NOTES"

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
  --title "Love Around $VERSION" --notes "$NOTES" \
  "$STAGE/LoveAroundSetup.exe" "$STAGE/latest.json"
echo ">> done: https://github.com/$REPO/releases/tag/$TAG"
