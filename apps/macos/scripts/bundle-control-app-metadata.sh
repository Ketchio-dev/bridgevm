#!/usr/bin/env bash
# Install the native app's identity before the containing bundle is signed.
set -euo pipefail
[[ $# == 2 ]] || { echo "usage: $0 STAGED_APP SHORT_VERSION" >&2; exit 2; }
app="$1"
version="$2"
macos_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[[ "$app" == *.app && -d "$app/Contents/Resources" ]] || { echo "expected a staged app bundle" >&2; exit 2; }
[[ "$version" =~ ^[0-9]+(\.[0-9]+){2}([.-][0-9A-Za-z]+)*$ ]] || { echo "invalid bundle short version" >&2; exit 2; }
[[ ! -e "$app/Contents/Info.plist" && ! -L "$app/Contents/Info.plist" &&
   ! -e "$app/Contents/Resources/BridgeVM.icns" && ! -L "$app/Contents/Resources/BridgeVM.icns" ]] || {
  echo "refusing to overwrite app metadata" >&2; exit 1;
}
work="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-icon.XXXXXX")"
trap 'rm -rf "$work"' EXIT
/usr/bin/xcrun swiftc -parse-as-library \
  "$macos_dir/Sources/BridgeVMControl/BridgeVMMark.swift" \
  "$macos_dir/scripts/GenerateControlAppIcon.swift" -o "$work/render-icon"
"$work/render-icon" "$work/BridgeVM.iconset"
/usr/bin/iconutil --convert icns --output "$work/BridgeVM.icns" "$work/BridgeVM.iconset"
install -m 644 "$macos_dir/BridgeVMControl-Info.plist" "$work/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $version" "$work/Info.plist"
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$work/Info.plist")" == "$version" ]] || {
  echo "bundle short version did not persist" >&2; exit 1;
}
install -m 644 "$work/BridgeVM.icns" "$app/Contents/Resources/BridgeVM.icns"
install -m 644 "$work/Info.plist" "$app/Contents/Info.plist"
