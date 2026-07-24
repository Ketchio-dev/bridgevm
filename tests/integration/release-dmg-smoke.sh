#!/usr/bin/env bash
set -euo pipefail

# D2 smoke: build a DMG from a minimal ad-hoc-signed fixture .app, mount it,
# verify deep/strict codesign of the app inside, confirm the quickstart and the
# /Applications symlink are present, then detach. No network, no notarization.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DMG_SCRIPT="$ROOT/scripts/package-release-dmg.sh"
NOTARY_SCRIPT="$ROOT/scripts/notarize-submit.sh"

work="$(mktemp -d)"
mount_point=""
cleanup() {
  [[ -n "$mount_point" ]] && hdiutil detach "$mount_point" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT

fail() { echo "FAIL: $1" >&2; exit 1; }

# Minimal self-contained-looking fixture app, ad-hoc signed.
app="$work/BridgeVMControl.app"
mkdir -p "$app/Contents/MacOS"
cat > "$app/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleExecutable</key><string>BridgeVMControl</string>
  <key>CFBundleIdentifier</key><string>dev.bridgevm.control</string>
  <key>CFBundleName</key><string>BridgeVMControl</string>
  <key>CFBundleVersion</key><string>1</string>
</dict></plist>
EOF
printf '#!/bin/sh\nexit 0\n' > "$app/Contents/MacOS/BridgeVMControl"
chmod +x "$app/Contents/MacOS/BridgeVMControl"
codesign --force --sign - "$app" >/dev/null
codesign --verify --deep --strict "$app" || fail "fixture app failed self codesign"

dmg="$work/BridgeVM.dmg"
out="$(bash "$DMG_SCRIPT" --app "$app" --output "$dmg" --volname "BridgeVM-Test")"
[[ "$out" == "$dmg" && -f "$dmg" ]] || fail "dmg not produced"

# Refuse to overwrite existing output.
if bash "$DMG_SCRIPT" --app "$app" --output "$dmg" >/dev/null 2>&1; then
  fail "dmg script overwrote existing output"
fi

# Mount and inspect.
mount_point="$(mktemp -d)"
hdiutil attach "$dmg" -mountpoint "$mount_point" -nobrowse -readonly >/dev/null \
  || fail "hdiutil attach failed"
[[ -d "$mount_point/BridgeVMControl.app" ]] || fail "app missing in dmg"
[[ -f "$mount_point/QUICKSTART.md" ]] || fail "quickstart missing in dmg"
[[ -L "$mount_point/Applications" ]] || fail "/Applications symlink missing in dmg"
codesign --verify --deep --strict "$mount_point/BridgeVMControl.app" \
  || fail "app in dmg failed deep/strict codesign"
hdiutil detach "$mount_point" >/dev/null
mount_point=""

# Notarization boundary: with no Developer ID / profile, must be EXTERNAL, exit 0.
notary_out="$(bash "$NOTARY_SCRIPT" --dmg "$dmg" 2>&1)" || fail "notarize script errored"
grep -q "EXTERNAL_NOTARIZATION_REQUIRED" <<<"$notary_out" \
  || fail "notarize boundary not labelled EXTERNAL_NOTARIZATION_REQUIRED"

echo "PASS: release DMG builds, verifies, carries quickstart; notarization boundary EXTERNAL"
