# BridgeVM HVF Engine — Windows completion plan (Codex-ready)

Goal: take BridgeVM's from-scratch HVF VMM (the QEMU-independent VMM directly on Apple
Hypervisor.framework, `crates/bridgevm-hvf`) from "Windows 11 ARM64 Setup boots from
NVMe" all the way to **a usable Windows 11 ARM desktop** — install completes, boots to
desktop, keyboard/pointer/display/network work, and it is fast enough to use. This plan
is written so an AI coding agent (Codex) can execute it step by step. Live HVF boots must
run on this Mac (signed binary); pure code + unit tests can be done anywhere.

Scope note: Workstreams A–C (install → desktop) are specified in detail because they are
the concrete next work. Workstreams D–F (usability → polish) are **entry-condition-gated
and evidence-driven** — they cannot be fully pinned down until the desktop is reachable,
so they give the sub-goals, likely approaches, and acceptance criteria, and expect new
walls at each rung (exactly like `DRIVER_PNP_WATCHDOG` surfaced at the install rung).

---

## 0. Where we are (2026-07-22)

The historical installer blockers in workstreams A-C are closed. The custom
Hypervisor.framework VMM now boots an installed Windows 11 ARM64 desktop with
four vCPUs, persistent NVMe, ramfb/input, userspace-NAT virtio-net, and the
resident BVAGENT service. The macOS Windows HVF Lab ships the signed runner,
imports cloned RAW disk/vars media without changing the sources, extends small
imports to 64 GiB, and grows C: through a fail-closed first-boot action. Normal
app launches now explicitly disable the diagnostic per-boot watchdog, so a VM
does not disappear after 15 minutes; the experimental lab retains an opt-in
bounded watchdog and the resident agent keeps independent overdue telemetry.

Lifecycle evidence covers clean system-off/writeback, post-exit reopen, a
process-resident host pause/continue, and an in-process Windows restart. The
restart path resets BridgeVM devices, guest RAM, vCPU registers, and Apple's
in-kernel GIC; omitting the GIC reset was live-observed to freeze the second
firmware generation.

The normal GPU lifecycle now uses a four-stage, fail-closed cleanup/install/bind
state machine; the guest agent is installed by a LocalSystem bootstrap service
as an interactive highest-privilege logon task without disabling UAC. The host
has a protocol-aware trace gate and a versioned, generic title-manifest gate;
PPSSPP is the bundled first manifest rather than a one-off evaluator. The macOS
app consumes structured launch/release readiness, presents the mmap-backed live
display with keyboard and pointer input, and can be packaged with its signed HVF
probe, CLI, VirGL runtime, scripts, and explicitly supplied UEFI firmware.

The remaining release walls are live/external evidence and security lifecycle:
a fresh finalized ARM64 WDK package and same-boot Windows bind/title receipt,
production driver signing, and completion of vTPM 2.0 + measured/Secure Boot
migration, recovery, and guest proof.
The competitive architecture and performance-risk decisions are fixed in
`docs/hvf-competitive-architecture-and-risk-policy.md`: the app selects a
rollback-safe aggressive renderer lane, while vTPM/Secure Boot must ship as a
per-VM encrypted-state and Keychain-backed lifecycle rather than a
guest-visible device checkbox.

