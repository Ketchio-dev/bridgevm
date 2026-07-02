# BridgeVM HVF — Windows install-completion plan (Codex-ready)

Goal: take BridgeVM's from-scratch HVF VMM from "Windows 11 ARM64 Setup boots from
NVMe" to "a fully-installed Windows boots to the desktop", driven by an unattended /
WinPE-scripted install. This plan is written so an AI coding agent (Codex) can execute
it step by step. Live HVF boots must run on this Mac (signed binary); pure code + unit
tests can be done anywhere.

---

## 0. Where we are (2026-07-02)

Proven & committed:
- Windows 11 ARM64 **Setup boots from NVMe alone** (no ISO, no keyboard). Root cause of
  the old "EDK2 won't bind NVMe" wall was the **stale Homebrew firmware**; a current
  tianocore/edk2 ArmVirtQemu firmware is now vendored in-repo and is the default
  (`crates/bridgevm-hvf/firmware/edk2-aarch64-code.fd`, commit `ab9c45a`).
- **Second NVMe namespace** (blank install target) implemented (`029105c`): env
  `BRIDGEVM_NVME_DISK2` (+`_WRITABLE`), NSID-2 routed through Identify + NVM read/write.
- **WinPE-scripted install harness** proven: `\sources\boot.wim` (index 2) was edited so
  `winpeshl.ini` runs a custom `bvinstall.cmd` (diskpart + dism + bcdboot) instead of
  `setup.exe`. WinPE boots, runs the script, finds the source (`C:`), launches diskpart.

The one blocker:
- **`DRIVER_PNP_WATCHDOG (0x1D5)` bugcheck** the moment diskpart / Virtual Disk Service
  does a *full PnP device enumeration*. WinPE + basic volume mounting work; VDS's deeper
  enumeration stalls a PnP IRP on some modelled device → watchdog bugcheck. Reproduces
  even single-disk (so it is **not** the second namespace).

Assistant memory with the full history: `bridgevm-hvf-engine-status.md`.

---

## 1. Codebase map (files an agent will touch)

- Probe (the live VMM harness / `main()`): `crates/bridgevm-hvf/examples/hvf_gic_boot_probe.rs`
  - VM setup ~L1179–1319: `hv_vm_create` (~1203), firmware `map_file` (~1258),
    `alloc_zeroed` guest RAM (~1270), `hv_vcpu_create` (~1288), initial regs
    `HV_REG_PC=0x0`, `HV_REG_CPSR=0x3c5`, `HV_REG_X0=RAM_BASE` (~1312).
  - Run loop: `loop { … }` (~L1520). **PSCI handler** at `EC_HVC` (~L1685); `0x8400_0008 |
    0x8400_0009` (SYSTEM_OFF/RESET) currently does `stop_reason = …; break;` (~L1730) → the
    probe EXITS instead of rebooting.
  - Device attach from media (~L1410): `attach_nvme_raw_file`,
    `attach_nvme_second_namespace_raw_file`, `attach_pci_boot_media`, `attach_virtio_iso`.
- Platform (device container): `crates/bridgevm-hvf/src/platform_virt.rs`
  - `struct VirtPlatform` (~L124): `fw_cfg, uart, rtc, pcie, nvme, xhci, virtio_iso,
    pci_boot_media, ramfb, flash_vars`, plus nvme liveness flags + `dtb`.
  - `new_with_ramfb_state` (~L161); `on_mmio` dispatch (~L498).
- NVMe model: `crates/bridgevm-hvf/src/nvme.rs` (single controller, NSID 1 + optional NSID 2).
- xHCI model: `crates/bridgevm-hvf/src/xhci/` (DCI3 HID keyboard; built for key delivery).
- PCIe ECAM: `crates/bridgevm-hvf/src/pcie.rs` (NVMe 00:01.0, xHCI 00:02.0, virtio-blk 00:03.0).
- Machine map: `crates/bridgevm-hvf/src/machine.rs` — `FLASH_CODE(0x0,64MiB)`,
  `FLASH_VARS(0x0400_0000,64MiB)`, `RAM_BASE=0x4000_0000`, `PCIE_MMIO_32/64`.
