#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
MACOS_DIR="$ROOT/apps/macos"
DEBUG_ENTITLEMENTS="$MACOS_DIR/HvfRunner.entitlements"
RELEASE_ENTITLEMENTS="$MACOS_DIR/HvfRunner.release.entitlements"
IDENTITY="${BRIDGEVM_CODESIGN_IDENTITY:--}"
CODESIGN_OPTIONS="${BRIDGEVM_HVF_PROBE_CODESIGN_OPTIONS:-}"
RELEASE=0

usage() {
  cat >&2 <<'EOF'
usage: apps/macos/scripts/build-sign-hvf-windows-probe.sh [--release] [--output PATH]
       apps/macos/scripts/build-sign-hvf-windows-probe.sh --verify-only PATH

Builds the real hvf_gic_boot_probe used by the installed-Windows wrapper,
signs it with the Apple Hypervisor.framework entitlement, verifies the result,
and prints the signed binary path.

Environment:
  BRIDGEVM_CODESIGN_IDENTITY       codesign identity, defaults to ad-hoc '-'
  BRIDGEVM_HVF_PROBE_ENTITLEMENTS  entitlements plist path; defaults to the
                                    debug plist, or the release plist with
                                    --release
  BRIDGEVM_HVF_PROBE_CODESIGN_OPTIONS
                                    optional codesign --options value; defaults
                                    to runtime with --release
EOF
}

VERIFY_ONLY=""
OUTPUT=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --release)
      RELEASE=1
      shift
      ;;
    --output)
      [[ $# -ge 2 ]] || {
        usage
        exit 2
      }
      OUTPUT="$2"
      shift 2
      ;;
    --verify-only)
      [[ $# -ge 2 ]] || {
        usage
        exit 2
      }
      VERIFY_ONLY="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

ENTITLEMENTS="${BRIDGEVM_HVF_PROBE_ENTITLEMENTS:-$DEBUG_ENTITLEMENTS}"
if [[ "$RELEASE" == "1" ]]; then
  ENTITLEMENTS="${BRIDGEVM_HVF_PROBE_ENTITLEMENTS:-$RELEASE_ENTITLEMENTS}"
  CODESIGN_OPTIONS="${BRIDGEVM_HVF_PROBE_CODESIGN_OPTIONS:-runtime}"
fi

verify_probe() {
  local bin="$1"
  local entitlements_output
  codesign --verify --strict "$bin" >/dev/null 2>&1 || {
    echo "hvf_gic_boot_probe signature verification failed: $bin" >&2
    exit 1
  }
  entitlements_output="$(codesign -d --entitlements :- "$bin" 2>/dev/null || true)"
  case "$entitlements_output" in
    *"<key>com.apple.security.hypervisor</key>"*"<true/>"*) ;;
    *)
      echo "hvf_gic_boot_probe is missing com.apple.security.hypervisor entitlement: $bin" >&2
      exit 1
      ;;
  esac
}

if [[ -n "$VERIFY_ONLY" ]]; then
  [[ -x "$VERIFY_ONLY" ]] || {
    echo "hvf_gic_boot_probe is missing or not executable: $VERIFY_ONLY" >&2
    exit 1
  }
  verify_probe "$VERIFY_ONLY"
  printf '%s\n' "$VERIFY_ONLY"
  exit 0
fi

[[ -f "$ENTITLEMENTS" ]] || {
  echo "Entitlements file not found: $ENTITLEMENTS" >&2
  exit 1
}

cargo_args=(build --quiet -p bridgevm-hvf --example hvf_gic_boot_probe)
profile_dir="debug"
if [[ "$RELEASE" == "1" ]]; then
  cargo_args+=(--release)
  profile_dir="release"
fi
cargo "${cargo_args[@]}"

BIN="$ROOT/target/$profile_dir/examples/hvf_gic_boot_probe"
codesign_args=(--force --sign "$IDENTITY" --entitlements "$ENTITLEMENTS")
if [[ -n "$CODESIGN_OPTIONS" ]]; then
  codesign_args+=(--options "$CODESIGN_OPTIONS")
fi
codesign "${codesign_args[@]}" "$BIN" >/dev/null

SIGNED_BIN="$BIN"
if [[ -n "$OUTPUT" ]]; then
  install -d "$(dirname "$OUTPUT")"
  install -m 755 "$BIN" "$OUTPUT"
  codesign "${codesign_args[@]}" "$OUTPUT" >/dev/null
  SIGNED_BIN="$OUTPUT"
fi

verify_probe "$SIGNED_BIN"
printf '%s\n' "$SIGNED_BIN"
