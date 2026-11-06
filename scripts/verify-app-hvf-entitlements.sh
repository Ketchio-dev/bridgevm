#!/usr/bin/env bash
# Fail closed unless both packaged HVF executables carry release entitlements, and
# the app and the probe, which ask for and open the guest microphone, carry audio-input.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

verify_binary() {
  local binary="$1" label="$2"; shift 2
  [[ -e "$binary" ]] || { echo "$label is missing: $binary" >&2; return 1; }
  [[ -d "$binary" || -x "$binary" ]] || { echo "$label is not executable: $binary" >&2; return 1; }
  codesign --verify --strict "$binary" >/dev/null 2>&1 || {
    echo "$label signature verification failed: $binary" >&2
    return 1
  }
  codesign -d --entitlements :- "$binary" 2>/dev/null |
    python3 "$ROOT/scripts/verify-signing-entitlements.py" "$@" --profile release
}

verify_app() {
  local app="$1"
  [[ -d "$app/Contents" ]] || { echo "invalid app bundle: $app" >&2; return 1; }
  verify_binary "$app" "app" com.apple.security.device.audio-input || return 1
  verify_binary "$app/Contents/Resources/target/release/hvf-runner" "hvf-runner" \
    com.apple.security.hypervisor || return 1
  verify_binary "$app/Contents/Resources/target/release/examples/hvf_gic_boot_probe" "hvf_gic_boot_probe" \
    com.apple.security.hypervisor com.apple.security.device.audio-input || return 1
}

self_test() {
  python3 "$ROOT/tests/integration/signing-entitlements-contract.py"
}

case "${1:-}" in
  --self-test) self_test ;;
  "") echo "usage: scripts/verify-app-hvf-entitlements.sh APP|--self-test" >&2; exit 2 ;;
  *) verify_app "$1" ;;
esac
