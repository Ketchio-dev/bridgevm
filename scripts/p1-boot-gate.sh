#!/usr/bin/env bash
# Boot-reliability gate: N cold boots, count how many reach a *fresh* stage4.
#
# Each iteration is a fresh APFS clone plus the injector, so every boot runs the
# four-stage firstboot from stage1 rather than inheriting a finished image. That
# matters: four earlier runs once shared a byte-identical inherited firstboot
# log, so a plain marker check reported a pass for runs that had never run
# firstboot at all.
#
# Two rules this script exists to enforce:
#   1. A stage4 marker only counts with freshness. `stage4_pass=1` alone is not
#      evidence; the log must also have changed during the run
#      (`firstboot_fresh=1`).
#   2. Aggregation never globs. Results are summarised from the run directories
#      this invocation created, listed explicitly, so a stale
#      `p1gate-*` directory from a previous gate cannot inflate the count.
set -uo pipefail

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO"

BOOTS=10
OUT=""
BASE_IMAGE=${BASE_IMAGE:-$HOME/BridgeVM/work/wall-c8-clean-12041.raw}
BASE_VARS=${BASE_VARS:-$HOME/BridgeVM/work/wall-c8-clean-inject-vars.fd}
INJECTOR=${INJECTOR:-/tmp/inj-det-1.raw}
VIOGPU_DIR=${VIOGPU_DIR:-$HOME/BridgeVM/work/download-120.45-backing-only}
# Pass 2 must outlast a full four-stage firstboot, which needs >= 2400 s.
# Kill mode does not make that ceiling cheap, contrary to what this comment
# used to claim: measured end to end, a stalled boot takes 57 min with kill
# mode and 50 min without. The stall happens after all four stages and four
# reboots, so there is little left to cut. Budget ~45-60 min per boot.
PASS2_WATCHDOG_MS=${PASS2_WATCHDOG_MS:-2400000}
EXTRA_ARGS=()
# The stage4 hang ends with the guest never notifying the control queue again,
# so proving whether the completion interrupt was raised needs the venus-start
# MSI-X trace. Set from the option loop directly: --trace-venus-start is not
# an EXTRA_ARGS entry, and reading it from that array silently never matched.
trace_venus_start=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --boots) BOOTS="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --no-3d) EXTRA_ARGS+=("--no-3d"); shift ;;
    --trace-venus-start) trace_venus_start=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[[ -n "$OUT" ]] || OUT="$HOME/BridgeVM/runs/p1gate-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$OUT"

for required in "$BASE_IMAGE" "$BASE_VARS" "$INJECTOR"; do
  [[ -e "$required" ]] || { echo "FAIL: missing required input: $required" >&2; exit 1; }
done

# The injector carries the guest-side scripts, so an injector older than
# scripts/win-assets means the gate silently measures stale code. That is not
# hypothetical: a 10-boot gate once ran entirely against an injector built 23
# hours before the vkCreateInstance timeout landed, and every stall it found
# was a bug that had already been fixed in the repository.
newest_asset=$(find scripts/win-assets -type f -newer "$INJECTOR" -print -quit 2>/dev/null)
if [[ -n "$newest_asset" ]]; then
  echo "FAIL: injector is older than scripts/win-assets" >&2
  echo "  injector: $INJECTOR ($(stat -f '%Sm' "$INJECTOR"))" >&2
  echo "  newer:    $newest_asset ($(stat -f '%Sm' "$newest_asset"))" >&2
  echo "  rebuild:  VIOGPU3D_DIR=... OUT=$INJECTOR scripts/build-hvf-windows-viogpu3d-injector.sh" >&2
  echo "  override: STALE_INJECTOR_OK=1 (only when the change cannot affect the guest)" >&2
  [[ "${STALE_INJECTOR_OK:-0}" == "1" ]] || exit 1
  echo "  continuing anyway: STALE_INJECTOR_OK=1" >&2
fi

# Explicit list of this invocation's run directories. Aggregation reads only
# these, never a wildcard.
MANIFEST="$OUT/run-directories.txt"
: > "$MANIFEST"
PROGRESS="$OUT/progress.txt"
: > "$PROGRESS"

use_3d=1
for arg in ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}; do
  [[ "$arg" == "--no-3d" ]] && use_3d=0
done

