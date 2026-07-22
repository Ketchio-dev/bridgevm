#!/usr/bin/env bash
# Build a WinPE injector image for a test-signed Windows ARM64 viogpu3d package.
# This is a thin P3 wrapper around build-hvf-windows-driver-injector.sh: it
# validates the driver package shape, stages viogpu3d under \drivers\viogpu3d,
# and enables offline BCD test-signing through the injector marker.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_INJECTOR="${BUILD_INJECTOR:-"$ROOT/scripts/build-hvf-windows-driver-injector.sh"}"
CHECK_PACKAGE="${CHECK_PACKAGE:-"$ROOT/scripts/check-hvf-windows-viogpu3d-package.sh"}"
VIOGPU3D_DIR="${VIOGPU3D_DIR:-${1:-}}"
NETKVM_DIR="${NETKVM_DIR:-"$HOME/BridgeVM/drivers/netkvm"}"
OUT="${OUT:-"$HOME/BridgeVM/win-viogpu3d-injector.raw"}"

usage() {
  cat >&2 <<'EOF'
usage: VIOGPU3D_DIR=/path/to/viogpu3d-package scripts/build-hvf-windows-viogpu3d-injector.sh

Environment:
  VIOGPU3D_DIR       Directory containing viogpu3d .inf/.sys/.cat files. .sys
                     and any .dll files must be ARM64 PE images. May also be
                     passed as the first positional argument.
  VIOGPU3D_PROTOCOL  auto, venus, or virgl. Passed to the package checker.
                     Default: auto.
  VIOGPU3D_MANIFEST  Optional package manifest path. Passed to the checker.
  NETKVM_DIR         Optional netkvm driver directory to stage too. Default:
                     $HOME/BridgeVM/drivers/netkvm when present.
  OUT                Output raw injector image. Default:
                     $HOME/BridgeVM/win-viogpu3d-injector.raw.
  EXTRA_DRIVER_DIRS  Optional extra "name:path" specs prepended to DRIVER_DIRS.
  PPSSPP_DIR         Optional native ARM64 PPSSPP directory. Stages the real
                     title with BridgeVM's 30-second Venus promotion gate.

The wrapper always sets ENABLE_TESTSIGNING=1 for the underlying injector.
EOF
}

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

log() {
  printf '[build-viogpu3d-injector] %s\n' "$*"
}

reject_whitespace_path() {
  case "$2" in
    *[[:space:]]*) fail "$1 path contains whitespace, unsupported by DRIVER_DIRS: $2" ;;
  esac
}

[[ "${1:-}" != "-h" && "${1:-}" != "--help" ]] || { usage; exit 0; }
[[ -n "$VIOGPU3D_DIR" ]] || { usage; fail "VIOGPU3D_DIR is required"; }
[[ -d "$VIOGPU3D_DIR" ]] || fail "viogpu3d driver directory not found: $VIOGPU3D_DIR"
reject_whitespace_path VIOGPU3D_DIR "$VIOGPU3D_DIR"
reject_whitespace_path OUT "$OUT"
checker_args=("$CHECK_PACKAGE")
if [[ -n "${VIOGPU3D_MANIFEST:-}" ]]; then
  checker_args+=(--manifest "$VIOGPU3D_MANIFEST")
fi
checker_args+=("$VIOGPU3D_DIR")
package_report="$(VIOGPU3D_PROTOCOL="${VIOGPU3D_PROTOCOL:-auto}" "${checker_args[@]}")"
while IFS= read -r line; do
  log "$line"
done <<<"$package_report"
package_protocol="$(awk -F= '$1 == "protocol" { print $2; exit }' <<<"$package_report")"
[[ -n "$package_protocol" ]] || fail "package checker did not report protocol"

driver_dirs="${EXTRA_DRIVER_DIRS:-}"
if [[ -d "$NETKVM_DIR" ]]; then
  reject_whitespace_path NETKVM_DIR "$NETKVM_DIR"
  if compgen -G "$NETKVM_DIR/*.inf" >/dev/null; then
    driver_dirs="${driver_dirs:+$driver_dirs }netkvm:$NETKVM_DIR"
  else
    log "NETKVM_DIR has no .inf; skipping $NETKVM_DIR"
  fi
else
  log "NETKVM_DIR not found; staging viogpu3d only: $NETKVM_DIR"
fi
driver_dirs="${driver_dirs:+$driver_dirs }viogpu3d:$VIOGPU3D_DIR"

log "driver package: $VIOGPU3D_DIR"
log "driver protocol: $package_protocol"
log "output: $OUT"
log "driver dirs: $driver_dirs"
ENABLE_TESTSIGNING=1 DRIVER_DIRS="$driver_dirs" OUT="$OUT" "$BUILD_INJECTOR"
