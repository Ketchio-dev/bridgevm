#!/usr/bin/env bash
# Bundle and verify the fixed CAT helper against the app-local libcrypto.
set -euo pipefail

IDENTITY="${BRIDGEVM_CODESIGN_IDENTITY:--}"
APP=""
SOURCE=""
VERIFY_ONLY=0
usage() {
  echo 'usage: bundle-windows-catalog-verifier.sh --app APP --verifier FILE | --verify-only APP' >&2
}
while [[ $# -gt 0 ]]; do
  case "$1" in
    --app) [[ $# -ge 2 ]] || { usage; exit 2; }; APP="$2"; shift 2 ;;
    --verifier) [[ $# -ge 2 ]] || { usage; exit 2; }; SOURCE="$2"; shift 2 ;;
    --verify-only) [[ $# -ge 2 ]] || { usage; exit 2; }; APP="$2"; VERIFY_ONLY=1; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) usage; exit 2 ;;
  esac
done
[[ "$APP" == *.app && -d "$APP/Contents" ]] || { usage; exit 2; }

HELPER="$APP/Contents/Resources/helpers/bridgevm-catalog-verify"
FRAMEWORKS="$APP/Contents/Frameworks"
RUNTIME="$APP/Contents/Resources/catalog-verifier"
MANIFEST="$RUNTIME/manifest.txt"
SWTPM_MANIFEST="$APP/Contents/Resources/swtpm/manifest.txt"
SWTPM_LICENSES="$APP/Contents/Resources/swtpm/licenses"

verify_runtime() {
  [[ -f "$HELPER" && -x "$HELPER" && ! -L "$HELPER" ]] || {
    echo 'bundled catalog verifier is missing, non-executable, or a symlink' >&2
    exit 1
  }
  [[ -s "$MANIFEST" ]] || { echo 'catalog verifier manifest is missing' >&2; exit 1; }
  grep -Fx 'format=bridgevm-windows-catalog-verifier-v1' "$MANIFEST" >/dev/null || {
    echo 'catalog verifier manifest has the wrong format' >&2
    exit 1
  }
  local crypto_name dependency expected actual openssl_version formula_version
  crypto_name="$(awk -F= '$1 == "libcrypto" { print $2; exit }' "$MANIFEST")"
  [[ "$crypto_name" == 'libcrypto.3.dylib' && -f "$FRAMEWORKS/$crypto_name" ]] || {
    echo 'catalog verifier app-local libcrypto is missing' >&2
    exit 1
  }
  dependency="$(otool -L "$HELPER" | awk 'NR > 1 { print $1 }' | grep 'libcrypto\.3\.dylib$' || true)"
  [[ "$dependency" == '@executable_path/../../Frameworks/libcrypto.3.dylib' ]] || {
    echo 'catalog verifier retains a non-app-local libcrypto dependency' >&2
    exit 1
  }
  if otool -L "$HELPER" | awk 'NR > 1 { print $1 }' | grep -E '/Users/|/opt/homebrew/|/usr/local/' >/dev/null; then
    echo 'catalog verifier retains a development-host dependency path' >&2
    exit 1
  fi
  expected="$(awk -F= '$1 == "helper_sha256" { print $2; exit }' "$MANIFEST")"
  actual="$(shasum -a 256 "$HELPER" | awk '{ print $1 }')"
  [[ "$expected" =~ ^[0-9a-f]{64}$ && "$actual" == "$expected" ]] || {
    echo 'catalog verifier manifest digest mismatch' >&2
    exit 1
  }
  codesign --verify --strict "$HELPER" >/dev/null 2>&1 || {
    echo 'catalog verifier code signature is invalid' >&2
    exit 1
  }
  openssl_version="$("$HELPER" --version)"
  grep -Fx "helper_version=$openssl_version" "$MANIFEST" >/dev/null || {
    echo 'catalog verifier version differs from its manifest' >&2
    exit 1
  }
  formula_version="$(awk -F'|' '$1 == "component" && $2 == "openssl@3" { print $3; exit }' "$SWTPM_MANIFEST")"
  [[ -n "$formula_version" && "$openssl_version" == *"OpenSSL $formula_version "* ]] || {
    echo 'catalog verifier and bundled OpenSSL component versions differ' >&2
    exit 1
  }
  find "$SWTPM_LICENSES" -type f -name "openssl@3-$formula_version-LICENSE*" -print -quit | grep -q . || {
    echo 'bundled OpenSSL license is missing for the catalog verifier' >&2
    exit 1
  }
}

if [[ "$VERIFY_ONLY" == 1 ]]; then
  verify_runtime
  printf '%s\n' "$APP"
  exit 0
fi

[[ "$SOURCE" == /* && -f "$SOURCE" && -x "$SOURCE" && ! -L "$SOURCE" ]] || {
  echo 'catalog verifier source must be an absolute regular executable' >&2
  exit 2
}
[[ ! -e "$HELPER" ]] || { echo 'refusing to overwrite a bundled catalog verifier' >&2; exit 1; }
source_crypto="$(otool -L "$SOURCE" | awk 'NR > 1 { print $1 }' | grep '/libcrypto\.3\.dylib$' || true)"
[[ -n "$source_crypto" ]] || { echo 'catalog verifier source has no libcrypto dependency' >&2; exit 1; }
other_non_system="$(otool -L "$SOURCE" | awk 'NR > 1 { print $1 }' | grep -Ev '^(/usr/lib/|/System/Library/)' | grep -v '/libcrypto\.3\.dylib$' || true)"
[[ -z "$other_non_system" ]] || { echo 'catalog verifier has an unexpected non-system dependency' >&2; exit 1; }
[[ -f "$FRAMEWORKS/libcrypto.3.dylib" ]] || {
  echo 'bundle swtpm and its libcrypto closure before the catalog verifier' >&2
  exit 1
}
install -d "$(dirname "$HELPER")" "$RUNTIME"
install -m 755 "$SOURCE" "$HELPER"
install_name_tool -change "$source_crypto" \
  '@executable_path/../../Frameworks/libcrypto.3.dylib' "$HELPER"
if [[ "$IDENTITY" == '-' ]]; then
  codesign --force --sign - "$HELPER" >/dev/null
else
  codesign --force --sign "$IDENTITY" --options runtime --timestamp "$HELPER" >/dev/null
fi
helper_version="$("$HELPER" --version)"
{
  printf '%s\n' \
    'format=bridgevm-windows-catalog-verifier-v1' \
    'libcrypto=libcrypto.3.dylib' \
    "helper_version=$helper_version" \
    "helper_sha256=$(shasum -a 256 "$HELPER" | awk '{ print $1 }')"
} > "$MANIFEST"
verify_runtime
printf '%s\n' "$APP"