`SEC-TPM-FRONTEND` has now reached E2/local proof: BridgeVM owns a five-locality
TPM 2.0 TIS/FIFO state machine, a bounded swtpm Unix data-socket backend, the
QEMU `virt` platform-bus MMIO reservation at `0x0c000000`, a persistent 1 KiB
PPI mailbox at `0x0c005000`, PPI 1.3 and reset-mitigation `_DSM` AML, optional
ACPI `TPM0`/`MSFT0101`/`_CRS` emission, and platform/run-loop dispatch selected
by `BRIDGEVM_SWTPM_DATA_SOCKET`. The installed-boot launcher can own exactly one
swtpm process with `--vtpm-state-dir`, records socket readiness, fails closed,
and preserves the state directory after shutdown. The macOS product path now
uses a stable VM ID to load or atomically create a 256-bit
`WhenUnlockedThisDeviceOnly` Keychain item, transfers it through a one-shot
stdin FD, and starts swtpm with AES-256-CBC encrypt-then-MAC state protection.
No key appears in argv or a disk keyfile; an existing state directory whose
Keychain item is missing fails closed without minting a replacement key. ACPI
also emits QEMU's
revision-4 TPM2 FIFO table and relocates its LASA field to a zero-initialized
64 KiB `etc/tpm/log` allocation. Construction fails if ACPI presence and backend
presence disagree. The default EDK2 code volume is now a reproducible,
commit-pinned 3 MiB build with Secure Boot and TPM2 enabled. Fresh-install
finalization fail-closed provisions the exact Microsoft-only ARM64
`secureboot_objects` v1.6.5 `dbx`, `db`, `KEK`, and `PK` payloads (PK last),
validates every hash/ESL/provenance field, preserves exact existing state, and
rejects partial or conflicting state without mutation. Packaging includes the
policy, firmware build receipt, and license notices. This still does **not**
close the gates: firmware-populated measured-boot events, a bundled/signed
swtpm distribution, explicit move/clone/restore/reset UX, firmware processing
of PPI requests, `Confirm-SecureBootUEFI`/PCR 7 proof, BitLocker recovery, and a
live Windows receipt remain.

Windows HVF suspend/resume is explicitly not a v1 product capability; the
experimental single-vCPU checkpoint path must not be advertised as durable
suspend. See `docs/hvf-windows-v1-suspend-decision.md`.

Assistant memory with the full history: `bridgevm-hvf-engine-status.md`.

---

## 1. Codebase map (files an agent will touch)

- Probe (the live VMM harness / `main()`): `crates/bridgevm-hvf/examples/hvf_gic_boot_probe.rs`
  - VM setup ~L1179–1319: `hv_vm_create` (~1203), firmware `map_file` (~1258),
    `alloc_zeroed` guest RAM (~1270), `hv_vcpu_create` (~1288), initial regs
    `HV_REG_PC=0x0`, `HV_REG_CPSR=0x3c5`, `HV_REG_X0=RAM_BASE` (~1312).
  - The PSCI handler treats SYSTEM_OFF as terminal and SYSTEM_RESET as another
    bounded boot generation after vCPU join plus GIC/platform/RAM/vCPU reset.
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

**Completed and live-proven 2026-07-12.** The reset must include Apple's
in-kernel GIC, not only BridgeVM-owned devices, RAM, and CPU registers. A
WDK-triggered reboot exposed the missing step by freezing the second firmware
generation. The probe now calls `hv_gic_reset()` after all secondary vCPUs
stop and join; a nonzero status fails closed. A controlled installed-Windows
restart returned `0x0`, reached a second BVAGENT `READY` in the same process,
then powered off with NVMe/vars writeback and a status-0 service gate. See
`docs/windows-arm/evidence/reboot-gic-reset-20260712.md`.

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

## Workstream D — Usable desktop I/O   *(entry condition: Workstream C done — Windows boots to desktop)*

These cannot be fully specified until the desktop is reachable; new walls will surface.
Method for each: reach the surface, capture ramfb/trace evidence, fix, prove.

### D1. Input — keyboard + pointer
- Current: the xHCI DCI3 HID **keyboard** delivers reports into the Windows HID ring, but
  Setup's GUI never acted on them ("leg-A" wall; root cause was *above* the xHCI layer —
  WinPE input focus/routing or report interpretation, never diagnosable from the host).
- First test on the real desktop: does the existing keyboard register in a desktop app
  (Notepad)? The Setup-GUI non-response may be Setup-specific, and a booted desktop can
  finally run **guest-side diagnostic tools** we never had in WinPE.
- Add a **pointer**: model a USB HID **absolute pointer / tablet** (like QEMU `usb-tablet`)
  as a second HID endpoint on the xHCI — absolute coordinates avoid pointer-capture and
  are what VMs use. Needs a report descriptor + an inject path for move/click.
- If keys still don't register: diagnose the HID report descriptor + input routing with
  guest tools; try boot-protocol vs report-protocol, or a distinct HID interface.
- Acceptance (live, ramfb): type visible text into a desktop app AND move+click the pointer.

