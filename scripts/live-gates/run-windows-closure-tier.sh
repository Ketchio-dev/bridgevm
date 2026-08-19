#!/usr/bin/env bash
# Exact-input F1-F4 closure: inject into a clone, retain it, prove on another clone.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT=""; INPUT_MANIFEST=""; SEALED_BINARY=""; JOB_ID=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --input-manifest) INPUT_MANIFEST="$2"; shift 2 ;;
    --sealed-binary) SEALED_BINARY="$2"; shift 2 ;;
    --job-id) JOB_ID="$2"; shift 2 ;;
    *) echo "unknown Windows closure tier option: $1" >&2; exit 2 ;;
  esac
done
[[ -n "$OUT" && -n "$INPUT_MANIFEST" && -n "$SEALED_BINARY" && -n "$JOB_ID" ]] || exit 2
mkdir -p "$OUT"

seal() {
  [[ -f "$1" && ! -L "$1" ]] || { printf absent; return; }
  openssl dgst -sha256 -r "$1" | cut -d' ' -f1 | tr -d '\n'
}
source "$REPO/scripts/live-gates/windows-closure-manifest.sh"
RECEIPT="$REPO/scripts/live-gates/write-windows-closure-receipt.py"
COMMIT="$(git -C "$REPO" rev-parse HEAD)"
MANIFEST_HASH="$(seal "$INPUT_MANIFEST")"
write_receipt() {
  local reason="${1:-}"
  local args=(--out "$OUT" --job-id "$JOB_ID" --commit "$COMMIT" \
    --input-manifest-hash "$MANIFEST_HASH")
  [[ -z "$reason" ]] || args+=(--reason "$reason")
  python3 "$RECEIPT" "${args[@]}"
}
refuse() { echo "FAIL: $1" >&2; write_receipt "$2" >/dev/null 2>&1 || true; exit 1; }

verify_closure_manifest || refuse 'invalid or changed Windows closure input manifest' refused-input
"$REPO/scripts/live-gates/verify-windows-closure-binary.sh" "$SEALED_BINARY" \
  "$(closure_manifest_path virglrenderer)" || refuse 'sealed probe policy failed' refused-binary

IMAGE="$(closure_manifest_path image)"
VARS="$(closure_manifest_path vars)"
INJECTOR="$(closure_manifest_path injector)"
VIOGPU_DIR="$(closure_manifest_path viogpu_dir)"
MOLTENVK="$(closure_manifest_path moltenvk)"
SOURCE_IMAGE_HASH="$(seal "$IMAGE")"; SOURCE_VARS_HASH="$(seal "$VARS")"
SOURCE_INJECTOR_HASH="$(seal "$INJECTOR")"

WORK="$HOME/BridgeVM/work/windows-closure-$JOB_ID"
PREPARED_ROOT="$HOME/BridgeVM/prepared/windows-1.0"
STAGE="$WORK/prepared-stage"
PROOF_WORK="$WORK/proof"
cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT
rm -rf "$WORK"; mkdir -p "$STAGE" "$PROOF_WORK" "$OUT/injection" "$OUT/prepared"

cp -c "$IMAGE" "$STAGE/disk.raw"; cp "$VARS" "$STAGE/vars.fd"; cp -c "$INJECTOR" "$STAGE/injector.raw"
[[ "$(seal "$STAGE/disk.raw")" == "$SOURCE_IMAGE_HASH" && "$(seal "$STAGE/vars.fd")" == "$SOURCE_VARS_HASH" \
  && "$(seal "$STAGE/injector.raw")" == "$SOURCE_INJECTOR_HASH" ]] || refuse 'initial clones differ from sealed inputs' clone-hash-mismatch
BRIDGEVM_PREBUILT_PROBE="$SEALED_BINARY" BRIDGEVM_VULKAN_LIB="$MOLTENVK" \
BRIDGEVM_BOOT_PROGRESS_KILL=1 \
"$REPO/scripts/run-hvf-windows-installed-boot.sh" \
  --target "$STAGE/disk.raw" --vars "$STAGE/vars.fd" \
  --placeholder-nsid1 "$STAGE/injector.raw" --evidence-dir "$OUT/injection" \
  --watchdog-ms 600000 --ram-mib 6144 --smp-cpus 4 --max-reboots 8 \
  --skip-build --release --virtio-gpu-3d \
  --gpu-trace "$OUT/injection/virtio-gpu.jsonl" --gpu-trace-protocol venus \
  --viogpu3d-dir "$VIOGPU_DIR" > "$OUT/injection/launcher.out" 2>&1

