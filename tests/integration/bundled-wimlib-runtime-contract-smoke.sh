#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD="$ROOT/scripts/build-pinned-wimlib-runtime.sh"
VERIFY="$ROOT/scripts/verify-bundled-wimlib-runtime.sh"
PACKAGE="$ROOT/apps/macos/scripts/package-hvf-control-app.sh"
BUNDLE="$ROOT/apps/macos/scripts/bundle-wimlib-runtime.sh"
RELEASE="$ROOT/.github/workflows/release.yml"
SWIFT="$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsWimlib.swift"

bash -n "$BUILD" "$VERIFY" "$PACKAGE" "$BUNDLE"
"$BUILD" --help >/dev/null 2>&1
grep -qF '84221a3abd5b91228f15f8e6065c335a336237b5738197b75bf419eea561a194' "$BUILD"
grep -qF -- '--without-ntfs-3g --without-fuse --disable-shared --enable-static' "$BUILD"
grep -qF 'source/wimlib-1.14.5.tar.gz' "$BUNDLE"
grep -qF 'codesign --force --sign' "$BUNDLE"
grep -qF 'bundle-wimlib-runtime.sh" --runtime "$WIMLIB_RUNTIME"' "$PACKAGE"
grep -qF -- '--wimlib-runtime "$RUNNER_TEMP/wimlib-runtime"' "$RELEASE"
grep -qF 'scripts/build-pinned-wimlib-runtime.sh' "$RELEASE"
grep -qF 'helpers/wimlib-imagex' "$SWIFT"
grep -qF '#if DEBUG' "$SWIFT"
grep -qF '/opt/homebrew/bin/wimlib-imagex' "$SWIFT"
grep -qF 'wimlib-imagex 1.14.5' "$ROOT/THIRD-PARTY-NOTICES.md"
grep -qF 'GPL-3.0-or-later' "$ROOT/THIRD-PARTY-NOTICES.md"

python3 - "$SWIFT" <<'PY'
import pathlib
import sys

text = pathlib.Path(sys.argv[1]).read_text()
debug_start = text.index("#if DEBUG")
debug_end = text.index("#endif", debug_start)
for forbidden in ("/opt/homebrew/bin/wimlib-imagex", "/usr/local/bin/wimlib-imagex"):
    position = text.index(forbidden)
    if not debug_start < position < debug_end:
        raise SystemExit(f"ambient runtime candidate escaped the DEBUG-only block: {forbidden}")
PY

echo "bundled wimlib runtime contract: PASS"
