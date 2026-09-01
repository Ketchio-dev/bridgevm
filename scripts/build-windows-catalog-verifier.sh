#!/usr/bin/env bash
# Build the BridgeVM-owned Windows CAT verifier against a fixed OpenSSL prefix.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE="$ROOT/apps/macos/Helpers/bridgevm-catalog-verify.c"
OUTPUT=""
OPENSSL_PREFIX=""
usage() {
  echo 'usage: build-windows-catalog-verifier.sh --output FILE --openssl-prefix DIR' >&2
}
while [[ $# -gt 0 ]]; do
  case "$1" in
    --output) [[ $# -ge 2 ]] || { usage; exit 2; }; OUTPUT="$2"; shift 2 ;;
    --openssl-prefix) [[ $# -ge 2 ]] || { usage; exit 2; }; OPENSSL_PREFIX="$2"; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) usage; exit 2 ;;
  esac
done
[[ "$OUTPUT" == /* && ! -e "$OUTPUT" ]] || {
  echo 'catalog verifier output must be an absent absolute path' >&2
  exit 2
}
[[ "$OPENSSL_PREFIX" == /* && -d "$OPENSSL_PREFIX/include/openssl" ]] || {
  echo 'OpenSSL prefix is missing its public headers' >&2
  exit 2
}
[[ -f "$OPENSSL_PREFIX/lib/libcrypto.3.dylib" && ! -L "$OPENSSL_PREFIX/lib/libcrypto.3.dylib" ]] || {
  echo 'OpenSSL prefix is missing a regular libcrypto.3.dylib' >&2
  exit 2
}
[[ -f "$SOURCE" ]] || { echo 'catalog verifier source is missing' >&2; exit 1; }
mkdir -p "$(dirname "$OUTPUT")"
/usr/bin/clang -std=c11 -O2 -Wall -Wextra -Werror \
  -I"$OPENSSL_PREFIX/include" "$SOURCE" "$ROOT/apps/macos/Helpers/bridgevm-catalog-content-digest.c" \
  -L"$OPENSSL_PREFIX/lib" -lcrypto -o "$OUTPUT"
[[ -x "$OUTPUT" && ! -L "$OUTPUT" ]] || { echo 'catalog verifier build failed' >&2; exit 1; }
otool -L "$OUTPUT" | awk 'NR > 1 { print $1 }' | grep -F '/libcrypto.3.dylib' >/dev/null || {
  echo 'catalog verifier is not linked to libcrypto.3.dylib' >&2
  exit 1
}
"$OUTPUT" --version | grep -F 'bridgevm-catalog-verify-v1 libcrypto=OpenSSL 3.' >/dev/null || {
  echo 'catalog verifier reported an unsupported libcrypto' >&2
  exit 1
}
printf '%s\n' "$OUTPUT"