# bash 3.2 (macOS system bash) has no mapfile, so build the array directly.
set_gpu_args() {
  GPU_ARGS=()
  [[ "$use_3d" == 1 ]] || return 0
  GPU_ARGS=(--virtio-gpu-3d --gpu-trace "$1/virtio-gpu.jsonl"
            --gpu-trace-protocol venus --viogpu3d-dir "$VIOGPU_DIR")
  if [[ "${trace_venus_start:-0}" == 1 ]]; then
    GPU_ARGS+=(--trace-venus-start)
  fi
}

for i in $(seq 1 "$BOOTS"); do
  W="$HOME/BridgeVM/work/p1gate-work-$i"
  D="$OUT/boot-$i"
  rm -rf "$W" "$D" "$D-inject"
  mkdir -p "$W" "$D" "$D-inject"

  # cp -c: APFS clone, so the source lineage is never written to.
  cp -c "$BASE_IMAGE" "$W/disk.raw"
  cp "$BASE_VARS" "$W/vars.fd"
  cp "$INJECTOR" "$W/inj.raw"

  # Pass 1: the WinPE injector plants the four-stage script, clears
  # stage1/2/3.flag, and re-arms RunOnce.
  set_gpu_args "$D-inject"
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$W/disk.raw" --vars "$W/vars.fd" --placeholder-nsid1 "$W/inj.raw" \
    --evidence-dir "$D-inject" --watchdog-ms 600000 --ram-mib 6144 --smp-cpus 4 \
    "${GPU_ARGS[@]}" > "$D-inject/launcher.out" 2>&1

  injected=$(grep -h '^injector_boot_observed=' "$D-inject/target-stat.txt" 2>/dev/null | cut -d= -f2)

  # Pass 2: boot installed Windows. firstboot runs stage1..stage4 across its own
  # internal reboots, which the probe follows within one invocation.
  set_gpu_args "$D"
  BRIDGEVM_BOOT_PROGRESS_KILL=1 \
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$W/disk.raw" --vars "$W/vars.fd" \
    --evidence-dir "$D" --watchdog-ms "$PASS2_WATCHDOG_MS" --ram-mib 6144 --smp-cpus 4 \
    "${GPU_ARGS[@]}" > "$D/launcher.out" 2>&1

  echo "$D" >> "$MANIFEST"

  stage4=$(grep -h '^stage4_pass=' "$D/firstboot-stage.txt" 2>/dev/null | cut -d= -f2)
  fresh=$(grep -h '^firstboot_fresh=' "$D/firstboot-stage.txt" 2>/dev/null | cut -d= -f2)
  last=$(grep -h '^last_stage_observed=' "$D/firstboot-stage.txt" 2>/dev/null | cut -d= -f2)
  stall=$(grep -c 'boot-progress watchdog stalled_for_ms' "$D/run.log" 2>/dev/null | tr -d '\n ' || echo 0)
  reboots=$(grep -c 'SYSTEM_RESET: reboot' "$D/run.log" 2>/dev/null | tr -d '\n ' || echo 0)

  printf 'boot %-3s injected=%-5s stage4_pass=%-3s fresh=%-3s last_stage=%-8s stalls=%-3s reboots=%s\n' \
    "$i" "${injected:-?}" "${stage4:-?}" "${fresh:-?}" "${last:-none}" "$stall" "$reboots" \
    | tee -a "$PROGRESS"

  rm -rf "$W"
done

# A boot counts only when the marker is present AND the log changed this run.
pass=0
total=0
while IFS= read -r dir; do
  total=$((total + 1))
  s=$(grep -h '^stage4_pass=' "$dir/firstboot-stage.txt" 2>/dev/null | cut -d= -f2)
  f=$(grep -h '^firstboot_fresh=' "$dir/firstboot-stage.txt" 2>/dev/null | cut -d= -f2)
  [[ "$s" == "1" && "$f" == "1" ]] && pass=$((pass + 1))
done < "$MANIFEST"

SUMMARY="$OUT/summary.txt"
{
  echo "gate_dir=$OUT"
  echo "boots=$total"
  echo "pass=$pass"
  echo "rule=stage4_pass==1 AND firstboot_fresh==1"
} | tee "$SUMMARY"

# Non-zero exit when the release bar is missed, so callers can branch on it.
[[ "$pass" -ge $(( (total * 9 + 9) / 10 )) ]]
