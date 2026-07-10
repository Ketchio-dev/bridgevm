#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEFAULT_EVIDENCE_DIR="$HOME/bridgevm-live-evidence/apple-vz-proxy-crop-2026-06-18-auto-verified"
EVIDENCE_DIR="${BRIDGEVM_PRODUCT_GATE_VZ_PROXY_CROP_EVIDENCE_DIR:-$DEFAULT_EVIDENCE_DIR}"
LIVE_AGENT="${BRIDGEVM_LIVE_GUEST_TOOLS_AGENT_BINARY:-$ROOT/target/aarch64-unknown-linux-gnu/release/bridgevm-tools-linux}"

status_line() {
  local status="$1"
  local gate="$2"
  local detail="$3"
  printf '%-8s %s - %s\n' "$status" "$gate" "$detail"
}

has_forbidden_percent() {
  grep -Eq '[0-9]+%' <<<"$1"
}

contains_text() {
  local haystack="$1"
  local needle="$2"
  case "$haystack" in
    *"$needle"*) return 0 ;;
    *) return 1 ;;
  esac
}

byte_octal() {
  printf '%03o' "$1"
}

write_bytes() {
  local path="$1"
  local offset="$2"
  local bytes="$3"
  printf '%b' "$bytes" | dd of="$path" bs=1 seek="$offset" conv=notrunc 2>/dev/null
}

write_byte() {
  local path="$1"
  local offset="$2"
  local value="$3"
  write_bytes "$path" "$offset" "\\$(byte_octal "$value")"
}

write_le16() {
  local path="$1"
  local offset="$2"
  local value="$3"
  write_bytes "$path" "$offset" "\\$(byte_octal $((value & 0xff)))\\$(byte_octal $(((value >> 8) & 0xff)))"
}

write_le32() {
  local path="$1"
  local offset="$2"
  local value="$3"
  write_bytes "$path" "$offset" "\\$(byte_octal $((value & 0xff)))\\$(byte_octal $(((value >> 8) & 0xff)))\\$(byte_octal $(((value >> 16) & 0xff)))\\$(byte_octal $(((value >> 24) & 0xff)))"
}

write_le64() {
  local path="$1"
  local offset="$2"
  local value="$3"
  local bytes=""
  local shift
  for shift in 0 8 16 24 32 40 48 56; do
    bytes+="\\$(byte_octal $(((value >> shift) & 0xff)))"
  done
  write_bytes "$path" "$offset" "$bytes"
}

write_uefi_fv_fixture() {
  local path="$1"
  local size="$2"
  dd if=/dev/zero of="$path" bs="$size" count=1 2>/dev/null
  write_bytes "$path" 16 '\214\214\371\141\322\113\054\117\212\211\042\115\257\334\361\157'
  write_le64 "$path" 32 "$size"
  write_bytes "$path" 40 '_FVH'
  write_le32 "$path" 44 327423
  write_le16 "$path" 48 72
  write_le16 "$path" 52 0
  write_byte "$path" 54 0
  write_byte "$path" 55 2
  write_le32 "$path" 56 1
  write_le32 "$path" 60 "$size"
  write_le32 "$path" 64 0
  write_le32 "$path" 68 0

  local sum
  sum="$(od -An -tu2 -N72 -v "$path" | awk '{ for (i = 1; i <= NF; i++) s = (s + $i) % 65536 } END { print s + 0 }')"
  local checksum=$(((65536 - sum) % 65536))
  write_le16 "$path" 50 "$checksum"
}

windows_hvf_machine_plan_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-machine.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  local installer="ISO/Win11_25H2_English_Arm64_v2.iso"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Windows Arm HVF product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p bridgevm-cli -- hvf machine-plan --installer "$installer" 2>&1
  )" || {
    printf 'bridgevm hvf machine-plan failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p hvf-runner -- --machine-plan --installer "$installer" 2>&1
  )" || {
    printf 'hvf-runner --machine-plan failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "Windows 11 Arm HVF machine plan" \
    || { printf 'missing machine-plan title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "vCPU lifecycle:" \
    || { printf 'missing vCPU lifecycle metadata'; return 1; }
  contains_text "$combined" "Devices:" \
    || { printf 'missing device metadata'; return 1; }
  contains_text "$combined" "read-only installer media" \
    || { printf 'missing read-only installer media metadata'; return 1; }
  contains_text "$combined" "ISO-backed reads and read-only write rejection" \
    || { printf 'missing ISO-backed read/write-rejection metadata'; return 1; }
  contains_text "$combined" "system boot disk" \
    || { printf 'missing system boot disk metadata'; return 1; }
  contains_text "$combined" "writable host-file sector write/flush/reopen persistence boundary" \
    || { printf 'missing writable host-file persistence boundary metadata'; return 1; }
  contains_text "$combined" "sparse raw GPT/ESP/MSR/Windows layout probe" \
    || { printf 'missing boot-disk layout boundary metadata'; return 1; }
  contains_text "$combined" "firmware handoff" \
    || { printf 'missing firmware handoff blocker metadata'; return 1; }
  contains_text "$combined" "Overall: blocked" \
    || { printf 'missing blocked readiness status'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in no-QEMU machine-plan output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in machine-plan output'; return 1; }

  return 0
}

windows_hvf_boot_disk_layout_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-boot-disk-layout.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  local cli_disk="$store/cli-win11-arm-hvf.raw"
  local runner_disk="$store/runner-win11-arm-hvf.raw"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Windows Arm HVF boot-disk layout product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p bridgevm-cli -- hvf windows-boot-disk-layout-probe --disk "$cli_disk" --size-gib 8 --create 2>&1
  )" || {
    printf 'bridgevm hvf windows-boot-disk-layout-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p hvf-runner -- --windows-boot-disk-layout-probe --disk "$runner_disk" --size-gib 8 --create 2>&1
  )" || {
    printf 'hvf-runner --windows-boot-disk-layout-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  [[ -f "$cli_disk" ]] || { printf 'CLI sparse disk was not created'; return 1; }
  [[ -f "$runner_disk" ]] || { printf 'runner sparse disk was not created'; return 1; }

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "Windows 11 Arm HVF boot disk layout probe" \
    || { printf 'missing boot-disk layout title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "HVF: not entered" \
    || { printf 'missing HVF: not entered'; return 1; }
  contains_text "$combined" "Disk bytes: 0x200000000" \
    || { printf 'missing 8 GiB disk size evidence'; return 1; }
  contains_text "$combined" "Protective MBR verified: true" \
    || { printf 'missing protective MBR verification'; return 1; }
  contains_text "$combined" "Primary GPT verified: true" \
    || { printf 'missing primary GPT verification'; return 1; }
  contains_text "$combined" "Backup GPT verified: true" \
    || { printf 'missing backup GPT verification'; return 1; }
  contains_text "$combined" "Partition entries verified: true" \
    || { printf 'missing partition-entry verification'; return 1; }
  contains_text "$combined" "EFI System Partition" \
    || { printf 'missing ESP partition'; return 1; }
  contains_text "$combined" "Microsoft Reserved" \
    || { printf 'missing MSR partition'; return 1; }
  contains_text "$combined" "Windows Basic Data" \
    || { printf 'missing Windows data partition'; return 1; }
  contains_text "$combined" "Blockers: none" \
    || { printf 'missing blocker-free layout evidence'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in boot-disk layout output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in boot-disk layout output'; return 1; }

  return 0
}

windows_hvf_xhci_hid_boot_key_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-xhci-hid-boot-key.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  mkdir -p "$fake_bin"

  fail_xhci_hid_boot_key_metadata() {
    local message="$1"
    rm -rf "$store"
    printf '%s' "$message"
    return 1
  }

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Windows Arm HVF xHCI HID boot-key product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p bridgevm-cli -- hvf windows-xhci-hid-boot-key-probe 2>&1
  )" || {
    fail_xhci_hid_boot_key_metadata "bridgevm hvf windows-xhci-hid-boot-key-probe failed: $cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p hvf-runner -- --windows-xhci-hid-boot-key-probe 2>&1
  )" || {
    fail_xhci_hid_boot_key_metadata "hvf-runner --windows-xhci-hid-boot-key-probe failed: $runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    fail_xhci_hid_boot_key_metadata "backend or GUI launch attempted: $(cat "$backend_log")"
    return 1
  fi

  assert_xhci_hid_boot_key_marker() {
    local output="$1"
    local surface="$2"
    local marker="$3"
    local message="$4"
    contains_text "$output" "$marker" \
      || { fail_xhci_hid_boot_key_metadata "$surface missing $message"; return 1; }
  }

  check_xhci_hid_boot_key_surface() {
    local surface_output="$1"
    local surface="$2"
    assert_xhci_hid_boot_key_marker "$surface_output" "$surface" "Windows 11 Arm HVF xHCI HID boot-key report probe" "xHCI HID boot-key title" || return 1
    assert_xhci_hid_boot_key_marker "$surface_output" "$surface" "QEMU: not used" "QEMU: not used" || return 1
    assert_xhci_hid_boot_key_marker "$surface_output" "$surface" "Apple VZ: not used" "Apple VZ: not used" || return 1
    assert_xhci_hid_boot_key_marker "$surface_output" "$surface" "HVF: not entered" "HVF: not entered" || return 1
    assert_xhci_hid_boot_key_marker "$surface_output" "$surface" "Windows boot: not claimed" "Windows boot: not claimed" || return 1
    assert_xhci_hid_boot_key_marker "$surface_output" "$surface" "Usage page: 0x07" "HID usage page" || return 1
    assert_xhci_hid_boot_key_marker "$surface_output" "$surface" "Usage ID: 0x2c" "HID usage ID" || return 1
    assert_xhci_hid_boot_key_marker "$surface_output" "$surface" "Key report: 00 00 2c 00 00 00 00 00" "Space key report" || return 1
    assert_xhci_hid_boot_key_marker "$surface_output" "$surface" "Release report: 00 00 00 00 00 00 00 00" "release report" || return 1
    assert_xhci_hid_boot_key_marker "$surface_output" "$surface" "Transfer events: 2" "transfer event count" || return 1
    assert_xhci_hid_boot_key_marker "$surface_output" "$surface" "Blockers: none" "blocker-free xHCI HID evidence" || return 1
    ! contains_text "$surface_output" "qemu-system" \
      || { fail_xhci_hid_boot_key_metadata "$surface reported qemu-system in xHCI HID boot-key output"; return 1; }
    ! contains_text "$surface_output" "Windows boot: claimed" \
      || { fail_xhci_hid_boot_key_metadata "$surface xHCI HID boot-key output claimed Windows boot"; return 1; }
    ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$surface_output" \
      || { fail_xhci_hid_boot_key_metadata "$surface reported forbidden percentage in xHCI HID boot-key output"; return 1; }
  }

  check_xhci_hid_boot_key_surface "$cli_output" "CLI" || return 1
  check_xhci_hid_boot_key_surface "$runner_output" "runner" || return 1

  rm -rf "$store"
  return 0
}

