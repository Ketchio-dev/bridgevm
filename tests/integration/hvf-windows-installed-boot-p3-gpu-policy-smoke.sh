#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-installed-p3-gpu-policy.XXXXXX")"
TARGET="$STORE/windows-target.raw"
VARS="$STORE/vars.fd"
EVIDENCE="$STORE/evidence"
TRACE="$STORE/evidence/p3-gpu.jsonl"
VIOGPU3D="$STORE/viogpu3d"

touch "$TARGET" "$VARS"
mkdir -p "$EVIDENCE" "$VIOGPU3D"

cleanup() {
  rm -rf "$STORE"
}
trap cleanup EXIT

write_minimal_pe() {
  local path="$1"
  local machine_low_octal="$2"
  local machine_high_octal="$3"

  dd if=/dev/zero of="$path" bs=512 count=1 >/dev/null 2>&1
  printf 'MZ' | dd of="$path" bs=1 seek=0 conv=notrunc >/dev/null 2>&1
  printf '\200\000\000\000' | dd of="$path" bs=1 seek=60 conv=notrunc >/dev/null 2>&1
  printf "PE\000\000\\$machine_low_octal\\$machine_high_octal" |
    dd of="$path" bs=1 seek=128 conv=notrunc >/dev/null 2>&1
}

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  trap - EXIT
  exit 1
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) ;;
    *) fail "$label missing '$needle'; got: $haystack" ;;
  esac
}

cat >"$VIOGPU3D/viogpu3d.inf" <<'INF'
[Manufacturer]
%RedHat% = RedHat,NTarm64

[RedHat.NTarm64]
%VirtioGpu3D.DeviceDesc% = viogpu3d_Device, PCI\VEN_1AF4&DEV_10F7

; BridgeVMProtocol=venus
INF
write_minimal_pe "$VIOGPU3D/viogpu3d.sys" 144 252
printf 'fake catalog\n' >"$VIOGPU3D/viogpu3d.cat"

output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --virtio-gpu-3d \
    --gpu-trace "$TRACE" \
    --gpu-trace-protocol venus \
    --require-gpu-trace-gate \
    --viogpu3d-dir "$VIOGPU3D" \
    --require-viogpu3d-readiness \
    --print-policy 2>&1
)" || fail "installed boot P3 GPU policy failed: $output"

assert_contains "$output" "BRIDGEVM_VIRTIO_GPU_3D=1" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=venus" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_GPU=1" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_GPU_3D_BIND_ID=1" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_GPU_PCI_DEVICE_ID=" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_GPU_TRACE_JSONL=$TRACE" "installed boot policy"
assert_contains "$output" "BRIDGEVM_GPU_TRACE_PROTOCOL=venus" "installed boot policy"
assert_contains "$output" "BRIDGEVM_REQUIRE_GPU_TRACE_GATE=1" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIOGPU3D_DIR=$VIOGPU3D" "installed boot policy"
assert_contains "$output" "BRIDGEVM_REQUIRE_VIOGPU3D_READINESS=1" "installed boot policy"
assert_contains "$output" "BRIDGEVM_RAM_MIB=4096" "installed boot policy"
assert_contains "$output" "BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=900000" "installed boot policy"
assert_contains "$output" "BRIDGEVM_SMP_CPUS=<unset> (probe default 1)" "installed boot policy"
assert_contains "$output" "BRIDGEVM_BOOT_TIMER=<unset>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_BOOT_TIMER_RAMFB_MS=<probe-default 1000>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64=<unset>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_BOOT_TIMER_DESKTOP_AGENT=<unset>" "installed boot policy"
assert_contains "$output" "BUILD_PROFILE=debug" "installed boot policy"
assert_contains "$output" "BRIDGEVM_NVME_DISK_WRITABLE=1 when booting target as only NVMe" "installed boot policy"

boot_timer_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer-ramfb-ms 250 \
    --boot-timer-desktop-checksum64 0x1234abcd \
    --print-policy 2>&1
)" || fail "installed boot timer policy failed: $boot_timer_output"

assert_contains "$boot_timer_output" "BRIDGEVM_BOOT_TIMER=1" "boot timer policy"
assert_contains "$boot_timer_output" "BRIDGEVM_BOOT_TIMER_RAMFB_MS=250" "boot timer policy"
assert_contains "$boot_timer_output" "BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64=0x1234abcd" "boot timer policy"

agent_timer_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer-desktop-agent \
    --print-policy 2>&1
)" || fail "installed boot agent timer policy failed: $agent_timer_output"
assert_contains "$agent_timer_output" "BRIDGEVM_BOOT_TIMER_DESKTOP_AGENT=1" "agent timer policy"

boot_timer_env_output="$(
  bash -c '
    set -euo pipefail
    source scripts/run-hvf-windows-installed-boot-validation.sh
    source scripts/run-hvf-windows-installed-boot-args.sh
    source scripts/run-hvf-windows-installed-boot-runner.sh
    init_installed_boot_defaults
    parse_installed_boot_args "$@"
    build_installed_boot_env_args
  ' _ \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer-ramfb-ms 250 \
    --boot-timer-desktop-checksum64 0x1234abcd
)" || fail "installed boot timer env failed: $boot_timer_env_output"

