#!/usr/bin/env bash
# Build and install the CAT verifier after the app-local crypto closure.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
APP="${1:-}"
WORK="${2:-}"
[[ "$APP" == /*.app && -d "$APP/Contents" && "$WORK" == /* && -d "$WORK" ]] || {
  echo 'usage: install-windows-catalog-verifier.sh ABSOLUTE_APP ABSOLUTE_WORK_DIR' >&2
  exit 2
}
if [[ -d /opt/homebrew/opt/openssl@3 ]]; then
  openssl_prefix=/opt/homebrew/opt/openssl@3
else
  openssl_prefix=/usr/local/opt/openssl@3
fi
verifier="$WORK/bridgevm-catalog-verify"
"$ROOT/scripts/build-windows-catalog-verifier.sh" --output "$verifier" --openssl-prefix "$openssl_prefix" >/dev/null
BRIDGEVM_CODESIGN_IDENTITY="${BRIDGEVM_CODESIGN_IDENTITY:--}" \
  "$ROOT/apps/macos/scripts/bundle-swtpm-runtime.sh" --app "$APP" >/dev/null
BRIDGEVM_CODESIGN_IDENTITY="${BRIDGEVM_CODESIGN_IDENTITY:--}" \
  "$ROOT/apps/macos/scripts/bundle-windows-catalog-verifier.sh" --app "$APP" --verifier "$verifier" >/dev/null