windows_hvf_firmware_handoff_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-firmware-handoff.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  local cli_firmware="$store/cli-AAVMF_CODE.fd"
  local cli_vars_template="$store/cli-AAVMF_VARS.fd"
  local cli_vars="$store/cli-vars.fd"
  local runner_firmware="$store/runner-AAVMF_CODE.fd"
  local runner_vars_template="$store/runner-AAVMF_VARS.fd"
  local runner_vars="$store/runner-vars.fd"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Windows Arm HVF firmware handoff product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  write_uefi_fv_fixture "$cli_firmware" 131072
  write_uefi_fv_fixture "$cli_vars_template" 65536
  write_uefi_fv_fixture "$runner_firmware" 131072
  write_uefi_fv_fixture "$runner_vars_template" 65536

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p bridgevm-cli -- hvf windows-firmware-handoff-probe --firmware "$cli_firmware" --vars-template "$cli_vars_template" --vars "$cli_vars" --create-vars 2>&1
  )" || {
    printf 'bridgevm hvf windows-firmware-handoff-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p hvf-runner -- --windows-firmware-handoff-probe --firmware "$runner_firmware" --vars-template "$runner_vars_template" --vars "$runner_vars" --create-vars 2>&1
  )" || {
    printf 'hvf-runner --windows-firmware-handoff-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  [[ -f "$cli_vars" ]] || { printf 'CLI mutable vars store was not created'; return 1; }
  [[ -f "$runner_vars" ]] || { printf 'runner mutable vars store was not created'; return 1; }
  cmp -s "$cli_vars_template" "$cli_vars" \
    || { printf 'CLI mutable vars store does not match template'; return 1; }
  cmp -s "$runner_vars_template" "$runner_vars" \
    || { printf 'runner mutable vars store does not match template'; return 1; }

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "Windows 11 Arm HVF UEFI firmware handoff probe" \
    || { printf 'missing firmware handoff title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "HVF: not entered" \
    || { printf 'missing HVF: not entered'; return 1; }
  contains_text "$combined" "Firmware verified: true" \
    || { printf 'missing firmware verification'; return 1; }
  contains_text "$combined" "Firmware volume checksum verified: true" \
    || { printf 'missing firmware FV checksum verification'; return 1; }
  contains_text "$combined" "Vars template verified: true" \
    || { printf 'missing vars template verification'; return 1; }
  contains_text "$combined" "Vars created: true" \
    || { printf 'missing vars creation evidence'; return 1; }
  contains_text "$combined" "Vars verified: true" \
    || { printf 'missing vars verification'; return 1; }
  contains_text "$combined" "Vars volume checksum verified: true" \
    || { printf 'missing vars FV checksum verification'; return 1; }
  contains_text "$combined" "Planned reset vector IPA: 0x8000000" \
    || { printf 'missing planned reset vector IPA'; return 1; }
  contains_text "$combined" "Blockers: none" \
    || { printf 'missing blocker-free firmware handoff evidence'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in firmware handoff output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in firmware handoff output'; return 1; }

  return 0
}

windows_hvf_pflash_map_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-pflash-map.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  local cli_firmware="$store/cli-AAVMF_CODE.fd"
  local cli_vars_template="$store/cli-AAVMF_VARS.fd"
  local cli_vars="$store/cli-vars.fd"
  local runner_firmware="$store/runner-AAVMF_CODE.fd"
  local runner_vars_template="$store/runner-AAVMF_VARS.fd"
  local runner_vars="$store/runner-vars.fd"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Windows Arm HVF pflash map product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  write_uefi_fv_fixture "$cli_firmware" 131072
  write_uefi_fv_fixture "$cli_vars_template" 65536
  write_uefi_fv_fixture "$runner_firmware" 131072
  write_uefi_fv_fixture "$runner_vars_template" 65536

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p bridgevm-cli -- hvf windows-pflash-map-probe --firmware "$cli_firmware" --vars-template "$cli_vars_template" --vars "$cli_vars" --create-vars 2>&1
  )" || {
    printf 'bridgevm hvf windows-pflash-map-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p hvf-runner -- --windows-pflash-map-probe --firmware "$runner_firmware" --vars-template "$runner_vars_template" --vars "$runner_vars" --create-vars 2>&1
  )" || {
    printf 'hvf-runner --windows-pflash-map-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  [[ -f "$cli_vars" ]] || { printf 'CLI mutable vars store was not created'; return 1; }
  [[ -f "$runner_vars" ]] || { printf 'runner mutable vars store was not created'; return 1; }
  cmp -s "$cli_vars_template" "$cli_vars" \
    || { printf 'CLI mutable vars store does not match template'; return 1; }
  cmp -s "$runner_vars_template" "$runner_vars" \
    || { printf 'runner mutable vars store does not match template'; return 1; }

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "Windows 11 Arm HVF UEFI pflash map probe" \
    || { printf 'missing pflash map title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "HVF: not entered" \
    || { printf 'missing HVF: not entered'; return 1; }
  contains_text "$combined" "AArch64 UEFI pflash slots loaded into memory images" \
    || { printf 'missing pflash memory image evidence'; return 1; }
  contains_text "$combined" "Firmware verified: true" \
    || { printf 'missing firmware verification'; return 1; }
  contains_text "$combined" "Vars created: true" \
    || { printf 'missing vars creation evidence'; return 1; }
  contains_text "$combined" "Vars verified: true" \
    || { printf 'missing vars verification'; return 1; }
  contains_text "$combined" "Firmware pflash loaded: true" \
    || { printf 'missing firmware pflash load evidence'; return 1; }
  contains_text "$combined" "Firmware pflash prefix verified: true" \
    || { printf 'missing firmware pflash prefix verification'; return 1; }
  contains_text "$combined" "Firmware pflash padding zeroed: true" \
    || { printf 'missing firmware pflash padding verification'; return 1; }
  contains_text "$combined" "Vars pflash loaded: true" \
    || { printf 'missing vars pflash load evidence'; return 1; }
  contains_text "$combined" "Vars pflash writable: true" \
    || { printf 'missing writable vars pflash evidence'; return 1; }
  contains_text "$combined" "Vars pflash prefix verified: true" \
    || { printf 'missing vars pflash prefix verification'; return 1; }
  contains_text "$combined" "Vars pflash padding zeroed: true" \
    || { printf 'missing vars pflash padding verification'; return 1; }
  contains_text "$combined" "Pflash slots non-overlapping: true" \
    || { printf 'missing pflash non-overlap verification'; return 1; }
  contains_text "$combined" "Guest RAM overlap verified: true" \
    || { printf 'missing guest RAM overlap verification'; return 1; }
  contains_text "$combined" "Device MMIO overlap verified: true" \
    || { printf 'missing device MMIO overlap verification'; return 1; }
  contains_text "$combined" "Pflash map verified: true" \
    || { printf 'missing pflash map verification'; return 1; }
  contains_text "$combined" "Planned reset vector IPA: 0x8000000" \
    || { printf 'missing planned reset vector IPA'; return 1; }
  contains_text "$combined" "Blockers: none" \
    || { printf 'missing blocker-free pflash map evidence'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in pflash map output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in pflash map output'; return 1; }

  return 0
}

windows_hvf_pflash_hvf_map_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-pflash-hvf-map.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  local cli_firmware="$store/cli-AAVMF_CODE.fd"
  local cli_vars_template="$store/cli-AAVMF_VARS.fd"
  local cli_vars="$store/cli-vars.fd"
  local runner_firmware="$store/runner-AAVMF_CODE.fd"
  local runner_vars_template="$store/runner-AAVMF_VARS.fd"
  local runner_vars="$store/runner-vars.fd"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Windows Arm HVF pflash HVF map product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  write_uefi_fv_fixture "$cli_firmware" 131072
  write_uefi_fv_fixture "$cli_vars_template" 65536
  write_uefi_fv_fixture "$runner_firmware" 131072
  write_uefi_fv_fixture "$runner_vars_template" 65536

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP= \
    cargo run -q -p bridgevm-cli -- hvf windows-pflash-hvf-map-probe --firmware "$cli_firmware" --vars-template "$cli_vars_template" --vars "$cli_vars" --create-vars 2>&1
  )" || {
    printf 'bridgevm hvf windows-pflash-hvf-map-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP= \
    cargo run -q -p hvf-runner -- --windows-pflash-hvf-map-probe --firmware "$runner_firmware" --vars-template "$runner_vars_template" --vars "$runner_vars" --create-vars 2>&1
  )" || {
    printf 'hvf-runner --windows-pflash-hvf-map-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  [[ -f "$cli_vars" ]] || { printf 'CLI mutable vars store was not created'; return 1; }
  [[ -f "$runner_vars" ]] || { printf 'runner mutable vars store was not created'; return 1; }
  cmp -s "$cli_vars_template" "$cli_vars" \
    || { printf 'CLI mutable vars store does not match template'; return 1; }
  cmp -s "$runner_vars_template" "$runner_vars" \
    || { printf 'runner mutable vars store does not match template'; return 1; }

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "Windows 11 Arm HVF UEFI pflash HVF map/unmap probe" \
    || { printf 'missing pflash HVF map title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Guest execution: not entered" \
    || { printf 'missing no-guest-execution boundary'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default pflash HVF map probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Attempted: false" \
    || { printf 'default pflash HVF map probe attempted live HVF mapping'; return 1; }
  contains_text "$combined" "VM created: false" \
    || { printf 'default pflash HVF map probe created an HVF VM'; return 1; }
  contains_text "$combined" "Firmware memory mapped: false" \
    || { printf 'default pflash HVF map probe mapped firmware memory'; return 1; }
  contains_text "$combined" "Vars memory mapped: false" \
    || { printf 'default pflash HVF map probe mapped vars memory'; return 1; }
  contains_text "$combined" "Pflash map verified: true" \
    || { printf 'missing pflash map verification'; return 1; }
  contains_text "$combined" "Firmware slot IPA: 0x8000000" \
    || { printf 'missing firmware pflash IPA'; return 1; }
  contains_text "$combined" "Vars slot IPA: 0xc000000" \
    || { printf 'missing vars pflash IPA'; return 1; }
  contains_text "$combined" "Firmware source bytes: 0x20000" \
    || { printf 'missing firmware source byte count'; return 1; }
  contains_text "$combined" "Vars source bytes: 0x10000" \
    || { printf 'missing vars source byte count'; return 1; }
  contains_text "$combined" "Firmware map flags: read|exec" \
    || { printf 'missing firmware map flags'; return 1; }
  contains_text "$combined" "Vars map flags: read|write" \
    || { printf 'missing vars map flags'; return 1; }
  contains_text "$combined" "VM create status: not attempted" \
    || { printf 'default pflash HVF map probe created or attempted a VM'; return 1; }
  contains_text "$combined" "set BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP=1 or pass --allow-map" \
    || { printf 'missing pflash HVF map opt-in blocker'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in pflash HVF map output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in pflash HVF map output'; return 1; }

  return 0
}

windows_hvf_reset_vector_entry_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-reset-vector-entry.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  local cli_firmware="$store/cli-AAVMF_CODE.fd"
  local cli_vars_template="$store/cli-AAVMF_VARS.fd"
  local cli_vars="$store/cli-vars.fd"
  local runner_firmware="$store/runner-AAVMF_CODE.fd"
  local runner_vars_template="$store/runner-AAVMF_VARS.fd"
  local runner_vars="$store/runner-vars.fd"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Windows Arm HVF reset-vector entry product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  write_uefi_fv_fixture "$cli_firmware" 131072
  write_uefi_fv_fixture "$cli_vars_template" 65536
  write_uefi_fv_fixture "$runner_firmware" 131072
  write_uefi_fv_fixture "$runner_vars_template" 65536

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY= \
    cargo run -q -p bridgevm-cli -- hvf windows-reset-vector-entry-probe --firmware "$cli_firmware" --vars-template "$cli_vars_template" --vars "$cli_vars" --create-vars 2>&1
  )" || {
    printf 'bridgevm hvf windows-reset-vector-entry-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY= \
    cargo run -q -p hvf-runner -- --windows-reset-vector-entry-probe --firmware "$runner_firmware" --vars-template "$runner_vars_template" --vars "$runner_vars" --create-vars 2>&1
  )" || {
    printf 'hvf-runner --windows-reset-vector-entry-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  [[ -f "$cli_vars" ]] || { printf 'CLI mutable vars store was not created'; return 1; }
  [[ -f "$runner_vars" ]] || { printf 'runner mutable vars store was not created'; return 1; }
  cmp -s "$cli_vars_template" "$cli_vars" \
    || { printf 'CLI mutable vars store does not match template'; return 1; }
  cmp -s "$runner_vars_template" "$runner_vars" \
    || { printf 'runner mutable vars store does not match template'; return 1; }

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "Windows 11 Arm HVF UEFI reset-vector entry probe" \
    || { printf 'missing reset-vector entry title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Guest execution: UEFI reset vector entered under watchdog" \
    || { printf 'missing reset-vector guest-execution boundary'; return 1; }
  contains_text "$combined" "Windows boot: not claimed" \
    || { printf 'reset-vector probe claimed Windows boot'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default reset-vector probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Attempted: false" \
    || { printf 'default reset-vector probe attempted live HVF entry'; return 1; }
  contains_text "$combined" "VM created: false" \
    || { printf 'default reset-vector probe created an HVF VM'; return 1; }
  contains_text "$combined" "vCPU created: false" \
    || { printf 'default reset-vector probe created a vCPU'; return 1; }
  contains_text "$combined" "Run attempted: false" \
    || { printf 'default reset-vector probe attempted hv_vcpu_run'; return 1; }
  contains_text "$combined" "Reset-vector entry observed: false" \
    || { printf 'default reset-vector probe observed guest entry'; return 1; }
  contains_text "$combined" "Firmware progress observed: false" \
    || { printf 'default reset-vector probe reported firmware progress'; return 1; }
  contains_text "$combined" "Pflash map verified: true" \
    || { printf 'missing pflash map verification'; return 1; }
  contains_text "$combined" "Reset vector IPA: 0x8000000" \
    || { printf 'missing reset vector IPA'; return 1; }
  contains_text "$combined" "Firmware source bytes: 0x20000" \
    || { printf 'missing firmware source byte count'; return 1; }
  contains_text "$combined" "Vars source bytes: 0x10000" \
    || { printf 'missing vars source byte count'; return 1; }
  contains_text "$combined" "Firmware map flags: read|exec" \
    || { printf 'missing firmware map flags'; return 1; }
  contains_text "$combined" "Vars map flags: read|write" \
    || { printf 'missing vars map flags'; return 1; }
  contains_text "$combined" "VM create status name: not attempted" \
    || { printf 'default reset-vector probe created or attempted a VM'; return 1; }
  contains_text "$combined" "Run status name: not attempted" \
    || { printf 'default reset-vector probe attempted hv_vcpu_run'; return 1; }
  contains_text "$combined" "Exit exception class name: not observed" \
    || { printf 'default reset-vector probe reported an exception class'; return 1; }
  contains_text "$combined" "set BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY=1 or pass --allow-entry" \
    || { printf 'missing reset-vector entry opt-in blocker'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in reset-vector entry output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in reset-vector entry output'; return 1; }

  return 0
}

windows_hvf_firmware_run_loop_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-firmware-run-loop.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  local cli_firmware="$store/cli-AAVMF_CODE.fd"
  local cli_vars_template="$store/cli-AAVMF_VARS.fd"
  local cli_vars="$store/cli-vars.fd"
  local cli_continue_vars="$store/cli-continue-vars.fd"
  local cli_iso="$store/cli-Win11_Arm64.iso"
  local cli_disk="$store/cli-windows-arm.raw"
  local runner_firmware="$store/runner-AAVMF_CODE.fd"
  local runner_vars_template="$store/runner-AAVMF_VARS.fd"
  local runner_vars="$store/runner-vars.fd"
  local runner_continue_vars="$store/runner-continue-vars.fd"
  local runner_iso="$store/runner-Win11_Arm64.iso"
  local runner_disk="$store/runner-windows-arm.raw"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Windows Arm HVF firmware run-loop product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  write_uefi_fv_fixture "$cli_firmware" 131072
  write_uefi_fv_fixture "$cli_vars_template" 65536
  write_uefi_fv_fixture "$runner_firmware" 131072
  write_uefi_fv_fixture "$runner_vars_template" 65536

  local cli_output=""
  local runner_output=""
  local cli_continue_output=""
  local runner_continue_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP= \
    cargo run -q -p bridgevm-cli -- hvf windows-firmware-run-loop-probe --firmware "$cli_firmware" --vars-template "$cli_vars_template" --vars "$cli_vars" --create-vars --iso "$cli_iso" --writable-disk "$cli_disk" 2>&1
  )" || {
    printf 'bridgevm hvf windows-firmware-run-loop-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP= \
    cargo run -q -p hvf-runner -- --windows-firmware-run-loop-probe --firmware "$runner_firmware" --vars-template "$runner_vars_template" --vars "$runner_vars" --create-vars --iso "$runner_iso" --writable-disk "$runner_disk" 2>&1
  )" || {
    printf 'hvf-runner --windows-firmware-run-loop-probe failed: %s' "$runner_output"
    return 1
  }

  cli_continue_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP= \
    cargo run -q -p bridgevm-cli -- hvf windows-firmware-run-loop-probe --firmware "$cli_firmware" --vars-template "$cli_vars_template" --vars "$cli_continue_vars" --create-vars --iso "$cli_iso" --writable-disk "$cli_disk" --map-low-pflash-alias --repair-low-vector-diagnostic-page --continue-after-low-vector-repair 2>&1
  )" || {
    printf 'bridgevm hvf windows-firmware-run-loop-probe continue failed: %s' "$cli_continue_output"
    return 1
  }

  runner_continue_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP= \
    cargo run -q -p hvf-runner -- --windows-firmware-run-loop-probe --firmware "$runner_firmware" --vars-template "$runner_vars_template" --vars "$runner_continue_vars" --create-vars --iso "$runner_iso" --writable-disk "$runner_disk" --map-low-pflash-alias --repair-low-vector-diagnostic-page --continue-after-low-vector-repair 2>&1
  )" || {
    printf 'hvf-runner --windows-firmware-run-loop-probe continue failed: %s' "$runner_continue_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  [[ -f "$cli_vars" ]] || { printf 'CLI mutable vars store was not created'; return 1; }
  [[ -f "$runner_vars" ]] || { printf 'runner mutable vars store was not created'; return 1; }
  [[ -f "$cli_continue_vars" ]] || { printf 'CLI continue mutable vars store was not created'; return 1; }
  [[ -f "$runner_continue_vars" ]] || { printf 'runner continue mutable vars store was not created'; return 1; }
  cmp -s "$cli_vars_template" "$cli_vars" \
    || { printf 'CLI mutable vars store does not match template'; return 1; }
  cmp -s "$runner_vars_template" "$runner_vars" \
    || { printf 'runner mutable vars store does not match template'; return 1; }
  cmp -s "$cli_vars_template" "$cli_continue_vars" \
    || { printf 'CLI continue mutable vars store does not match template'; return 1; }
  cmp -s "$runner_vars_template" "$runner_continue_vars" \
    || { printf 'runner continue mutable vars store does not match template'; return 1; }

  local combined="$cli_output"$'\n'"$runner_output"$'\n'"$cli_continue_output"$'\n'"$runner_continue_output"
  contains_text "$combined" "Windows 11 Arm HVF UEFI firmware run-loop probe" \
    || { printf 'missing firmware run-loop title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Guest execution: bounded UEFI firmware exit classification loop" \
    || { printf 'missing firmware run-loop guest-execution boundary'; return 1; }
  contains_text "$combined" "Windows boot: not claimed" \
    || { printf 'firmware run-loop probe claimed Windows boot'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default firmware run-loop probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Attempted: false" \
    || { printf 'default firmware run-loop probe attempted live HVF entry'; return 1; }
  contains_text "$combined" "VM created: false" \
    || { printf 'default firmware run-loop probe created an HVF VM'; return 1; }
  contains_text "$combined" "Guest RAM memory allocated: false" \
    || { printf 'default firmware run-loop probe allocated guest RAM'; return 1; }
  contains_text "$combined" "Guest RAM memory mapped: false" \
    || { printf 'default firmware run-loop probe mapped guest RAM'; return 1; }
  contains_text "$combined" "Low firmware alias mapped: false" \
    || { printf 'default firmware run-loop probe mapped low firmware alias'; return 1; }
  contains_text "$combined" "Low vars alias mapped: false" \
    || { printf 'default firmware run-loop probe mapped low vars alias'; return 1; }
  contains_text "$combined" "Platform DTB populated: false" \
    || { printf 'default firmware run-loop probe populated the platform DTB'; return 1; }
  contains_text "$combined" "Diagnostic vector seed requested: false" \
    || { printf 'default firmware run-loop probe requested diagnostic vector'; return 1; }
  contains_text "$combined" "Low vector diagnostic page previous descriptor: not observed" \
    || { printf 'default firmware run-loop observed low-vector previous descriptor'; return 1; }
  contains_text "$combined" "Low vector diagnostic page slot restored: false" \
    || { printf 'default firmware run-loop restored low-vector slot without live repair'; return 1; }
  contains_text "$combined" "Low vector diagnostic page restore before ERET requested: false" \
    || { printf 'default firmware run-loop requested low-vector restore-before-ERET'; return 1; }
  contains_text "$combined" "Low vector diagnostic page restore before ERET attempted: false" \
    || { printf 'default firmware run-loop attempted low-vector restore-before-ERET'; return 1; }
  contains_text "$combined" "Low vector diagnostic page repeated fault observed: false" \
    || { printf 'default firmware run-loop observed repeated low-vector fault'; return 1; }
  contains_text "$combined" "Continue after low-vector repair requested: false" \
    || { printf 'default firmware run-loop requested low-vector post-repair continue'; return 1; }
  contains_text "$combined" "Low pflash alias requested: true" \
    || { printf 'continue firmware run-loop did not request low pflash alias'; return 1; }
  contains_text "$combined" "Low vector diagnostic page repair requested: true" \
    || { printf 'continue firmware run-loop did not request low-vector repair'; return 1; }
  contains_text "$combined" "Continue after low-vector repair requested: true" \
    || { printf 'continue firmware run-loop did not record requested low-vector post-repair continue'; return 1; }
  contains_text "$combined" "Continue after low-vector repair attempted: false" \
    || { printf 'default firmware run-loop attempted low-vector post-repair continue'; return 1; }
  contains_text "$combined" "Post-repair unsupported exit observed: false" \
    || { printf 'default firmware run-loop observed post-repair unsupported exit'; return 1; }
  contains_text "$combined" "Post-repair unsupported exit reason name: not observed" \
    || { printf 'default firmware run-loop observed post-repair unsupported exit reason'; return 1; }
  contains_text "$combined" "Post-repair unsupported exit classification: not observed" \
    || { printf 'default firmware run-loop observed post-repair unsupported exit classification'; return 1; }
  contains_text "$combined" "Post-repair first exit observed: false" \
    || { printf 'default firmware run-loop observed post-repair first exit'; return 1; }
  contains_text "$combined" "Post-repair first exit: not observed" \
    || { printf 'default firmware run-loop observed post-repair first exit index'; return 1; }
  contains_text "$combined" "Post-repair first exit reason name: not observed" \
    || { printf 'default firmware run-loop observed post-repair first exit reason'; return 1; }
  contains_text "$combined" "Post-repair first exit classification: not observed" \
    || { printf 'default firmware run-loop observed post-repair first exit classification'; return 1; }
  contains_text "$combined" "Post-repair first exit PC: not observed" \
    || { printf 'default firmware run-loop observed post-repair first exit PC'; return 1; }
  contains_text "$combined" "Post-repair first interaction kind: not observed" \
    || { printf 'default firmware run-loop observed post-repair first interaction kind'; return 1; }
  contains_text "$combined" "Post-repair first device interaction observed: false" \
    || { printf 'default firmware run-loop observed post-repair first device interaction'; return 1; }
  contains_text "$combined" "Post-repair first device interaction: not observed" \
    || { printf 'default firmware run-loop observed post-repair first device interaction index'; return 1; }
  contains_text "$combined" "Post-repair first device interaction reason name: not observed" \
    || { printf 'default firmware run-loop observed post-repair first device interaction reason'; return 1; }
  contains_text "$combined" "Post-repair first device interaction classification: not observed" \
    || { printf 'default firmware run-loop observed post-repair first device interaction classification'; return 1; }
  contains_text "$combined" "Post-repair first device interaction PC: not observed" \
    || { printf 'default firmware run-loop observed post-repair first device interaction PC'; return 1; }
  contains_text "$combined" "Post-repair first device interaction kind: not observed" \
    || { printf 'default firmware run-loop observed post-repair first device interaction kind'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access observed: false" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access: not observed" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access index'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access reason name: not observed" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access reason'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access classification: not observed" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access classification'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access PC: not observed" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access PC'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access syndrome: not observed" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access syndrome'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access kind: not observed" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access kind'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access direction: not observed" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access direction'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access register: not observed" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access register'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access value: not observed" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access value'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access handler result: not observed" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access handler result'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access MMIO IPA: not observed" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access MMIO IPA'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access MMIO width: not observed" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access MMIO width'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access MMIO device kind: not observed" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access MMIO device kind'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access sysreg: not observed" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access sysreg'; return 1; }
  contains_text "$combined" "Post-repair first unhandled access sysreg name: not observed" \
    || { printf 'default firmware run-loop observed post-repair first unhandled access sysreg name'; return 1; }
  contains_text "$combined" "Low vector diagnostic page resume attempted: false" \
    || { printf 'default firmware run-loop attempted low-vector resume'; return 1; }
  contains_text "$combined" "Low vector diagnostic page resume armed: false" \
    || { printf 'default firmware run-loop armed low-vector resume'; return 1; }
  contains_text "$combined" "Low vector diagnostic page resume original PC: not observed" \
    || { printf 'default firmware run-loop observed low-vector resume original PC'; return 1; }
  contains_text "$combined" "Low vector diagnostic page resume original ELR_EL1: not observed" \
    || { printf 'default firmware run-loop observed low-vector resume original ELR_EL1'; return 1; }
  contains_text "$combined" "Low vector diagnostic page resume original ESR_EL1: not observed" \
    || { printf 'default firmware run-loop observed low-vector resume original ESR_EL1'; return 1; }
  contains_text "$combined" "Low vector diagnostic page resume original FAR_EL1: not observed" \
    || { printf 'default firmware run-loop observed low-vector resume original FAR_EL1'; return 1; }
  contains_text "$combined" "Low vector diagnostic page resume original SPSR_EL1: not observed" \
    || { printf 'default firmware run-loop observed low-vector resume original SPSR_EL1'; return 1; }
  contains_text "$combined" "X0 DTB IPA set: false" \
    || { printf 'default firmware run-loop probe set X0 DTB handoff'; return 1; }
  contains_text "$combined" "SP_EL1 set: false" \
    || { printf 'default firmware run-loop probe set SP_EL1'; return 1; }
  contains_text "$combined" "Diagnostic vector VBAR_EL1 set: false" \
    || { printf 'default firmware run-loop probe set diagnostic VBAR'; return 1; }
  contains_text "$combined" "Recommended vector-base VBAR requested: false" \
    || { printf 'default firmware run-loop requested recommended vector-base VBAR redirect'; return 1; }
  contains_text "$combined" "Recommended vector-base VBAR attempted: false" \
    || { printf 'default firmware run-loop attempted recommended vector-base VBAR redirect'; return 1; }
  contains_text "$combined" "Recommended vector-base VBAR set: false" \
    || { printf 'default firmware run-loop set recommended vector-base VBAR'; return 1; }
  contains_text "$combined" "Recommended vector-base VBAR diagnostic vector populated: false" \
    || { printf 'default firmware run-loop populated recommended vector-base diagnostic vector'; return 1; }
  contains_text "$combined" "Recommended vector-base VBAR resume requested: false" \
    || { printf 'default firmware run-loop requested recommended vector-base VBAR resume'; return 1; }
  contains_text "$combined" "Recommended vector-base VBAR resume attempted: false" \
    || { printf 'default firmware run-loop attempted recommended vector-base VBAR resume'; return 1; }
  contains_text "$combined" "Recommended vector-base VBAR resume armed: false" \
    || { printf 'default firmware run-loop armed recommended vector-base VBAR resume'; return 1; }
  contains_text "$combined" "Recommended vector-base VBAR reason: not requested" \
    || { printf 'default firmware run-loop did not report recommended vector-base VBAR as not requested'; return 1; }
  contains_text "$combined" "Recommended vector-base VBAR follow-up exit observed: false" \
    || { printf 'default firmware run-loop observed a recommended vector-base VBAR follow-up exit'; return 1; }
  contains_text "$combined" "Recommended vector-base VBAR follow-up classification: not observed" \
    || { printf 'default firmware run-loop classified a recommended vector-base VBAR follow-up exit'; return 1; }
  contains_text "$combined" "Recommended vector-base VBAR set status name: not attempted" \
    || { printf 'default firmware run-loop attempted recommended vector-base VBAR status'; return 1; }
  contains_text "$combined" "Recommended vector-base VBAR resume ELR_EL1 set status name: not attempted" \
    || { printf 'default firmware run-loop attempted recommended vector-base VBAR resume ELR status'; return 1; }
  contains_text "$combined" "Recommended vector-base VBAR resume VBAR_EL1 set status name: not attempted" \
    || { printf 'default firmware run-loop attempted recommended vector-base VBAR resume VBAR_EL1 status'; return 1; }
  contains_text "$combined" "Recommended vector-base VBAR resume SPSR_EL1 set status name: not attempted" \
    || { printf 'default firmware run-loop attempted recommended vector-base VBAR resume SPSR status'; return 1; }
  contains_text "$combined" "Recommended vector-base VBAR resume PC set status name: not attempted" \
    || { printf 'default firmware run-loop attempted recommended vector-base VBAR resume PC status'; return 1; }
  contains_text "$combined" "vCPU created: false" \
    || { printf 'default firmware run-loop probe created a vCPU'; return 1; }
  contains_text "$combined" "PC set: false" \
    || { printf 'default firmware run-loop probe set PC'; return 1; }
  contains_text "$combined" "CPSR set: false" \
    || { printf 'default firmware run-loop probe set CPSR'; return 1; }
  contains_text "$combined" "Run loop attempted: false" \
    || { printf 'default firmware run-loop probe attempted hv_vcpu_run'; return 1; }
  contains_text "$combined" "Firmware progress observed: false" \
    || { printf 'default firmware run-loop probe reported firmware progress'; return 1; }
  contains_text "$combined" "Unsupported exit observed: false" \
    || { printf 'default firmware run-loop probe observed unsupported exit'; return 1; }
  contains_text "$combined" "Pflash map verified: true" \
    || { printf 'missing pflash map verification'; return 1; }
  contains_text "$combined" "Guest RAM IPA: 0x40000000" \
    || { printf 'missing guest RAM IPA'; return 1; }
  contains_text "$combined" "Platform DTB IPA: 0x40010000" \
    || { printf 'missing platform DTB IPA'; return 1; }
  contains_text "$combined" "Platform DTB guest RAM offset: 0x10000" \
    || { printf 'missing platform DTB guest RAM offset'; return 1; }
  contains_text "$combined" "SP_EL1 seed IPA: 0x43fffff0" \
    || { printf 'missing SP_EL1 seed IPA'; return 1; }
  contains_text "$combined" "Diagnostic vector location: pflash" \
    || { printf 'missing diagnostic vector location'; return 1; }
  contains_text "$combined" "Diagnostic vector IPA: 0x8000000" \
    || { printf 'missing diagnostic vector IPA'; return 1; }
  contains_text "$combined" "Diagnostic vector bytes: 0x800" \
    || { printf 'missing diagnostic vector byte count'; return 1; }
  contains_text "$combined" "Installer ISO path: $cli_iso" \
    || { printf 'missing CLI installer ISO path metadata'; return 1; }
  contains_text "$combined" "Installer ISO path: $runner_iso" \
    || { printf 'missing runner installer ISO path metadata'; return 1; }
  contains_text "$combined" "Writable target disk path: $cli_disk" \
    || { printf 'missing CLI writable target disk path metadata'; return 1; }
  contains_text "$combined" "Writable target disk path: $runner_disk" \
    || { printf 'missing runner writable target disk path metadata'; return 1; }
  contains_text "$combined" "Firmware block devices:" \
    || { printf 'missing firmware block device metadata section'; return 1; }
  contains_text "$combined" "- role=installer-iso, label=VirtIO-MMIO installer ISO, node=virtio_mmio@10002000, base=0x10002000, bytes=0x1000, read_only=true, backing_kind=host-iso-readonly, backing_path=$cli_iso, device_features=0x20" \
    || { printf 'missing CLI installer ISO block-device metadata'; return 1; }
  contains_text "$combined" "- role=installer-iso, label=VirtIO-MMIO installer ISO, node=virtio_mmio@10002000, base=0x10002000, bytes=0x1000, read_only=true, backing_kind=host-iso-readonly, backing_path=$runner_iso, device_features=0x20" \
    || { printf 'missing runner installer ISO block-device metadata'; return 1; }
  contains_text "$combined" "- role=target-disk, label=VirtIO-MMIO target disk, node=virtio_mmio@10003000, base=0x10003000, bytes=0x1000, read_only=false, backing_kind=host-file-writable, backing_path=$cli_disk, device_features=0x0" \
    || { printf 'missing CLI target disk block-device metadata'; return 1; }
  contains_text "$combined" "- role=target-disk, label=VirtIO-MMIO target disk, node=virtio_mmio@10003000, base=0x10003000, bytes=0x1000, read_only=false, backing_kind=host-file-writable, backing_path=$runner_disk, device_features=0x0" \
    || { printf 'missing runner target disk block-device metadata'; return 1; }
  contains_text "$combined" "Guest RAM bytes: 0x4000000" \
    || { printf 'missing guest RAM byte count'; return 1; }
  contains_text "$combined" "Platform DTB bytes: 0x" \
    || { printf 'missing platform DTB byte count'; return 1; }
  contains_text "$combined" "Platform DTB magic: 0xd00dfeed" \
    || { printf 'missing platform DTB magic'; return 1; }
  contains_text "$combined" "Platform DTB magic verified: true" \
    || { printf 'platform DTB magic was not verified'; return 1; }
  contains_text "$combined" "Requested exits: 8" \
    || { printf 'missing requested exit count'; return 1; }
  contains_text "$combined" "Observed exits: 0" \
    || { printf 'default firmware run-loop probe observed exits'; return 1; }
  contains_text "$combined" "Watchdog timeout ms: 100" \
    || { printf 'missing default firmware watchdog timeout'; return 1; }
  contains_text "$combined" "VTimer offset value: not observed" \
    || { printf 'missing default firmware VTimer offset metadata'; return 1; }
  contains_text "$combined" "CNTV_CVAL_EL0 value: not observed" \
    || { printf 'missing default firmware CNTV_CVAL metadata'; return 1; }
  contains_text "$combined" "CNTV_CTL_EL0 value: not observed" \
    || { printf 'missing default firmware CNTV_CTL metadata'; return 1; }
  contains_text "$combined" "VTimer exit count: 0" \
    || { printf 'default firmware run-loop reported unexpected VTimer exit count'; return 1; }
  contains_text "$combined" "Pending IRQ injected count: 0" \
    || { printf 'default firmware run-loop reported unexpected pending IRQ injection count'; return 1; }
  contains_text "$combined" "Device IRQ line asserted count: 0" \
    || { printf 'default firmware run-loop reported unexpected device IRQ injection count'; return 1; }
  contains_text "$combined" "Device IRQ line deasserted count: 0" \
    || { printf 'default firmware run-loop reported unexpected device IRQ clear count'; return 1; }
  contains_text "$combined" "Handled MMIO read count: 0" \
    || { printf 'default firmware run-loop reported unexpected handled MMIO read count'; return 1; }
  contains_text "$combined" "Handled MMIO write count: 0" \
    || { printf 'default firmware run-loop reported unexpected handled MMIO write count'; return 1; }
  contains_text "$combined" "Handled PL011 MMIO count: 0" \
    || { printf 'default firmware run-loop reported unexpected PL011 MMIO count'; return 1; }
  contains_text "$combined" "Handled PL031 MMIO count: 0" \
    || { printf 'default firmware run-loop reported unexpected PL031 MMIO count'; return 1; }
  contains_text "$combined" "Handled GICD MMIO count: 0" \
    || { printf 'default firmware run-loop reported unexpected GICD MMIO count'; return 1; }
  contains_text "$combined" "Handled GICR MMIO count: 0" \
    || { printf 'default firmware run-loop reported unexpected GICR MMIO count'; return 1; }
  contains_text "$combined" "Handled VirtIO installer ISO MMIO count: 0" \
    || { printf 'default firmware run-loop reported unexpected VirtIO installer ISO MMIO count'; return 1; }
  contains_text "$combined" "Handled VirtIO target disk MMIO count: 0" \
    || { printf 'default firmware run-loop reported unexpected VirtIO target disk MMIO count'; return 1; }
  contains_text "$combined" "VirtIO queue_notify count: 0" \
    || { printf 'default firmware run-loop reported unexpected VirtIO queue_notify count'; return 1; }
  contains_text "$combined" "VirtIO request completion count: 0" \
    || { printf 'default firmware run-loop reported unexpected VirtIO request completion count'; return 1; }
  contains_text "$combined" "Handled ICC read count: 0" \
    || { printf 'default firmware run-loop reported unexpected handled ICC read count'; return 1; }
  contains_text "$combined" "Handled ICC write count: 0" \
    || { printf 'default firmware run-loop reported unexpected handled ICC write count'; return 1; }
  contains_text "$combined" "Handled ICC_IAR1 read count: 0" \
    || { printf 'default firmware run-loop reported unexpected ICC_IAR1 read count'; return 1; }
  contains_text "$combined" "Handled ICC_EOIR1 write count: 0" \
    || { printf 'default firmware run-loop reported unexpected ICC_EOIR1 write count'; return 1; }
  contains_text "$combined" "Handled ICC_DIR write count: 0" \
    || { printf 'default firmware run-loop reported unexpected ICC_DIR write count'; return 1; }
  contains_text "$combined" "Last ICC_IAR1 INTID: not observed" \
    || { printf 'default firmware run-loop observed ICC_IAR1 INTID'; return 1; }
  contains_text "$combined" "Last ICC_EOIR1 INTID: not observed" \
    || { printf 'default firmware run-loop observed ICC_EOIR1 INTID'; return 1; }
  contains_text "$combined" "Last ICC_DIR INTID: not observed" \
    || { printf 'default firmware run-loop observed ICC_DIR INTID'; return 1; }
  contains_text "$combined" "VTimer offset set status name: not attempted" \
    || { printf 'default firmware run-loop set VTimer offset'; return 1; }
  contains_text "$combined" "X0 DTB IPA set status name: not attempted" \
    || { printf 'default firmware run-loop set X0 DTB IPA'; return 1; }
  contains_text "$combined" "CNTV_CVAL_EL0 set status name: not attempted" \
    || { printf 'default firmware run-loop set CNTV_CVAL_EL0'; return 1; }
  contains_text "$combined" "CNTV_CTL_EL0 set status name: not attempted" \
    || { printf 'default firmware run-loop set CNTV_CTL_EL0'; return 1; }
  contains_text "$combined" "Low vector diagnostic page resume ELR_EL1 set status name: not attempted" \
    || { printf 'default firmware run-loop set low-vector resume ELR_EL1'; return 1; }
  contains_text "$combined" "Low vector diagnostic page resume SPSR_EL1 set status name: not attempted" \
    || { printf 'default firmware run-loop set low-vector resume SPSR_EL1'; return 1; }
  contains_text "$combined" "Low vector diagnostic page resume CPSR set status name: not attempted" \
    || { printf 'default firmware run-loop set low-vector resume CPSR'; return 1; }
  contains_text "$combined" "Low vector diagnostic page resume PC set status name: not attempted" \
    || { printf 'default firmware run-loop set low-vector resume PC'; return 1; }
  contains_text "$combined" "VTimer initial unmask status name: not attempted" \
    || { printf 'default firmware run-loop unmasked VTimer initially'; return 1; }
  contains_text "$combined" "Last pending IRQ set status name: not attempted" \
    || { printf 'default firmware run-loop set pending IRQ state'; return 1; }
  contains_text "$combined" "Last device IRQ line assert status name: not attempted" \
    || { printf 'default firmware run-loop set device IRQ pending state'; return 1; }
  contains_text "$combined" "Last device IRQ line deassert status name: not attempted" \
    || { printf 'default firmware run-loop cleared device IRQ pending state'; return 1; }
  contains_text "$combined" "Last VTimer unmask status name: not attempted" \
    || { printf 'default firmware run-loop unmasked VTimer'; return 1; }
  contains_text "$combined" "Final PC status name: not attempted" \
    || { printf 'default firmware run-loop read final PC'; return 1; }
  contains_text "$combined" "Final PC: not observed" \
    || { printf 'default firmware run-loop observed final PC'; return 1; }
  contains_text "$combined" "Run-loop exits:" \
    || { printf 'missing run-loop exits section'; return 1; }
  contains_text "$combined" "- none" \
    || { printf 'default firmware run-loop probe should have no exit records'; return 1; }
  contains_text "$combined" "set BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP=1 or pass --allow-loop" \
    || { printf 'missing firmware run-loop opt-in blocker'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in firmware run-loop output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in firmware run-loop output'; return 1; }

  return 0
}

windows_hvf_firmware_device_discovery_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-firmware-device-discovery.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  local cli_firmware="$store/cli-AAVMF_CODE.fd"
  local cli_vars_template="$store/cli-AAVMF_VARS.fd"
  local cli_vars="$store/cli-vars.fd"
  local cli_iso="$store/cli-Win11_Arm64.iso"
  local cli_disk="$store/cli-windows-arm.raw"
  local runner_firmware="$store/runner-AAVMF_CODE.fd"
  local runner_vars_template="$store/runner-AAVMF_VARS.fd"
  local runner_vars="$store/runner-vars.fd"
  local runner_iso="$store/runner-Win11_Arm64.iso"
  local runner_disk="$store/runner-windows-arm.raw"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Windows Arm HVF firmware device-discovery product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  write_uefi_fv_fixture "$cli_firmware" 131072
  write_uefi_fv_fixture "$cli_vars_template" 65536
  write_uefi_fv_fixture "$runner_firmware" 131072
  write_uefi_fv_fixture "$runner_vars_template" 65536

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP= \
    cargo run -q -p bridgevm-cli -- hvf windows-firmware-device-discovery-probe --firmware "$cli_firmware" --vars-template "$cli_vars_template" --vars "$cli_vars" --create-vars --iso "$cli_iso" --writable-disk "$cli_disk" 2>&1
  )" || {
    printf 'bridgevm hvf windows-firmware-device-discovery-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP= \
    cargo run -q -p hvf-runner -- --windows-firmware-device-discovery-probe --firmware "$runner_firmware" --vars-template "$runner_vars_template" --vars "$runner_vars" --create-vars --iso "$runner_iso" --writable-disk "$runner_disk" 2>&1
  )" || {
    printf 'hvf-runner --windows-firmware-device-discovery-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  cmp -s "$cli_vars_template" "$cli_vars" \
    || { printf 'CLI mutable vars store does not match template'; return 1; }
  cmp -s "$runner_vars_template" "$runner_vars" \
    || { printf 'runner mutable vars store does not match template'; return 1; }

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "Windows 11 Arm HVF UEFI firmware device-discovery probe" \
    || { printf 'missing firmware device-discovery title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Windows boot: not claimed" \
    || { printf 'firmware device-discovery probe claimed Windows boot'; return 1; }
  contains_text "$combined" "Underlying probe: windows-firmware-run-loop-probe" \
    || { printf 'missing underlying run-loop probe marker'; return 1; }
  contains_text "$combined" "Device discovery boundary reached: false" \
    || { printf 'default firmware device-discovery unexpectedly reached a device boundary'; return 1; }
  contains_text "$combined" "Device discovery boundary status: not reached" \
    || { printf 'missing not-reached device-discovery status'; return 1; }
  contains_text "$combined" "Device discovery ready: false" \
    || { printf 'default firmware device-discovery unexpectedly reported ready'; return 1; }
  contains_text "$combined" "Device discovery blocker: firmware has not reached a non-diagnostic MMIO/sysreg boundary yet" \
    || { printf 'missing not-reached device-discovery blocker'; return 1; }
  contains_text "$combined" "Handled MMIO access count: 0" \
    || { printf 'unexpected handled MMIO count in default device-discovery'; return 1; }
  contains_text "$combined" "Handled ICC access count: 0" \
    || { printf 'unexpected handled ICC count in default device-discovery'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default firmware device-discovery did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Attempted: false" \
    || { printf 'default firmware device-discovery attempted live HVF entry'; return 1; }
  contains_text "$combined" "Run loop attempted: false" \
    || { printf 'default firmware device-discovery attempted the run loop'; return 1; }
  contains_text "$combined" "Low pflash alias requested: true" \
    || { printf 'device-discovery probe did not force low pflash alias request'; return 1; }
  contains_text "$combined" "Low vector diagnostic page repair requested: true" \
    || { printf 'device-discovery probe did not force low-vector repair request'; return 1; }
  contains_text "$combined" "Continue after low-vector repair requested: true" \
    || { printf 'device-discovery probe did not force post-repair continue request'; return 1; }
  contains_text "$combined" "Interrupt/timer wiring requested: true" \
    || { printf 'device-discovery probe did not force interrupt/timer wiring request'; return 1; }
  contains_text "$combined" "Stop at first post-repair device boundary requested: true" \
    || { printf 'device-discovery probe did not request stop-at-boundary'; return 1; }
  contains_text "$combined" "Pflash map verified: true" \
    || { printf 'missing pflash map verification'; return 1; }
  contains_text "$combined" "Platform DTB magic verified: true" \
    || { printf 'missing platform DTB verification'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in firmware device-discovery output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in firmware device-discovery output'; return 1; }

  return 0
}

windows_hvf_platform_description_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-winarm-platform-description.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Windows Arm HVF platform-description product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p bridgevm-cli -- hvf windows-platform-description-probe --memory-gib 8 --vcpus 6 2>&1
  )" || {
    printf 'bridgevm hvf windows-platform-description-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p hvf-runner -- --windows-platform-description-probe --memory-gib 8 --vcpus 6 2>&1
  )" || {
    printf 'hvf-runner --windows-platform-description-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "Windows 11 Arm HVF platform description probe" \
    || { printf 'missing platform-description title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "HVF: not entered" \
    || { printf 'platform-description probe entered HVF'; return 1; }
  contains_text "$combined" "Guest execution: not entered; metadata-only FDT platform description" \
    || { printf 'missing metadata-only FDT boundary'; return 1; }
  contains_text "$combined" "Format: FDT" \
    || { printf 'missing FDT format'; return 1; }
  contains_text "$combined" "FDT magic: 0xd00dfeed" \
    || { printf 'missing FDT magic'; return 1; }
  contains_text "$combined" "FDT magic verified: true" \
    || { printf 'FDT magic was not verified'; return 1; }
  contains_text "$combined" "Memory node base: 0x40000000" \
    || { printf 'missing Windows Arm guest RAM memory node'; return 1; }
  contains_text "$combined" "Requested CPU count: 6" \
    || { printf 'missing requested CPU count'; return 1; }
  contains_text "$combined" "CPU count: 6" \
    || { printf 'missing CPU count'; return 1; }
  contains_text "$combined" "CPU count verified: true" \
    || { printf 'CPU count was not verified'; return 1; }
  contains_text "$combined" "Device MMIO window: 0x10000000..0x20000000" \
    || { printf 'missing Windows device MMIO window'; return 1; }
  contains_text "$combined" "PL011/PL031/VirtIO-MMIO installer ISO/target disk nodes inside device window: true" \
    || { printf 'missing device-window node verification'; return 1; }
  contains_text "$combined" "PL011 node: serial@10000000" \
    || { printf 'missing PL011 FDT node'; return 1; }
  contains_text "$combined" "PL031 node: rtc@10001000" \
    || { printf 'missing PL031 FDT node'; return 1; }
  contains_text "$combined" "VirtIO-MMIO installer ISO node: virtio_mmio@10002000" \
    || { printf 'missing installer ISO VirtIO-MMIO FDT node'; return 1; }
  contains_text "$combined" "VirtIO-MMIO installer ISO node base: 0x10002000" \
    || { printf 'missing installer ISO VirtIO-MMIO FDT node base'; return 1; }
  contains_text "$combined" "VirtIO-MMIO installer ISO node bytes: 0x1000" \
    || { printf 'missing installer ISO VirtIO-MMIO FDT node window size'; return 1; }
  contains_text "$combined" "VirtIO-MMIO target disk node: virtio_mmio@10003000" \
    || { printf 'missing target disk VirtIO-MMIO FDT node'; return 1; }
  contains_text "$combined" "VirtIO-MMIO target disk node base: 0x10003000" \
    || { printf 'missing target disk VirtIO-MMIO FDT node base'; return 1; }
  contains_text "$combined" "VirtIO-MMIO target disk node bytes: 0x1000" \
    || { printf 'missing target disk VirtIO-MMIO FDT node window size'; return 1; }
  contains_text "$combined" "Root interrupt-parent: 0x1" \
    || { printf 'missing FDT root interrupt-parent'; return 1; }
  contains_text "$combined" "GIC phandle: 0x1" \
    || { printf 'missing FDT GIC phandle'; return 1; }
  contains_text "$combined" "GIC distributor base: 0x10010000" \
    || { printf 'missing FDT GIC distributor base'; return 1; }
  contains_text "$combined" "GIC distributor bytes: 0x10000" \
    || { printf 'missing FDT GIC distributor window size'; return 1; }
  contains_text "$combined" "GIC redistributor base: 0x10020000" \
    || { printf 'missing FDT GIC redistributor base'; return 1; }
  contains_text "$combined" "GIC redistributor bytes: 0xc0000" \
    || { printf 'missing FDT GIC redistributor window size'; return 1; }
  contains_text "$combined" "GIC nodes inside device window: true" \
    || { printf 'FDT GIC nodes were not verified inside device window'; return 1; }
  contains_text "$combined" "ARM arch timer node present: true" \
    || { printf 'missing FDT ARM arch timer node'; return 1; }
  contains_text "$combined" "ARM arch timer interrupt count: 4" \
    || { printf 'missing FDT ARM arch timer interrupt count'; return 1; }
  contains_text "$combined" "Interrupt nodes described: true" \
    || { printf 'FDT device interrupt nodes were not all described'; return 1; }
  contains_text "$combined" "PL011 interrupt number: 0x0" \
    || { printf 'missing PL011 FDT SPI interrupt number'; return 1; }
  contains_text "$combined" "PL031 interrupt number: 0x1" \
    || { printf 'missing PL031 FDT SPI interrupt number'; return 1; }
  contains_text "$combined" "VirtIO-MMIO installer ISO interrupt number: 0x2" \
    || { printf 'missing installer ISO VirtIO-MMIO FDT SPI interrupt number'; return 1; }
  contains_text "$combined" "VirtIO-MMIO target disk interrupt number: 0x3" \
    || { printf 'missing target disk VirtIO-MMIO FDT SPI interrupt number'; return 1; }
  contains_text "$combined" "ACPI: not implemented" \
    || { printf 'platform-description probe claimed ACPI'; return 1; }
  contains_text "$combined" "fw_cfg: not used" \
    || { printf 'platform-description probe used fw_cfg'; return 1; }
  contains_text "$combined" "GIC: described/not emulated" \
    || { printf 'missing GIC described/not emulated status'; return 1; }
  contains_text "$combined" "GIC emulated: false" \
    || { printf 'platform-description probe claimed GIC emulation'; return 1; }
  contains_text "$combined" "Blockers: none" \
    || { printf 'platform-description metadata unexpectedly has blockers'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in platform-description output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in platform-description output'; return 1; }

  return 0
}