assert_contains "$boot_timer_env_output" "BRIDGEVM_BOOT_TIMER=1" "boot timer env"
assert_contains "$boot_timer_env_output" "BRIDGEVM_BOOT_TIMER_RAMFB_MS=250" "boot timer env"
assert_contains "$boot_timer_env_output" "BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64=0x1234abcd" "boot timer env"

daily_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --daily \
    --print-policy 2>&1
)" || fail "installed boot daily policy failed: $daily_output"

assert_contains "$daily_output" "DAILY_PRESET=1" "daily policy"
assert_contains "$daily_output" "BRIDGEVM_RAM_MIB=6144" "daily policy"
assert_contains "$daily_output" "BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=86400000" "daily policy"
assert_contains "$daily_output" "BRIDGEVM_SMP_CPUS=4" "daily policy"
assert_contains "$daily_output" "BRIDGEVM_XHCI_REPORT_INTERVAL_MS=30" "daily policy"
assert_contains "$daily_output" "BUILD_PROFILE=release" "daily policy"

daily_override_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --daily \
    --ram-mib 2048 \
    --watchdog-ms 12345 \
    --smp-cpus 1 \
    --print-policy 2>&1
)" || fail "installed boot daily override policy failed: $daily_override_output"

assert_contains "$daily_override_output" "BRIDGEVM_RAM_MIB=2048" "daily override policy"
assert_contains "$daily_override_output" "BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=12345" "daily override policy"
assert_contains "$daily_override_output" "BRIDGEVM_SMP_CPUS=1" "daily override policy"
assert_contains "$daily_override_output" "BRIDGEVM_XHCI_REPORT_INTERVAL_MS=30" "daily override policy"
assert_contains "$daily_override_output" "BUILD_PROFILE=release" "daily override policy"

bad_smp_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --smp-cpus 0 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted invalid SMP count: $bad_smp_output"

assert_contains "$bad_smp_output" "--smp-cpus requires an integer from 1 to 123" "invalid SMP policy"

huge_smp_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --smp-cpus 999999999999999999999999999999999999 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted overflowing SMP count: $huge_smp_output"
assert_contains "$huge_smp_output" "--smp-cpus requires an integer from 1 to 123" "overflowing SMP policy"

bad_boot_timer_ms_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer-ramfb-ms 99 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted invalid boot timer interval: $bad_boot_timer_ms_output"

assert_contains "$bad_boot_timer_ms_output" "--boot-timer-ramfb-ms requires an integer from 100 to 60000" "invalid boot timer interval policy"

huge_boot_timer_ms_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer-ramfb-ms 999999999999999999999999999999999999 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted overflowing boot timer interval: $huge_boot_timer_ms_output"
assert_contains "$huge_boot_timer_ms_output" "--boot-timer-ramfb-ms requires an integer from 100 to 60000" "overflowing boot timer policy"

bad_boot_timer_checksum_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer-desktop-checksum64 0x10000000000000000 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted invalid boot timer checksum: $bad_boot_timer_checksum_output"

assert_contains "$bad_boot_timer_checksum_output" "--boot-timer-desktop-checksum64 requires a u64 decimal or 0x-prefixed hex value" "invalid boot timer checksum policy"

conflicting_oracle_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer-desktop-agent \
    --boot-timer-desktop-checksum64 0x1 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted two desktop oracles: $conflicting_oracle_output"
assert_contains "$conflicting_oracle_output" "choose exactly one BOOT_TIMER desktop oracle" "desktop oracle conflict"

bad_smp_trace_output="$(
  BRIDGEVM_SMP_TRACE=1 scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted BRIDGEVM_SMP_TRACE with boot timer: $bad_smp_trace_output"

assert_contains "$bad_smp_trace_output" "--boot-timer cannot be measured with BRIDGEVM_SMP_TRACE=1" "boot timer smp trace rejection"

trimmed_smp_trace_output="$(
  BRIDGEVM_SMP_TRACE=' true ' scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted whitespace-padded true SMP trace: $trimmed_smp_trace_output"
assert_contains "$trimmed_smp_trace_output" "--boot-timer cannot be measured with BRIDGEVM_SMP_TRACE= true " "trimmed SMP trace rejection"

false_smp_trace_output="$(
  BRIDGEVM_SMP_TRACE=0 scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer \
    --print-policy 2>&1
)" || fail "installed boot rejected falsey BRIDGEVM_SMP_TRACE with boot timer: $false_smp_trace_output"

assert_contains "$false_smp_trace_output" "BRIDGEVM_BOOT_TIMER=1" "boot timer falsey smp trace policy"

trimmed_false_smp_trace_output="$(
  BRIDGEVM_SMP_TRACE=' false ' scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer \
    --print-policy 2>&1
)" || fail "installed boot rejected whitespace-padded false SMP trace: $trimmed_false_smp_trace_output"
assert_contains "$trimmed_false_smp_trace_output" "BRIDGEVM_BOOT_TIMER=1" "trimmed false SMP trace policy"

