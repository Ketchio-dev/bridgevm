#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
MACOS_DIR="$ROOT/apps/macos"
DEBUG_ENTITLEMENTS="$MACOS_DIR/AppleVzRunner.entitlements"
RELEASE_ENTITLEMENTS="$MACOS_DIR/AppleVzRunner.release.entitlements"
IDENTITY="${BRIDGEVM_CODESIGN_IDENTITY:--}"
CODESIGN_OPTIONS="${BRIDGEVM_APPLE_VZ_CODESIGN_OPTIONS:-}"
BUILD_CONFIGURATION="${BRIDGEVM_APPLE_VZ_BUILD_CONFIGURATION:-debug}"

usage() {
  cat >&2 <<'EOF'
usage: apps/macos/scripts/build-sign-apple-vz-runner.sh [--release] [--output PATH]
       apps/macos/scripts/build-sign-apple-vz-runner.sh --verify-only PATH

Builds AppleVzRunner with SwiftPM, signs it with the configured Apple
virtualization entitlements, verifies that entitlement, and prints the signed
binary path.

Environment:
  BRIDGEVM_CODESIGN_IDENTITY       codesign identity, defaults to ad-hoc '-'
  BRIDGEVM_APPLE_VZ_ENTITLEMENTS   entitlements plist path, defaults to
                                    apps/macos/AppleVzRunner.entitlements, or
                                    apps/macos/AppleVzRunner.release.entitlements
                                    when --release is used
  BRIDGEVM_APPLE_VZ_CODESIGN_OPTIONS
                                    optional codesign --options value; defaults
                                    to runtime when --release is used
  BRIDGEVM_APPLE_VZ_BUILD_CONFIGURATION
                                    SwiftPM build configuration, defaults to
                                    debug, or release when --release is used

For Developer ID release packaging, provide a Developer ID identity and set
--release. Use --output to copy the signed helper into an app bundle.
EOF
}

VERIFY_ONLY=""
OUTPUT=""
RELEASE=0
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

ENTITLEMENTS="${BRIDGEVM_APPLE_VZ_ENTITLEMENTS:-$DEBUG_ENTITLEMENTS}"
if [[ "$RELEASE" == "1" ]]; then
  ENTITLEMENTS="${BRIDGEVM_APPLE_VZ_ENTITLEMENTS:-$RELEASE_ENTITLEMENTS}"
  CODESIGN_OPTIONS="${BRIDGEVM_APPLE_VZ_CODESIGN_OPTIONS:-runtime}"
  BUILD_CONFIGURATION="${BRIDGEVM_APPLE_VZ_BUILD_CONFIGURATION:-release}"
fi

verify_entitlement() {
  local bin="$1"
  local entitlements_output
  codesign --verify --strict "$bin" >/dev/null 2>&1 || {
    echo "AppleVzRunner signature verification failed: $bin" >&2
    exit 1
  }
  entitlements_output="$(codesign -d --entitlements :- "$bin" 2>/dev/null || true)"
  case "$entitlements_output" in
    *"<key>com.apple.security.virtualization</key>"*"<true/>"*) ;;
    *)
      echo "AppleVzRunner is missing com.apple.security.virtualization entitlement: $bin" >&2
      exit 1
      ;;
  esac
}

if [[ -n "$VERIFY_ONLY" ]]; then
  [[ -x "$VERIFY_ONLY" ]] || {
    echo "AppleVzRunner binary is not executable: $VERIFY_ONLY" >&2
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
case "$BUILD_CONFIGURATION" in
  debug|release) ;;
  *)
    echo "BRIDGEVM_APPLE_VZ_BUILD_CONFIGURATION must be debug or release, got: $BUILD_CONFIGURATION" >&2
    exit 2
    ;;
esac

(
  cd "$MACOS_DIR"
  swift build --configuration "$BUILD_CONFIGURATION" --quiet --product AppleVzRunner
)

BIN="$(cd "$MACOS_DIR" && swift build --configuration "$BUILD_CONFIGURATION" --show-bin-path)/AppleVzRunner"
CODESIGN_OPTION_ARGS=()
if [[ -n "$CODESIGN_OPTIONS" ]]; then
  CODESIGN_OPTION_ARGS=(--options "$CODESIGN_OPTIONS")
fi
if [[ "${#CODESIGN_OPTION_ARGS[@]}" -gt 0 ]]; then
  codesign --force "${CODESIGN_OPTION_ARGS[@]}" --sign "$IDENTITY" --entitlements "$ENTITLEMENTS" "$BIN" >/dev/null
else
  codesign --force --sign "$IDENTITY" --entitlements "$ENTITLEMENTS" "$BIN" >/dev/null
fi

SIGNED_BIN="$BIN"
if [[ -n "$OUTPUT" ]]; then
  OUTPUT_DIR="$(dirname "$OUTPUT")"
  if [[ ! -d "$OUTPUT_DIR" ]]; then
    install -d "$OUTPUT_DIR"
  fi
  install -m 755 "$BIN" "$OUTPUT"
  SIGNED_BIN="$OUTPUT"
fi

verify_entitlement "$SIGNED_BIN"
printf '%s\n' "$SIGNED_BIN"