hvf_vm_probe_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-vm-probe.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF VM probe product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    cargo run -q -p bridgevm-cli -- hvf vm-probe 2>&1
  )" || {
    printf 'bridgevm hvf vm-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    cargo run -q -p hvf-runner -- --vm-probe 2>&1
  )" || {
    printf 'hvf-runner --vm-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "HVF VM create/destroy probe" \
    || { printf 'missing VM probe title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default VM probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Attempted: false" \
    || { printf 'default VM probe attempted to create a VM'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in HVF VM probe output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in HVF VM probe output'; return 1; }

  return 0
}

hvf_vcpu_probe_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-vcpu-probe.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF vCPU probe product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    cargo run -q -p bridgevm-cli -- hvf vcpu-probe 2>&1
  )" || {
    printf 'bridgevm hvf vcpu-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    cargo run -q -p hvf-runner -- --vcpu-probe 2>&1
  )" || {
    printf 'hvf-runner --vcpu-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "HVF vCPU create/destroy probe" \
    || { printf 'missing vCPU probe title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default vCPU probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Attempted: false" \
    || { printf 'default vCPU probe attempted to create a VM/vCPU'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in HVF vCPU probe output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in HVF vCPU probe output'; return 1; }

  return 0
}

hvf_vcpu_run_probe_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-vcpu-run-probe.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF vCPU run probe product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    cargo run -q -p bridgevm-cli -- hvf vcpu-run-probe 2>&1
  )" || {
    printf 'bridgevm hvf vcpu-run-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    cargo run -q -p hvf-runner -- --vcpu-run-probe 2>&1
  )" || {
    printf 'hvf-runner --vcpu-run-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "HVF vCPU run/cancel probe" \
    || { printf 'missing vCPU run probe title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Guest execution: pre-canceled before entry" \
    || { printf 'missing pre-canceled guest-execution boundary'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default vCPU run probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Run attempted: false" \
    || { printf 'default vCPU run probe attempted hv_vcpu_run'; return 1; }
  contains_text "$combined" "Run boundary observed: false" \
    || { printf 'default vCPU run probe reported a live run boundary'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in HVF vCPU run probe output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in HVF vCPU run probe output'; return 1; }

  return 0
}

