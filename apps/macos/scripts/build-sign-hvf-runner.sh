#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
MACOS_DIR="$ROOT/apps/macos"
DEBUG_ENTITLEMENTS="$MACOS_DIR/HvfRunner.entitlements"
IDENTITY="${BRIDGEVM_CODESIGN_IDENTITY:--}"
BUILD_PROFILE="debug"

usage() {
  cat >&2 <<'EOF'
usage: apps/macos/scripts/build-sign-hvf-runner.sh [--release] [--output PATH]
       apps/macos/scripts/build-sign-hvf-runner.sh --verify-only PATH

Builds hvf-runner, signs it with the Apple Hypervisor.framework entitlement,
verifies that entitlement, and prints the signed binary path.

Environment:
  BRIDGEVM_CODESIGN_IDENTITY     codesign identity, defaults to ad-hoc '-'
  BRIDGEVM_HVF_ENTITLEMENTS      entitlements plist path, defaults to
                                  apps/macos/HvfRunner.entitlements

The signed runner can execute the empty HVF VM create/destroy probe. It still
does not enter firmware, create vCPUs, or boot Windows.
EOF
}

VERIFY_ONLY=""
OUTPUT=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --release)
      BUILD_PROFILE="release"
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

ENTITLEMENTS="${BRIDGEVM_HVF_ENTITLEMENTS:-$DEBUG_ENTITLEMENTS}"
LOCK_DIR="${BRIDGEVM_HVF_RUNNER_SIGN_LOCK_DIR:-$ROOT/target/.bridgevm-hvf-runner-sign.lock}"

acquire_sign_lock() {
  install -d "$ROOT/target"
  local attempts=0
  while ! mkdir "$LOCK_DIR" 2>/dev/null; do
    attempts=$((attempts + 1))
    if [[ "$attempts" -ge 600 ]]; then
      echo "timed out waiting for hvf-runner signing lock: $LOCK_DIR" >&2
      exit 1
    fi
    sleep 0.1
  done
  trap 'rmdir "$LOCK_DIR" 2>/dev/null || true' EXIT
}

verify_entitlement() {
  local bin="$1"
  local entitlements_output
  codesign --verify --strict "$bin" >/dev/null 2>&1 || {
    echo "hvf-runner signature verification failed: $bin" >&2
    exit 1
  }
  entitlements_output="$(codesign -d --entitlements :- "$bin" 2>/dev/null || true)"
  case "$entitlements_output" in
    *"<key>com.apple.security.hypervisor</key>"*"<true/>"*) ;;
    *)
      echo "hvf-runner is missing com.apple.security.hypervisor entitlement: $bin" >&2
      exit 1
      ;;
  esac
}

acquire_sign_lock

if [[ -n "$VERIFY_ONLY" ]]; then
  [[ -x "$VERIFY_ONLY" ]] || {
    echo "hvf-runner binary is not executable: $VERIFY_ONLY" >&2
    exit 1
  }
  verify_entitlement "$VERIFY_ONLY"
  printf '%s\n' "$VERIFY_ONLY"
  exit 0
fi

[[ -f "$ENTITLEMENTS" ]] || {
  echo "Entitlements file not found: $ENTITLEMENTS" >&2
  exit 1
}

case "$BUILD_PROFILE" in
  debug)
    cargo build -p hvf-runner --quiet
    BIN="$ROOT/target/debug/hvf-runner"
    ;;
  release)
    cargo build -p hvf-runner --release --quiet
    BIN="$ROOT/target/release/hvf-runner"
    ;;
  *)
    echo "invalid build profile: $BUILD_PROFILE" >&2
    exit 2
    ;;
esac

codesign --force --sign "$IDENTITY" --entitlements "$ENTITLEMENTS" "$BIN" >/dev/null

SIGNED_BIN="$BIN"
if [[ -n "$OUTPUT" ]]; then
  OUTPUT_DIR="$(dirname "$OUTPUT")"
  if [[ ! -d "$OUTPUT_DIR" ]]; then
    install -d "$OUTPUT_DIR"
  fi
  install -m 755 "$BIN" "$OUTPUT"
  codesign --force --sign "$IDENTITY" --entitlements "$ENTITLEMENTS" "$OUTPUT" >/dev/null
  SIGNED_BIN="$OUTPUT"
fi

verify_entitlement "$SIGNED_BIN"
printf '%s\n' "$SIGNED_BIN"
