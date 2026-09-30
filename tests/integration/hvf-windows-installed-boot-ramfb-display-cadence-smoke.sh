#!/usr/bin/env bash
# The 3D-off display window's ramfb export thread has its own period,
# --ramfb-display-export-ms (16..60000 ms), forwarded only with the shared
# framebuffer. It must not move --display-export-ms, which still paces the PPM
# exporter and virtio-gpu CPU readback with its unchanged 100..60000 bound.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
export ROOT

STORE="$(mktemp -d "/tmp/bridgevm-ramfb-display-cadence.XXXXXX")"
trap 'rm -rf "$STORE"' EXIT
touch "$STORE/target.raw" "$STORE/vars.fd"
mkdir -p "$STORE/evidence"
FB="$STORE/evidence/display.fb"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

installed_boot_env() {
  bash -c '
    set -euo pipefail
    source scripts/run-hvf-windows-installed-boot-validation.sh
    source scripts/run-hvf-windows-installed-boot-args.sh
    source scripts/run-hvf-windows-installed-boot-runner.sh
    init_installed_boot_defaults
    parse_installed_boot_args "$@"
    build_installed_boot_env_args
  ' _ --target "$STORE/target.raw" --vars "$STORE/vars.fd" \
    --evidence-dir "$STORE/evidence" "$@" 2>&1
}

# The last value the probe would see for $2 in env output $1, or "unset".
last_value() {
  local line
  line="$(grep "^$2=" <<< "$1" | tail -1 || true)"
  [[ -n "$line" ]] && echo "${line#*=}" || echo "unset"
}

expect() {
  local output="$1" name="$2" want="$3" got
  got="$(last_value "$output" "$name")"
  [[ "$got" == "$want" ]] || fail "$name is $got, want $want; env: $output"
}

app="$(installed_boot_env --display-export-ms 100 --display-export-fb "$FB" \
  --ramfb-display-export-ms 33)" || fail "app arguments rejected: $app"
expect "$app" BRIDGEVM_RAMFB_DISPLAY_EXPORT_MS 33
expect "$app" BRIDGEVM_DISPLAY_EXPORT_MS 100
expect "$app" BRIDGEVM_VIRTIO_GPU_SCANOUT_READBACK_MS 100
expect "$app" BRIDGEVM_DISPLAY_EXPORT_FB "$FB"

shared="$(installed_boot_env --display-export-ms 250 --display-export-fb "$FB")" ||
  fail "shared cadence rejected: $shared"
expect "$shared" BRIDGEVM_RAMFB_DISPLAY_EXPORT_MS unset
expect "$shared" BRIDGEVM_DISPLAY_EXPORT_MS 250

no_fb="$(installed_boot_env --ramfb-display-export-ms 33)" || fail "no-fb run rejected: $no_fb"
expect "$no_fb" BRIDGEVM_RAMFB_DISPLAY_EXPORT_MS unset

for bad in 15 60001 fast; do
  if out="$(installed_boot_env --display-export-fb "$FB" --ramfb-display-export-ms "$bad")"; then
    fail "--ramfb-display-export-ms $bad was accepted"
  fi
  grep -q "FAIL: --ramfb-display-export-ms requires an integer from 16 to 60000" <<< "$out" ||
    fail "--ramfb-display-export-ms $bad: $out"
done
if out="$(installed_boot_env --display-export-ms 33)"; then
  fail "--display-export-ms lost its 100 ms floor"
fi
grep -q "FAIL: --display-export-ms requires an integer from 100 to 60000" <<< "$out" ||
  fail "--display-export-ms 33: $out"

echo "PASS: ramfb display cadence is its own bounded flag"