hvf_interrupt_timer_probe_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-interrupt-timer-probe.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF interrupt/timer probe product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_INTERRUPT_TIMER= \
    cargo run -q -p bridgevm-cli -- hvf interrupt-timer-probe 2>&1
  )" || {
    printf 'bridgevm hvf interrupt-timer-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_INTERRUPT_TIMER= \
    cargo run -q -p hvf-runner -- --interrupt-timer-probe 2>&1
  )" || {
    printf 'hvf-runner --interrupt-timer-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "HVF interrupt/timer probe" \
    || { printf 'missing interrupt/timer probe title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Guest execution: not entered" \
    || { printf 'default interrupt/timer probe entered guest execution'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default interrupt/timer probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Pending IRQ set: false" \
    || { printf 'default interrupt/timer probe set pending IRQ'; return 1; }
  contains_text "$combined" "VTimer masked: false" \
    || { printf 'default interrupt/timer probe masked vtimer'; return 1; }
  contains_text "$combined" "VTimer offset requested: 0x1000" \
    || { printf 'missing vtimer offset metadata'; return 1; }
  contains_text "$combined" "Interrupt/timer boundary observed: false" \
    || { printf 'default interrupt/timer probe reported a live boundary'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in HVF interrupt/timer probe output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in HVF interrupt/timer probe output'; return 1; }

  return 0
}

hvf_vtimer_exit_probe_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-vtimer-exit-probe.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF VTimer exit probe product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VTIMER_EXIT= \
    cargo run -q -p bridgevm-cli -- hvf vtimer-exit-probe 2>&1
  )" || {
    printf 'bridgevm hvf vtimer-exit-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VTIMER_EXIT= \
    cargo run -q -p hvf-runner -- --vtimer-exit-probe 2>&1
  )" || {
    printf 'hvf-runner --vtimer-exit-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "HVF VTimer exit probe" \
    || { printf 'missing VTimer exit probe title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Guest execution: WFI wait loop with host-programmed virtual timer" \
    || { printf 'missing VTimer WFI guest-execution boundary'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default VTimer exit probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Attempted: false" \
    || { printf 'default VTimer exit probe attempted live HVF entry'; return 1; }
  contains_text "$combined" "Memory mapped: false" \
    || { printf 'default VTimer exit probe mapped guest memory'; return 1; }
  contains_text "$combined" "vCPU created: false" \
    || { printf 'default VTimer exit probe created a vCPU'; return 1; }
  contains_text "$combined" "VTimer offset set: false" \
    || { printf 'default VTimer exit probe set the VTimer offset'; return 1; }
  contains_text "$combined" "CNTV_CVAL_EL0 set: false" \
    || { printf 'default VTimer exit probe set CNTV_CVAL_EL0'; return 1; }
  contains_text "$combined" "CNTV_CTL_EL0 set: false" \
    || { printf 'default VTimer exit probe set CNTV_CTL_EL0'; return 1; }
  contains_text "$combined" "VTimer unmasked: false" \
    || { printf 'default VTimer exit probe unmasked the VTimer'; return 1; }
  contains_text "$combined" "Run attempted: false" \
    || { printf 'default VTimer exit probe attempted hv_vcpu_run'; return 1; }
  contains_text "$combined" "VTimer exit observed: false" \
    || { printf 'default VTimer exit probe reported a live VTimer exit'; return 1; }
  contains_text "$combined" "Pending IRQ injected: false" \
    || { printf 'default VTimer exit probe injected a pending IRQ'; return 1; }
  contains_text "$combined" "VTimer mask observed after exit: not observed" \
    || { printf 'default VTimer exit probe observed a post-exit mask'; return 1; }
  contains_text "$combined" "Instructions: WFI; HVC #0" \
    || { printf 'missing VTimer WFI instruction metadata'; return 1; }
  contains_text "$combined" "CNTV_CTL_EL0 requested: 0x1" \
    || { printf 'missing VTimer control-register metadata'; return 1; }
  contains_text "$combined" "Exit reason name: not observed" \
    || { printf 'default VTimer exit probe reported an exit reason'; return 1; }
  contains_text "$combined" "set BRIDGEVM_HVF_ALLOW_VTIMER_EXIT=1 or pass --allow-vtimer-exit" \
    || { printf 'missing VTimer exit opt-in blocker'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in HVF VTimer exit output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in HVF VTimer exit output'; return 1; }

  return 0
}

