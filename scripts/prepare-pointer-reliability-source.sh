#!/usr/bin/env bash
# Complete driver firstboot once, then retain a hash-named immutable B4 source.
set -euo pipefail
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd); cd "$REPO"
OUT=${OUT:?}; SOURCE=${SOURCE:?}; SOURCE_VARS=${SOURCE_VARS:?}; VIOGPU_DIR=${VIOGPU_DIR:?}; JOB_ID=${JOB_ID:?}
VIOGPU_MANIFEST=${VIOGPU_MANIFEST:?}; VIOGPU_PACKAGE_SHA256=${VIOGPU_PACKAGE_SHA256:?}; VIOGPU_UMD_SHA256=${VIOGPU_UMD_SHA256:?}; VIOGPU_INF_SHA256=${VIOGPU_INF_SHA256:?}
ROOT=${POINTER_PREPARED_ROOT:-$HOME/BridgeVM/prepared/pointer-reliability}; WORK=$HOME/BridgeVM/work/pointer-source-$JOB_ID; STAGE=$WORK/stage; PACKAGE_TOOL=$REPO/scripts/live-gates/b4-diagnostic-package.py
source "$REPO/scripts/live-gates/pointer-prepared-cache.sh"
seal() { openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'; }
fail() { echo "FAIL: $*" >&2; exit 1; }
[[ "$($PACKAGE_TOOL verify --manifest "$VIOGPU_MANIFEST" --dir "$VIOGPU_DIR")" == "$VIOGPU_PACKAGE_SHA256" ]] || fail 'sealed B4 package failed initial verification'
[[ -f "$SOURCE" && ! -L "$SOURCE" && ! -w "$SOURCE" && -f "$SOURCE_VARS" && ! -L "$SOURCE_VARS" && ! -w "$SOURCE_VARS" ]] || fail 'source media must be immutable regular files'
source_image=$(seal "$SOURCE"); source_vars=$(seal "$SOURCE_VARS")
[[ "$(basename "$(dirname "$SOURCE")")" == "$source_image-$source_vars" ]] || fail 'source hash identity mismatch'
mkdir -p "$ROOT" "$OUT"
found=''
for meta in "$ROOT"/*/retained.env; do
  [[ -f "$meta" ]] || continue
  pointer_cache_matches "$meta" "$source_image" "$source_vars" "$VIOGPU_PACKAGE_SHA256" "$VIOGPU_UMD_SHA256" "$VIOGPU_INF_SHA256" || continue
  dir=$(dirname "$meta"); image=$(pointer_metadata_value "$meta" image_sha256); vars=$(pointer_metadata_value "$meta" vars_sha256)
  [[ $(basename "$dir") == "$image-$vars" && ! -w "$dir/disk.raw" && ! -w "$dir/vars.fd" && $(seal "$dir/disk.raw") == "$image" && $(seal "$dir/vars.fd") == "$vars" ]] || fail 'cached B4 source failed verification'
  [[ -z "$found" ]] || fail 'multiple verified B4 prepared sources'
  found=$dir; found_image=$image; found_vars=$vars
done
if [[ -n "$found" ]]; then pointer_write_result "$found" "$found_image" "$found_vars" "$OUT" "$VIOGPU_PACKAGE_SHA256" "$VIOGPU_UMD_SHA256" "$VIOGPU_INF_SHA256"; exit 0; fi
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
  local pattern="$1" need="$2" limit="$3" observed; local deadline=$((SECONDS + limit))
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
verify_cmd="powershell -NoProfile -Command \"\$dev=Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue | Where-Object { \$_.InstanceId -match '^PCI\\\\VEN_1AF4&DEV_(1050|10F7)(?:&|\$)' -and \$_.Status -eq 'OK' } | Select-Object -First 1; \$drv=Get-CimInstance Win32_PnPSignedDriver | Where-Object { \$_.DeviceID -eq \$dev.InstanceId } | Select-Object -First 1; if(-not \$dev -or -not \$drv -or \$drv.DriverVersion -cne '120.50.0.0'){exit 4}; \$bound=Join-Path \$env:windir ('INF\\' + \$drv.InfName); if((Get-FileHash -Algorithm SHA256 -LiteralPath \$bound).Hash.ToLowerInvariant() -cne '$VIOGPU_INF_SHA256'){exit 5}; \$matches=@(Get-ChildItem -LiteralPath (Join-Path \$env:windir 'System32\\DriverStore\\FileRepository') -Filter viogpu_d3d10.dll -File -Recurse | Where-Object { (Get-FileHash -Algorithm SHA256 -LiteralPath \$_.FullName).Hash.ToLowerInvariant() -ceq '$VIOGPU_UMD_SHA256' }); if(\$matches.Count -lt 1){exit 6}; Write-Output 'BVPACKAGE_VERIFIED version=120.50.0.0'; exit 0\""
send "$verify_cmd" && grep -aq '^BVPACKAGE_VERIFIED version=120.50.0.0\r*$' "$LOG" || fail 'installed B4 package identity mismatch'
printf '%s\n' 'shutdown /s /t 3' >> "$CTL"
for _ in $(seq 1 300); do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
kill -0 "$pid" 2>/dev/null && fail 'B4 source shutdown timeout'
wait "$pid" || fail 'B4 source launcher failed'; pid=''
[[ $(seal "$SOURCE") == "$source_image" && $(seal "$SOURCE_VARS") == "$source_vars" ]] || fail 'immutable source changed'
[[ "$($PACKAGE_TOOL verify --manifest "$VIOGPU_MANIFEST" --dir "$VIOGPU_DIR")" == "$VIOGPU_PACKAGE_SHA256" ]] || fail 'sealed B4 package changed during firstboot'
image=$(seal "$STAGE/disk.raw"); vars=$(seal "$STAGE/vars.fd"); retained=$ROOT/$image-$vars
[[ ! -e "$retained" ]] || fail 'prepared B4 identity already exists'
{
  echo firstboot_ready=true; echo "image_sha256=$image"; echo "vars_sha256=$vars"
  echo "source_image_sha256=$source_image"; echo "source_vars_sha256=$source_vars"
  echo installed_package_verified=true; echo driver_version=120.50.0.0
  echo "viogpu_package_sha256=$VIOGPU_PACKAGE_SHA256"; echo "installed_umd_sha256=$VIOGPU_UMD_SHA256"; echo "installed_inf_sha256=$VIOGPU_INF_SHA256"
} > "$STAGE/retained.env"
chmod 400 "$STAGE/disk.raw" "$STAGE/vars.fd" "$STAGE/retained.env"; mv "$STAGE" "$retained"
pointer_write_result "$retained" "$image" "$vars" "$OUT" "$VIOGPU_PACKAGE_SHA256" "$VIOGPU_UMD_SHA256" "$VIOGPU_INF_SHA256"
