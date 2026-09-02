#!/usr/bin/env bash
# Fail closed unless both packaged HVF executables carry release entitlements.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

verify_binary() {
  local binary="$1" label="$2" entitlements
  [[ -x "$binary" ]] || { echo "$label is missing or not executable: $binary" >&2; return 1; }
  codesign --verify --strict "$binary" >/dev/null 2>&1 || {
    echo "$label signature verification failed: $binary" >&2
    return 1
  }
  entitlements="$(codesign -d --entitlements :- "$binary" 2>/dev/null || true)"
  case "$entitlements" in
    *"<key>com.apple.security.hypervisor</key>"*"<true/>"*) ;;
    *) echo "$label is missing com.apple.security.hypervisor: $binary" >&2; return 1 ;;
  esac
  case "$entitlements" in
    *"<key>com.apple.security.get-task-allow</key>"*"<true/>"*)
      echo "$label retains the debug get-task-allow entitlement: $binary" >&2
      return 1
      ;;
  esac
}

verify_app() {
  local app="$1"
  [[ -d "$app/Contents" ]] || { echo "invalid app bundle: $app" >&2; return 1; }
  verify_binary "$app/Contents/Resources/target/release/hvf-runner" "hvf-runner" || return 1
  verify_binary "$app/Contents/Resources/target/release/examples/hvf_gic_boot_probe" "hvf_gic_boot_probe" || return 1
}

self_test() (
  local temporary app runner probe
  temporary="$(mktemp -d)"
  trap 'rm -rf "$temporary"' EXIT
  app="$temporary/BridgeVM.app"
  runner="$app/Contents/Resources/target/release/hvf-runner"
  probe="$app/Contents/Resources/target/release/examples/hvf_gic_boot_probe"
  install -d "$(dirname "$runner")" "$(dirname "$probe")"
  install -m 755 /bin/echo "$runner"
  install -m 755 /bin/echo "$probe"
  codesign --force --sign - --entitlements "$ROOT/apps/macos/HvfRunner.release.entitlements" "$runner" >/dev/null
  codesign --force --sign - --entitlements "$ROOT/apps/macos/HvfRunner.release.entitlements" "$probe" >/dev/null
  verify_app "$app"
  install -m 755 /bin/echo "$runner"
  codesign --force --sign - "$runner" >/dev/null
  if verify_app "$app" >/dev/null 2>&1; then
    echo "missing-entitlement fixture unexpectedly passed" >&2
    return 1
  fi
  install -m 755 /bin/echo "$runner"
  codesign --force --sign - --entitlements "$ROOT/apps/macos/HvfRunner.entitlements" "$runner" >/dev/null
  if verify_app "$app" >/dev/null 2>&1; then
    echo "debug-entitlement fixture unexpectedly passed" >&2
    return 1
  fi
  echo "packaged HVF entitlement contract: PASS"
)

case "${1:-}" in
  --self-test) self_test ;;
  "") echo "usage: scripts/verify-app-hvf-entitlements.sh APP|--self-test" >&2; exit 2 ;;
  *) verify_app "$1" ;;
esac