- Media/env config: `crates/bridgevm-hvf/src/media.rs`.

---

## 2. GROUND RULES — read before any live boot (these cost real time if skipped)

1. **Re-sign after EVERY probe rebuild.** A fresh `cargo build` produces a linker-signed
   binary *without* the hypervisor entitlement, so `hv_vm_create` returns `0xfae94007`
   (HV_DENIED) — which looks exactly like a throttle but is not. After building:
   ```
   codesign --sign - --entitlements apps/macos/HvfRunner.entitlements --force \
     target/debug/examples/hvf_gic_boot_probe
   codesign -d --entitlements - target/debug/examples/hvf_gic_boot_probe | grep hypervisor
   ```
2. **HVF cooldown.** Back-to-back live boots eventually get denied; wait ~60–180 s between
   boots (kill stragglers first: `pkill -9 -f hvf_gic_boot_probe`).
3. **ramfb sampling:** `BRIDGEVM_RAMFB_SAMPLE_MS` values must each be **< ~200000** or the
   schedule is rejected (`parse_error=too_large`) and you get no periodic frames.
4. **Never regress the working NVMe boot.** Keep `cargo test -p bridgevm-hvf` green
   (currently 440+ lib tests). NSID-1 behaviour must be unchanged when NSID 2 is absent.
5. **Build+sign+run recipe:**
   ```
   cargo build -p bridgevm-hvf --example hvf_gic_boot_probe
   codesign --sign - --entitlements apps/macos/HvfRunner.entitlements --force \
     target/debug/examples/hvf_gic_boot_probe
   env BRIDGEVM_RAM_MIB=4096 BRIDGEVM_RAMFB=1 \
     BRIDGEVM_RAMFB_DUMP_DIR=<dir> BRIDGEVM_RAMFB_SAMPLE_MS=60000,120000,180000 \
     BRIDGEVM_NVME_DISK=<installer.img> \
     BRIDGEVM_NVME_DISK2=<target.img> BRIDGEVM_NVME_DISK2_WRITABLE=1 \
     BRIDGEVM_AARCH64_UEFI_VARS=<writable-vars.fd> BRIDGEVM_AARCH64_UEFI_VARS_WRITABLE=1 \
     BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=280000 \
     target/debug/examples/hvf_gic_boot_probe > run.log 2>&1
   ```
6. Convert a ramfb dump to view: `sips -s format png <dump>.ppm --out out.png`.
7. Install assets already staged (session scratch — regenerate if gone): a bootable Win11
   ARM64 installer disk with `\efi\boot\bootaa64.efi`, `\sources\boot.wim`,
   `install.swm`/`install2.swm`, and the boot.wim already carrying `winpeshl.ini` +
   `bvinstall.cmd` + `bvdiskpart.txt`; plus a 24 GB sparse target and a validated
   `autounattend.xml`. See `bridgevm-hvf-engine-status.md` for exact paths and the
   `wimlib-imagex update boot.wim 2 …` recipe.

---

## Workstream A (P1) — Isolate & fix `DRIVER_PNP_WATCHDOG (0x1D5)`

Hypothesis: during a full PnP rescan Windows enumerates every modelled PCI device; one
device's driver never completes a PnP start/query IRP (our NVMe survives a *basic*
stornvme boot but VDS does more). Prime suspect: **xHCI** (built only for HID key
delivery). The scripted install is keyboard-free, so xHCI is not required for install.

### A1. Add env-gated device-disable switches to the probe/platform  *(code — Codex)*
- Add a `VirtPlatformConfig`/builder or simple flags so the platform can be built
  **without** xHCI, and/or without the virtio boot-media / legacy virtio-mmio, and/or
  without ramfb. Suggested env → media/platform flags:
  `BRIDGEVM_DISABLE_XHCI`, `BRIDGEVM_DISABLE_VIRTIO_ISO`, `BRIDGEVM_DISABLE_RAMFB_DEVICE`.