hvf_memory_map_probe_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-memory-map-probe.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF memory map probe product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    cargo run -q -p bridgevm-cli -- hvf memory-map-probe 2>&1
  )" || {
    printf 'bridgevm hvf memory-map-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    cargo run -q -p hvf-runner -- --memory-map-probe 2>&1
  )" || {
    printf 'hvf-runner --memory-map-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "HVF memory map/unmap probe" \
    || { printf 'missing memory map probe title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Guest execution: not entered" \
    || { printf 'missing no-guest-execution boundary'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default memory map probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Memory mapped: false" \
    || { printf 'default memory map probe mapped memory'; return 1; }
  contains_text "$combined" "Guest IPA start: 0x40000000" \
    || { printf 'missing planned guest IPA'; return 1; }
  contains_text "$combined" "Bytes: 16384" \
    || { printf 'missing planned map size'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in HVF memory map probe output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in HVF memory map probe output'; return 1; }

  return 0
}

hvf_guest_entry_probe_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-guest-entry-probe.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF guest entry probe product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    BRIDGEVM_HVF_ALLOW_GUEST_ENTRY= \
    cargo run -q -p bridgevm-cli -- hvf guest-entry-probe 2>&1
  )" || {
    printf 'bridgevm hvf guest-entry-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    BRIDGEVM_HVF_ALLOW_GUEST_ENTRY= \
    cargo run -q -p hvf-runner -- --guest-entry-probe 2>&1
  )" || {
    printf 'hvf-runner --guest-entry-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "HVF guest entry probe" \
    || { printf 'missing guest entry probe title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Guest execution: one HVC instruction with watchdog" \
    || { printf 'missing HVC guest-execution boundary'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default guest entry probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Run attempted: false" \
    || { printf 'default guest entry probe attempted hv_vcpu_run'; return 1; }
  contains_text "$combined" "Entry boundary observed: false" \
    || { printf 'default guest entry probe reported live guest execution'; return 1; }
  contains_text "$combined" "Instruction: HVC #0" \
    || { printf 'missing guest instruction metadata'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in HVF guest entry probe output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in HVF guest entry probe output'; return 1; }

  return 0
}

hvf_guest_exit_loop_probe_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-guest-exit-loop-probe.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF guest exit loop probe product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    BRIDGEVM_HVF_ALLOW_GUEST_ENTRY= \
    BRIDGEVM_HVF_ALLOW_EXIT_LOOP= \
    cargo run -q -p bridgevm-cli -- hvf guest-exit-loop-probe 2>&1
  )" || {
    printf 'bridgevm hvf guest-exit-loop-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    BRIDGEVM_HVF_ALLOW_GUEST_ENTRY= \
    BRIDGEVM_HVF_ALLOW_EXIT_LOOP= \
    cargo run -q -p hvf-runner -- --guest-exit-loop-probe 2>&1
  )" || {
    printf 'hvf-runner --guest-exit-loop-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "HVF guest exit loop probe" \
    || { printf 'missing guest exit loop probe title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Guest execution: two HVC instructions with PC advance watchdog" \
    || { printf 'missing two-HVC guest-execution boundary'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default guest exit loop probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "First run attempted: false" \
    || { printf 'default guest exit loop probe attempted first hv_vcpu_run'; return 1; }
  contains_text "$combined" "Second run attempted: false" \
    || { printf 'default guest exit loop probe attempted second hv_vcpu_run'; return 1; }
  contains_text "$combined" "Exit loop observed: false" \
    || { printf 'default guest exit loop probe reported live exit loop'; return 1; }
  contains_text "$combined" "Instructions: HVC #0; HVC #1" \
    || { printf 'missing guest exit loop instruction metadata'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in HVF guest exit loop probe output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in HVF guest exit loop probe output'; return 1; }

  return 0
}

hvf_mmio_read_probe_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-mmio-read-probe.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF MMIO read probe product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    BRIDGEVM_HVF_ALLOW_GUEST_ENTRY= \
    BRIDGEVM_HVF_ALLOW_EXIT_LOOP= \
    BRIDGEVM_HVF_ALLOW_MMIO_READ= \
    cargo run -q -p bridgevm-cli -- hvf mmio-read-probe 2>&1
  )" || {
    printf 'bridgevm hvf mmio-read-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    BRIDGEVM_HVF_ALLOW_GUEST_ENTRY= \
    BRIDGEVM_HVF_ALLOW_EXIT_LOOP= \
    BRIDGEVM_HVF_ALLOW_MMIO_READ= \
    cargo run -q -p hvf-runner -- --mmio-read-probe 2>&1
  )" || {
    printf 'hvf-runner --mmio-read-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "HVF MMIO read exit probe" \
    || { printf 'missing MMIO read probe title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Guest execution: one unmapped LDR read with watchdog" \
    || { printf 'missing unmapped LDR guest-execution boundary'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default MMIO read probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Run attempted: false" \
    || { printf 'default MMIO read probe attempted hv_vcpu_run'; return 1; }
  contains_text "$combined" "MMIO exit observed: false" \
    || { printf 'default MMIO read probe reported live MMIO exit'; return 1; }
  contains_text "$combined" "MMIO IPA: 0x50000000" \
    || { printf 'missing MMIO IPA metadata'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in HVF MMIO read probe output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in HVF MMIO read probe output'; return 1; }

  return 0
}

hvf_mmio_read_emulation_probe_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-mmio-read-emulation-probe.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF MMIO read emulation probe product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    BRIDGEVM_HVF_ALLOW_GUEST_ENTRY= \
    BRIDGEVM_HVF_ALLOW_EXIT_LOOP= \
    BRIDGEVM_HVF_ALLOW_MMIO_READ= \
    BRIDGEVM_HVF_ALLOW_MMIO_EMULATION= \
    cargo run -q -p bridgevm-cli -- hvf mmio-read-emulation-probe 2>&1
  )" || {
    printf 'bridgevm hvf mmio-read-emulation-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    BRIDGEVM_HVF_ALLOW_GUEST_ENTRY= \
    BRIDGEVM_HVF_ALLOW_EXIT_LOOP= \
    BRIDGEVM_HVF_ALLOW_MMIO_READ= \
    BRIDGEVM_HVF_ALLOW_MMIO_EMULATION= \
    cargo run -q -p hvf-runner -- --mmio-read-emulation-probe 2>&1
  )" || {
    printf 'hvf-runner --mmio-read-emulation-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "HVF MMIO read emulation probe" \
    || { printf 'missing MMIO read emulation probe title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Guest execution: unmapped LDR, injected read value, then HVC" \
    || { printf 'missing MMIO emulation guest-execution boundary'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default MMIO read emulation probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Emulated value injected: false" \
    || { printf 'default MMIO read emulation probe injected a value'; return 1; }
  contains_text "$combined" "Continuation exit observed: false" \
    || { printf 'default MMIO read emulation probe reported continuation'; return 1; }
  contains_text "$combined" "Emulated value: 0x123456789abcdef0" \
    || { printf 'missing emulated value metadata'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in HVF MMIO read emulation output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in HVF MMIO read emulation output'; return 1; }

  return 0
}

hvf_mmio_write_emulation_probe_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-mmio-write-emulation-probe.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF MMIO write emulation probe product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    BRIDGEVM_HVF_ALLOW_GUEST_ENTRY= \
    BRIDGEVM_HVF_ALLOW_EXIT_LOOP= \
    BRIDGEVM_HVF_ALLOW_MMIO_READ= \
    BRIDGEVM_HVF_ALLOW_MMIO_EMULATION= \
    BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION= \
    cargo run -q -p bridgevm-cli -- hvf mmio-write-emulation-probe 2>&1
  )" || {
    printf 'bridgevm hvf mmio-write-emulation-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    BRIDGEVM_HVF_ALLOW_GUEST_ENTRY= \
    BRIDGEVM_HVF_ALLOW_EXIT_LOOP= \
    BRIDGEVM_HVF_ALLOW_MMIO_READ= \
    BRIDGEVM_HVF_ALLOW_MMIO_EMULATION= \
    BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION= \
    cargo run -q -p hvf-runner -- --mmio-write-emulation-probe 2>&1
  )" || {
    printf 'hvf-runner --mmio-write-emulation-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "HVF MMIO write emulation probe" \
    || { printf 'missing MMIO write emulation probe title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Guest execution: unmapped STR, captured write value, then HVC" \
    || { printf 'missing MMIO write emulation guest-execution boundary'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default MMIO write emulation probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Write value captured: false" \
    || { printf 'default MMIO write emulation probe captured a value'; return 1; }
  contains_text "$combined" "Continuation exit observed: false" \
    || { printf 'default MMIO write emulation probe reported continuation'; return 1; }
  contains_text "$combined" "Write value: 0xfedcba987654321" \
    || { printf 'missing write value metadata'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in HVF MMIO write emulation output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in HVF MMIO write emulation output'; return 1; }

  return 0
}

hvf_mmio_serial_device_probe_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-mmio-serial-device-probe.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF MMIO serial device probe product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    BRIDGEVM_HVF_ALLOW_GUEST_ENTRY= \
    BRIDGEVM_HVF_ALLOW_EXIT_LOOP= \
    BRIDGEVM_HVF_ALLOW_MMIO_READ= \
    BRIDGEVM_HVF_ALLOW_MMIO_EMULATION= \
    BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION= \
    BRIDGEVM_HVF_ALLOW_MMIO_SERIAL_DEVICE= \
    cargo run -q -p bridgevm-cli -- hvf mmio-serial-device-probe 2>&1
  )" || {
    printf 'bridgevm hvf mmio-serial-device-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    BRIDGEVM_HVF_ALLOW_GUEST_ENTRY= \
    BRIDGEVM_HVF_ALLOW_EXIT_LOOP= \
    BRIDGEVM_HVF_ALLOW_MMIO_READ= \
    BRIDGEVM_HVF_ALLOW_MMIO_EMULATION= \
    BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION= \
    BRIDGEVM_HVF_ALLOW_MMIO_SERIAL_DEVICE= \
    cargo run -q -p hvf-runner -- --mmio-serial-device-probe 2>&1
  )" || {
    printf 'hvf-runner --mmio-serial-device-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "HVF MMIO serial device probe" \
    || { printf 'missing MMIO serial device probe title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Guest execution: STR data register, LDR status register, then HVC" \
    || { printf 'missing MMIO serial device guest-execution boundary'; return 1; }
  contains_text "$combined" "Device model: PL011 UART skeleton" \
    || { printf 'missing PL011 UART device model metadata'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default MMIO serial device probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Device bus created: false" \
    || { printf 'default MMIO serial device probe created the device bus'; return 1; }
  contains_text "$combined" "Device bus device count: 0" \
    || { printf 'default MMIO serial device probe reported device bus devices'; return 1; }
  contains_text "$combined" "Write exit observed: false" \
    || { printf 'default MMIO serial device probe reported a write exit'; return 1; }
  contains_text "$combined" "Write handled by device: false" \
    || { printf 'default MMIO serial device probe handled a write through the device bus'; return 1; }
  contains_text "$combined" "Status exit observed: false" \
    || { printf 'default MMIO serial device probe reported a status exit'; return 1; }
  contains_text "$combined" "Status handled by device: false" \
    || { printf 'default MMIO serial device probe handled a status read through the device bus'; return 1; }
  contains_text "$combined" "Serial data IPA: 0x50000000" \
    || { printf 'missing serial data IPA metadata'; return 1; }
  contains_text "$combined" "Serial status IPA: 0x50000018" \
    || { printf 'missing serial status IPA metadata'; return 1; }
  contains_text "$combined" "Serial write value: 0x41" \
    || { printf 'missing serial write value metadata'; return 1; }
  contains_text "$combined" "Serial status value: 0x90" \
    || { printf 'missing serial status value metadata'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in HVF MMIO serial device output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in HVF MMIO serial device output'; return 1; }

  return 0
}

hvf_mmio_rtc_device_probe_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-hvf-mmio-rtc-device-probe.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF MMIO RTC device probe product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local cli_output=""
  local runner_output=""
  cli_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    BRIDGEVM_HVF_ALLOW_GUEST_ENTRY= \
    BRIDGEVM_HVF_ALLOW_EXIT_LOOP= \
    BRIDGEVM_HVF_ALLOW_MMIO_READ= \
    BRIDGEVM_HVF_ALLOW_MMIO_EMULATION= \
    BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION= \
    BRIDGEVM_HVF_ALLOW_MMIO_SERIAL_DEVICE= \
    BRIDGEVM_HVF_ALLOW_MMIO_RTC_DEVICE= \
    cargo run -q -p bridgevm-cli -- hvf mmio-rtc-device-probe 2>&1
  )" || {
    printf 'bridgevm hvf mmio-rtc-device-probe failed: %s' "$cli_output"
    return 1
  }

  runner_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    BRIDGEVM_HVF_ALLOW_VM_CREATE= \
    BRIDGEVM_HVF_ALLOW_VCPU_RUN= \
    BRIDGEVM_HVF_ALLOW_MEMORY_MAP= \
    BRIDGEVM_HVF_ALLOW_GUEST_ENTRY= \
    BRIDGEVM_HVF_ALLOW_EXIT_LOOP= \
    BRIDGEVM_HVF_ALLOW_MMIO_READ= \
    BRIDGEVM_HVF_ALLOW_MMIO_EMULATION= \
    BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION= \
    BRIDGEVM_HVF_ALLOW_MMIO_SERIAL_DEVICE= \
    BRIDGEVM_HVF_ALLOW_MMIO_RTC_DEVICE= \
    cargo run -q -p hvf-runner -- --mmio-rtc-device-probe 2>&1
  )" || {
    printf 'hvf-runner --mmio-rtc-device-probe failed: %s' "$runner_output"
    return 1
  }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi

  local combined="$cli_output"$'\n'"$runner_output"
  contains_text "$combined" "HVF MMIO RTC device probe" \
    || { printf 'missing MMIO RTC device probe title'; return 1; }
  contains_text "$combined" "QEMU: not used" \
    || { printf 'missing QEMU: not used'; return 1; }
  contains_text "$combined" "Apple VZ: not used" \
    || { printf 'missing Apple VZ: not used'; return 1; }
  contains_text "$combined" "Guest execution: LDR RTC data register, then HVC" \
    || { printf 'missing MMIO RTC device guest-execution boundary'; return 1; }
  contains_text "$combined" "Device models: PL011 UART skeleton; PL031 RTC skeleton" \
    || { printf 'missing PL011/PL031 device model metadata'; return 1; }
  contains_text "$combined" "Allowed: false" \
    || { printf 'default MMIO RTC device probe did not remain opt-in blocked'; return 1; }
  contains_text "$combined" "Device bus created: false" \
    || { printf 'default MMIO RTC device probe created the device bus'; return 1; }
  contains_text "$combined" "Device bus device count: 0" \
    || { printf 'default MMIO RTC device probe reported device bus devices'; return 1; }
  contains_text "$combined" "RTC exit observed: false" \
    || { printf 'default MMIO RTC device probe reported an RTC exit'; return 1; }
  contains_text "$combined" "RTC handled by device: false" \
    || { printf 'default MMIO RTC device probe handled a read through the device bus'; return 1; }
  contains_text "$combined" "UART IPA: 0x50000000" \
    || { printf 'missing UART IPA metadata'; return 1; }
  contains_text "$combined" "RTC IPA: 0x50001000" \
    || { printf 'missing RTC IPA metadata'; return 1; }
  contains_text "$combined" "RTC value: 0x20260618" \
    || { printf 'missing RTC value metadata'; return 1; }
  ! contains_text "$combined" "qemu-system" \
    || { printf 'reported qemu-system in HVF MMIO RTC device output'; return 1; }
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in HVF MMIO RTC device output'; return 1; }

  return 0
}

hvf_mmio_block_device_probe_metadata() {
  local cli_output=""
  local runner_output=""

  cli_output="$("$ROOT/tests/integration/hvf-mmio-block-device-probe-cli-smoke.sh" 2>&1)" || {
    printf 'HVF MMIO block device CLI smoke failed: %s' "$cli_output"
    return 1
  }

  runner_output="$("$ROOT/tests/integration/hvf-mmio-block-device-probe-runner-smoke.sh" 2>&1)" || {
    printf 'HVF MMIO block device runner smoke failed: %s' "$runner_output"
    return 1
  }

  return 0
}

hvf_mmio_block_queue_probe_metadata() {
  local cli_output=""
  local runner_output=""

  cli_output="$("$ROOT/tests/integration/hvf-mmio-block-queue-probe-cli-smoke.sh" 2>&1)" || {
    printf 'HVF MMIO block queue CLI smoke failed: %s' "$cli_output"
    return 1
  }

  runner_output="$("$ROOT/tests/integration/hvf-mmio-block-queue-probe-runner-smoke.sh" 2>&1)" || {
    printf 'HVF MMIO block queue runner smoke failed: %s' "$runner_output"
    return 1
  }

  return 0
}

