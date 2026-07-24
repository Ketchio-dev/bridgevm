#!/usr/bin/env bash
set -euo pipefail

# D2 RELEASE-TURNKEY: wrap a packaged self-contained BridgeVMControl.app into a
# distributable DMG with a first-run quickstart. Verifies the app codesign
# deep/strict inside the image. Notarization is a separate, credential-gated
# step (scripts/notarize-submit.sh); on a free Apple account it stays EXTERNAL.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat >&2 <<'EOF'
usage: scripts/package-release-dmg.sh --app APP.app --output OUT.dmg [--volname NAME]

Builds OUT.dmg containing APP.app plus QUICKSTART.md and an /Applications
symlink. The app must already be a self-contained, deep/strict-signed bundle
(apps/macos/scripts/package-hvf-control-app.sh output). Existing output is
never overwritten.
EOF
}

APP=""
OUTPUT=""
VOLNAME="BridgeVM"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --app) [[ $# -ge 2 ]] || { usage; exit 2; }; APP="$2"; shift 2 ;;
    --output) [[ $# -ge 2 ]] || { usage; exit 2; }; OUTPUT="$2"; shift 2 ;;
    --volname) [[ $# -ge 2 ]] || { usage; exit 2; }; VOLNAME="$2"; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) usage; exit 2 ;;
  esac
done

[[ -n "$APP" && -n "$OUTPUT" ]] || { usage; exit 2; }
[[ "$OUTPUT" == *.dmg ]] || { echo "output must end in .dmg: $OUTPUT" >&2; exit 2; }
[[ ! -e "$OUTPUT" ]] || { echo "refusing to overwrite existing output: $OUTPUT" >&2; exit 1; }
[[ -d "$APP" && "$APP" == *.app ]] || { echo "app must be a .app bundle: $APP" >&2; exit 1; }

# The app must already be a valid deep/strict-signed self-contained bundle.
codesign --verify --deep --strict "$APP" || {
  echo "app failed deep/strict codesign verification: $APP" >&2
  exit 1
}

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
cp -R "$APP" "$stage/"
ln -s /Applications "$stage/Applications"

cat > "$stage/QUICKSTART.md" <<'EOF'
# BridgeVM — Quickstart

1. Drag **BridgeVMControl.app** onto the **Applications** shortcut in this window.
2. Launch BridgeVMControl from Applications.
3. On first run, choose **Import existing Windows VM** and select:
   - the installed Windows raw disk image,
   - its 64 MiB UEFI vars file,
   - (optional) its vTPM state directory.
4. Click **Boot** to start the imported VM to the Windows desktop.

Notes
- This build is signed with a development/ad-hoc identity. Distribution to other
  Macs requires a Developer ID signature and Apple notarization
  (see scripts/notarize-submit.sh) — those are external to this build.
- The app is self-contained: firmware, swtpm/libtpms, and the HVF runner are
  bundled. No separate install step is required.
EOF

# hdiutil builds a compressed read-only image from the staging folder.
hdiutil create \
  -volname "$VOLNAME" \
  -srcfolder "$stage" \
  -fs HFS+ \
  -format UDZO \
  -ov \
  "$OUTPUT" >/dev/null

printf '%s\n' "$OUTPUT"
