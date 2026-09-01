#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="${1:-}"
[[ -n "$APP" && -d "$APP/Contents" ]] || { echo "usage: scripts/verify-app-third-party-notices.sh APP" >&2; exit 2; }
resources="$APP/Contents/Resources"
frameworks="$APP/Contents/Frameworks"
project_license="$resources/LICENSE"
notice="$resources/THIRD-PARTY-NOTICES.md"
[[ -s "$project_license" ]] || { echo "BridgeVM license is missing from the app" >&2; exit 1; }
cmp -s "$ROOT/LICENSE" "$project_license" || { echo "bundled BridgeVM license differs from the repository source" >&2; exit 1; }
[[ -s "$notice" ]] || { echo "third-party notice is missing from the app" >&2; exit 1; }
cmp -s "$ROOT/THIRD-PARTY-NOTICES.md" "$notice" || {
  echo "bundled third-party notice differs from the repository source" >&2
  exit 1
}
"$ROOT/scripts/verify-bundled-firmware-provenance.sh" "$APP"
"$ROOT/scripts/verify-bundled-wimlib-runtime.sh" "$APP"
"$ROOT/apps/macos/scripts/bundle-windows-catalog-verifier.sh" --verify-only "$APP" >/dev/null
for license in virglrenderer-MIT.txt libepoxy-MIT.txt; do
  [[ -s "$resources/licenses/$license" ]] || {
    echo "bundled host license is missing: $license" >&2
    exit 1
  }
done
"$ROOT/scripts/verify-rust-dependency-inventory.sh" "$resources/licenses/rust-dependencies.tsv"
"$ROOT/scripts/verify-rust-license-bundle.sh" "$resources/licenses/rust-dependencies.tsv" "$resources/licenses/rust-license-texts.txt"
if find "$APP/Contents" -type f -name '*.a' -print -quit | grep -q .; then
  echo "static archive found in distribution; LGPL dynamic-link proof is invalid" >&2
  exit 1
fi
# These are the LGPL components in the bundled swtpm dependency closure. Each
# must remain a separate Mach-O dylib and appear in another shipped Mach-O's
# otool dependency list (its own LC_ID_DYLIB is not proof of dynamic use).
for lgpl_lib in \
  libglib-2.0.0.dylib \
  libgobject-2.0.0.dylib \
  libgio-2.0.0.dylib \
  libgmodule-2.0.0.dylib \
  libjson-glib-1.0.0.dylib \
  libintl.8.dylib
do
  [[ -f "$frameworks/$lgpl_lib" && ! -L "$frameworks/$lgpl_lib" ]] || {
    echo "LGPL component is not a replaceable bundled dylib: $lgpl_lib" >&2
    exit 1
  }
  consumer=""
  for artifact in "$APP/Contents/Helpers/swtpm" "$frameworks"/*.dylib; do
    [[ "$(basename "$artifact")" == "$lgpl_lib" ]] && continue
    if otool -L "$artifact" | awk 'NR > 1 { print $1 }' | grep -F "/$lgpl_lib" >/dev/null; then
      consumer="${artifact#$APP/Contents/}"
      break
    fi
  done
  [[ -n "$consumer" ]] || {
    echo "otool found no dynamic consumer for LGPL component: $lgpl_lib" >&2
    exit 1
  }
  printf 'lgpl_dynamic_link=%s consumer=%s\n' "$lgpl_lib" "$consumer"
done
printf 'notice_sha256=%s\n' "$(shasum -a 256 "$notice" | awk '{ print $1 }')"
printf 'framework_count=%s\n' "$(find "$frameworks" -maxdepth 1 -type f -name '*.dylib' | wc -l | tr -d ' ')"
echo 'third_party_notice_gate=pass'
