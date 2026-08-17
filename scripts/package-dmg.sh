#!/usr/bin/env bash
# Produce a compressed .dmg for a packaged BridgeVM app.
#
# The dmg is the conventional-looking install path. Unlike the tarball it DOES
# acquire com.apple.quarantine when downloaded by a browser, so docs/install.md
# walks the user through the one-time "Open Anyway" flow.
set -euo pipefail

usage() { echo "usage: $0 --app <path/to/BridgeVM.app> --output <path/to/out.dmg>" >&2; exit 2; }

APP="" OUT=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --app) APP=$2; shift 2 ;;
    --output) OUT=$2; shift 2 ;;
    *) usage ;;
  esac
done
[[ -n "$APP" && -n "$OUT" ]] || usage
[[ -d "$APP" ]] || { echo "FAIL: app bundle not found: $APP" >&2; exit 1; }

codesign --verify --deep --strict "$APP" ||
  { echo "FAIL: $APP does not pass codesign verification" >&2; exit 1; }

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
cp -R "$APP" "$stage/"
ln -s /Applications "$stage/Applications"

mkdir -p "$(dirname "$OUT")"
rm -f "$OUT"
hdiutil create -quiet -volname "BridgeVM" -srcfolder "$stage" -format UDZO "$OUT"

shasum -a 256 "$OUT"
echo "dmg: PASS ($OUT)"
