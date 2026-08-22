#!/usr/bin/env bash
# Complete driver firstboot once, then retain a hash-named immutable B4 source.
set -euo pipefail
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd); cd "$REPO"
OUT=${OUT:?}; SOURCE=${SOURCE:?}; SOURCE_VARS=${SOURCE_VARS:?}; VIOGPU_DIR=${VIOGPU_DIR:?}; JOB_ID=${JOB_ID:?}
ROOT=${POINTER_PREPARED_ROOT:-$HOME/BridgeVM/prepared/pointer-reliability}
WORK=$HOME/BridgeVM/work/pointer-source-$JOB_ID; STAGE=$WORK/stage
seal() { openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'; }
fail() { echo "FAIL: $*" >&2; exit 1; }
value() { awk -F= -v key="$2" '$1==key{print substr($0,index($0,"=")+1)}' "$1"; }
write_result() {
  local dir="$1" image="$2" vars="$3"
  printf 'target=%s\nvars=%s\nimage_sha256=%s\nvars_sha256=%s\n' \
    "$dir/disk.raw" "$dir/vars.fd" "$image" "$vars" > "$OUT/source.env"
}
[[ -f "$SOURCE" && ! -L "$SOURCE" && ! -w "$SOURCE" && -f "$SOURCE_VARS" && ! -L "$SOURCE_VARS" && ! -w "$SOURCE_VARS" ]] || fail 'source media must be immutable regular files'
source_image=$(seal "$SOURCE"); source_vars=$(seal "$SOURCE_VARS")
[[ "$(basename "$(dirname "$SOURCE")")" == "$source_image-$source_vars" ]] || fail 'source hash identity mismatch'
mkdir -p "$ROOT" "$OUT"
found=''
for meta in "$ROOT"/*/retained.env; do
  [[ -f "$meta" ]] || continue
  [[ $(value "$meta" source_image_sha256) == "$source_image" && $(value "$meta" source_vars_sha256) == "$source_vars" ]] || continue
  dir=$(dirname "$meta"); image=$(value "$meta" image_sha256); vars=$(value "$meta" vars_sha256)
  [[ $(value "$meta" firstboot_ready) == true && $(basename "$dir") == "$image-$vars" \
    && ! -w "$dir/disk.raw" && ! -w "$dir/vars.fd" && $(seal "$dir/disk.raw") == "$image" && $(seal "$dir/vars.fd") == "$vars" ]] || fail 'cached B4 source failed verification'
  [[ -z "$found" ]] || fail 'multiple verified B4 prepared sources'
  found=$dir; found_image=$image; found_vars=$vars
done
if [[ -n "$found" ]]; then write_result "$found" "$found_image" "$found_vars"; exit 0; fi
rm -rf "$WORK"; mkdir -p "$STAGE" "$OUT/preparation/share"
cp -c "$SOURCE" "$STAGE/disk.raw"; cp "$SOURCE_VARS" "$STAGE/vars.fd"; chmod 600 "$STAGE"/*
CTL=$OUT/preparation/agent.ctl; : > "$CTL"; LOG=$OUT/preparation/run.log
pid=''
cleanup() {
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then printf '%s\n' 'shutdown /s /t 3' >> "$CTL"; sleep 5; kill "$pid" 2>/dev/null || true; fi
  [[ -z "$pid" ]] || wait "$pid" 2>/dev/null || true; rm -rf "$WORK"
}
trap cleanup EXIT
wait_for() {
  local pattern="$1" need="$2" limit="$3" deadline=$((SECONDS + limit)) observed
  while (( SECONDS < deadline )); do
    observed=$(grep -acE "$pattern" "$LOG" 2>/dev/null || true)
    [[ "$observed" =~ ^[0-9]+$ ]] && (( observed >= need )) && return 0
    kill -0 "$pid" 2>/dev/null || return 1; sleep 1
  done
  return 1
}
send() {
  local command="$1" before; before=$(grep -ac '^BVAGENT END ' "$LOG" 2>/dev/null || true)
  printf '%s\n' "$command" >> "$CTL"; wait_for '^BVAGENT END ' $((before + 1)) 180
  [[ $(grep -aE '^BVAGENT CMD .* exit=' "$LOG" | tail -1) == *' exit=0' ]]
}
BRIDGEVM_BOOT_PROGRESS_KILL=1 scripts/run-hvf-windows-installed-boot.sh \
  --target "$STAGE/disk.raw" --vars "$STAGE/vars.fd" --evidence-dir "$OUT/preparation" \
  --watchdog-ms 720000 --ram-mib 6144 --smp-cpus 4 --max-reboots 8 --release --enable-xhci \
  --agent-service-control "$CTL" --agent-share-host "$OUT/preparation/share" \
  --agent-share-guest 'C:\BridgeVMPtr' --agent-share-ms 500 --virtio-gpu-3d \
  --gpu-trace "$OUT/preparation/virtio-gpu.jsonl" --gpu-trace-protocol venus \
  --viogpu3d-dir "$VIOGPU_DIR" > "$OUT/preparation/launcher.out" 2>&1 &
pid=$!
wait_for '^BVAGENT SERVICE start' 1 1200 || fail 'B4 source agent timeout'
ready_cmd='powershell -NoProfile -Command "& schtasks.exe /Query /TN BridgeVM-VioGpu3DFirstBoot *> $null; $task=($LASTEXITCODE -eq 0); $ready=(Test-Path C:\BridgeVM\stage3.flag) -and (-not $task); if($ready){Write-Output BVFIRSTBOOT_READY; exit 0}; Write-Output BVFIRSTBOOT_PENDING; exit 3"'
deadline=$((SECONDS + 2700)); ready=false
while (( SECONDS < deadline )); do
  if send "$ready_cmd" && grep -aq '^BVFIRSTBOOT_READY\r*$' "$LOG"; then ready=true; break; fi
  sleep 5
done
[[ "$ready" == true ]] || fail 'B4 source firstboot readiness timeout'
printf '%s\n' 'shutdown /s /t 3' >> "$CTL"
for _ in $(seq 1 300); do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
kill -0 "$pid" 2>/dev/null && fail 'B4 source shutdown timeout'
wait "$pid" || fail 'B4 source launcher failed'; pid=''
[[ $(seal "$SOURCE") == "$source_image" && $(seal "$SOURCE_VARS") == "$source_vars" ]] || fail 'immutable source changed'
image=$(seal "$STAGE/disk.raw"); vars=$(seal "$STAGE/vars.fd"); retained=$ROOT/$image-$vars
[[ ! -e "$retained" ]] || fail 'prepared B4 identity already exists'
{
  echo firstboot_ready=true; echo "image_sha256=$image"; echo "vars_sha256=$vars"
  echo "source_image_sha256=$source_image"; echo "source_vars_sha256=$source_vars"
} > "$STAGE/retained.env"
chmod 400 "$STAGE/disk.raw" "$STAGE/vars.fd" "$STAGE/retained.env"; mv "$STAGE" "$retained"
write_result "$retained" "$image" "$vars"
