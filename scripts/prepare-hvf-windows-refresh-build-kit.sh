#!/usr/bin/env bash
# Export the pinned 2D DOD source and BridgeVM timing patch for an ARM64 WDK build.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PIN=47e330f26153619255c3190c6aa8810cd9e7b7fd
SOURCE_DIR=""
OUT_DIR=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --source-dir) SOURCE_DIR="$2"; shift 2 ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    *) echo "usage: $0 --source-dir virtio-win-checkout --out-dir NEW-DIRECTORY" >&2; exit 2 ;;
  esac
done
[[ -n "$SOURCE_DIR" && -n "$OUT_DIR" && ! -e "$OUT_DIR" ]] || {
  echo "Provide a virtio-win checkout and an output directory that does not exist." >&2; exit 2;
}
git -C "$SOURCE_DIR" cat-file -e "$PIN^{commit}"
mkdir -p "$OUT_DIR/source"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
# Export the commit, never the checkout's working files or .git credentials.
git -C "$SOURCE_DIR" archive "$PIN" | tar -x -C "$OUT_DIR/source"
cp "$ROOT/scripts/patches/virtio-win-viogpudo-refresh.patch" "$OUT_DIR/"
git -C "$OUT_DIR/source" apply --check "$OUT_DIR/virtio-win-viogpudo-refresh.patch"
git -C "$OUT_DIR/source" apply "$OUT_DIR/virtio-win-viogpudo-refresh.patch"
# Exercise the exact helper used by GetScanLine, without Windows SDK headers.
RASTER_TEST_DIR="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-refresh-raster.XXXXXX")"
trap 'rm -rf "$RASTER_TEST_DIR"' EXIT
"${CXX:-c++}" -std=c++17 -Wall -Wextra -Werror \
  -I "$OUT_DIR/source/viogpu/viogpudo" \
  "$ROOT/tests/integration/viogpudo-refresh-raster-test.cpp" -o "$RASTER_TEST_DIR/raster-test"
"$RASTER_TEST_DIR/raster-test"
python3 "$ROOT/tests/integration/viogpudo-refresh-monitor-test.py" \
  "$OUT_DIR/source/viogpu/viogpudo/viogpudo.cpp"
cp "$ROOT/scripts/build-hvf-windows-refresh-driver.ps1" "$OUT_DIR/build.ps1"
cp "$ROOT/scripts/sign-hvf-windows-refresh-test-package.ps1" "$OUT_DIR/test-sign.ps1"
cp "$ROOT/docs/windows-refresh-driver.md" "$OUT_DIR/README.md"
printf 'upstream=https://github.com/virtio-win/kvm-guest-drivers-windows\ncommit=%s\n' "$PIN" > "$OUT_DIR/source-version.txt"
(cd "$OUT_DIR" && shasum -a 256 virtio-win-viogpudo-refresh.patch build.ps1 test-sign.ps1 > build-input-sha256.txt)
echo "Prepared $OUT_DIR (source only; no compiled or signed driver)."
