# Windows 11 Arm Direction

Document status: **Current engine guide with preserved history**
Last reviewed: **2026-07-22**

The BridgeVM-owned HVF engine now boots an installed Windows 11 ARM64 desktop
without QEMU, including SMP, persistent NVMe, display/input, networking, audio,
resident guest-agent control, restart, and experimental 3D. The remaining
release gates are vTPM/Secure Boot lifecycle, a fresh signed ARM64 driver, live
same-boot product receipts, and distribution signing/notarization. Use
[STATUS.md](../../STATUS.md) for the concise state and the
[Windows completion plan](../hvf-windows-install-completion-plan.md) for the
authoritative sequence.

BridgeVM's Windows 11 Arm goal is a non-QEMU, Mac-lightweight path that can
eventually feel closer to Parallels than to a generic QEMU frontend.

This creates three separate engine tracks:

| Track | Backend | Role |
| --- | --- | --- |
| Compatibility Engine | QEMU + HVF/TCG | Keep broad OS compatibility and current Windows installer evidence. |
| Apple VZ Engine | Apple Virtualization.framework | Fast Mode for Linux/macOS Arm guests. |
| BridgeVM HVF Engine | Apple Hypervisor.framework + BridgeVM VMM | Experimental no-QEMU installed-Windows path; installer and product-completion work remains. |

QEMU remains supported for compatibility. It is not the final Windows 11 Arm
performance architecture.

## Preserved live-evidence boundary (2026-07-12)

The BridgeVM-owned HVF engine is no longer only a firmware research scaffold.
On preserved, cloned media it has booted an already-installed Windows 11 ARM64
desktop with four vCPUs, RAM framebuffer/input, virtio-net, writable NVMe, and
the resident BVAGENT virtio-console service without QEMU. The app-service live
run proved `whoami`, Windows version reporting, a host-to-guest shared-file read,
guest-requested `shutdown.exe /p /f`, PSCI system-off, 190 successful NVMe
writes, 18 flushes, and final disk/vars writeback. See the
[2026-07-12 evidence index](evidence/installed-app-service-20260712.md).

The shipped macOS bundle now contains an isolated `BridgeVMControl.app`, the
installed-Windows wrappers, and a release probe signed with
`com.apple.security.hypervisor`; Settings exposes it as **Windows HVF Lab**.
The lab bundle now carries a signed, pinned swtpm 0.10.1/libtpms 0.10.2 runtime,
its complete rewritten dylib closure, component license notices, and a
SHA-256 manifest. The packaged helper has passed a real encrypted key-FD/socket
startup check. The app also provides an authenticated recovery package,
same-ID migration policy, fresh-TPM clone, and archive-before-reset receipts.
Clean-second-Mac migration and BitLocker recovery are still live release gates;
failure remains closed rather than falling back to a TPM-less VM.
The live Windows TIS path is now separately evidenced: a 120-second cloned run
completed 1,032 TPM commands with no backend or malformed-packet failures and
exercised PCR, capability, session, key-creation, and NV-public operations. The
PPI mailbox was read but not written, so a real PPI action remains open. See the
[2026-07-22 vTPM command-path receipt](evidence/vtpm-windows-command-path-20260722.md).
The lab imports an installed RAW disk and its matching writable UEFI vars by
clone/copy, leaving the selected source files unchanged. Imported disks smaller
than 64 GiB are extended sparsely, then C: is grown through the resident agent
on first boot; the retry marker is cleared only after an exit-0 guest proof. See
the [2026-07-12 disk-growth evidence index](evidence/imported-disk-growth-20260712.md).

The in-process reboot loop now also resets Apple's in-kernel GIC after all
vCPUs stop. A live Windows restart returned `hv_gic_reset=0`, reached a second
agent `READY` in the same VMM process, then powered off cleanly with disk and
vars writeback. See the
[2026-07-12 reboot evidence index](evidence/reboot-gic-reset-20260712.md).

Resident-agent command output is now bounded and chunked above 24 KiB while
remaining strict lockstep. A live 131,072-byte response reassembled to the
exact expected hash, the following command remained aligned, and a separate
256 MiB NVMe create/flush/offline-read check matched guest and host hashes.
See the [2026-07-12 agent/output and storage evidence index](evidence/agent-chunked-output-storage-20260712.md).

The isolated buffered-NVMe comparison subsequently completed WDK and Windows
SDK installation, found native InfVerif/SignTool plus Inf2Cat, shut down with a
status-0 service gate, and matched both 256 MiB files again from a host-side
read-only NTFS mount. See the
[2026-07-12 WDK/SDK and buffered-storage evidence index](evidence/wdk-sdk-buffered-storage-20260712.md).

The test-signed full `viogpu3d` package has also bound live to `DEV_1050` with
status OK. `dxdiag` identified `viogpu_d3d10.dll`, WDDM 1.3, and feature level
10_0; the matching 23,421-event VirGL trace passed the protocol-specific P3
gate before a clean PSCI shutdown and writeback. See the
[2026-07-12 live VirGL/WDDM evidence index](evidence/viogpu3d-virgl-live-20260712.md).

This is still an experimental installed-image path. A packaged from-scratch
installer, the remaining PPI/Secure Boot lifecycle, distributable Windows
3D/WDDM, durable disk-backed suspend, and polished single-surface product UX
remain open.

## Local Installer Baseline

The local Windows installer currently available in this workspace is:

```text
ISO/Win11_25H2_English_Arm64_v2.iso
```

Use it as the baseline image for observing Windows 11 Arm boot requirements.
The first observations can still use the restricted QEMU/HVF path because that
path already reaches Windows Setup and exposes what Windows expects from
firmware, display, input, storage, TPM, and Secure Boot. Those observations feed
the BridgeVM HVF VMM design; they do not make QEMU the target engine.

