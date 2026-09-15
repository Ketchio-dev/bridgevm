#!/bin/bash
set -euo pipefail
umask 077
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
work=$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-packaged-resources.XXXXXX")
trap 'rm -rf "$work"' EXIT
mkdir "$work/build" "$work/relocated" "$work/runtime-tmp"
"$root/tests/integration/compile-control-resources-smoke.sh" \
  "$root/tests/integration/PackagedControlResourcesSmoke.swift" "$work/build/ResourceSmoke"
app="$work/build/ResourceSmoke.app"
resources="$app/Contents/Resources/BridgeVMApp_BridgeVMControl.bundle"
mkdir -p "$app/Contents/MacOS" "$resources"
cp "$work/build/ResourceSmoke" "$app/Contents/MacOS/ResourceSmoke"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dev.bridgevm.packaged-resource-smoke</string>
<key>CFBundleExecutable</key><string>ResourceSmoke</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
PLIST
for name in windows-boot-seed-vars.fd.gz secureboot-microsoft-windows-transition-aarch64-v1.6.5.json; do
  cp "$root/apps/macos/Sources/BridgeVMControl/Resources/$name" "$resources/$name"
done
mv "$app" "$work/relocated/ResourceSmoke.app"
rm -rf "$work/build"
for mode in present missing-seed missing-policy missing-bundle root-decoy corrupt-policy; do
  destination="$work/$mode/ResourceSmoke.app"
  mkdir "$work/$mode"
  ditto "$work/relocated/ResourceSmoke.app" "$destination"
  bundle="$destination/Contents/Resources/BridgeVMApp_BridgeVMControl.bundle"
  case "$mode" in
    missing-seed) rm "$bundle/windows-boot-seed-vars.fd.gz" ;;
    missing-policy) rm "$bundle/secureboot-microsoft-windows-transition-aarch64-v1.6.5.json" ;;
    missing-bundle) rm -rf "$bundle" ;;
    root-decoy) mv "$bundle" "$destination/BridgeVMApp_BridgeVMControl.bundle" ;;
    corrupt-policy)
      python3 - "$bundle/secureboot-microsoft-windows-transition-aarch64-v1.6.5.json" <<'PY'
import json
import pathlib
import sys
path = pathlib.Path(sys.argv[1])
policy = json.loads(path.read_text())
policy["policy"] = "owned-corrupt-fixture"
path.write_text(json.dumps(policy))
PY
      ;;
  esac
  TMPDIR="$work/runtime-tmp/" "$destination/Contents/MacOS/ResourceSmoke" "$mode"
done
echo 'PASS: real relocated resource callers, exact bytes, typed missing errors, rejected corrupt policy'
