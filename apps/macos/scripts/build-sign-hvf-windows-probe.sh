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
usage: apps/macos/scripts/build-sign-hvf-windows-probe.sh [--release] [--output PATH] [--bundle-frameworks DIR]
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
BUNDLE_FRAMEWORKS=""
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
    --bundle-frameworks)
      [[ $# -ge 2 ]] || {
        usage
        exit 2
      }
      BUNDLE_FRAMEWORKS="$2"
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
  if [[ "$IDENTITY" != "-" ]]; then
    CODESIGN_OPTIONS="${BRIDGEVM_HVF_PROBE_CODESIGN_OPTIONS:-runtime}"
  fi
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
  if [[ "$bin" == */Contents/Resources/target/release/examples/hvf_gic_boot_probe ]]; then
    local frameworks="${bin%/Contents/Resources/target/release/examples/hvf_gic_boot_probe}/Contents/Frameworks"
    local dependency
    for dependency in libvirglrenderer.1.dylib libepoxy.0.dylib; do
      [[ -f "$frameworks/$dependency" ]] || {
        echo "hvf_gic_boot_probe bundled dependency is missing: $frameworks/$dependency" >&2
        exit 1
      }
      codesign --verify --strict "$frameworks/$dependency" >/dev/null 2>&1 || {
        echo "hvf_gic_boot_probe bundled dependency signature failed: $frameworks/$dependency" >&2
        exit 1
      }
    done
    if otool -L "$bin" "$frameworks/libvirglrenderer.1.dylib" \
      | awk '/^[[:space:]]+\// { print }' \
      | grep -E '/Users/|/opt/homebrew/' >/dev/null; then
      echo "hvf_gic_boot_probe retains a development-host dylib path" >&2
      exit 1
    fi
  fi
}

bundle_runtime() {
  local bin="$1"
  local frameworks="$2"
  local virgl_source epoxy_source virgl_name epoxy_name
  virgl_source="$(otool -L "$bin" | awk '/libvirglrenderer[^[:space:]]*\.dylib/ { print $1; exit }')"
  [[ -f "$virgl_source" ]] || {
    echo "hvf_gic_boot_probe libvirglrenderer dependency is missing: ${virgl_source:-<not linked>}" >&2
    exit 1
  }
  epoxy_source="$(otool -L "$virgl_source" | awk '/libepoxy[^[:space:]]*\.dylib/ { print $1; exit }')"
  [[ -f "$epoxy_source" ]] || {
    echo "libvirglrenderer libepoxy dependency is missing: ${epoxy_source:-<not linked>}" >&2
    exit 1
  }
  virgl_name="$(basename "$virgl_source")"
  epoxy_name="$(basename "$epoxy_source")"
  [[ "$virgl_name" == "libvirglrenderer.1.dylib" && "$epoxy_name" == "libepoxy.0.dylib" ]] || {
    echo "unexpected HVF graphics dependency names: $virgl_name $epoxy_name" >&2
    exit 1
  }
  install -d "$frameworks"
  install -m 755 "$virgl_source" "$frameworks/$virgl_name"
  install -m 755 "$epoxy_source" "$frameworks/$epoxy_name"
  install_name_tool -id "@rpath/$virgl_name" "$frameworks/$virgl_name"
  install_name_tool -change "$epoxy_source" "@loader_path/$epoxy_name" "$frameworks/$virgl_name"
  install_name_tool -id "@rpath/$epoxy_name" "$frameworks/$epoxy_name"
  install_name_tool -change "$virgl_source" "@loader_path/../../../../Frameworks/$virgl_name" "$bin"
  local dependency
  for dependency in "$frameworks/$epoxy_name" "$frameworks/$virgl_name"; do
    local dependency_codesign=(--force --sign "$IDENTITY")
    if [[ -n "$CODESIGN_OPTIONS" ]]; then
      dependency_codesign+=(--options "$CODESIGN_OPTIONS")
    fi
    codesign "${dependency_codesign[@]}" "$dependency" >/dev/null
  done
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

cargo_args=(build --quiet -p bridgevm-hvf --example hvf_gic_boot_probe --features venus)
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

if [[ -n "$BUNDLE_FRAMEWORKS" ]]; then
  [[ -n "$OUTPUT" ]] || {
    echo "--bundle-frameworks requires --output" >&2
    exit 2
  }
  bundle_runtime "$SIGNED_BIN" "$BUNDLE_FRAMEWORKS"
  codesign "${codesign_args[@]}" "$SIGNED_BIN" >/dev/null
fi

verify_probe "$SIGNED_BIN"
printf '%s\n' "$SIGNED_BIN"