## Development Order (historical sequence)

1. **Lock the product boundary**
   - Windows 11 Arm auto-selection must stay out of Apple VZ Fast Mode.
   - QEMU must be described as Compatibility Engine only.
   - The Windows lightweight target must be named as a BridgeVM-owned HVF VMM.
   - `bridgevm hvf windows-plan` and `hvf-runner --windows-plan` must remain
     metadata-only and report `QEMU: not used`.
   - `bridgevm hvf machine-plan` and `hvf-runner --machine-plan` must remain
     metadata-only, report `QEMU: not used`, record the minimum HVF machine
     shape, and avoid entering firmware or starting a VM.
   - `bridgevm hvf host-capabilities` and `hvf-runner --host-capabilities`
     query Apple HVF host metadata without creating a VM.
   - `bridgevm hvf vm-probe` and `hvf-runner --vm-probe` must default to
     no-create; explicit opt-in may create and immediately destroy an empty HVF
     VM when the runner is signed with `com.apple.security.hypervisor`, but
     still does not enter firmware, create vCPUs, or boot Windows.
   - `bridgevm hvf vcpu-probe` and `hvf-runner --vcpu-probe` may extend the
     signed opt-in path through one empty vCPU create/destroy lifecycle, but
     still must not call `hv_vcpu_run`.
   - `bridgevm hvf vcpu-run-probe` and `hvf-runner --vcpu-run-probe` may extend
     the signed opt-in path through one pre-canceled `hv_vcpu_run` return, but
     still must not map guest memory, enter firmware, or boot Windows.
   - `bridgevm hvf interrupt-timer-probe` and
     `hvf-runner --interrupt-timer-probe` may extend the signed opt-in path
     through HVF pending IRQ set/get plus virtual timer mask/offset set/get,
     but still must not enter guest code, enter firmware, or boot Windows.
   - `bridgevm hvf memory-map-probe` and `hvf-runner --memory-map-probe` may
     extend the signed opt-in path through one 16 KiB guest RAM map/unmap at
     IPA `0x40000000`, but still must not create vCPUs, enter guest code, enter
     firmware, or boot Windows.
   - `bridgevm hvf guest-entry-probe` and `hvf-runner --guest-entry-probe` may
     extend the signed opt-in path through one mapped `HVC #0` guest-entry exit
     under a watchdog, but still must not enter firmware or boot Windows.
   - `bridgevm hvf guest-exit-loop-probe` and
     `hvf-runner --guest-exit-loop-probe` may extend the signed opt-in path
     through two mapped `HVC` exits with an explicit PC read/advance between
     them, but still must not enter firmware or boot Windows.
   - `bridgevm hvf mmio-read-probe` and `hvf-runner --mmio-read-probe` may
     extend the signed opt-in path through one unmapped `LDR X0, [X1]` read at
     IPA `0x50000000`, but still must not claim any block, network, display, or
     TPM device implementation.
   - `bridgevm hvf mmio-read-emulation-probe` and
     `hvf-runner --mmio-read-emulation-probe` may extend the signed opt-in path
     by injecting `X0=0x123456789abcdef0`, advancing PC, and continuing to
     `HVC #0`, but still must not claim a real device model.
   - `bridgevm hvf mmio-write-emulation-probe` and
     `hvf-runner --mmio-write-emulation-probe` may extend the signed opt-in
     path by capturing `X0=0xfedcba987654321` from an unmapped `STR`, advancing
     PC, and continuing to `HVC #0`, but still must not claim a real device
     model.
   - `bridgevm hvf mmio-serial-device-probe` and
     `hvf-runner --mmio-serial-device-probe` may extend the signed opt-in path
     by capturing a serial data-register write `X0=0x41`, injecting a
     status-register read `X0=0x90`, advancing PC twice, and continuing to
     `HVC #0`, but still must not claim firmware console, block, network,
     display, TPM, or Windows boot support.
   - `bridgevm hvf mmio-block-device-probe` and
     `hvf-runner --mmio-block-device-probe` may extend the signed opt-in path
     by routing VirtIO-MMIO block magic/version/device/vendor identity register
     reads through the BridgeVM MMIO device bus, injecting `0x74726976`, `0x2`,
     `0x2`, and `0x4252564d`, advancing PC across all four MMIO exits, and
     continuing to `HVC #0`, but still must not claim queues, ISO attach, target
     disk IO, persistence, firmware boot, or Windows boot support.
   - `bridgevm hvf mmio-block-queue-probe [--disk <path>|--iso <path>|--writable-disk <path>]` and
     `hvf-runner --mmio-block-queue-probe [--disk <path>|--iso <path>|--writable-disk <path>]` may extend the signed opt-in path by
     routing VirtIO-MMIO block feature, driver feature, queue select/size/ready,
     descriptor/driver/device ring addresses, status, queue notify, interrupt
     status, config generation, and capacity registers through the BridgeVM MMIO
     device bus, seeding one synthetic in-guest-memory read request, completing
     it immediately after `queue_notify`, writing data/status/used-ring state,
     raising used-buffer interrupt status, optionally reading that same live
     completion from a host-file backing at byte offset `0xe00` when `--disk`
     is supplied, from a read-only installer-media backing at byte offset
     `0xe00` when `--iso` is supplied, or through a signed live
     read/write/flush/reopen persistence path when `--writable-disk` is supplied,
     advertising per-backing capacity sectors in the Windows firmware
     installer/target-disk topology, rejecting unexpected `queue_notify`
     IPAs/values, clearing queue/interrupt/avail-index state on status-zero
     reset, advancing PC across mixed read/write exits, and continuing to
     `HVC #0`, but
     still must not claim full persistent boot disk lifecycle, firmware boot, or
     Windows boot support.
   - `bridgevm hvf windows-boot-disk-layout-probe --disk <path> --create` and
     `hvf-runner --windows-boot-disk-layout-probe --disk <path> --create` may
     create a sparse raw Windows 11 Arm target disk without QEMU, Apple VZ, or
     entering HVF, write a protective MBR plus primary/backup GPT, model the
     expected EFI System Partition, Microsoft Reserved partition, and Windows
     Basic Data partition, then reopen the disk and verify the protective MBR,
     GPT header CRCs, GPT partition-entry CRCs, and partition names/ranges. This
     proves the boot-disk layout boundary, not firmware handoff, installer
     partitioning, installed Windows state, reboot persistence, or Windows boot.
   - `bridgevm hvf windows-firmware-handoff-probe --firmware <AAVMF_CODE.fd>
     --vars-template <AAVMF_VARS.fd> --vars <vars.fd> --create-vars` and
     `hvf-runner --windows-firmware-handoff-probe --firmware <AAVMF_CODE.fd>
     --vars-template <AAVMF_VARS.fd> --vars <vars.fd> --create-vars` may
     validate AArch64 UEFI FD and vars-template firmware volume headers, verify
     FV checksums, seed a mutable vars store from the template, reopen it, and
     report planned code/vars pflash IPA slots without QEMU, Apple VZ, GUI
     launch, or entering HVF. This proves the metadata handoff boundary for
     pflash inputs, not reset-vector entry, UEFI Boot Manager execution,
     installer boot, installed Windows state, reboot persistence, or Windows
     boot.
   - `bridgevm hvf windows-pflash-map-probe --firmware <AAVMF_CODE.fd>
     --vars-template <AAVMF_VARS.fd> --vars <vars.fd> --create-vars` and
     `hvf-runner --windows-pflash-map-probe --firmware <AAVMF_CODE.fd>
     --vars-template <AAVMF_VARS.fd> --vars <vars.fd> --create-vars` may load
     verified UEFI code/vars inputs into planned 64 MiB pflash memory images,
     verify copied prefixes, zero padding, non-overlapping IPA ranges, guest RAM
     separation, and device MMIO separation without QEMU, Apple VZ, GUI launch,
     or entering HVF. This proves the pflash memory-image mapping boundary, not
     reset-vector entry, UEFI Boot Manager execution, installer boot, installed
     Windows state, reboot persistence, or Windows boot.
   - `bridgevm hvf windows-pflash-hvf-map-probe --firmware <AAVMF_CODE.fd>
     --vars-template <AAVMF_VARS.fd> --vars <vars.fd> --create-vars` and
     `hvf-runner --windows-pflash-hvf-map-probe --firmware <AAVMF_CODE.fd>
     --vars-template <AAVMF_VARS.fd> --vars <vars.fd> --create-vars` may
     validate the prepared UEFI code/vars pflash images and default to a
     no-live-map opt-in blocker. With `--allow-map` or
     `BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP=1` on a signed runner, they may create
     an empty HVF VM, populate the code/vars pflash buffers, map firmware
     read/execute and vars read/write at the planned IPAs, unmap them, and
     destroy the VM without QEMU, Apple VZ, GUI launch, vCPU creation, or guest
     execution. This proves the pflash HVF map/unmap boundary, not reset-vector
     entry, UEFI Boot Manager execution, installer boot, installed Windows
     state, reboot persistence, or Windows boot.
   - `bridgevm hvf windows-reset-vector-entry-probe --firmware <AAVMF_CODE.fd>
     --vars-template <AAVMF_VARS.fd> --vars <vars.fd> --create-vars` and
     `hvf-runner --windows-reset-vector-entry-probe --firmware <AAVMF_CODE.fd>
     --vars-template <AAVMF_VARS.fd> --vars <vars.fd> --create-vars` may
     validate the prepared UEFI code/vars pflash images and default to a
     no-live-entry opt-in blocker. With `--allow-entry` or
     `BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY=1` on a signed runner, they may
     create an HVF VM, populate and map the code/vars pflash buffers, create one
     vCPU, set PC to the UEFI reset vector, set CPSR to the masked EL1h entry
     state, run once under a watchdog, observe the first exit, classify the Arm
     exception class, report whether PC progressed beyond the reset vector, and
     clean up without QEMU, Apple VZ, or GUI launch. The separate
     `BRIDGEVM_HVF_ALLOW_REAL_EDK2_RESET_VECTOR_ENTRY=1
     tests/integration/windows-arm-hvf-real-edk2-reset-vector-live-opt-in-smoke.sh`
     smoke uses a real AArch64 edk2 pflash image when available and expects PC
     to move beyond the reset vector before the first unhandled exception exit.
     These prove reset-vector entry, first-exit classification, and cleanup
     only, not UEFI Boot Manager execution, installer boot, installed Windows
     state, reboot persistence, or Windows boot.
   - `bridgevm hvf windows-firmware-run-loop-probe --firmware <AAVMF_CODE.fd>
     --vars-template <AAVMF_VARS.fd> --vars <vars.fd> --create-vars` and
     `hvf-runner --windows-firmware-run-loop-probe --firmware <AAVMF_CODE.fd>
     --vars-template <AAVMF_VARS.fd> --vars <vars.fd> --create-vars` may
     validate the prepared UEFI code/vars pflash images and default to a
     no-live-loop opt-in blocker. With `--allow-loop` or
     `BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP=1` on a signed runner, they may
     create an HVF VM, populate and map code read/execute pflash, vars
     read/write pflash, and guest RAM read/write/execute, populate the
     generated FDT platform DTB in guest RAM at `0x40010000`, create one vCPU,
     set PC to the UEFI reset vector, set `X0` to the platform DTB IPA, set
     CPSR to the masked EL1h entry state, run a bounded firmware
     exit-classification loop, optionally map low pflash
     aliases, optionally wire HVF pending IRQ/vtimer controls with
     `--wire-interrupt-timer`, program/report vtimer offset plus
     a future `CNTV_CVAL_EL0` deadline and `CNTV_CTL_EL0=1`, report
     vtimer-exit count, route the VTimer event as PPI 11 / INTID 27 through
     the single-vCPU GIC CPU-interface path, report pending-IRQ injection count,
     the VTimer auto-mask observation, per-exit deadline rearm status, and the
     last timer/IRQ status names, handled MMIO read/write counts, per-device
     MMIO counts, VirtIO `queue_notify` and request-completion counts, handled
     ICC read/write counts, per-`ICC_IAR1`/`ICC_EOIR1`/`ICC_DIR` counts, and last
     `ICC_IAR1`/`ICC_EOIR1`/`ICC_DIR` INTIDs, report the watchdog timeout, ESR
     abort ISS and fault-status details plus
     mapped-region hints, read the `PC` instruction word/hint from mapped
     pflash, record `X0`-`X4`, `CPSR`, and EL1 exception/vector sysreg plus MMU
     translation sysreg snapshots, and clean up without QEMU, Apple VZ, or GUI
     launch. The optional interrupt/timer wiring is for firmware wait-state
     diagnosis only; it is not a GUI, network, TPM, Secure Boot, installer, or
     Windows boot claim. The firmware loop now mirrors the proven timer
     programming boundary when `--wire-interrupt-timer` is requested, including
     minimal timer PPI-to-GIC CPU-interface delivery. The
     signed real-edk2 check with that flag now avoids the previous immediate
     CVAL VTimer storm at the reset vector and reaches the same low-vector
     `PC=0x200` blocker as the non-timer run. A separate signed opt-in HVF
     VTimer exit probe programs Apple Hypervisor.framework virtual timer state,
     unmasks the timer, observes
     `HV_EXIT_REASON_VTIMER_ACTIVATED`, validates the automatic VTimer mask, and
     handles the pending-IRQ/re-unmask boundary. This is still timer/interrupt
     substrate evidence, not UEFI Boot Manager execution,
     installer boot, GUI, network, TPM, Secure Boot support, persistence,
     drivers, or Windows boot. The real-edk2 firmware run-loop now distinguishes
     raw timer events from deliverable guest interrupts: repair/continue paths
     record `vtimer_ppi_pending_recorded=true`,
     `vtimer_irq_line_assertable=false`, `vtimer_gic_group1_enabled=false`,
     `vtimer_gic_priority_mask=0xff`, `vtimer_gic_running_priority=0xff`,
     `vtimer_gic_priority_threshold=0xff`,
     `vtimer_gic_pending_intid=spurious`,
     `vtimer_pending_irq=not attempted`, post-repair
     `vtimer_unmask=not attempted`, and
     `Last pending IRQ set status name: not attempted`, meaning the pending PPI
     is modeled but the guest GIC/ICC state has not yet enabled Group1 delivery
     or selected INTID 27 as a deliverable interrupt. Initial low-vector repair
     still permits
     `vtimer_unmask=HV_SUCCESS` so the diagnostic sequence can complete, while
     post-repair VTimer exits now stop cleanly instead of spinning to
     `--max-exits` or watchdog cancellation. The separate
     `BRIDGEVM_HVF_ALLOW_REAL_EDK2_FIRMWARE_RUN_LOOP=1
     tests/integration/windows-arm-hvf-real-edk2-firmware-run-loop-live-opt-in-smoke.sh`
     smoke uses a real AArch64 edk2 pflash image when available. The current
     frontier is explicit: with `--map-low-pflash-alias` and a 2000 ms
     watchdog, the previous low-PA `translation fault level 2` and watchdog
     cancel frontier are gone in the repair/continue live checks; the
     continue/remap paths now keep the repaired low-vector diagnostic page
     installed, resume the captured `ELR_EL1`/`SPSR_EL1` context through
     `ERET`, and finish with `VTimer exit count: 1`, `Observed exits: 5`,
     `Final PC: 0x20c`, and `Blockers: none`. The resume telemetry now records
     `Low vector diagnostic page resume ELR_EL1 set status name: HV_SUCCESS`,
     `SPSR_EL1 set status name: HV_SUCCESS`, `PC set status name: HV_SUCCESS`,
     and `CPSR set status name: not attempted`, proving the old direct
     `CPSR`/`PC` resume path was removed. The first post-repair exit is now a
     low-vector diagnostic HVC at `PC=0x204`; the current frontier is the
     low-vector diagnostic ERET landing at `PC=0x20c`, not the old
     restored-erased-pflash `PC=0x200` path. The pre-ERET target snapshot now
     proves the loop cause: `ELR_EL1=0x200` points at BridgeVM's installed
     low-vector diagnostic `HVC #1` (`0xd4000022`) on descriptor `0xf8f`, while
     the preserved original slot was erased pflash (`0xffffffffffffffffffffffff`
     / `0xffffffff`). The run-loop still records the
     AArch64 instruction word/hint plus `X0`-`X4` and `CPSR` from the observed
     vCPU state. The
     run-loop now seeds `SP_EL1` to the top of guest RAM and captures
     `VBAR_EL1`, `ELR_EL1`, `ESR_EL1`, `FAR_EL1`, and `SPSR_EL1` to make that
     next diagnosis evidence-backed; the current live observation is
     `VBAR_EL1=0x0`, `ELR_EL1=0x200`, `ESR_EL1=0x86000007`
     (`instruction abort same EL`, `translation fault level 3`), and
     `FAR_EL1=0x200`. It also captures `SCTLR_EL1`, `TCR_EL1`, `TTBR0_EL1`,
     `TTBR1_EL1`, `MAIR_EL1`, `SP_EL1`, a stage-1 leaf descriptor for the
     final PC, and per-address stage-1 walk entries (`table_ipa`, index,
     entry IPA, descriptor, next-table/output metadata) so the guest MMU mapping
     state can be diagnosed from the live run output. A diagnostic-vector run
     can seed `VBAR_EL1=0x08000000` and patch
     the current-EL/SPx vector slot at `0x08000200`; that proves the slot is
     reached but remains non-executable under the live page tables
     (`pc_stage1_leaf_descriptor=0x60000008000c01`, `PXN=true`, `UXN=true`,
     `diagnosis=diagnostic-vector-stage1-xn-permission-fault`). A guest RAM
     diagnostic-vector run can instead seed `VBAR_EL1=0x40000000` and reach
     `PC=0x40000200` with the same HVC instruction present, but that identity
     RAM block is also execute-never
     (`pc_stage1_leaf_descriptor=0x60000040000f0d`, `PXN=true`, `UXN=true`,
     `diagnosis=guest-ram-diagnostic-vector-stage1-xn-permission-fault`).
     Each firmware exit line also renders a stage-1 descriptor sample set and a
     full stage-1 walk trace for low-vector, pflash, guest-RAM, PC, VBAR, ELR,
     FAR, executable diagnostic-vector, and SP addresses; the current real edk2
     sample set shows `0x0`/`0x200` as invalid L3 descriptors,
     firmware reset/vector addresses in XN L2 block `0x60000008000c01`, the
     guest RAM diagnostic vector in XN L2 block `0x60000040000f0d`, and the
     seeded `SP_EL1=0x43fffff0` in XN L2 block `0x60000043e00f0d`. Each exit
     also scans known pflash/guest-RAM ranges for EL1-executable stage-1 leaf
     candidates and records each candidate's `vector_sync_va`,
     `vector_sync_pa`, `vector_sync_instruction`, and `vector_sync_hint` at the
     current-EL/SPx sync slot, then scans 2 KiB-aligned vector-base candidates
     inside each executable leaf while filtering zero/erased slots and reporting
     scanned/suppressed/limit telemetry plus a passive recommended-vector-base
     selection that can feed the opt-in one-shot
     `--try-recommended-vector-base-vbar` redirect experiment; the current real edk2 run finds the low firmware
     pflash alias at `0x200000` as a 2 MiB executable block candidate
     (`descriptor=0x200f8d`, `PXN=false`, `UXN=false`). The redirect experiment
     records requested/attempted/set/source-exit/target/status plus follow-up-exit telemetry, seeds
     a diagnostic vector into the selected base before setting `VBAR_EL1`, and only claims that diagnostic
     vector routing boundary, not boot; the current follow-up reaches `PC=0x200204` with
     `diagnosis=executable-diagnostic-vector-hvc-exit`, routes through `ERET`, and stops at
     `PC=0x20020c` with `diagnosis=executable-diagnostic-vector-eret-landing-hvc-exit`. The opt-in
     `--continue-after-recommended-vector-base-vbar` variant captures the source `ELR_EL1`/`SPSR_EL1`,
     arms an `ERET` resume with `HV_SUCCESS` status names, and still reports the no-repair blocker:
     restoring `ELR_EL1=0x200` returns to the still-faulting low-vector path, so exit 3 repeats the
     recommended-vector diagnostic HVC instead of advancing to UEFI boot. When that same continuation is
     combined with `--repair-low-vector-diagnostic-page --continue-after-low-vector-repair --wire-interrupt-timer`,
     the signed live run now keeps the repaired low-vector diagnostic page installed,
     records the recommended-vector VBAR redirect as requested but not armed, classifies
     the follow-up as `low-vector-diagnostic-page-hvc-exit`, arms the original
     `ELR_EL1`/`SPSR_EL1` context through the diagnostic `ERET`, and stops at the
     low-vector diagnostic landing `PC=0x20c` with `Blockers: none`. Its post-repair
     first-exit telemetry records exit 4 as `HV_EXIT_REASON_EXCEPTION` at `PC=0x204`
     with `diagnosis=low-vector-diagnostic-page-hvc-exit` and
     `interaction=exception:non-mmio`, so this is still diagnostic-vector continuation
     evidence, not firmware device discovery; its separate first post-repair
     device-interaction telemetry skips diagnostic continuation and raw VTimer exits,
     so the first post-repair device interaction remains `not observed`. The VTimer
     exit is still recorded as timer telemetry with `vtimer_ppi_pending_recorded=true`
     and `vtimer_gic_pending_intid=spurious`; it is not counted as MMIO/ICC device
     discovery while firmware has not enabled the guest-visible GIC/ICC Group1 delivery
     path. A fourth diagnostic
     vector mode can also seed that executable candidate, set `VBAR_EL1=0x200000`,
     reach a real `HVC AArch64` exit at `PC=0x200204` with
     `diagnosis=executable-diagnostic-vector-hvc-exit`, handle that HVC, rewrite
     `ELR_EL1` to the executable landing pad, resume through `ERET`, and stop
     cleanly at `PC=0x20020c` with
     `diagnosis=executable-diagnostic-vector-eret-landing-hvc-exit`. A repair
     mode now wires the firmware VTimer deadline path, handles the first
     `HV_EXIT_REASON_VTIMER_ACTIVATED` boundary, patches the real low-vector L3
     stage-1 descriptor at entry IPA `0xc000` from previous descriptor `0x0`
     to `0xf8f`, records whether a repeated low-vector fault appears after
     repair, reaches the low-vector diagnostic `HVC` at `PC=0x204`, routes that
     exit through `ERET`, reaches the landing `HVC` at `PC=0x20c`, then arms a
     one-shot low-vector `ERET` resume back to the captured original
     `ELR_EL1`/`SPSR_EL1` context with explicit `HV_SUCCESS` status telemetry.
     The non-continue proof still stops at the synthetic landing path, while
     `--continue-after-low-vector-repair` now keeps the diagnostic page patched
     instead of restoring the original low-vector bytes, avoids direct `CPSR` resume
     (`Low vector diagnostic page resume CPSR set status name: not attempted`), and
     arms the original context through `ELR_EL1`/`SPSR_EL1` plus the diagnostic
     `ERET`. The signed live smoke records `Low vector diagnostic page slot restored:
     false`, `Observed exits: 5`, `VTimer exit count: 1`, and `Final PC: 0x20c`; the
     current frontier is the repeated low-vector diagnostic HVC/ERET landing
     (`PC=0x204`/`PC=0x20c`), not the old restored-erased-pflash `PC=0x200` path.
     The post-repair first-exit summary now also prints the context that proves
     the loop shape: `instruction=0xd69f03e0` (`eret`), `ELR_EL1=0x200`,
     `ESR_EL1=0x86000007`, and `FAR_EL1=0x200`. In the combined recommended
     VBAR plus low-vector repair proof, that same first-exit context keeps the
     previously set `VBAR_EL1=0x200000`; plain low-vector repair paths can still
     show `VBAR_EL1=0x0`.
     The pre-ERET target snapshot adds the missing target proof: the preserved
     original slot bytes are erased pflash, but the actual `ERET` target at
     `ELR_EL1=0x200` contains the installed diagnostic `HVC #1`
     (`0xd4000022`) on descriptor `0xf8f`. A separate
     `--restore-low-vector-slot-before-eret` opt-in now uses an executable
     pflash `ERET` trampoline, restores the preserved low-vector slot before
     the original-context `ERET`, and proves that target becomes `0xffffffff` /
     `erased-pflash` with `Low vector diagnostic page slot restored: true`,
     `Observed exits: 4`, `VTimer exit count: 2`, and `Final PC: 0x200`.
     This is repair-and-resume timer/vector telemetry, not UEFI Boot Manager,
     installer, or Windows boot. The
     `--remap-low-vector-to-recommended-vector` variant now separates the remap
     primitive from the candidate policy: the descriptor patcher can map the low
     vector L3 page to a recommended vector page, but the run-loop only attempts
     that remap when the candidate has a populated, non-BridgeVM Current EL/SPx
     sync slot. The current real-edk2 recommendation is still the fallback empty
     vector scan (`vector_sync_instruction=0x00000000`), so it is rejected for
     remap and the path falls back to diagnostic-page repair plus the same ERET
     continuation evidence. The remap telemetry currently reports
     `Low vector recommended-vector remap succeeded: false`, target PA
     `not observed`, descriptor `not observed`, and the same `PC=0x204`/`PC=0x20c`
     diagnostic frontier while first device interaction remains `not observed`. Each exit
     line also renders a `diagnosis=` classifier for the
     observed vector/MMU fault pattern. The run-loop now also
     accepts installer ISO plus writable target disk paths as first-class
     no-QEMU metadata, verifies the generated FDT magic before the live handoff,
     reports the platform DTB byte count and `X0` DTB set status, and has a
     first firmware data-abort MMIO routing path for the Windows device window
     through the BridgeVM PL011/PL031, GICv3 distributor/redistributor MMIO
     register skeletons, plus VirtIO-MMIO installer ISO (`0x10002000`,
     read-only) and target disk (`0x10003000`, writable) skeleton bus. The
     GICv3 skeletons are wired for common firmware MMIO register accesses,
     including status and group modifier registers, and the live run-loop now
     has a single-vCPU Group1 `ICC_*` CPU-interface sysreg skeleton for
     `ICC_SRE_EL1`, `ICC_CTLR_EL1`, `ICC_PMR_EL1`, `ICC_BPR1_EL1`,
     `ICC_IGRPEN1_EL1`, `ICC_HPPIR1_EL1`, `ICC_IAR1_EL1`, `ICC_EOIR1_EL1`,
     and `ICC_DIR_EL1`, plus conservative firmware-tolerant `ICC_BPR0_EL1`,
     `ICC_IGRPEN0_EL1`, `ICC_RPR_EL1`, `ICC_AP0R*`/`ICC_AP1R*`, Group0
     spurious, and `ICC_SGI1R_EL1` stubs. Successful VirtIO block
     `queue_notify` completion in the live run-loop now raises used-buffer
     interrupt status, mirrors that status into the matching GICD FDT SPI
     pending bit, gates HVF IRQ line assertion on GICD `EnableGrp1NS`,
     GICD/GICR `IGROUPR` Group1 bits, SPI/PPI enable/pending state, and
     `ICC_IGRPEN1` plus PMR/current-running-priority threshold state, lets
     `ICC_HPPIR1_EL1`/`ICC_IAR1_EL1` choose the highest-priority pending Group1
     interrupt across redistributor PPI and distributor SPI candidates, moves
     the acknowledged INTID active, treats
     `ICC_EOIR1_EL1` as priority drop plus deactivate when `ICC_CTLR_EL1`
     EOImode is clear, requires `ICC_DIR_EL1` for deactivate when EOImode is
     set, refreshes and re-pends level VirtIO sources after actual
     deactivation, can deassert the line when VirtIO ACK/status reset clears
     the source, and reports separate device IRQ line assert/deassert
     count/status fields, handled MMIO read/write counts, per-device MMIO
     counts, VirtIO `queue_notify` and request-completion counts, handled ICC
     read/write counts, per-`ICC_IAR1`/`ICC_EOIR1`/`ICC_DIR` counts, and last
     `ICC_IAR1`/`ICC_EOIR1`/`ICC_DIR` INTIDs; this proves only the
     VirtIO-status-to-GICD-SPI-to-priority-selected-ICC-IAR/EOIR/DIR-to-HVF-line
     skeleton boundary plus minimal timer PPI-to-GIC CPU-interface delivery,
     not full GIC delivery beyond the minimal single-vCPU SPI/PPI paths,
     complete nested preemption, binary-point/List Register behavior, multi-vCPU
     routing, complete deactivation-stack semantics, UEFI Boot Manager handoff,
     or Windows boot.
   - `bridgevm hvf windows-firmware-device-discovery-probe --firmware
     <AAVMF_CODE.fd> --vars-template <AAVMF_VARS.fd> --vars <vars.fd>
     --create-vars` and `hvf-runner --windows-firmware-device-discovery-probe`
     expose the named no-QEMU device-discovery boundary above the firmware
     run-loop. The wrapper forces low pflash alias mapping, low-vector repair,
     post-repair continuation, interrupt/timer wiring, and
     stop-at-first-post-repair-device-boundary policy, then reports whether the
     first post-repair MMIO/ICC device interaction was reached, its status, and
     whether the boundary is ready. The default metadata-safe smokes verify that
     the command stays opt-in blocked, does not launch QEMU, Apple VZ, or GUI
     tooling, copies vars from the template, and reports the boundary as not
     reached. This is the explicit firmware device-discovery gate, not UEFI Boot
     Manager execution, installer boot, installed Windows state, reboot
     persistence, GUI/network/TPM/Secure Boot support, or Windows boot.
   - `bridgevm hvf windows-platform-description-probe --memory-gib 6 --vcpus 4`
     and `hvf-runner --windows-platform-description-probe --memory-gib 6
     --vcpus 4` build the metadata-only FDT platform description without
     entering HVF. The probe verifies FDT magic `0xd00dfeed`, guest RAM at
     `0x40000000`, requested CPU nodes, and PL011/PL031 plus VirtIO-MMIO
     installer ISO (`0x10002000`) and target disk (`0x10003000`) nodes inside
     the `0x10000000..0x20000000` Windows device window, root
     `interrupt-parent` phandle `0x1`, GICv3 distributor/redistributor ranges,
     four ARM arch timer interrupts, and PL011/PL031/VirtIO FDT SPI interrupt
     cells `0..3`, while reporting `ACPI: not implemented`, `fw_cfg: not used`,
     `GIC: described/not emulated`, and `GIC emulated: false`. The firmware
     run-loop now uses that FDT shape for the guest-RAM DTB handoff at
     `0x40010000` and seeds UEFI entry `X0` with that IPA when live execution is
     explicitly allowed. The current next blocker is interrupt-controller
     behavior beyond the single-vCPU Group1 skeleton plus firmware device
     discovery and UEFI Boot Manager handoff.
     This proves bounded firmware execution and blocker classification only,
     not UEFI Boot Manager execution, installer boot, installed Windows state,
     reboot persistence, GUI/network/TPM/Secure Boot support, or Windows boot.
   - `bridgevm hvf virtio-block-request-model-probe` and
     `hvf-runner --virtio-block-request-model-probe` may extend the storage
     model by using VirtIO-MMIO queue setup writes through the MMIO bus plus
     queue notify through the device bus, completing one synthetic in-memory
     `VIRTIO_BLK_T_IN` descriptor chain, writing data/status/used ring state,
     and raising interrupt status without QEMU, Apple VZ, or entering HVF, but
     still must not claim live block IO, ISO attach, persistent boot disk,
     firmware boot, or Windows boot support.
   - `bridgevm hvf virtio-block-file-backing-probe --disk <path>` and
     `hvf-runner --virtio-block-file-backing-probe --disk <path>` may extend
     the storage model by completing one `VIRTIO_BLK_T_IN` descriptor chain
     from a host disk-image file at byte offset `0xe00`, writing
     data/status/used ring state, and raising interrupt status without QEMU,
     Apple VZ, or entering HVF, but still must not claim persistent boot disk
     lifecycle, firmware boot, or Windows boot support.
   - `bridgevm hvf virtio-block-writable-file-backing-probe --disk <path>` and
     `hvf-runner --virtio-block-writable-file-backing-probe --disk <path>` may
     extend the storage model by completing an initial `VIRTIO_BLK_T_IN` read
     from a host disk-image file, then completing one `VIRTIO_BLK_T_OUT` write
     and one `VIRTIO_BLK_T_FLUSH` at byte offset `0xe00`, reopening the host
     file, and verifying the written bytes persisted without QEMU, Apple VZ, or
     entering HVF, but still must not claim full persistent boot disk lifecycle,
     partition install state, firmware boot, or Windows boot support.
   - `bridgevm hvf virtio-block-iso-backing-probe --iso <path>` and
     `hvf-runner --virtio-block-iso-backing-probe --iso <path>` may extend the
     installer media model by completing one `VIRTIO_BLK_T_IN` descriptor chain
     from a read-only ISO backing at byte offset `0xe00`, writing
     data/status/used ring state, then rejecting one `VIRTIO_BLK_T_OUT` write
     request with `S_IOERR` while writing status/used-ring state and raising
     interrupt status without QEMU, Apple VZ, or entering HVF, but still must
     not claim UEFI boot, installer boot, persistent boot disk lifecycle, or
     Windows boot support.