hvf_virtio_block_request_model_metadata() {
  local cli_output=""
  local runner_output=""

  cli_output="$("$ROOT/tests/integration/hvf-virtio-block-request-model-cli-smoke.sh" 2>&1)" || {
    printf 'VirtIO block request model CLI smoke failed: %s' "$cli_output"
    return 1
  }

  runner_output="$("$ROOT/tests/integration/hvf-virtio-block-request-model-runner-smoke.sh" 2>&1)" || {
    printf 'VirtIO block request model runner smoke failed: %s' "$runner_output"
    return 1
  }

  return 0
}

hvf_virtio_block_file_backing_metadata() {
  local cli_output=""
  local runner_output=""

  cli_output="$("$ROOT/tests/integration/hvf-virtio-block-file-backing-cli-smoke.sh" 2>&1)" || {
    printf 'VirtIO block file backing CLI smoke failed: %s' "$cli_output"
    return 1
  }

  runner_output="$("$ROOT/tests/integration/hvf-virtio-block-file-backing-runner-smoke.sh" 2>&1)" || {
    printf 'VirtIO block file backing runner smoke failed: %s' "$runner_output"
    return 1
  }

  return 0
}

hvf_virtio_block_writable_file_backing_metadata() {
  local cli_output=""
  local runner_output=""

  cli_output="$("$ROOT/tests/integration/hvf-virtio-block-writable-file-backing-cli-smoke.sh" 2>&1)" || {
    printf 'VirtIO block writable file backing CLI smoke failed: %s' "$cli_output"
    return 1
  }

  runner_output="$("$ROOT/tests/integration/hvf-virtio-block-writable-file-backing-runner-smoke.sh" 2>&1)" || {
    printf 'VirtIO block writable file backing runner smoke failed: %s' "$runner_output"
    return 1
  }

  return 0
}

hvf_virtio_block_iso_backing_metadata() {
  local cli_output=""
  local runner_output=""

  cli_output="$("$ROOT/tests/integration/hvf-virtio-block-iso-backing-cli-smoke.sh" 2>&1)" || {
    printf 'VirtIO block ISO backing CLI smoke failed: %s' "$cli_output"
    return 1
  }

  runner_output="$("$ROOT/tests/integration/hvf-virtio-block-iso-backing-runner-smoke.sh" 2>&1)" || {
    printf 'VirtIO block ISO backing runner smoke failed: %s' "$runner_output"
    return 1
  }

  return 0
}

apple_vz_linux_template_stage_metadata() {
  local store
  store="$(mktemp -d "/tmp/bridgevm-product-vz-linux-template.XXXXXX")"
  local fake_bin="$store/bin"
  local backend_log="$store/backend-launch.log"
  local vm="product-vz-linux"
  local template_id="debian-arm64-apple-vz-linux-kernel-raw"
  local ubuntu_vm="product-ubuntu-vz-linux"
  local ubuntu_template_id="ubuntu-arm64-apple-vz-linux-kernel-raw"
  local bundle="$store/vms/$vm.vmbridge"
  local ubuntu_bundle="$store/vms/$ubuntu_vm.vmbridge"
  local launch_spec="$bundle/metadata/apple-vz-launch.json"
  local ubuntu_launch_spec="$ubuntu_bundle/metadata/apple-vz-launch.json"
  local runner_metadata="$bundle/metadata/runner.json"
  local ubuntu_runner_metadata="$ubuntu_bundle/metadata/runner.json"
  mkdir -p "$fake_bin"

  local backend
  for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
    cat >"$fake_bin/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Apple VZ Linux template product gate: $(basename "$0")" >&2
exit 99
SH
    chmod +x "$fake_bin/$backend"
  done

  local template_output=""
  template_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p bridgevm-cli -- --store "$store" templates 2>&1
  )" || {
    printf 'bridgevm templates failed: %s' "$template_output"
    return 1
  }

  contains_text "$template_output" "Boot template id: $template_id" \
    || { printf 'missing Apple VZ Linux template id'; return 1; }
  contains_text "$template_output" "Boot template id: $ubuntu_template_id" \
    || { printf 'missing Ubuntu Apple VZ Linux template id'; return 1; }
  contains_text "$template_output" "Boot template: linux-kernel" \
    || { printf 'missing linux-kernel template mode'; return 1; }
  contains_text "$template_output" "Primary disk path: disks/root.raw" \
    || { printf 'missing raw disk template path'; return 1; }
  contains_text "$template_output" "Primary disk format: raw" \
    || { printf 'missing raw disk template format'; return 1; }
  contains_text "$template_output" "Primary disk size: 32GiB" \
    || { printf 'missing Ubuntu raw disk template size'; return 1; }

  local create_output=""
  create_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p bridgevm-cli -- --store "$store" create "$vm" --template "$template_id" 2>&1
  )" || {
    printf 'bridgevm create from Apple VZ Linux template failed: %s' "$create_output"
    return 1
  }
  contains_text "$create_output" "Created fast VM" \
    || { printf 'template create did not create a Fast VM'; return 1; }

  local ubuntu_create_output=""
  ubuntu_create_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p bridgevm-cli -- --store "$store" create "$ubuntu_vm" --template "$ubuntu_template_id" 2>&1
  )" || {
    printf 'bridgevm create from Ubuntu Apple VZ Linux template failed: %s' "$ubuntu_create_output"
    return 1
  }
  contains_text "$ubuntu_create_output" "Created fast VM" \
    || { printf 'Ubuntu template create did not create a Fast VM'; return 1; }

  mkdir -p "$bundle/boot" "$bundle/disks"
  printf 'fake arm64 linux kernel fixture\n' >"$bundle/boot/vmlinuz"
  printf 'fake initrd fixture\n' >"$bundle/boot/initrd"
  truncate -s 1M "$bundle/disks/root.raw"
  mkdir -p "$ubuntu_bundle/boot" "$ubuntu_bundle/disks"
  printf 'fake Ubuntu arm64 linux kernel fixture\n' >"$ubuntu_bundle/boot/vmlinuz"
  printf 'fake Ubuntu initrd fixture\n' >"$ubuntu_bundle/boot/initrd"
  truncate -s 1M "$ubuntu_bundle/disks/root.raw"

  local prepare_output=""
  prepare_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p bridgevm-cli -- --store "$store" prepare-run "$vm" 2>&1
  )" || {
    printf 'bridgevm prepare-run for Apple VZ Linux template failed: %s' "$prepare_output"
    return 1
  }
  local ubuntu_prepare_output=""
  ubuntu_prepare_output="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p bridgevm-cli -- --store "$store" prepare-run "$ubuntu_vm" 2>&1
  )" || {
    printf 'bridgevm prepare-run for Ubuntu Apple VZ Linux template failed: %s' "$ubuntu_prepare_output"
    return 1
  }

  local runner_status=""
  runner_status="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p bridgevm-cli -- --store "$store" runner-status "$vm" 2>&1
  )" || {
    printf 'bridgevm runner-status for Apple VZ Linux template failed: %s' "$runner_status"
    return 1
  }
  local ubuntu_runner_status=""
  ubuntu_runner_status="$(
    PATH="$fake_bin:$PATH" \
    BRIDGEVM_FAKE_BACKEND_LOG="$backend_log" \
    BRIDGEVM_APPLE_VZ_RUNNER="$fake_bin/AppleVzRunner" \
    cargo run -q -p bridgevm-cli -- --store "$store" runner-status "$ubuntu_vm" 2>&1
  )" || {
    printf 'bridgevm runner-status for Ubuntu Apple VZ Linux template failed: %s' "$ubuntu_runner_status"
    return 1
  }

  local combined="$template_output"$'\n'"$create_output"$'\n'"$ubuntu_create_output"$'\n'"$prepare_output"$'\n'"$ubuntu_prepare_output"$'\n'"$runner_status"$'\n'"$ubuntu_runner_status"
  contains_text "$combined" "Launch ready: true" \
    || { printf 'missing launch-ready status for Apple VZ Linux template'; return 1; }
  contains_text "$combined" "Command: lightvm-runner --launch-spec $launch_spec" \
    || { printf 'missing launch-spec runner command'; return 1; }
  contains_text "$combined" "Command: lightvm-runner --launch-spec $ubuntu_launch_spec" \
    || { printf 'missing Ubuntu launch-spec runner command'; return 1; }
  contains_text "$combined" "Disk format: raw" \
    || { printf 'missing raw disk readiness evidence'; return 1; }
  contains_text "$combined" "console=hvc0 root=/dev/vda2 rw systemd.unit=graphical.target" \
    || { printf 'missing Ubuntu desktop-target kernel command line'; return 1; }
  [[ -f "$launch_spec" ]] || { printf 'launch spec was not written'; return 1; }
  [[ -f "$runner_metadata" ]] || { printf 'runner metadata was not written'; return 1; }
  [[ -f "$ubuntu_launch_spec" ]] || { printf 'Ubuntu launch spec was not written'; return 1; }
  [[ -f "$ubuntu_runner_metadata" ]] || { printf 'Ubuntu runner metadata was not written'; return 1; }
  grep -Fq '"mode": "linux-kernel"' "$launch_spec" \
    || { printf 'launch spec omitted linux-kernel mode'; return 1; }
  grep -Fq '"format": "raw"' "$launch_spec" \
    || { printf 'launch spec omitted raw disk format'; return 1; }
  grep -Fq '"ready": true' "$launch_spec" \
    || { printf 'launch spec was not ready'; return 1; }
  grep -Fq '"os": "ubuntu"' "$ubuntu_launch_spec" \
    || { printf 'Ubuntu launch spec omitted guest OS'; return 1; }
  grep -Fq '"mode": "linux-kernel"' "$ubuntu_launch_spec" \
    || { printf 'Ubuntu launch spec omitted linux-kernel mode'; return 1; }
  grep -Fq '"format": "raw"' "$ubuntu_launch_spec" \
    || { printf 'Ubuntu launch spec omitted raw disk format'; return 1; }
  grep -Fq '"ready": true' "$ubuntu_launch_spec" \
    || { printf 'Ubuntu launch spec was not ready'; return 1; }
  grep -Fq 'systemd.unit=graphical.target' "$ubuntu_launch_spec" \
    || { printf 'Ubuntu launch spec omitted graphical target command line'; return 1; }
  grep -Fq '"--launch-spec"' "$runner_metadata" \
    || { printf 'runner metadata omitted launch-spec command'; return 1; }
  grep -Fq '"--launch-spec"' "$ubuntu_runner_metadata" \
    || { printf 'Ubuntu runner metadata omitted launch-spec command'; return 1; }

  if [[ -s "$backend_log" ]]; then
    printf 'backend or GUI launch attempted: %s' "$(cat "$backend_log")"
    return 1
  fi
  ! grep -Eq '[0-9]+([.][0-9]+)?%' <<<"$combined" \
    || { printf 'reported forbidden percentage in Apple VZ Linux template output'; return 1; }

  return 0
}

ubuntu_cloudimg_prepare_metadata() {
  local output
  output="$("$ROOT/tests/integration/vz-ubuntu-cloudimg-prepare-smoke.sh" 2>&1)" || {
    printf 'Ubuntu cloudimg Apple VZ prepare smoke failed: %s' "$output"
    return 1
  }
  contains_text "$output" "PASS: Ubuntu cloudimg Apple VZ prepare smoke" \
    || { printf 'missing Ubuntu cloudimg prepare smoke pass marker'; return 1; }
  return 0
}

ubuntu_boot_artifacts_prepare_metadata() {
  local output
  output="$("$ROOT/tests/integration/vz-ubuntu-boot-artifacts-prep-smoke.sh" 2>&1)" || {
    printf 'Ubuntu boot-artifacts Apple VZ prep smoke failed: %s' "$output"
    return 1
  }
  contains_text "$output" "PASS: Ubuntu boot-artifacts Apple VZ prep smoke" \
    || { printf 'missing Ubuntu boot-artifacts prep smoke pass marker'; return 1; }
  return 0
}

live_guest_tools_blockers() {
  local blockers=()
  [[ "${BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START:-}" == "1" ]] \
    || blockers+=("BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START=1 not set")
  [[ -n "${BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK:-}" ]] \
    || blockers+=("BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK not set")
  if [[ -n "${BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK:-}" && ! -f "$BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK" ]]; then
    blockers+=("qcow2 disk missing: $BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK")
  fi
  [[ -x "$LIVE_AGENT" ]] || blockers+=("cross-compiled bridgevm-tools-linux missing: $LIVE_AGENT")

  local joined=""
  local blocker
  for blocker in "${blockers[@]}"; do
    if [[ -z "$joined" ]]; then
      joined="$blocker"
    else
      joined="$joined; $blocker"
    fi
  done
  printf '%s' "$joined"
}

verify_vz_proxy_crop_evidence() {
  [[ -d "$EVIDENCE_DIR" ]] || return 1
  "$ROOT/tests/integration/verify-vz-proxy-crop-evidence.sh" "$EVIDENCE_DIR" >/dev/null 2>&1
}