FAKE_PROBE="$STORE/fake-probe.sh"
SANITIZE_EVIDENCE="$STORE/sanitize-evidence"
mkdir -p "$SANITIZE_EVIDENCE"
cat >"$FAKE_PROBE" <<'PROBE'
#!/usr/bin/env bash
set -euo pipefail
printf 'smp=%s\n' "${BRIDGEVM_SMP_CPUS-<unset>}"
printf 'timer=%s\n' "${BRIDGEVM_BOOT_TIMER-<unset>}"
printf 'timer_ms=%s\n' "${BRIDGEVM_BOOT_TIMER_RAMFB_MS-<unset>}"
printf 'desktop=%s\n' "${BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64-<unset>}"
printf 'desktop_agent=%s\n' "${BRIDGEVM_BOOT_TIMER_DESKTOP_AGENT-<unset>}"
PROBE
chmod +x "$FAKE_PROBE"

sanitized_probe_output="$(
  BRIDGEVM_SMP_CPUS=99 \
  BRIDGEVM_BOOT_TIMER=1 \
  BRIDGEVM_BOOT_TIMER_RAMFB_MS=777 \
  BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64=0xdead \
  BRIDGEVM_BOOT_TIMER_DESKTOP_AGENT=1 \
  bash -c '
    set -euo pipefail
    source scripts/run-hvf-windows-installed-boot-runner.sh
    BIN="$1"
    EVIDENCE_DIR="$2"
    ENV_ARGS=(BRIDGEVM_RAM_MIB=4096)
    PROBE_PID=""
    run_probe_process
    cat "$EVIDENCE_DIR/run.log"
  ' _ "$FAKE_PROBE" "$SANITIZE_EVIDENCE"
)" || fail "ambient CLI-owned env sanitization failed: $sanitized_probe_output"
assert_contains "$sanitized_probe_output" "smp=<unset>" "ambient SMP sanitization"
assert_contains "$sanitized_probe_output" "timer=<unset>" "ambient timer sanitization"
assert_contains "$sanitized_probe_output" "timer_ms=<unset>" "ambient timer interval sanitization"
assert_contains "$sanitized_probe_output" "desktop=<unset>" "ambient desktop checksum sanitization"
assert_contains "$sanitized_probe_output" "desktop_agent=<unset>" "ambient desktop agent sanitization"

explicit_probe_output="$(
  BRIDGEVM_SMP_CPUS=99 \
  BRIDGEVM_BOOT_TIMER=0 \
  BRIDGEVM_BOOT_TIMER_RAMFB_MS=777 \
  BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64=0xdead \
  bash -c '
    set -euo pipefail
    source scripts/run-hvf-windows-installed-boot-runner.sh
    BIN="$1"
    EVIDENCE_DIR="$2"
    ENV_ARGS=(
      BRIDGEVM_SMP_CPUS=2
      BRIDGEVM_BOOT_TIMER=1
      BRIDGEVM_BOOT_TIMER_RAMFB_MS=250
      BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64=0x1234
    )
    PROBE_PID=""
    run_probe_process
    cat "$EVIDENCE_DIR/run.log"
  ' _ "$FAKE_PROBE" "$SANITIZE_EVIDENCE"
)" || fail "explicit CLI-owned env application failed: $explicit_probe_output"
assert_contains "$explicit_probe_output" "smp=2" "explicit SMP env"
assert_contains "$explicit_probe_output" "timer=1" "explicit timer env"
assert_contains "$explicit_probe_output" "timer_ms=250" "explicit timer interval env"
assert_contains "$explicit_probe_output" "desktop=0x1234" "explicit desktop checksum env"

explicit_id_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --virtio-gpu-3d \
    --virtio-gpu-device-id 1050 \
    --print-policy 2>&1
)" || fail "installed boot explicit GPU PCI ID policy failed: $explicit_id_output"

assert_contains "$explicit_id_output" "BRIDGEVM_VIRTIO_GPU_3D_BIND_ID=<unset> (explicit device id 0x1050)" "installed boot explicit PCI ID policy"
assert_contains "$explicit_id_output" "BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=venus" "installed boot explicit PCI ID policy"
assert_contains "$explicit_id_output" "BRIDGEVM_VIRTIO_GPU_PCI_DEVICE_ID=0x1050" "installed boot explicit PCI ID policy"

id_without_gpu_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --virtio-gpu-device-id 1050 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted GPU PCI ID without --virtio-gpu-3d: $id_without_gpu_output"

assert_contains "$id_without_gpu_output" "--virtio-gpu-device-id requires --virtio-gpu-3d" "GPU PCI ID without GPU policy"

virgl_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --virtio-gpu-3d \
    --gpu-trace-protocol virgl \
    --print-policy 2>&1
)" || fail "installed boot unexpectedly rejected virgl protocol: $virgl_output"

assert_contains "$virgl_output" "BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=virgl" "virgl policy"
assert_contains "$virgl_output" "BRIDGEVM_GPU_TRACE_PROTOCOL=virgl" "virgl policy"

echo "PASS: installed Windows P3 GPU policy smoke ($STORE)"
