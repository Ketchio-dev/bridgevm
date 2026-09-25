#!/usr/bin/env bash
# Select packaged executables without repository or target-directory overrides.

packaged_resources_root() {
  [[ "$ROOT" == *.app/Contents/Resources ]]
}

require_packaged_executable() {
  local path="$1"
  [[ -f "$path" && -x "$path" && ! -L "$path" && "$(/bin/realpath "$path")" == "$path" ]] || {
    echo "FAIL: packaged executable is missing or escapes its app: $path" >&2
    return 1
  }
}

select_installed_boot_probe() {
  local cargo_target_dir="${CARGO_TARGET_DIR:-}"
  if packaged_resources_root; then
    local app="${ROOT%/Contents/Resources}"
    [[ "$(/bin/realpath "$ROOT")" == "$ROOT" && "$app" == *.app \
      && "$BUILD_PROFILE" == release && "$SKIP_BUILD" == 1 \
      && -z "$cargo_target_dir" && -z "${BRIDGEVM_PREBUILT_PROBE:-}" ]] || {
      echo "FAIL: packaged release probe forbids repository and Cargo target overrides" >&2
      return 1
    }
    BIN="$ROOT/target/release/examples/hvf_gic_boot_probe"
    require_packaged_executable "$BIN" || return 1
    require_packaged_executable "$ROOT/target/release/bridgevm" || return 1
    if [[ -n "${VTPM_STATE_DIR:-}" ]]; then
      local bundled_swtpm="$app/Contents/Helpers/swtpm"
      [[ "$(/bin/realpath "$SWTPM_BIN" 2>/dev/null)" == "$bundled_swtpm" ]] || {
        echo "FAIL: packaged vTPM requires its bundled swtpm" >&2
        return 1
      }
      require_packaged_executable "$bundled_swtpm" || return 1
    fi
    /usr/bin/codesign --verify --deep --strict "$app" >/dev/null 2>&1 || {
      echo "FAIL: packaged app signature verification failed" >&2
      return 1
    }
    return 0
  fi
  if [[ -n "${BRIDGEVM_PREBUILT_PROBE:-}" ]]; then
    if [[ "$BUILD_PROFILE" != release || "$SKIP_BUILD" != 1 \
      || "$BRIDGEVM_PREBUILT_PROBE" != /* || ! -f "$BRIDGEVM_PREBUILT_PROBE" \
      || -L "$BRIDGEVM_PREBUILT_PROBE" ]]; then
      echo "FAIL: BRIDGEVM_PREBUILT_PROBE requires an absolute regular release binary with --skip-build" >&2
      return 1
    fi
    BIN="$BRIDGEVM_PREBUILT_PROBE"
  else
    if [[ -n "$cargo_target_dir" && "$cargo_target_dir" != /* ]]; then
      cargo_target_dir="$ROOT/$cargo_target_dir"
    fi
    if [[ "$BUILD_PROFILE" == release ]]; then
      BIN="${cargo_target_dir:+$cargo_target_dir/}release/examples/hvf_gic_boot_probe"
      [[ -n "$cargo_target_dir" ]] || BIN="target/release/examples/hvf_gic_boot_probe"
    else
      BIN="${cargo_target_dir:+$cargo_target_dir/}debug/examples/hvf_gic_boot_probe"
      [[ -n "$cargo_target_dir" ]] || BIN="target/debug/examples/hvf_gic_boot_probe"
    fi
  fi
}

run_bridgevm_cli() {
  local packaged_cli="$ROOT/target/release/bridgevm"
  if [[ -x "$packaged_cli" ]]; then
    "$packaged_cli" "$@"
  elif packaged_resources_root; then
    echo "FAIL: packaged bridgevm CLI is missing" >&2
    return 1
  else
    cargo run -q -p bridgevm-cli -- "$@"
  fi
}
