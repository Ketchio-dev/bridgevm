#!/usr/bin/env bash
# Product-only WinPE builder: offline DISM from a host-verified package snapshot.
set -euo pipefail

ROOT="$(cd "${BASH_SOURCE[0]%/*}/.." && pwd -P)"
BUILD="$ROOT/scripts/build-hvf-windows-driver-injector.sh"; CHECK="$ROOT/scripts/check-hvf-windows-viogpu3d-package.sh"
ASSET_SOURCE="$ROOT/scripts/win-assets"
ISO=""; PACKAGE=""; OUT=""; WIMLIB=""
[[ "${1:-}" == --print-policy && $# == 1 ]] && { printf '%s\n' 'source=bundle-private-verified-snapshot' 'activation=offline-dism-only' 'testsigning=disabled' 'firstboot=absent' 'guest-agent=absent' 'overrides=cleared'; exit 0; }
while (( $# )); do
  case "$1" in
    --iso) ISO="$2"; shift 2;;
    --package) PACKAGE="$2"; shift 2;;
    --out) OUT="$2"; shift 2;;
    --wimlib) WIMLIB="$2"; shift 2;;
    *) echo "FAIL: unknown or incomplete argument: $1" >&2; exit 2;;
  esac
done
for value in "$ISO" "$PACKAGE" "$OUT" "$WIMLIB"; do
  [[ "$value" == /* && "$value" != *[[:space:]]* ]] || {
    echo "FAIL: all kernel-policy builder paths must be absolute and whitespace-free" >&2; exit 2; }
done
[[ -f "$ISO" && ! -L "$ISO" && -d "$PACKAGE" && ! -L "$PACKAGE" ]] || {
  echo "FAIL: ISO/package input is missing or symlinked" >&2; exit 1; }
[[ "$WIMLIB" == /opt/homebrew/bin/wimlib-imagex || "$WIMLIB" == /usr/local/bin/wimlib-imagex ]]
[[ -x "$WIMLIB" && -f "$BUILD" && -f "$CHECK" && ! -e "$OUT" && ! -L "$OUT" ]] || {
  echo "FAIL: fixed helper or exclusive output precondition failed" >&2; exit 1; }

WORK="$(/usr/bin/mktemp -d /private/tmp/bridgevm-kp-assets.XXXXXX)"
cleanup(){ /bin/rm -rf "$WORK"; }
trap cleanup EXIT INT TERM
/bin/chmod 700 "$WORK"
/bin/cp "$ASSET_SOURCE/bvinject.cmd" "$ASSET_SOURCE/winpeshl-inject.ini" "$WORK/"

/usr/bin/env -i PATH=/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin \
  VIOGPU3D_PROTOCOL=virgl /bin/bash "$CHECK" "$PACKAGE"
/usr/bin/env -i PATH=/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin \
  ISO="$ISO" OUT="$OUT" ASSETS="$WORK" WIMLIB_IMAGEX="$WIMLIB" \
  DRIVER_DIRS="viogpu3d:$PACKAGE" ENABLE_TESTSIGNING=0 KERNEL_POLICY_PACKAGE=1 \
  SKIP_OFFLINE_DISM=0 QUARANTINE_VIOGPU3D=0 DIAGNOSTICS_ONLY=0 PLANT_AGENT=0 \
  /bin/bash "$BUILD"
HASH="$(/usr/bin/shasum -a 256 "$OUT" | /usr/bin/awk '{print $1}')"
printf '%s\n' "$HASH" > "$OUT.sha256"; /bin/chmod 600 "$OUT.sha256"
printf 'injector_sha256=%s\n' "$HASH"
