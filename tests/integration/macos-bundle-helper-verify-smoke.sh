#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-bundle-helper-verify.XXXXXX")"
APP="$WORKDIR/BridgeVM.app"
RUNNER="$APP/Contents/Helpers/AppleVzRunner"

cleanup() {
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Helpers"
cat >"$APP/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>BridgeVMApp</string>
  <key>CFBundleIdentifier</key>
  <string>dev.bridgevm.bundle-helper-smoke</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
</dict>
</plist>
EOF
printf '#!/bin/sh\necho BridgeVMApp smoke\n' >"$APP/Contents/MacOS/BridgeVMApp"
printf '#!/bin/sh\necho unsigned AppleVzRunner smoke\n' >"$RUNNER"
chmod +x "$APP/Contents/MacOS/BridgeVMApp" "$RUNNER"
codesign --force --deep --sign - "$APP" >/dev/null

cp -R "$APP" "$WORKDIR/MissingIdentifier.app"
/usr/libexec/PlistBuddy -c "Delete :CFBundleIdentifier" "$WORKDIR/MissingIdentifier.app/Contents/Info.plist"
codesign --force --deep --sign - "$WORKDIR/MissingIdentifier.app" >/dev/null

set +e
output="$("$ROOT/packaging/macos/build-debug-app-bundle.sh" --verify-only "$WORKDIR/MissingIdentifier.app" 2>&1)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "bundle verify-only accepted a missing CFBundleIdentifier"
[[ "$output" == *"Info.plist is missing CFBundleIdentifier"* ]] || {
  fail "bundle verify-only did not report the missing CFBundleIdentifier"
}

set +e
output="$("$ROOT/packaging/macos/build-debug-app-bundle.sh" --verify-only "$APP" 2>&1)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "bundle verify-only accepted a helper without the virtualization entitlement"
[[ "$output" == *"AppleVzRunner is missing com.apple.security.virtualization entitlement"* ]] || {
  fail "bundle verify-only did not report the missing AppleVzRunner entitlement"
}

echo "PASS: macOS bundle helper verify smoke"
