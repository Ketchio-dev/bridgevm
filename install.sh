#!/usr/bin/env bash
# BridgeVM installer: fetch the release tarball and place the app in
# /Applications. curl+tar attaches no quarantine attribute, so the app opens
# without the Gatekeeper block that a browser-downloaded dmg triggers.
#
#   curl -fsSL https://raw.githubusercontent.com/Ketchio-dev/bridgevm/main/install.sh | bash
set -euo pipefail

REPO="Ketchio-dev/bridgevm"
DEST="/Applications"

if [[ "$(uname -s)/$(uname -m)" != "Darwin/arm64" ]]; then
  echo "BridgeVM 1.0 runs Windows 11 ARM on Apple Silicon Macs only." >&2
  exit 1
fi

echo "Resolving the latest BridgeVM release..."
url=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" |
  /usr/bin/python3 -c 'import json,sys
assets = json.load(sys.stdin).get("assets", [])
for a in assets:
    if a["name"].endswith(".tar.gz"):
        print(a["browser_download_url"]); break')
[[ -n "$url" ]] || { echo "FAIL: no .tar.gz asset on the latest release" >&2; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
echo "Downloading $url"
curl -fL --progress-bar "$url" -o "$work/bridgevm.tar.gz"
tar -xzf "$work/bridgevm.tar.gz" -C "$work"

app=$(find "$work" -maxdepth 1 -name '*.app' | head -1)
[[ -n "$app" ]] || { echo "FAIL: tarball did not contain an .app bundle" >&2; exit 1; }

name=$(basename "$app")
if [[ -e "$DEST/$name" ]]; then
  echo "Replacing existing $DEST/$name"
  rm -rf "$DEST/$name"
fi
mv "$app" "$DEST/"

echo
echo "Installed $DEST/$name"
echo "First run: open it from Launchpad or:  open '$DEST/$name'"
echo "Windows 11 ARM setup is guided inside the app (docs/install.md has details)."