- The disabled device must be fully absent from **DTB + PCIe ECAM + ACPI + MMIO dispatch**
  (not just "attached=false"), so Windows never sees it during enumeration.
- Unit tests: with a device disabled, `device_at`/`pcie` no longer expose it, and the DTB
  omits its node.
- Acceptance: `cargo test -p bridgevm-hvf` green; a device can be cleanly omitted.

### A2. Bisection matrix  *(live — run on this Mac)*
Boot the WinPE-scripted install (two disks) repeatedly, each time omitting one device,
and watch whether diskpart gets **past the banner** and starts writing to NSID 2
(`op=0x01(write) nsid=2`, target file grows) instead of BSOD `0x1D5`:
| run | disabled | expected signal |
|-----|----------|-----------------|
| 1 | xHCI | diskpart proceeds? |
| 2 | virtio boot-media + legacy mmio | diskpart proceeds? |
| 3 | ramfb device | (control) |
The first run that lets diskpart partition NSID 2 names the culprit.

### A3. Fix the culprit's PnP behaviour  *(code — depends on A2)*
- If **xHCI**: either (a) keep it disabled for the scripted install (acceptable — input
  is not needed to install), and/or (b) make the xHCI model complete whatever PnP-time
  operation stalls (e.g. a control transfer / port-status / doorbell it currently ignores
  when Windows does a full enumerate rather than the HID-only path). Add a trace of the
  last xHCI TRB/register touched before the hang to guide the fix.
- If **another device**: implement/return the missing PnP-time response so the IRP
  completes.
- Optional deeper diagnosis if bisection is ambiguous: enable Windows kernel debug in the
  WinPE BCD (`bcdedit /dbgsettings serial debugport:1 baudrate:115200` on the boot.wim's
  BCD) and read the bugcheck parameters over the PL011 UART (which the probe already
  models + captures) — the 4 params of `0x1D5` name the exact stalled device object.
- Acceptance (live): diskpart partitions NSID 2, `dism /apply-image` starts writing
  (`nsid=2` writes climb, target file grows into the GBs), no `0x1D5`.

---

## Workstream B (P2) — Probe reboot-loop (survive PSCI SYSTEM_RESET)

Windows install reboots several times (apply → specialize → OOBE). The probe must reboot
the **existing** VM, not exit — and must **not** call `hv_vm_create` again (that path is
brittle). NVMe disk contents and the writable UEFI vars must survive the reset; firmware
`FLASH_CODE` is mapped read/exec so it is already pristine.

### B1. Make VM setup re-entrant  *(code — Codex)*
- Refactor `main()` in `hvf_gic_boot_probe.rs` so the *reset-able* state (guest RAM
  contents, vcpu registers, `VirtPlatform`) can be re-initialised inside a
  `'reboot: loop { … }` without re-creating the VM/vcpu/GIC or re-mapping firmware.
- On the PSCI `0x8400_0009` (SYSTEM_RESET) branch: instead of `break`, perform a reset
  (below) and `continue 'reboot`. Keep `0x8400_0008` (SYSTEM_OFF) as a real exit.
- Add a reboot counter + a max-reboots guard and a per-boot watchdog so a wedged guest
  still terminates.

### B2. `VirtPlatform::reset()` preserving disks + vars  *(code — Codex)*
- Add `pub fn reset(&mut self)` that returns every device to power-on state **except**:
  - **NVMe**: keep both namespace backing stores (`disk`, `disk2`); reset only the
    controller registers/queues/CSTS/CC and completion state. (Add a
    `NvmeController::reset_registers_keep_disks()` — do NOT `*self = Self::new(...)`.)
  - **flash_vars** (`P30NorFlash`): keep contents (holds the UEFI boot entry Setup wrote).
  - Reset xHCI, ramfb, pending MSI-X/SPI queues, fw_cfg selector, uart/rtc, and the
    `nvme_*` liveness flags.
