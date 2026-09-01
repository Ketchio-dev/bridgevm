#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORK="${1:-}"
[[ "$WORK" == /* && -d "$WORK" ]] || { echo 'usage: build-windows-catalog-verifier-test-helper.sh ABSOLUTE_WORK_DIR' >&2; exit 2; }
if [[ -d /opt/homebrew/opt/openssl@3 ]]; then
  openssl_prefix=/opt/homebrew/opt/openssl@3
else
  openssl_prefix=/usr/local/opt/openssl@3
fi
output="$WORK/bridgevm-catalog-verify"
"$ROOT/scripts/build-windows-catalog-verifier.sh" --output "$output" --openssl-prefix "$openssl_prefix" >/dev/null
printf '%s\n' "$output"