### D2. Display
- Current: EDK2 presents our `ramfb` as the UEFI GOP framebuffer, so Windows' Basic Display
  Adapter should drive a basic (unaccelerated) desktop once booted. Confirm resolution,
  stride/format, and redraw.
- Real use: a usable resolution (≥1080p) and smooth redraw. ramfb is fixed-mode; a
  resizable/accelerated path (virtio-gpu or a paravirtual display) is a larger future item
  — defer until basic display is confirmed usable.
- Acceptance: desktop renders correctly at a usable resolution; windows/cursor update
  smoothly enough to use.

### D3. Network
- Implemented and live-proven: `virtio-net-pci` with the injected ARM64 netkvm driver and
  an in-process userspace NAT provides DHCP, DNS, ICMP, TCP, and UDP. Windows obtains
  10.0.2.15 and reaches the internet; driver injection remains part of image preparation.
- Acceptance achieved 2026-07-09: gateway/external ping, DNS, and HTTP all passed in-guest.

### D4. Storage persistence *(live-proven)*
- The NSID-2 target is a persistent host file (write-back), so the installed OS persists.
  Raw write-back `FLUSH` now issues `File::sync_data()`, and the common final persistence
  hook uses the same path. On 2026-07-11, 9/9 clean agent-driven shutdowns reached PSCI
  SYSTEM_OFF and final write-back; a changed post-exit image was then reopened, booted to
  agent READY, and shut down successfully again.
- Acceptance achieved: installed changes survive reboot, and the explicit post-exit reopen
  chain is preserved under `/Users/user/BridgeVM/post-exit-reopen-boot-20260711-v1/`.

### D5. Lifecycle / pause and resume *(process-resident proof landed; durable suspend open)*
- A live 2026-07-11 `powercfg /a` probe reports no available guest sleep state. Graphics
  disables S1/S2/S3, hibernation is disabled, and firmware does not support S0 Low Power
  Idle. The FADT correctly leaves `LOW_POWER_S0_IDLE_CAPABLE` clear because this platform
  has no matching low-power engine.
- `run-hvf-windows-installed-boot.sh --host-pause-resume-proof-ms 10000` now stops the
  complete running HVF process after the resident agent is ready, verifies the log stays
  frozen for 10 seconds, continues the process, requires another successful agent command,
  and then requires clean PSCI SYSTEM_OFF plus NVMe write-back. The live proof is preserved
  under `/Users/user/BridgeVM/host-pause-resume-wrapper-proof-20260711-v1/`.
- This gate is intentionally labelled `process-resident-host-pause-resume` and records
  `disk_backed_suspend=false`. It proves host pause/continue integrity only; it does not
  serialize RAM, vCPU, interrupt-controller, timer, or device state for later restoration.
- Remaining product decision: either define v1 as having no durable suspend, or implement
  and validate disk-backed state save/restore. If guest-initiated sleep is required too,
  it additionally needs an honest ACPI sleep/wake contract and resolution of the Windows
  Graphics/WDDM blocker.

### D6. Installed-image capacity on import *(packaged and live-proven)*
- The Windows HVF Lab clone/copies the selected RAW disk and UEFI vars without mutating
  the sources, sparsely extends an undersized imported disk to 64 GiB, and records a
  first-boot retry marker.
- Once the new BVAGENT service generation is live, the backend refreshes host-storage
  state and extends C: to Windows' reported supported maximum. The marker is removed only
  after exit 0 plus `BRIDGEVM_DISK_GROW_OK`; a failure or interrupted boot retains it.
- A separate live 24 GiB -> 48 GiB proof grew C: from 25,478,299,648 to
  51,249,135,104 bytes, recovered 24.7 GiB free, cleanly powered off, and left the backup
  GPT at the new final LBA. See
  `docs/windows-arm/evidence/imported-disk-growth-20260712.md`.