- Unit test: write to NVMe (both namespaces) + flash_vars, call `reset()`, assert disk +
  vars contents survive but controller registers/queues are cleared.

### B3. Reset guest CPU + RAM  *(code — Codex)*
- Re-zero the guest RAM allocation and re-copy the DTB to `RAM_BASE`; set
  `HV_REG_PC=0x0`, `HV_REG_CPSR=0x3c5`, `HV_REG_X0=RAM_BASE`, re-unmask the vtimer, and
  re-apply MPIDR/DFR0 as in initial setup. (Factor the initial vcpu-reg block into a
  reusable `reset_vcpu(vcpu)` and call it from both first boot and reboot.)
- Acceptance (live, cheap): a guest that issues SYSTEM_RESET (e.g. a WinPE `wpeutil
  reboot`, or EDK2 `reset`) causes the firmware to re-run from `0x0` and NVMe data written
  before the reset is still readable after it.

---

## Workstream C (P3) — Drive the install to a booted desktop

Prereqs: A (no `0x1D5`) and B (reboot-loop) done.

### C1. Re-verify the scripted install harness *(mostly done — re-check after A/B)*
- boot.wim index 2 runs `bvinstall.cmd`: `wpeinit` → find source drive → `diskpart /s`
  (partition NSID 2 as GPT ESP+MSR+NTFS) → `dism /apply-image /imagefile:…\install.swm
  /swmfile:…\install*.swm /index:1 /applydir:W:\` → `bcdboot W:\Windows /s S: /f UEFI`.
- Confirm `diskpart` disk index actually maps NSID 2 (installer=disk 0, target=disk 1);
  if enumeration order flips, select by size or add `list disk` logging.

### C2. Run the full install *(live)*
- Two disks, generous watchdog. Expect a long `dism` apply (~13 GB through the emulated
  NVMe — **watch throughput**; if it is impractically slow, that becomes its own perf
  task: batch NVMe writes / speed up the command-processing loop). Success = target file
  grows to ~10+ GB, `bcdboot` succeeds, then the guest reboots (needs B).

### C3. Boot the installed OS to desktop *(live)*
- After the install reboot, EDK2 should boot the NSID-2 Windows Boot Manager entry
  (writable vars preserve it across the reboot-loop). Windows runs specialize → OOBE;
  `autounattend.xml` (oobeSystem pass) auto-skips OOBE + autologons `bridge`/`bridge`.
- Capture ramfb screenshots proving the desktop. Multiple specialize/OOBE reboots will
  exercise the reboot-loop (B).

Performance note: if the emulated-NVMe apply is too slow to be usable, add an NVMe
fast-path (bulk PRP copy, fewer per-command allocations) as a follow-up task.

---

## Workstream D (later) — real usability (not needed to *install*)
- **Input**: keys already reach the Windows HID stack but the Setup GUI did not act on
  them (leg-A wall). Revisit for interactive desktop use (focus/routing, or a different
  HID/absolute-pointer device).
- **Display**: ramfb is enough for proof; a faster/native GOP path for real use.

---

## Definition of done
1. `cargo test -p bridgevm-hvf` and `-p hvf-runner` green after every code change.
2. A live WinPE-scripted install partitions NSID 2, applies the WIM, and `bcdboot`s with
   no `DRIVER_PNP_WATCHDOG`.
3. The probe reboots the VM on SYSTEM_RESET with NVMe + UEFI vars preserved.
4. The installed Windows reaches the desktop (ramfb screenshot), input/display deferred to
   Workstream D.

Suggested order: **A1 → A2 → A3** (unblock the install), then **B1→B2→B3** (reboots), then
**C**. A and B are independent and can be developed in parallel; both need live boots to
confirm, which must run on this Mac with the signed probe.
