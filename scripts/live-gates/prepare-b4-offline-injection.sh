#!/usr/bin/env bash
# Build the exact B4 injector and apply it once to a disposable source clone.
set -euo pipefail
: "${REPO:?}" "${OUT:?}" "${STAGE:?}" "${SOURCE:?}" "${SOURCE_VARS:?}" "${INJECTOR_VARS:?}" "${INJECTOR_ISO:?}"
: "${VIOGPU_DIR:?}" "${VIOGPU_MANIFEST:?}" "${VIOGPU_PACKAGE_SHA256:?}"
PACKAGE_TOOL="$REPO/scripts/live-gates/b4-diagnostic-package.py"
INJECTOR="$STAGE/injector.raw"
seal() { openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'; }
fail() { echo "FAIL: $*" >&2; exit 1; }
cleanup() { rm -f "$INJECTOR" "$INJECTOR.assets-sha256"; }
trap cleanup EXIT
[[ "$STAGE" == "$HOME/BridgeVM/work/pointer-source-"*/stage && "$OUT" == */preparation/injection ]] || fail 'unsafe B4 injection paths'
for input in "$SOURCE" "$SOURCE_VARS" "$INJECTOR_VARS" "$INJECTOR_ISO"; do [[ -f "$input" && ! -L "$input" ]] && head -c1 "$input" >/dev/null 2>&1 || fail "B4 injection input is unsafe: $input"; done
injector_vars_hash=$(seal "$INJECTOR_VARS"); injector_iso_hash=$(seal "$INJECTOR_ISO")
[[ "$($PACKAGE_TOOL verify --manifest "$VIOGPU_MANIFEST" --dir "$VIOGPU_DIR")" == "$VIOGPU_PACKAGE_SHA256" ]] || fail 'sealed package failed pre-injection verification'
mkdir -p "$OUT/run"
ISO="$INJECTOR_ISO" NETKVM_DIR="$STAGE/no-unsealed-netkvm" KEEP_RUNNING=1 QUARANTINE_VIOGPU3D=1 \
  VIOGPU3D_PROTOCOL=venus VIOGPU3D_DIR="$VIOGPU_DIR" OUT="$INJECTOR" \
  "$REPO/scripts/build-hvf-windows-viogpu3d-injector.sh" >"$OUT/build.log" 2>&1
[[ -f "$INJECTOR" && -f "$INJECTOR.assets-sha256" ]] || fail 'B4 injector build outputs are absent'
injector_hash=$(seal "$INJECTOR"); assets_hash=$(tr -d '\r\n' < "$INJECTOR.assets-sha256")
[[ "$injector_hash" =~ ^[0-9a-f]{64}$ && "$assets_hash" =~ ^[0-9a-f]{64}$ ]] || fail 'B4 injector identity is malformed'
cp -c "$SOURCE" "$STAGE/disk.raw"; cp "$INJECTOR_VARS" "$STAGE/vars.fd"
chmod 600 "$STAGE/disk.raw" "$STAGE/vars.fd" "$INJECTOR" "$INJECTOR.assets-sha256"
BRIDGEVM_BOOT_PROGRESS_KILL=1 "$REPO/scripts/run-hvf-windows-installed-boot.sh" \
  --target "$STAGE/disk.raw" --vars "$STAGE/vars.fd" --placeholder-nsid1 "$INJECTOR" \
  --evidence-dir "$OUT/run" --watchdog-ms 600000 --ram-mib 6144 --smp-cpus 4 \
  --max-reboots 0 --release --virtio-gpu-3d --gpu-trace "$OUT/run/virtio-gpu.jsonl" \
  --gpu-trace-protocol venus --viogpu3d-dir "$VIOGPU_DIR" >"$OUT/launcher.out" 2>&1
observed=$(awk -F= '$1=="injector_boot_observed"{print $2}' "$OUT/run/target-stat.txt" 2>/dev/null)
[[ "$observed" == true ]] || fail "sealed B4 injector boot was not observed: ${observed:-absent}"
cp "$SOURCE_VARS" "$STAGE/vars.fd"; chmod 600 "$STAGE/vars.fd"
[[ "$(seal "$STAGE/vars.fd")" == "$(seal "$SOURCE_VARS")" ]] || fail 'sealed target vars restore failed'
[[ "$($PACKAGE_TOOL verify --manifest "$VIOGPU_MANIFEST" --dir "$VIOGPU_DIR")" == "$VIOGPU_PACKAGE_SHA256" ]] || fail 'sealed package changed during injection'
printf 'injector_boot_observed=true\ninjector_sha256=%s\ninjector_assets_sha256=%s\ninjector_vars_sha256=%s\ninjector_iso_sha256=%s\nsealed_package_sha256=%s\n' \
  "$injector_hash" "$assets_hash" "$injector_vars_hash" "$injector_iso_hash" "$VIOGPU_PACKAGE_SHA256" > "$OUT/injection.env"