2. **Use QEMU/HVF as the observation bridge**
   - Boot the local Windows 11 Arm ISO through the restricted Compatibility
     profile.
   - Record the exact firmware, disk, display, input, TPM, Secure Boot, and
     installer behaviors Windows requires.
   - Keep this evidence separate from any "fast path" claim.

3. **Define the minimum BridgeVM HVF machine**
   - AArch64 vCPU create/run/exit loop.
   - Guest physical memory map.
   - Interrupt/timer model.
   - UEFI boot handoff plan.
   - Minimal block, network, display, keyboard, pointer, and serial/debug
     devices.

4. **Boot firmware without QEMU**
   - Prove that the BridgeVM HVF runner can enter firmware code and report
     deterministic exits.
   - Add metadata and smoke tests that prove QEMU is not in the process tree.

5. **Boot Windows installer without QEMU**
   - Present the Windows ISO and target disk through BridgeVM-owned devices.
   - Reach the same setup screen currently proven through QEMU/HVF.
   - Preserve graphical/serial evidence and runner metadata.

6. **Complete a usable Windows install**
   - Validate target disk install, first boot, reboot, time sync, networking, and
     basic input/display.
   - Keep Microsoft authorization and Windows licensing claims explicit and
     conservative.

7. **Build Windows guest tools**
   - Windows service for heartbeat, IP, time sync, clipboard, resolution,
     shared-folder channel, file transfer, and diagnostics.
   - Installer/update story.
   - Driver work only where required by the custom device stack.