main() {
  local output=""
  local line=""

  line="BridgeVM product gates report"
  output+="$line"$'\n'
  line="No percentage estimate: this report distinguishes locally verified metadata from explicitly labelled preserved live evidence."
  output+="$line"$'\n'

  if verify_vz_proxy_crop_evidence; then
    line="$(status_line "PASS" "Fast/VZ GUI display" "preserved VZVirtualMachineView capture and app-direct framebuffer crop evidence verifies: $EVIDENCE_DIR")"
  else
    line="$(status_line "BLOCKED" "Fast/VZ GUI display" "no verifier-accepted VZ proxy crop evidence at: $EVIDENCE_DIR")"
  fi
  output+="$line"$'\n'

  local apple_vz_linux_template_detail
  if apple_vz_linux_template_detail="$(apple_vz_linux_template_stage_metadata)"; then
    line="$(status_line "PASS" "Apple VZ Linux template/staging" "debian-arm64-apple-vz-linux-kernel-raw and ubuntu-arm64-apple-vz-linux-kernel-raw create Fast Mode linux-kernel/raw-disk VMs, fake fixtures can be staged into real bundles, prepare-run writes metadata/apple-vz-launch.json with Launch ready: true, Ubuntu carries a desktop-target graphical kernel command line, and no backend or GUI process is spawned.")"
  else
    line="$(status_line "BLOCKED" "Apple VZ Linux template/staging" "$apple_vz_linux_template_detail")"
  fi
  output+="$line"$'\n'

  local ubuntu_cloudimg_prepare_detail
  if ubuntu_cloudimg_prepare_detail="$(ubuntu_cloudimg_prepare_metadata)"; then
    line="$(status_line "PASS" "Ubuntu cloudimg Apple VZ preparation" "prepare-vz-ubuntu-cloudimg-fixture.sh produces vmlinuz, initrd, root.raw, and artifacts.json for a whole-disk ext4 root=/dev/vda Apple VZ linux-kernel/raw handoff; the resulting fixture stages to Launch ready: true without QEMU runtime, AppleVzRunner live start, or GUI spawn.")"
  else
    line="$(status_line "BLOCKED" "Ubuntu cloudimg Apple VZ preparation" "$ubuntu_cloudimg_prepare_detail")"
  fi
  output+="$line"$'\n'

  local ubuntu_boot_artifacts_prepare_detail
  if ubuntu_boot_artifacts_prepare_detail="$(ubuntu_boot_artifacts_prepare_metadata)"; then
    line="$(status_line "PASS" "Ubuntu qcow2-to-Apple-VZ artifact preparation" "prepare-vz-ubuntu-arm64-boot-artifacts.sh records qemu-img as offline inspection/conversion only, converts qcow2 to raw, extracts kernel/initrd metadata from the same Ubuntu root filesystem through the docker-offline boundary, derives a root=UUID kernel command line instead of assuming /dev/vda2, writes artifacts.json, and stages to Launch ready: true without qemu-system, AppleVzRunner live start, or GUI spawn.")"
  else
    line="$(status_line "BLOCKED" "Ubuntu qcow2-to-Apple-VZ artifact preparation" "$ubuntu_boot_artifacts_prepare_detail")"
  fi
  output+="$line"$'\n'

  if [[ -x "$ROOT/tests/integration/guest-tools-app-window-real-backend-cli-smoke.sh" && -x "$ROOT/tests/integration/guest-tools-app-window-live-gui-opt-in-smoke.sh" ]]; then
    line="$(status_line "PARTIAL" "Coherence-lite" "Linux app/window metadata, host proxy crop plumbing, and opt-in live GUI harness are wired; true live per-window compositor is not proven.")"
  else
    line="$(status_line "BLOCKED" "Coherence-lite" "app/window smoke or live GUI harness is missing.")"
  fi
  output+="$line"$'\n'

  local live_blockers
  live_blockers="$(live_guest_tools_blockers)"
  if [[ -z "$live_blockers" ]]; then
    line="$(status_line "READY" "Ubuntu GUI live opt-in" "all local prerequisites are present; run guest-tools-app-window-live-gui-opt-in-smoke.sh to produce fresh live evidence.")"
  else
    line="$(status_line "BLOCKED" "Ubuntu GUI live opt-in" "$live_blockers")"
  fi
  output+="$line"$'\n'

  line="$(status_line "PARTIAL" "Networking" "NAT and live guest package-install paths have opt-in evidence, but this report did not boot a guest or prove current internet reachability.")"
  output+="$line"$'\n'

  line="$(status_line "RESEARCH" "Windows no-QEMU fast path" "The BridgeVM-owned HVF VMM has preserved live evidence for an installed Windows 11 ARM64 desktop, four vCPUs, virtio-net connectivity, and the resident service channel without QEMU. This metadata report does not rerun that mutable-media proof. Product readiness remains blocked on a trustworthy post-change performance matrix, clean shutdown/flush and suspend/resume proof, packaged setup, and a distributable ARM64 Windows 3D driver.")"
  output+="$line"$'\n'

  line="$(status_line "PARTIAL" "Windows HVF VMM" "BridgeVM now owns the QEMU-free Hypervisor.framework VMM/device stack and has preserved live proof of installed Windows 11 ARM64 reaching the desktop with four logical processors, virtio-net DHCP/DNS/HTTP/ICMP connectivity, persistent NVMe writes, framebuffer/display input, and a resident virtio-console service channel. This metadata-only report does not rerun mutable Windows media. The current engineering gates are an agent-oracle smp=1/2/4 BOOT_TIMER matrix, clean shutdown/flush and suspend/resume proof, turnkey packaging, a UMD-registered viogpu3d render candidate, and then live bind/trace proof.")"
  output+="$line"$'\n'

  line="$(status_line "PARTIAL" "Windows ARM64 viogpu3d artifact" "Preserved local CI artifacts include test-signed ARM64 INF/SYS/CAT packages for Venus and VirGL, plus a VirGL full package with five ARM64 Mesa DLLs. They are injection-ready inventory, not equivalent render candidates: three are KMD-only, while the five-DLL package copies its UMD payload but omits UserModeDriverName, OpenGLDriverName, OpenGLVersion, OpenGLFlags, and InstalledDisplayDrivers INF registrations. This metadata report does not rescan those out-of-tree artifacts, and test signing is not a distributable production signature.")"
  output+="$line"$'\n'

  line="$(status_line "BLOCKED" "Windows ARM64 viogpu3d render package" "No preserved package currently satisfies the repository's render-candidate gate. Regenerate the VirGL full package from an INF with active UserModeDriverName, OpenGLDriverName, OpenGLVersion, OpenGLFlags, and InstalledDisplayDrivers registrations whose DLL names resolve through active CopyFiles entries into DirID 11, then rebuild its catalog and test signature; do not edit the signed out-of-tree INF in place.")"
  output+="$line"$'\n'

  line="$(status_line "BLOCKED" "Windows ARM64 viogpu3d live bind/trace" "After a UMD-registered package exists, no verifier-accepted live receipt yet proves certificate trust and testsigning, pnputil install, a present DEV_1050/10F7 device with Status OK bound to the intended OEM INF, or a boot-bound coherent capset/blob/context/submit/fence trace.")"
  output+="$line"$'\n'

  line="$(status_line "RESEARCH" "Windows HVF low-vector resume telemetry" "Preserved opt-in live smoke output records low-vector repair-and-resume-once telemetry: the VMM patches the low-vector L3 descriptor at entry IPA 0xc000 from previous descriptor 0x0 to descriptor=0xf8f, records whether a repeated low-vector fault is observed, captures original PC/ELR_EL1/ESR_EL1/FAR_EL1/SPSR_EL1 from the fault, and reaches the low-vector HVC/ERET landing path. The continue-after-low-vector-repair option now has a metadata-safe no-opt-in smoke that records Continue after low-vector repair requested: true, Continue after low-vector repair attempted: false, and post-repair unsupported exit reason/classification plus first-device-interaction and first-unhandled-access telemetry remain not observed without live opt-in; in the default live path it keeps the diagnostic page patched and resumes through captured ELR_EL1/SPSR_EL1 plus the diagnostic ERET, while the separate --restore-low-vector-slot-before-eret opt-in restores the original low-vector bytes through an executable ERET trampoline before proving the target is erased-pflash-execution rather than another synthetic diagnostic-page stop. The low-vector classifier now requires instruction_word_after_exit == AARCH64_ERET for the synthetic low-vector HVC diagnosis, so restored 0xffffffff low-vector bytes surface as erased-pflash-execution. This is explicit repair/resume telemetry only; it is not UEFI Boot Manager handoff, installer boot, Windows boot, GUI, network, TPM, Secure Boot, or a usable Windows fast path.")"
  output+="$line"$'\n'

  line="$(status_line "RESEARCH" "Windows HVF GICv3 skeleton" "GICv3 distributor/redistributor MMIO register skeleton is wired to the Windows firmware bus for common firmware accesses, including status and group modifier registers, and the firmware run-loop now has a single-vCPU Group1 ICC_* CPU-interface sysreg skeleton for ICC_SRE_EL1, ICC_CTLR_EL1, ICC_PMR_EL1, ICC_BPR1_EL1, ICC_IGRPEN1_EL1, ICC_HPPIR1_EL1, ICC_IAR1_EL1, ICC_EOIR1_EL1, and ICC_DIR_EL1, plus conservative firmware-tolerant ICC_BPR0_EL1, ICC_IGRPEN0_EL1, ICC_RPR_EL1, ICC_AP0R*/ICC_AP1R*, Group0 spurious, and ICC_SGI1R_EL1 stubs. The skeleton can choose the highest-priority pending enabled Group1 interrupt across redistributor PPI and distributor SPI candidates through ICC_HPPIR1_EL1/ICC_IAR1_EL1 after GICD EnableGrp1NS plus GICD/GICR IGROUPR Group1 bits are set, gate delivery on the PMR/current-running-priority threshold, move it active, treat ICC_EOIR1_EL1 as priority drop plus deactivate when ICC_CTLR_EL1 EOImode is clear, require ICC_DIR_EL1 for deactivate when EOImode is set, refresh/re-pend level VirtIO sources after actual deactivation, and route the virtual timer PPI 11 / INTID 27 through the same single-vCPU GIC CPU-interface path. The run-loop report also emits handled MMIO read/write counts, per-device MMIO counts, virtio queue_notify/request completion counts, handled ICC read/write counts, per-ICC_IAR1/ICC_EOIR1/ICC_DIR counts, and last ICC_IAR1/ICC_EOIR1/ICC_DIR INTIDs. Full GIC delivery beyond the minimal single-vCPU SPI/PPI paths, complete nested preemption, binary-point/List Register behavior, multi-vCPU routing, complete deactivation-stack semantics, and complete GIC emulation are not complete.")"
  output+="$line"$'\n'

  local hvf_machine_detail
  if hvf_machine_detail="$(windows_hvf_machine_plan_metadata)"; then
    line="$(status_line "PASS" "Windows HVF machine-plan metadata" "bridgevm hvf machine-plan and hvf-runner --machine-plan report QEMU-free machine/readiness metadata without spawning a backend.")"
  else
    line="$(status_line "BLOCKED" "Windows HVF machine-plan metadata" "$hvf_machine_detail")"
  fi
  output+="$line"$'\n'

  local hvf_boot_disk_layout_detail
  if hvf_boot_disk_layout_detail="$(windows_hvf_boot_disk_layout_metadata)"; then
    line="$(status_line "PASS" "Windows HVF boot-disk layout boundary" "bridgevm hvf windows-boot-disk-layout-probe and hvf-runner --windows-boot-disk-layout-probe create sparse raw Windows Arm target disks, reopen them, and verify protective MBR, primary/backup GPT, ESP, MSR, and Windows Basic Data partitions without QEMU, Apple VZ, GUI launch, or HVF entry.")"
  else
    line="$(status_line "BLOCKED" "Windows HVF boot-disk layout boundary" "$hvf_boot_disk_layout_detail")"
  fi
  output+="$line"$'\n'

  local hvf_xhci_hid_boot_key_detail
  if hvf_xhci_hid_boot_key_detail="$(windows_hvf_xhci_hid_boot_key_metadata)"; then
    line="$(status_line "PASS" "Windows HVF xHCI HID boot-key report boundary" "bridgevm hvf windows-xhci-hid-boot-key-probe and hvf-runner --windows-xhci-hid-boot-key-probe prove queued USB boot-keyboard Space [00 00 2c 00 00 00 00 00] plus release [00 00 00 00 00 00 00 00] over BridgeVM-owned xHCI DCI3 without QEMU, Apple VZ, GUI launch, or HVF entry. This isolated metadata boundary does not itself claim Windows boot; separate installed-target live evidence covers the later desktop path.")"
  else
    line="$(status_line "BLOCKED" "Windows HVF xHCI HID boot-key report boundary" "$hvf_xhci_hid_boot_key_detail")"
  fi
  output+="$line"$'\n'

  local hvf_firmware_handoff_detail
  if hvf_firmware_handoff_detail="$(windows_hvf_firmware_handoff_metadata)"; then
    line="$(status_line "PASS" "Windows HVF firmware handoff boundary" "bridgevm hvf windows-firmware-handoff-probe and hvf-runner --windows-firmware-handoff-probe validate AArch64 UEFI FD and vars-template firmware volume headers, verify FV checksums, create mutable vars stores from templates, reopen them, and report planned code/vars pflash IPA slots without QEMU, Apple VZ, GUI launch, or HVF entry.")"
  else
    line="$(status_line "BLOCKED" "Windows HVF firmware handoff boundary" "$hvf_firmware_handoff_detail")"
  fi
  output+="$line"$'\n'

  local hvf_pflash_map_detail
  if hvf_pflash_map_detail="$(windows_hvf_pflash_map_metadata)"; then
    line="$(status_line "PASS" "Windows HVF pflash map boundary" "bridgevm hvf windows-pflash-map-probe and hvf-runner --windows-pflash-map-probe validate AArch64 UEFI FD/vars inputs, create mutable vars stores from templates, load code/vars into planned 64 MiB pflash memory images, verify copied prefixes, zero padding, non-overlapping IPA ranges, guest RAM separation, and device MMIO separation without QEMU, Apple VZ, GUI launch, or HVF entry.")"
  else
    line="$(status_line "BLOCKED" "Windows HVF pflash map boundary" "$hvf_pflash_map_detail")"
  fi
  output+="$line"$'\n'

  local hvf_pflash_hvf_map_detail
  if hvf_pflash_hvf_map_detail="$(windows_hvf_pflash_hvf_map_metadata)"; then
    line="$(status_line "PASS" "Windows HVF pflash HVF map boundary" "bridgevm hvf windows-pflash-hvf-map-probe and hvf-runner --windows-pflash-hvf-map-probe validate the prepared UEFI code/vars pflash images, keep the default path opt-in blocked, and define the signed live map/unmap proof with BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP=1 at the planned code read|exec and vars read|write IPA slots without QEMU, Apple VZ, GUI launch, vCPU creation, or guest execution.")"
  else
    line="$(status_line "BLOCKED" "Windows HVF pflash HVF map boundary" "$hvf_pflash_hvf_map_detail")"
  fi
  output+="$line"$'\n'

  local hvf_reset_vector_entry_detail
  if hvf_reset_vector_entry_detail="$(windows_hvf_reset_vector_entry_metadata)"; then
    line="$(status_line "PASS" "Windows HVF reset-vector entry boundary" "bridgevm hvf windows-reset-vector-entry-probe and hvf-runner --windows-reset-vector-entry-probe validate the prepared UEFI code/vars pflash images, keep the default path opt-in blocked, and define the signed live entry proof with BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY=1 to create an HVF VM, map code read|exec and vars read|write pflash slots, create one vCPU, set PC/CPSR, enter the UEFI reset vector once under a watchdog, observe the first exit, classify the Arm exception class, report whether PC progressed beyond the reset vector, and clean up without QEMU, Apple VZ, GUI launch, UEFI Boot Manager, or Windows boot claims.")"
  else
    line="$(status_line "BLOCKED" "Windows HVF reset-vector entry boundary" "$hvf_reset_vector_entry_detail")"
  fi
  output+="$line"$'\n'

  local hvf_firmware_run_loop_detail
  if hvf_firmware_run_loop_detail="$(windows_hvf_firmware_run_loop_metadata)"; then
    line="$(status_line "PASS" "Windows HVF firmware run-loop boundary" "bridgevm hvf windows-firmware-run-loop-probe and hvf-runner --windows-firmware-run-loop-probe validate the prepared UEFI code/vars pflash images, keep the default path opt-in blocked, expose installer ISO plus writable target disk metadata, populate the FDT platform DTB in guest RAM at 0x40010000, and define the signed live loop proof with BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP=1 to create an HVF VM, map code read|exec pflash, vars read|write pflash, guest RAM read|write|exec, and optional low pflash aliases, create one vCPU, set PC/X0-DTB/CPSR/SP_EL1, optionally seed a pflash, guest-RAM, or executable-candidate diagnostic VBAR/vector slot, classify bounded firmware exits, report watchdog timeout, ESR abort details, mapped-region hints, PC instruction word/hint, X0-X4/CPSR, EL1 exception/vector sysregs, EL1 MMU translation sysreg snapshots, PC stage-1 leaf descriptor/XN bits plus descriptor samples and walk entries for low-vector, pflash, guest-RAM, executable-vector, PC, VBAR, ELR, FAR, and SP addresses plus an EL1-executable leaf candidate scan with vector-sync VA/PA/instruction/hint telemetry plus 2 KiB-aligned vector-base scan/suppression/limit telemetry plus passive recommended-vector-base selection and opt-in one-shot recommended-vector-base VBAR set and follow-up-exit telemetry, automatic diagnosis classification, first firmware data-abort MMIO routing through the Windows device-window PL011/PL031 plus VirtIO-MMIO installer ISO at 0x10002000 read-only and target disk at 0x10003000 writable skeleton bus, a single-vCPU Group1 GIC CPU-interface sysreg skeleton for trapped ICC_* accesses plus conservative Group0/AP/RPR/SGI stubs, and a conservative device IRQ line boundary that mirrors VirtIO used-buffer interrupt status into GICD FDT SPI pending bits, gates assertion on GICD EnableGrp1NS, GICD/GICR IGROUPR Group1 bits, SPI/PPI enable/pending state, and ICC_IGRPEN1 plus PMR/current-running-priority threshold state, lets ICC_HPPIR1/ICC_IAR1 choose the highest-priority pending Group1 PPI/SPI, moves the acknowledged INTID active, treats ICC_EOIR1 as priority drop plus deactivate when ICC_CTLR EOImode is clear, requires ICC_DIR for deactivate when EOImode is set, refreshes/re-pends level VirtIO sources after actual deactivation, routes the virtual timer PPI 11 / INTID 27 through the same single-vCPU GIC CPU-interface path, asserts/deasserts the HVF IRQ line on edge changes, and reports device IRQ line assert/deassert counts and status names, handled MMIO read/write counts, per-device MMIO counts, virtio queue_notify/request completion counts, handled ICC read/write counts, per-ICC_IAR1/ICC_EOIR1/ICC_DIR counts, and last ICC_IAR1/ICC_EOIR1/ICC_DIR INTIDs; this proves only the VirtIO-status-to-GICD-SPI-to-priority-selected-ICC-IAR/EOIR/DIR-to-HVF-line skeleton boundary plus minimal timer PPI-to-GIC CPU-interface delivery and the one-shot recommended-vector-base diagnostic-vector population, VBAR set, and follow-up HVC/ERET observation boundary, not full GIC emulation, complete nested interrupt preemption, binary-point/List Register behavior, multi-vCPU routing, complete deactivation-stack semantics, UEFI Boot Manager, installer, or Windows boot, and cleans up without QEMU, Apple VZ, GUI launch, UEFI Boot Manager, or Windows boot claims.")"
  else
    line="$(status_line "BLOCKED" "Windows HVF firmware run-loop boundary" "$hvf_firmware_run_loop_detail")"
  fi
  output+="$line"$'\n'

  local hvf_firmware_device_discovery_detail
  if hvf_firmware_device_discovery_detail="$(windows_hvf_firmware_device_discovery_metadata)"; then
    line="$(status_line "PASS" "Windows HVF firmware device-discovery boundary" "bridgevm hvf windows-firmware-device-discovery-probe and hvf-runner --windows-firmware-device-discovery-probe wrap the UEFI firmware run-loop with a named no-QEMU device-discovery gate, force low pflash alias mapping, low-vector repair, post-repair continue, interrupt/timer wiring, and stop-at-first-post-repair-device-boundary policy, keep the default path opt-in blocked, and report Device discovery boundary reached/status/ready plus blockers without QEMU, Apple VZ, GUI launch, UEFI Boot Manager, installer boot, or Windows boot claims.")"
  else
    line="$(status_line "BLOCKED" "Windows HVF firmware device-discovery boundary" "$hvf_firmware_device_discovery_detail")"
  fi
  output+="$line"$'\n'

  local hvf_platform_description_detail
  if hvf_platform_description_detail="$(windows_hvf_platform_description_metadata)"; then
    line="$(status_line "PASS" "Windows HVF platform-description boundary" "bridgevm hvf windows-platform-description-probe and hvf-runner --windows-platform-description-probe build the metadata-only FDT platform description, verify FDT magic 0xd00dfeed, guest RAM at 0x40000000, requested CPU nodes, PL011/PL031 plus VirtIO-MMIO installer ISO at 0x10002000 and target disk at 0x10003000 inside the 0x10000000..0x20000000 Windows device window, root interrupt-parent 0x1, GICv3 distributor/redistributor ranges, four ARM arch timer interrupts, and PL011/PL031/VirtIO FDT SPI interrupt cells 0..3, while reporting ACPI not implemented, fw_cfg not used, GIC described/not emulated, GIC emulated false, and no QEMU, Apple VZ, GUI launch, HVF entry, UEFI Boot Manager, installer boot, or Windows boot claims.")"
  else
    line="$(status_line "BLOCKED" "Windows HVF platform-description boundary" "$hvf_platform_description_detail")"
  fi
  output+="$line"$'\n'

  local hvf_vm_probe_detail
  if hvf_vm_probe_detail="$(hvf_vm_probe_metadata)"; then
    line="$(status_line "PASS" "HVF VM create/destroy probe boundary" "default bridgevm/hvf-runner VM probes stay opt-in blocked, QEMU-free, and Apple VZ-free; live empty VM create/destroy requires BRIDGEVM_HVF_ALLOW_VM_CREATE=1 and a runner signed with com.apple.security.hypervisor.")"
  else
    line="$(status_line "BLOCKED" "HVF VM create/destroy probe boundary" "$hvf_vm_probe_detail")"
  fi
  output+="$line"$'\n'

  local hvf_vcpu_probe_detail
  if hvf_vcpu_probe_detail="$(hvf_vcpu_probe_metadata)"; then
    line="$(status_line "PASS" "HVF vCPU create/destroy probe boundary" "default bridgevm/hvf-runner vCPU probes stay opt-in blocked, QEMU-free, and Apple VZ-free; live vCPU lifecycle requires the signed HVF runner and does not call hv_vcpu_run.")"
  else
    line="$(status_line "BLOCKED" "HVF vCPU create/destroy probe boundary" "$hvf_vcpu_probe_detail")"
  fi
  output+="$line"$'\n'

  local hvf_vcpu_run_probe_detail
  if hvf_vcpu_run_probe_detail="$(hvf_vcpu_run_probe_metadata)"; then
    line="$(status_line "PASS" "HVF vCPU run/cancel probe boundary" "default bridgevm/hvf-runner vCPU run probes stay opt-in blocked, QEMU-free, and Apple VZ-free; live proof pre-cancels hv_vcpu_run and requires BRIDGEVM_HVF_ALLOW_VCPU_RUN=1 plus the signed HVF runner.")"
  else
    line="$(status_line "BLOCKED" "HVF vCPU run/cancel probe boundary" "$hvf_vcpu_run_probe_detail")"
  fi
  output+="$line"$'\n'

  local hvf_interrupt_timer_probe_detail
  if hvf_interrupt_timer_probe_detail="$(hvf_interrupt_timer_probe_metadata)"; then
    line="$(status_line "PASS" "HVF interrupt/timer probe boundary" "default bridgevm/hvf-runner interrupt/timer probes stay opt-in blocked, QEMU-free, Apple VZ-free, and guest-not-entered; the signed opt-in path verifies hv_vcpu_set/get_pending_interrupt for IRQ plus hv_vcpu_set/get_vtimer_mask and hv_vcpu_set/get_vtimer_offset with BRIDGEVM_HVF_ALLOW_INTERRUPT_TIMER=1. This is the substrate for firmware timer/interrupt wait-state handling, not firmware boot or Windows boot.")"
  else
    line="$(status_line "BLOCKED" "HVF interrupt/timer probe boundary" "$hvf_interrupt_timer_probe_detail")"
  fi
  output+="$line"$'\n'

  local hvf_vtimer_exit_probe_detail
  if hvf_vtimer_exit_probe_detail="$(hvf_vtimer_exit_probe_metadata)"; then
    line="$(status_line "PASS" "HVF VTimer exit probe boundary" "default bridgevm/hvf-runner VTimer exit probes stay opt-in blocked, QEMU-free, and Apple VZ-free; the signed opt-in path maps a tiny WFI guest, programs CNTV_CVAL_EL0 and CNTV_CTL_EL0, observes HV_EXIT_REASON_VTIMER_ACTIVATED, validates the automatic VTimer mask, injects a pending IRQ, and re-unmasks with BRIDGEVM_HVF_ALLOW_VTIMER_EXIT=1. This proves a timer-exit VMM boundary, not firmware boot or Windows boot.")"
  else
    line="$(status_line "BLOCKED" "HVF VTimer exit probe boundary" "$hvf_vtimer_exit_probe_detail")"
  fi
  output+="$line"$'\n'

  local hvf_memory_map_probe_detail
  if hvf_memory_map_probe_detail="$(hvf_memory_map_probe_metadata)"; then
    line="$(status_line "PASS" "HVF memory map/unmap probe boundary" "default bridgevm/hvf-runner memory probes stay opt-in blocked, QEMU-free, and Apple VZ-free; live proof maps and unmaps one 16 KiB guest RAM page with BRIDGEVM_HVF_ALLOW_MEMORY_MAP=1 plus the signed HVF runner.")"
  else
    line="$(status_line "BLOCKED" "HVF memory map/unmap probe boundary" "$hvf_memory_map_probe_detail")"
  fi
  output+="$line"$'\n'

  local hvf_guest_entry_probe_detail
  if hvf_guest_entry_probe_detail="$(hvf_guest_entry_probe_metadata)"; then
    line="$(status_line "PASS" "HVF guest entry probe boundary" "default bridgevm/hvf-runner guest-entry probes stay opt-in blocked, QEMU-free, and Apple VZ-free; live proof maps one HVC instruction and requires BRIDGEVM_HVF_ALLOW_GUEST_ENTRY=1 plus the signed HVF runner watchdog.")"
  else
    line="$(status_line "BLOCKED" "HVF guest entry probe boundary" "$hvf_guest_entry_probe_detail")"
  fi
  output+="$line"$'\n'

  local hvf_guest_exit_loop_probe_detail
  if hvf_guest_exit_loop_probe_detail="$(hvf_guest_exit_loop_probe_metadata)"; then
    line="$(status_line "PASS" "HVF guest exit loop probe boundary" "default bridgevm/hvf-runner guest-exit-loop probes stay opt-in blocked, QEMU-free, and Apple VZ-free; live proof runs two HVC exits with explicit PC advance and requires BRIDGEVM_HVF_ALLOW_EXIT_LOOP=1 plus the signed HVF runner watchdog.")"
  else
    line="$(status_line "BLOCKED" "HVF guest exit loop probe boundary" "$hvf_guest_exit_loop_probe_detail")"
  fi
  output+="$line"$'\n'

  local hvf_mmio_read_probe_detail
  if hvf_mmio_read_probe_detail="$(hvf_mmio_read_probe_metadata)"; then
    line="$(status_line "PASS" "HVF MMIO read exit probe boundary" "default bridgevm/hvf-runner MMIO read probes stay opt-in blocked, QEMU-free, and Apple VZ-free; live proof runs one unmapped LDR read against IPA 0x50000000 and requires BRIDGEVM_HVF_ALLOW_MMIO_READ=1 plus the signed HVF runner watchdog.")"
  else
    line="$(status_line "BLOCKED" "HVF MMIO read exit probe boundary" "$hvf_mmio_read_probe_detail")"
  fi
  output+="$line"$'\n'

  local hvf_mmio_read_emulation_probe_detail
  if hvf_mmio_read_emulation_probe_detail="$(hvf_mmio_read_emulation_probe_metadata)"; then
    line="$(status_line "PASS" "HVF MMIO read emulation probe boundary" "default bridgevm/hvf-runner MMIO emulation probes stay opt-in blocked, QEMU-free, and Apple VZ-free; live proof injects X0=0x123456789abcdef0, advances PC, and continues to HVC with BRIDGEVM_HVF_ALLOW_MMIO_EMULATION=1 plus the signed HVF runner watchdog.")"
  else
    line="$(status_line "BLOCKED" "HVF MMIO read emulation probe boundary" "$hvf_mmio_read_emulation_probe_detail")"
  fi
  output+="$line"$'\n'

  local hvf_mmio_write_emulation_probe_detail
  if hvf_mmio_write_emulation_probe_detail="$(hvf_mmio_write_emulation_probe_metadata)"; then
    line="$(status_line "PASS" "HVF MMIO write emulation probe boundary" "default bridgevm/hvf-runner MMIO write emulation probes stay opt-in blocked, QEMU-free, and Apple VZ-free; live proof captures X0=0xfedcba987654321 from an unmapped STR, advances PC, and continues to HVC with BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION=1 plus the signed HVF runner watchdog.")"
  else
    line="$(status_line "BLOCKED" "HVF MMIO write emulation probe boundary" "$hvf_mmio_write_emulation_probe_detail")"
  fi
  output+="$line"$'\n'

  local hvf_mmio_serial_device_probe_detail
  if hvf_mmio_serial_device_probe_detail="$(hvf_mmio_serial_device_probe_metadata)"; then
    line="$(status_line "PASS" "HVF MMIO serial device probe boundary" "default bridgevm/hvf-runner serial device probes stay opt-in blocked, QEMU-free, and Apple VZ-free; live proof routes PL011 UART data X0=0x41 and flags X0=0x90 through the BridgeVM MMIO device bus, advances PC twice, and continues to HVC with BRIDGEVM_HVF_ALLOW_MMIO_SERIAL_DEVICE=1 plus the signed HVF runner watchdog.")"
  else
    line="$(status_line "BLOCKED" "HVF MMIO serial device probe boundary" "$hvf_mmio_serial_device_probe_detail")"
  fi
  output+="$line"$'\n'

  local hvf_mmio_rtc_device_probe_detail
  if hvf_mmio_rtc_device_probe_detail="$(hvf_mmio_rtc_device_probe_metadata)"; then
    line="$(status_line "PASS" "HVF MMIO RTC device probe boundary" "default bridgevm/hvf-runner RTC device probes stay opt-in blocked, QEMU-free, and Apple VZ-free; live proof routes a PL031 RTC read X0=0x20260618 through a two-device BridgeVM MMIO bus with PL011 UART plus PL031 RTC, advances PC, and continues to HVC with BRIDGEVM_HVF_ALLOW_MMIO_RTC_DEVICE=1 plus the signed HVF runner watchdog.")"
  else
    line="$(status_line "BLOCKED" "HVF MMIO RTC device probe boundary" "$hvf_mmio_rtc_device_probe_detail")"
  fi
  output+="$line"$'\n'

  local hvf_mmio_block_device_probe_detail
  if hvf_mmio_block_device_probe_detail="$(hvf_mmio_block_device_probe_metadata)"; then
    line="$(status_line "PASS" "HVF MMIO block device probe boundary" "default bridgevm/hvf-runner block identity probes stay opt-in blocked, QEMU-free, and Apple VZ-free; live proof routes VirtIO-MMIO magic=0x74726976, version=0x2, block device ID=0x2, and vendor=0x4252564d through a three-device BridgeVM MMIO bus with BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_DEVICE=1 plus the signed HVF runner watchdog.")"
  else
    line="$(status_line "BLOCKED" "HVF MMIO block device probe boundary" "$hvf_mmio_block_device_probe_detail")"
  fi
  output+="$line"$'\n'

  local hvf_mmio_block_queue_probe_detail
  if hvf_mmio_block_queue_probe_detail="$(hvf_mmio_block_queue_probe_metadata)"; then
    line="$(status_line "PASS" "HVF MMIO block queue/config/address/notify probe boundary" "default bridgevm/hvf-runner block queue/config/address/notify probes stay opt-in blocked, QEMU-free, and Apple VZ-free; the signed opt-in path routes VirtIO-MMIO feature, driver feature, queue select/size/ready, status, descriptor/driver/device ring addresses, queue notify, interrupt status, config generation, and capacity registers through a three-device BridgeVM MMIO bus with BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_QUEUE=1 plus the signed HVF runner watchdog, seeds one synthetic VirtIO block read request in guest RAM, completes it immediately after queue_notify by writing data/status/used-ring state and raising used-buffer interrupt status, and has optional --disk/--iso/--writable-disk backing selectors, including a signed live writable queue_notify read/write/flush/reopen persistence boundary. The Windows firmware two-device path now advertises per-backing capacity sectors, rejects unexpected queue_notify IPAs/values, and resets queue/interrupt/avail-index state on status zero. It still does not prove full persistent disk lifecycle, firmware boot, or Windows boot.")"
  else
    line="$(status_line "BLOCKED" "HVF MMIO block queue/config/address/notify probe boundary" "$hvf_mmio_block_queue_probe_detail")"
  fi
  output+="$line"$'\n'

  local hvf_virtio_block_request_model_detail
  if hvf_virtio_block_request_model_detail="$(hvf_virtio_block_request_model_metadata)"; then
    line="$(status_line "PASS" "VirtIO block request model boundary" "default bridgevm/hvf-runner request model probes are QEMU-free, Apple-VZ-free, and HVF-not-entered; they are configured through MMIO bus writes, observe queue notify, complete one synthetic VIRTIO_BLK_T_IN descriptor chain through the device bus, write data/status/used ring state, raise interrupt status, and still do not prove live HVF block IO, ISO attach, persistent disk, firmware boot, or Windows boot.")"
  else
    line="$(status_line "BLOCKED" "VirtIO block request model boundary" "$hvf_virtio_block_request_model_detail")"
  fi
  output+="$line"$'\n'

  local hvf_virtio_block_file_backing_detail
  if hvf_virtio_block_file_backing_detail="$(hvf_virtio_block_file_backing_metadata)"; then
    line="$(status_line "PASS" "VirtIO block host-file backing boundary" "default bridgevm/hvf-runner file backing probes are QEMU-free, Apple-VZ-free, and HVF-not-entered; they create a host disk image fixture, configure the VirtIO-MMIO queue through the BridgeVM MMIO bus, observe queue notify, complete one VIRTIO_BLK_T_IN descriptor chain by reading sector data from the host file at byte offset 0xe00, write data/status/used ring state, and raise interrupt status. This is the metadata-safe host-backed storage read model; live HVF host-file completion is covered by the separate signed opt-in mmio-block-queue --disk path, live writable completion is covered by --writable-disk, and it still does not prove persistent boot disk lifecycle, firmware boot, or Windows boot.")"
  else
    line="$(status_line "BLOCKED" "VirtIO block host-file backing boundary" "$hvf_virtio_block_file_backing_detail")"
  fi
  output+="$line"$'\n'

  local hvf_virtio_block_writable_file_backing_detail
  if hvf_virtio_block_writable_file_backing_detail="$(hvf_virtio_block_writable_file_backing_metadata)"; then
    line="$(status_line "PASS" "VirtIO block writable host-file backing boundary" "default bridgevm/hvf-runner writable file backing probes are QEMU-free, Apple-VZ-free, and HVF-not-entered; they create a host disk image fixture, configure the VirtIO-MMIO queue through the BridgeVM MMIO bus, observe queue notify, complete an initial VIRTIO_BLK_T_IN read, then complete VIRTIO_BLK_T_OUT write plus VIRTIO_BLK_T_FLUSH at byte offset 0xe00 and reopen the host file to verify persisted bytes. The signed opt-in mmio-block-queue --writable-disk path now proves the same read/write/flush/reopen persistence boundary after a live HVF queue_notify. This proves a storage boundary, not full persistent boot disk lifecycle, partition install state, firmware boot, or Windows boot.")"
  else
    line="$(status_line "BLOCKED" "VirtIO block writable host-file backing boundary" "$hvf_virtio_block_writable_file_backing_detail")"
  fi
  output+="$line"$'\n'

  local hvf_virtio_block_iso_backing_detail
  if hvf_virtio_block_iso_backing_detail="$(hvf_virtio_block_iso_backing_metadata)"; then
    line="$(status_line "PASS" "VirtIO block ISO backing boundary" "default bridgevm/hvf-runner ISO backing probes are QEMU-free, Apple-VZ-free, and HVF-not-entered; they create a read-only installer-media fixture, configure the VirtIO-MMIO queue through the BridgeVM MMIO bus, observe queue notify, complete one VIRTIO_BLK_T_IN descriptor chain by reading sector data from the ISO backing at byte offset 0xe00, write data/status/used ring state, then reject one VIRTIO_BLK_T_OUT write request with S_IOERR while writing status/used-ring state and raising interrupt status. This proves the metadata-safe read-only installer media sector-read and write-rejection model, not UEFI boot, installer boot, persistent boot disk lifecycle, or Windows boot.")"
  else
    line="$(status_line "BLOCKED" "VirtIO block ISO backing boundary" "$hvf_virtio_block_iso_backing_detail")"
  fi
  output+="$line"$'\n'

  line="$(status_line "BLOCKED" "Parallels-style true Coherence" "no compositor-grade live per-window stream, dock/menu integration, or Windows DirectX/Metal translation claim yet.")"
  output+="$line"$'\n'

  line="$(status_line "BLOCKED" "Public release readiness boundary" "local app/readiness lanes exist, but public notarized release plus true Coherence/performance gates are not complete.")"
  output+="$line"$'\n'

  if has_forbidden_percent "$output"; then
    echo "FAIL: product gate report must not contain percentage estimates" >&2
    exit 1
  fi

  printf '%s' "$output"
}

main "$@"
