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
# Not /tmp: the injector is a gate input whose hash keys the prepared cache,
# and /tmp is cleared on reboot. Canonical inputs live on the external volume.
INJECTOR=${INJECTOR:-/Volumes/PortableSSD/BridgeVM/injectors/inj-a1-20260802.raw}
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
# Compared by content, not mtime: a fresh `git worktree` (which is how the
# live queue builds a sealed checkout) stamps every file with the checkout
# time, so an mtime test reports every asset as newer than any injector and
# the gate can never run there. The injector records the asset digest it was
# built from.
# Hashes the repo-relative name and the content of each file, never the
# absolute path: a sealed worktree lives at a different path and must produce
# the same digest as the checkout the injector was built from.
# LC_ALL=C: sort order is locale-dependent, and the LaunchAgent that runs
# queued gates has no locale while an interactive shell may have any. Without
# this the same tree hashes differently depending on who ran the gate.
assets_digest=$(cd scripts/win-assets && find . -type f -exec shasum -a 256 {} + \
  | LC_ALL=C sort -k2 | shasum -a 256 | cut -d' ' -f1)
built_from=$(cat "$INJECTOR.assets-sha256" 2>/dev/null || true)
{ echo "DIAG stat: $(stat -f '%z %Sp' "$INJECTOR.assets-sha256" 2>&1)"
  echo "DIAG cat stderr: $(cat "$INJECTOR.assets-sha256" 2>&1 1>/dev/null)"
  echo "DIAG cat stdout: [$(cat "$INJECTOR.assets-sha256" 2>/dev/null)]"
  echo "DIAG id: $(id -un) euid=$(id -u)"; } >&2
if [[ "$assets_digest" != "$built_from" ]]; then
  echo "FAIL: injector was not built from the current scripts/win-assets" >&2
  echo "  injector:    $INJECTOR" >&2
  echo "  built from:  ${built_from:-<unrecorded>}" >&2
  echo "  assets now:  $assets_digest" >&2
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

LANES="${LANES:-1}"

# When BASE_IMAGE lives on another volume (external SSD), cp -c cannot clone
# across volumes and would do a full copy per boot. Stage one internal cache
# copy up front; each boot then clones from the cache for free.
CACHE_DIR="$HOME/BridgeVM/work/p1gate-cache"
stage_base_cache() {
  local src="$1" dst="$2"
  if [[ "$(stat -f '%d' "$src")" == "$(stat -f '%d' "$HOME/BridgeVM/work")" ]]; then
    printf '%s' "$src"
    return 0
  fi
  mkdir -p "$CACHE_DIR"
  local want have
  want="$(stat -f '%m-%z' "$src")"
  have="$(cat "$dst.stamp" 2>/dev/null || true)"
  if [[ "$want" != "$have" || ! -e "$dst" ]]; then
    dd if="$src" of="$dst" bs=1m conv=sparse 2>/dev/null
    printf '%s' "$want" > "$dst.stamp"
  fi
  printf '%s' "$dst"
}
BASE_IMAGE="$(stage_base_cache "$BASE_IMAGE" "$CACHE_DIR/base-image.raw")"
INJECTOR="$(stage_base_cache "$INJECTOR" "$CACHE_DIR/injector.raw")"

# One build+sign for the whole gate. Lanes must not race cargo/codesign on the
# shared probe binary, so they all run with --skip-build.
cargo build -p bridgevm-hvf --features venus --example hvf_gic_boot_probe
codesign --sign - --entitlements apps/macos/HvfRunner.entitlements --force \
  target/debug/examples/hvf_gic_boot_probe

run_one_boot() {
  local i="$1"
  local W="$HOME/BridgeVM/work/p1gate-work-$i"
  local D="$OUT/boot-$i"
  rm -rf "$W" "$D"
  mkdir -p "$W" "$D"

  # Clone the prepared (already injected) pair. cp -c is an APFS clone when
  # source and destination share a volume, so this costs no real bytes.
  cp -c "$PREPARED_IMAGE" "$W/disk.raw"
  cp "$PREPARED_VARS" "$W/vars.fd"

  local injected
  injected=$(cat "$PREPARED_DIR/injected.txt" 2>/dev/null)

  # Boot installed Windows. firstboot runs stage1..stage4 across its own
  # internal reboots, which the probe follows within one invocation.
  set_gpu_args "$D"
  BRIDGEVM_SMP_TRACE="${BRIDGEVM_SMP_TRACE:-}" \
  BRIDGEVM_BOOT_PROGRESS_KILL=1 \
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$W/disk.raw" --vars "$W/vars.fd" \
    --evidence-dir "$D" --watchdog-ms "$PASS2_WATCHDOG_MS" --ram-mib 6144 --smp-cpus 4 \
    --skip-build \
    "${GPU_ARGS[@]}" > "$D/launcher.out" 2>&1

  echo "$D" >> "$MANIFEST"

  local stage4 fresh last stall reboots
  stage4=$(grep -h '^stage4_pass=' "$D/firstboot-stage.txt" 2>/dev/null | cut -d= -f2)
  fresh=$(grep -h '^firstboot_fresh=' "$D/firstboot-stage.txt" 2>/dev/null | cut -d= -f2)
  last=$(grep -h '^last_stage_observed=' "$D/firstboot-stage.txt" 2>/dev/null | cut -d= -f2)
  stall=$(grep -c 'boot-progress watchdog stalled_for_ms' "$D/run.log" 2>/dev/null | tr -d '\n ' || echo 0)
  reboots=$(grep -c 'SYSTEM_RESET: reboot' "$D/run.log" 2>/dev/null | tr -d '\n ' || echo 0)

  printf 'boot %-3s injected=%-5s stage4_pass=%-3s fresh=%-3s last_stage=%-8s stalls=%-3s reboots=%s\n' \
    "$i" "${injected:-?}" "${stage4:-?}" "${fresh:-?}" "${last:-none}" "$stall" "$reboots" \
    | tee -a "$PROGRESS"

  rm -rf "$W"
}