## Workstream E — Performance & scale   *(measure first — start only after an install completes)*
Profile and fix *measured* bottlenecks, not guessed ones.
- **E1. SMP / multi-vCPU** — implemented with per-vCPU run-loop threads, GIC
  redistributors, MPIDR affinity, PSCI CPU_ON, and synchronized platform access. A
  2026-07-11 round-robin release matrix produced 9/9 valid runs; median desktop READY was
  40.372s/31.193s/26.137s at 1/2/4 vCPUs. Secondary-vCPU terminal PSCI requests now wake
  and terminate/reset through CPU0 instead of leaving the VM alive.
- **E2. NVMe throughput** — the WIM apply and general IO go through the emulated NVMe.
  The command path now reuses PRP/segment/scratch storage, coalesces adjacent spans, and
  uses a zero-copy host-pointer path when guest RAM exposes one. The byte-identical
  buffered fallback remains independently selectable with
  `BRIDGEVM_NVME_BUFFERED_IO=1`; the probe prints the selected mode so long storage/hash
  diagnostics can compare the two paths without changing media or silently changing the
  production default.
- **E3. Run-loop / MMIO efficiency** — trim per-exit overhead on hot MMIO (xHCI/NVMe
  doorbells, GIC).
- Acceptance: install apply completes in a practical time; desktop is responsive enough
  for daily use (set a concrete target once E1 lands).

## Workstream F — Parallels-like integration   *(far future; optional for v1)*
Not required for "usable Windows". Sequence after D+E only if the product needs it.
- A Windows guest agent + paravirtual channel for dynamic resolution/resize, clipboard
  sharing, shared folders, drag-drop, graceful shutdown, audio. Prior art to reuse/extend:
  `crates/bridgevm-agentd`, `crates/bridgevm-agent-protocol`, `runners/bridgevm-tools-linux`.

---

## Milestone ladder — "완성" = a usable Windows 11 ARM on the HVF engine
1. ✅ **M1 — Setup boots from NVMe** (done this session).
2. ✅ **M2 — Scripted/unattended install completes** (A + B + C): WIM applied, bootable,
   reboots into the installed OS.
3. ✅ **M3 — Installed Windows reaches the desktop** (C3 + reboot-loop): OOBE auto-skipped,
   autologon, desktop rendered.
4. ✅ **M4 — Interactive desktop** (D1 + D2): keyboard + pointer work, display usable.
5. 🟡 **M5 — Connected, persistent, fast enough** (D3 + D4 + E): the core network,
   persistence, clean-shutdown, measured multi-vCPU, and process-resident host-pause gates
   pass. Packaged installed-image import, capacity growth, and app control are present;
   durable suspend (or an explicit no-suspend v1 contract) and a packaged from-scratch
   installer remain product gates.
6. ✅ **M6 — Integration polish** (F, optional): clipboard and shared folders are
   substantive; dynamic resize remains gated on the WDDM path.

**"Usable Windows" = M4 plus basic M5.** Everything past M3 is entry-condition-gated and
evidence-driven — expect a new wall at each rung.

## Definition of done (per milestone)
- Every code change keeps `cargo test -p bridgevm-hvf` and `-p hvf-runner` green; every
  guest-visible change carries a **signed-probe live proof** (ramfb screenshot / trace),
  as this repo already does (see the memory status file + `.omo/ulw-loop/evidence`).
- **M2:** WinPE-scripted install partitions NSID 2, applies the WIM, `bcdboot`s, with no
  `DRIVER_PNP_WATCHDOG`; the probe reboots on SYSTEM_RESET with NVMe + UEFI vars preserved.
- **M3:** installed Windows desktop in a ramfb screenshot.
- **M4:** type text + move/click on the desktop (ramfb proof).
- **M5:** Windows online (IP + page load), files persist across reboot, multi-vCPU active,
  clean shutdown/write-back, and host pause/continue recover the resident agent. Durable
  suspend is a separate acceptance gate unless explicitly excluded from v1.

## Suggested order
**A1→A2→A3** (unblock install) ∥ **B1→B2→B3** (reboots) → **C** (⇒ M3) → **D1, D2** (⇒ M4)
→ **D3, D4, E1** (⇒ M5) → F. A and B are parallelizable. Every workstream past C needs live
boots on this Mac with the signed probe. Keep the philosophy: **implement first, refactor
only where a feature demands it (B's re-entrant `main`, E1's SMP), optimize only after
measuring.**
