#!/usr/bin/env bash
# Verify the fixed nested-app identity required by the T17 Accessibility gate.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fail() { echo "product E2E helper verification failed: $*" >&2; return 1; }
plist_value() { /usr/libexec/PlistBuddy -c "Print :$2" "$1" 2>/dev/null; }
verify_app() {
  local app="$1" helper_app helper_bin plist signature
  helper_app="$app/Contents/Helpers/BridgeVMProductE2E.app"
  helper_bin="$helper_app/Contents/MacOS/BridgeVMProductE2E"
  plist="$helper_app/Contents/Info.plist"
  [[ -d "$app/Contents" && ! -L "$app" ]] || fail "invalid outer app: $app" || return 1
  [[ ! -e "$app/Contents/MacOS/BridgeVMProductE2E" ]] || fail "legacy bare helper is present" || return 1
  [[ -d "$helper_app" && ! -L "$helper_app" ]] || fail "nested helper app is missing or a symlink" || return 1
  [[ -f "$plist" && ! -L "$plist" ]] || fail "Info.plist is missing or a symlink" || return 1
  [[ -x "$helper_bin" && -f "$helper_bin" && ! -L "$helper_bin" ]] || fail "helper executable is missing, non-regular, or a symlink" || return 1
  [[ "$(plist_value "$plist" CFBundleExecutable)" == BridgeVMProductE2E ]] || fail "wrong CFBundleExecutable" || return 1
  [[ "$(plist_value "$plist" CFBundleIdentifier)" == dev.bridgevm.product-e2e ]] || fail "wrong CFBundleIdentifier" || return 1
  [[ "$(plist_value "$plist" CFBundlePackageType)" == APPL ]] || fail "wrong CFBundlePackageType" || return 1
  [[ "$(plist_value "$plist" LSUIElement)" == true ]] || fail "LSUIElement must be true" || return 1
  codesign --verify --strict "$helper_app" >/dev/null 2>&1 || fail "nested app signature is invalid" || return 1
  signature="$(codesign -dvv "$helper_app" 2>&1)" || fail "nested app signing metadata is unreadable" || return 1
  grep -Fq 'Identifier=dev.bridgevm.product-e2e' <<<"$signature" || fail "signed identifier is not stable" || return 1
}
self_test() (
  local temporary app helper
  temporary="$(mktemp -d)"; trap 'rm -rf "$temporary"' EXIT
  app="$temporary/BridgeVM.app"; helper="$app/Contents/Helpers/BridgeVMProductE2E.app"
  install -d "$app/Contents/MacOS" "$helper/Contents/MacOS"
  install -m 644 "$ROOT/apps/macos/BridgeVMProductE2E-Info.plist" "$helper/Contents/Info.plist"
  install -m 755 /bin/echo "$helper/Contents/MacOS/BridgeVMProductE2E"
  codesign --force --sign - "$helper" >/dev/null
  verify_app "$app"
  : > "$app/Contents/MacOS/BridgeVMProductE2E"
  if verify_app "$app" >/dev/null 2>&1; then fail "legacy-layout fixture unexpectedly passed"; fi
  rm "$app/Contents/MacOS/BridgeVMProductE2E"
  /usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier dev.bridgevm.mutable-helper' "$helper/Contents/Info.plist"
  codesign --force --sign - "$helper" >/dev/null
  if verify_app "$app" >/dev/null 2>&1; then fail "mutable-identifier fixture unexpectedly passed"; fi
  echo "product E2E nested helper contract: PASS"
)
case "${1:-}" in
  --self-test) self_test ;;
  "") echo "usage: scripts/verify-product-e2e-helper-app.sh APP|--self-test" >&2; exit 2 ;;
  *) verify_app "$1" ;;
esac