[[ "$(awk -F= '$1 == "injector_boot_observed" { print $2 }' "$OUT/injection/target-stat.txt")" == true ]] \
  || refuse 'injector did not boot from the sealed clone' injection-not-observed
[[ "$(awk -F= '$1 == "agent_sha256" { print $2 }' "$OUT/injection/guest-logs/bvagent-package.log")" == "$(awk -F '\t' '$1 == "agent" { print $3 }' "$INPUT_MANIFEST")" ]] || refuse 'injected agent identity does not match the sealed source' injection-agent-mismatch
[[ "$(seal "$IMAGE")" == "$SOURCE_IMAGE_HASH" && "$(seal "$VARS")" == "$SOURCE_VARS_HASH" \
  && "$(seal "$INJECTOR")" == "$SOURCE_INJECTOR_HASH" ]] \
  || refuse 'source media changed during injection' source-media-changed

python3 "$REPO/scripts/drop-injector-boot-entry.py" "$STAGE/vars.fd" \
  >> "$OUT/injection/launcher.out" 2>&1
PREPARED_IMAGE_HASH="$(seal "$STAGE/disk.raw")"
PREPARED_VARS_HASH="$(seal "$STAGE/vars.fd")"
RETAINED="$PREPARED_ROOT/$PREPARED_IMAGE_HASH-$PREPARED_VARS_HASH"
if [[ -e "$RETAINED" ]]; then
  refuse 'prepared identity already exists; refusing overwrite' prepared-identity-exists
fi
mkdir -p "$PREPARED_ROOT"
mv "$STAGE" "$RETAINED.staging-$JOB_ID"
rm -f "$RETAINED.staging-$JOB_ID/injector.raw"
mv "$RETAINED.staging-$JOB_ID" "$RETAINED"
{
  echo 'retained=true'
  echo "image_sha256=$PREPARED_IMAGE_HASH"
  echo "vars_sha256=$PREPARED_VARS_HASH"
  echo "source_image_sha256=$SOURCE_IMAGE_HASH"
  echo "source_vars_sha256=$SOURCE_VARS_HASH"
  echo "source_injector_sha256=$SOURCE_INJECTOR_HASH"
} > "$OUT/prepared/retained.env"
cp "$OUT/prepared/retained.env" "$RETAINED/retained.env"
chmod 400 "$RETAINED/disk.raw" "$RETAINED/vars.fd" "$RETAINED/retained.env"

cp -c "$RETAINED/disk.raw" "$PROOF_WORK/disk.raw"
cp "$RETAINED/vars.fd" "$PROOF_WORK/vars.fd"
chmod 600 "$PROOF_WORK/disk.raw" "$PROOF_WORK/vars.fd"
set +e
"$REPO/scripts/windows-1.0-closure-interact.sh" \
  --out "$OUT/proof" --target "$PROOF_WORK/disk.raw" --vars "$PROOF_WORK/vars.fd" \
  --binary "$SEALED_BINARY" --viogpu-dir "$VIOGPU_DIR" --moltenvk "$MOLTENVK"
PROOF_STATUS=$?; set -e
[[ "$(seal "$IMAGE")" == "$SOURCE_IMAGE_HASH" && "$(seal "$VARS")" == "$SOURCE_VARS_HASH" \
  && "$(seal "$INJECTOR")" == "$SOURCE_INJECTOR_HASH" ]] || refuse 'source media changed during proof' source-media-changed
[[ "$(seal "$RETAINED/disk.raw")" == "$PREPARED_IMAGE_HASH" \
  && "$(seal "$RETAINED/vars.fd")" == "$PREPARED_VARS_HASH" ]] \
  || refuse 'immutable prepared pair changed during proof' prepared-media-changed
write_receipt || true; (( PROOF_STATUS == 0 )) && python3 - "$OUT/receipt.json" <<'PY'
import json, sys
assert json.load(open(sys.argv[1]))["pass"] is True
PY
exit "$PROOF_STATUS"