# One injector pass for the whole gate, keyed by the hashes of everything that
# can change what it plants. Each boot then clones the prepared pair, so a
# 10-boot gate runs 10 boots rather than 20. Correctness rests on the key: if
# the base image, vars, injector or any guest asset changes, the key changes
# and the pass is redone.
prepared_key() {
  {
    stat -f '%d-%i-%m-%z' "$BASE_IMAGE" "$BASE_VARS" "$INJECTOR"
    find scripts/win-assets -type f -exec stat -f '%N-%m-%z' {} + | sort
  } | shasum -a 256 | cut -d' ' -f1
}

PREPARED_KEY="$(prepared_key)"
PREPARED_DIR="$CACHE_DIR/prepared-$PREPARED_KEY"
PREPARED_IMAGE="$PREPARED_DIR/disk.raw"
PREPARED_VARS="$PREPARED_DIR/vars.fd"

if [[ -e "$PREPARED_DIR/ready" ]]; then
  echo "prepared cache hit: $PREPARED_DIR" | tee -a "$PROGRESS"
else
  echo "prepared cache miss: running one injector pass" | tee -a "$PROGRESS"
  rm -rf "$PREPARED_DIR"
  mkdir -p "$PREPARED_DIR"
  cp -c "$BASE_IMAGE" "$PREPARED_IMAGE"
  cp "$BASE_VARS" "$PREPARED_VARS"
  cp "$INJECTOR" "$PREPARED_DIR/inj.raw"

  # The WinPE injector plants the four-stage script, clears stage1/2/3.flag,
  # and re-arms RunOnce.
  set_gpu_args "$PREPARED_DIR"
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$PREPARED_IMAGE" --vars "$PREPARED_VARS" \
    --placeholder-nsid1 "$PREPARED_DIR/inj.raw" \
    --evidence-dir "$PREPARED_DIR" --watchdog-ms 600000 --ram-mib 6144 --smp-cpus 4 \
    --skip-build \
    "${GPU_ARGS[@]}" > "$PREPARED_DIR/launcher.out" 2>&1

  grep -h '^injector_boot_observed=' "$PREPARED_DIR/target-stat.txt" 2>/dev/null \
    | cut -d= -f2 > "$PREPARED_DIR/injected.txt"

  # The injector pass leaves BootOrder pointing at Boot0003, which addresses
  # the injector's own GPT partition. That disk is absent when booting the
  # installed system, so firmware tries it, fails to find
  # \EFI\Boot\bootaa64.efi, and has to fall back. Most boots fall back
  # cleanly; some stall in the handoff and the guest never starts. Dropping the
  # dead entry removes the fallback altogether.
  python3 scripts/drop-injector-boot-entry.py "$PREPARED_VARS" \
    >> "$PREPARED_DIR/launcher.out" 2>&1

  if [[ "$(cat "$PREPARED_DIR/injected.txt" 2>/dev/null)" != "1" ]]; then
    echo "FAIL: the injector pass did not report injector_boot_observed=1" >&2
    echo "  see $PREPARED_DIR/launcher.out" >&2
    exit 1
  fi
  rm -f "$PREPARED_DIR/inj.raw"
  touch "$PREPARED_DIR/ready"
fi

# The boot dirs are still registered in gate order below; only execution is
# concurrent. The manifest is written per-boot inside run_one_boot, so entries
# can interleave -- aggregation reads every line regardless of order.
# Batched rather than pipelined: macOS bash 3.2 has no `wait -n`.
i=1
while [[ "$i" -le "$BOOTS" ]]; do
  batch_end=$((i + LANES - 1))
  [[ "$batch_end" -gt "$BOOTS" ]] && batch_end="$BOOTS"
  for j in $(seq "$i" "$batch_end"); do
    run_one_boot "$j" &
  done
  wait
  i=$((batch_end + 1))
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