8. **Optimize toward Parallels-like lightness**
   - Replace polling and heavyweight device paths.
   - Add resource and power policy hooks.
   - Move display presentation into the Metal/displayd path.
   - Treat WDDM/Direct3D-to-Metal as a later R&D track, not an MVP promise.

## Current Gate Status

`Pass` below means the named boundary or live proof is implemented and tested.
It does **not** mean the Windows 11 ARM path is product-complete: the proven
route imports an already-installed image, while installer, security, 3D,
durable suspend, and UX gates remain.

| Gate | Status |
| --- | --- |
| Windows auto mode rejects Apple VZ Fast Mode | Pass |
| `bridgevm hvf windows-plan` metadata boundary | Pass |
| `hvf-runner --windows-plan` metadata boundary | Pass |
| `bridgevm hvf machine-plan` metadata boundary | Pass |
| `hvf-runner --machine-plan` metadata boundary | Pass |
| Apple HVF host capability query boundary | Pass |
| Empty HVF VM create/destroy opt-in boundary | Pass |
| Empty HVF vCPU create/destroy opt-in boundary | Pass |
| Pre-canceled HVF vCPU run/cancel opt-in boundary | Pass |
| HVF pending IRQ/vtimer control opt-in boundary | Pass |
| HVF virtual timer activated exit and firmware deadline wiring opt-in boundary | Pass |
| HVF 16 KiB guest RAM map/unmap opt-in boundary | Pass |
| HVF one-instruction guest entry opt-in boundary | Pass |
| HVF two-exit PC-advance loop opt-in boundary | Pass |
| HVF unmapped MMIO/data-abort read opt-in boundary | Pass |
| HVF injected MMIO read-emulation continuation boundary | Pass |
| HVF captured MMIO write-emulation continuation boundary | Pass |
| HVF tiny PL011 UART MMIO device bus continuation boundary | Pass |
| HVF tiny PL031 RTC multi-device MMIO bus continuation boundary | Pass |
| HVF VirtIO-MMIO block identity multi-device MMIO bus boundary | Pass |
| HVF VirtIO-MMIO block queue/config/address/notify multi-device MMIO bus boundary | Pass |
| HVF VirtIO-MMIO writable block queue live read/write/flush/reopen boundary | Pass |
| BridgeVM-owned writable host-file sector write/flush/reopen model | Pass |
| BridgeVM-owned read-only installer ISO sector-read/write-rejection model | Pass |
| BridgeVM-owned AArch64 UEFI FD/vars pflash handoff verifier | Pass |
| BridgeVM-owned AArch64 UEFI code/vars pflash memory-image mapper | Pass |
| BridgeVM-owned AArch64 UEFI pflash HVF map/unmap opt-in boundary | Pass |
| BridgeVM-owned AArch64 UEFI firmware run-loop minimal priority-selected SPI/PPI-to-GIC CPU-interface delivery | Pass |
| BridgeVM-owned AArch64 UEFI firmware device-discovery boundary | Pass |
| BridgeVM-owned single-vCPU GICv3 `ICC_CTLR_EL1` EOImode EOIR/DIR split boundary | Pass |
| BridgeVM-owned Windows Arm FDT platform-description metadata | Pass |
| HVF runner `com.apple.security.hypervisor` signing helper | Pass |
| Restricted QEMU/HVF installer command shape | Pass |
| Windows Setup reachability through QEMU/HVF | Preserved evidence exists |
| BridgeVM HVF AArch64 vCPU run/exit loop | Pass |
| Installed Windows boot without QEMU | Pass (preserved live evidence) |
| BridgeVM-owned UEFI/boot device path for installed media | Pass (preserved live evidence) |
| Writable NVMe, ramfb/input, and virtio-net installed-media path | Pass (preserved live evidence) |
| Resident BVAGENT command/share/shutdown service | Pass (preserved live evidence) |
| Packaged macOS Windows HVF Lab, wrappers, and signed release probe | Pass |
| Imported RAW clone minimum size and first-boot C: growth | Pass (preserved live evidence) |
| TPM/Secure Boot in custom HVF path | Blocked |
| Windows guest tools/drivers | Partial: resident agent proven; distributable 3D driver remains |
| Windows installer without QEMU | Blocked |
| Disk-backed suspend/resume | Blocked; process-resident pause/resume only |
