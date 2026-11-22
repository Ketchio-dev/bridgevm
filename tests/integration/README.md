# Integration Tests

Executable smoke tests and contracts for the own-HVF CLI, runner, packaging and
app paths live here. Most are metadata-focused so they can run without booting a VM.

Run the current smoke coverage from the repository root:

```sh
tests/integration/windows-arm-hvf-plan-cli-smoke.sh
tests/integration/windows-arm-hvf-runner-cli-smoke.sh
tests/integration/windows-arm-hvf-machine-plan-cli-smoke.sh
tests/integration/windows-arm-hvf-machine-runner-cli-smoke.sh
tests/integration/windows-arm-hvf-boot-disk-layout-cli-smoke.sh
tests/integration/windows-arm-hvf-boot-disk-layout-runner-smoke.sh
tests/integration/windows-arm-hvf-firmware-handoff-cli-smoke.sh
tests/integration/windows-arm-hvf-firmware-handoff-runner-smoke.sh
tests/integration/windows-arm-hvf-pflash-map-cli-smoke.sh
tests/integration/windows-arm-hvf-pflash-map-runner-smoke.sh
tests/integration/windows-arm-hvf-pflash-hvf-map-cli-smoke.sh
tests/integration/windows-arm-hvf-pflash-hvf-map-runner-smoke.sh
tests/integration/windows-arm-hvf-reset-vector-entry-cli-smoke.sh
tests/integration/windows-arm-hvf-reset-vector-entry-runner-smoke.sh
tests/integration/windows-arm-hvf-firmware-run-loop-cli-smoke.sh
tests/integration/windows-arm-hvf-firmware-run-loop-runner-smoke.sh
tests/integration/windows-arm-hvf-firmware-device-discovery-cli-smoke.sh
tests/integration/windows-arm-hvf-firmware-device-discovery-runner-smoke.sh
tests/integration/windows-arm-hvf-platform-description-cli-smoke.sh
tests/integration/windows-arm-hvf-platform-description-runner-smoke.sh
tests/integration/hvf-host-capabilities-cli-smoke.sh
tests/integration/hvf-host-capabilities-runner-smoke.sh
tests/integration/hvf-vm-probe-cli-smoke.sh
tests/integration/hvf-vm-probe-runner-smoke.sh
tests/integration/hvf-vcpu-probe-cli-smoke.sh
tests/integration/hvf-vcpu-probe-runner-smoke.sh
tests/integration/hvf-vcpu-run-probe-cli-smoke.sh
tests/integration/hvf-vcpu-run-probe-runner-smoke.sh
tests/integration/hvf-memory-map-probe-cli-smoke.sh
tests/integration/hvf-memory-map-probe-runner-smoke.sh
tests/integration/hvf-guest-entry-probe-cli-smoke.sh
tests/integration/hvf-guest-entry-probe-runner-smoke.sh
tests/integration/hvf-guest-exit-loop-probe-cli-smoke.sh
tests/integration/hvf-guest-exit-loop-probe-runner-smoke.sh
tests/integration/hvf-mmio-read-probe-cli-smoke.sh
tests/integration/hvf-mmio-read-probe-runner-smoke.sh
tests/integration/hvf-mmio-read-emulation-probe-cli-smoke.sh
tests/integration/hvf-mmio-read-emulation-probe-runner-smoke.sh
tests/integration/hvf-mmio-write-emulation-probe-cli-smoke.sh
tests/integration/hvf-mmio-write-emulation-probe-runner-smoke.sh
tests/integration/hvf-mmio-serial-device-probe-cli-smoke.sh
tests/integration/hvf-mmio-serial-device-probe-runner-smoke.sh
tests/integration/hvf-mmio-rtc-device-probe-cli-smoke.sh
tests/integration/hvf-mmio-rtc-device-probe-runner-smoke.sh
tests/integration/hvf-mmio-block-device-probe-cli-smoke.sh
tests/integration/hvf-mmio-block-device-probe-runner-smoke.sh
tests/integration/hvf-mmio-block-queue-probe-cli-smoke.sh
tests/integration/hvf-mmio-block-queue-probe-runner-smoke.sh
tests/integration/hvf-virtio-block-request-model-cli-smoke.sh
tests/integration/hvf-virtio-block-request-model-runner-smoke.sh
tests/integration/hvf-virtio-block-file-backing-cli-smoke.sh
tests/integration/hvf-virtio-block-file-backing-runner-smoke.sh
tests/integration/hvf-virtio-block-writable-file-backing-cli-smoke.sh
tests/integration/hvf-virtio-block-writable-file-backing-runner-smoke.sh
tests/integration/hvf-virtio-block-iso-backing-cli-smoke.sh
tests/integration/hvf-virtio-block-iso-backing-runner-smoke.sh
tests/integration/hvf-windows-installed-boot-p3-gpu-policy-smoke.sh
tests/integration/hvf-windows-installed-boot-ramfb-display-cadence-smoke.sh
tests/integration/hvf-runner-installed-boot-launch-policy-smoke.sh
```

`hvf-windows-viogpu3d-render-stage-smoke.sh` is intentionally parser-only: it
uses synthetic ARM64 PE headers and a dummy catalog to prove the pinned staging,
INF registration, CopyFiles, stale-metadata rejection, and finalizer ordering
contracts. It does not claim InfVerif, Inf2Cat, SignTool, or live Windows proof.
`hvf-windows-viogpu3d-build-kit-smoke.sh` statically verifies the pinned x64
Windows → ARM64 clang-cl build kit; the actual PowerShell/WDK run remains an
external gate.

The HVF VM probe live opt-in smoke is intentionally outside the metadata-safe
suite. Set `BRIDGEVM_HVF_ALLOW_VM_CREATE=1` and run
`tests/integration/hvf-vm-probe-live-opt-in-smoke.sh` directly when you want to
build/sign `hvf-runner` with `com.apple.security.hypervisor`, then create and
immediately destroy an empty Apple Hypervisor.framework VM without entering
firmware, creating vCPUs, or booting Windows.

Run `tests/integration/hvf-vcpu-probe-live-opt-in-smoke.sh` with the same opt-in
when you also want to create and immediately destroy one empty HVF vCPU. That
still does not call `hv_vcpu_run`, enter firmware, or boot Windows.

The HVF vCPU run/cancel live opt-in smoke is also intentionally outside the
metadata-safe suite. Set `BRIDGEVM_HVF_ALLOW_VCPU_RUN=1` and run
`tests/integration/hvf-vcpu-run-probe-live-opt-in-smoke.sh` directly when you
want to build/sign `hvf-runner`, create an empty HVF VM/vCPU, call
`hv_vcpus_exit` before guest entry, then observe one immediate
`HV_EXIT_REASON_CANCELED` `hv_vcpu_run` return. This proves only the bounded
run-return API boundary, not mapped guest memory, firmware entry, or Windows
boot.

The HVF memory map/unmap live opt-in smoke is outside the metadata-safe suite
too. Set `BRIDGEVM_HVF_ALLOW_MEMORY_MAP=1` and run
`tests/integration/hvf-memory-map-probe-live-opt-in-smoke.sh` directly when you
want to build/sign `hvf-runner`, create an empty HVF VM, allocate one 16 KiB
guest RAM page, map it at IPA `0x40000000`, unmap it, and clean up. This proves
only the guest-memory mapping boundary, not vCPU execution, firmware entry, or
Windows boot.

The HVF guest-entry live opt-in smoke is the first mapped guest-instruction
boundary. Set `BRIDGEVM_HVF_ALLOW_GUEST_ENTRY=1` and run
`tests/integration/hvf-guest-entry-probe-live-opt-in-smoke.sh` directly when
you want to build/sign `hvf-runner`, map one `HVC #0` instruction at IPA
`0x40000000`, set PC/CPSR, run with a watchdog, and require an exception exit.
This proves only bounded guest code entry/exit, not firmware entry or Windows
boot.

The HVF guest-exit-loop live opt-in smoke is the first direct mini VMM loop.
Set `BRIDGEVM_HVF_ALLOW_EXIT_LOOP=1` and run
`tests/integration/hvf-guest-exit-loop-probe-live-opt-in-smoke.sh` directly
when you want to build/sign `hvf-runner`, run `HVC #0`, read and advance PC,
then run `HVC #1` under watchdog protection. This proves only a bounded
run/exit/PC-advance loop, not firmware entry or Windows boot.

The HVF MMIO read live opt-in smoke is the first data-abort/device-model exit
boundary. Set `BRIDGEVM_HVF_ALLOW_MMIO_READ=1` and run
`tests/integration/hvf-mmio-read-probe-live-opt-in-smoke.sh` directly when you
want to build/sign `hvf-runner`, run one `LDR X0, [X1]` against unmapped IPA
`0x50000000`, and observe the exit. This proves only the MMIO exit boundary,
not any implemented device.

The HVF MMIO read-emulation live opt-in smoke is the first minimal device read
continuation loop. Set `BRIDGEVM_HVF_ALLOW_MMIO_EMULATION=1` and run
`tests/integration/hvf-mmio-read-emulation-probe-live-opt-in-smoke.sh`
directly when you want to build/sign `hvf-runner`, handle one unmapped `LDR`
read by injecting `X0=0x123456789abcdef0`, advance PC, continue to `HVC #0`,
and verify the value remains in `X0`. This proves only a tiny injected read
path, not a real device model.

The HVF MMIO write-emulation live opt-in smoke is the matching minimal device
write continuation loop. Set `BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION=1` and
run `tests/integration/hvf-mmio-write-emulation-probe-live-opt-in-smoke.sh`
directly when you want to build/sign `hvf-runner`, handle one unmapped `STR` by
capturing `X0=0xfedcba987654321`, advance PC, continue to `HVC #0`, and verify
the value remains in `X0`. This proves only a tiny captured write path, not a
real device model.

The HVF MMIO serial-device live opt-in smoke is the first tiny PL011 UART-style
multi-register device loop. Set `BRIDGEVM_HVF_ALLOW_MMIO_SERIAL_DEVICE=1` and run
`tests/integration/hvf-mmio-serial-device-probe-live-opt-in-smoke.sh` directly
when you want to build/sign `hvf-runner`, handle one PL011 data-register write
`X0=0x41`, route the write plus flag-register read through the reusable BridgeVM
MMIO device bus, inject one flag read `X0=0x90`, advance PC across both MMIO
exits, and continue to `HVC #0`. This proves only a tiny PL011 UART skeleton,
not firmware console or Windows boot support.

The HVF MMIO RTC-device live opt-in smoke is the first two-device BridgeVM MMIO
bus dispatch. Set `BRIDGEVM_HVF_ALLOW_MMIO_RTC_DEVICE=1` and run
`tests/integration/hvf-mmio-rtc-device-probe-live-opt-in-smoke.sh` directly when
you want to build/sign `hvf-runner`, attach PL011 UART plus PL031 RTC skeletons,
route one PL031 data-register read through the reusable BridgeVM MMIO device
bus, inject `X0=0x20260618`, advance PC, and continue to `HVC #0`. This proves
only a tiny RTC skeleton and multi-device dispatch, not firmware time services
or Windows boot support.

The HVF MMIO block-device live opt-in smoke is the first storage-facing
VirtIO-MMIO identity loop. Set `BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_DEVICE=1` and run
`tests/integration/hvf-mmio-block-device-probe-live-opt-in-smoke.sh` directly
when you want to build/sign `hvf-runner`, attach PL011 UART, PL031 RTC, and a
VirtIO-MMIO block identity skeleton, route the magic/version/device/vendor
register reads through the reusable BridgeVM MMIO device bus, inject
`0x74726976`, `0x2`, `0x2`, and `0x4252564d`, advance PC across all four MMIO
exits, and continue to `HVC #0`. This proves only VirtIO-MMIO block identity
register handling, not queues, ISO attach, boot disk IO, persistence, or Windows
boot support.

The HVF MMIO block queue/config/address/notify live opt-in smoke is the next
storage-facing VirtIO-MMIO register loop. Set
`BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_QUEUE=1` and run
`tests/integration/hvf-mmio-block-queue-probe-live-opt-in-smoke.sh` directly
when you want to build/sign `hvf-runner`, attach PL011 UART, PL031 RTC, and a
VirtIO-MMIO block queue/config/address/notify skeleton, route feature, driver
feature, queue select/size/ready, descriptor/driver/device ring addresses,
status, queue notify, interrupt status, config generation, and capacity
registers through the reusable BridgeVM MMIO device bus, advance PC across
mixed read/write exits, seed one synthetic in-guest-memory read request,
complete it immediately after `queue_notify`, write data/status/used-ring
state, raise used-buffer interrupt status, then repeat the same signed live
HVF `queue_notify` path with `--disk <fixture>` so completion reads sector data
from a host-file backing at byte offset `0xe00`, then again with
`--iso <fixture>` so completion reads sector data from a read-only
installer-media backing at byte offset `0xe00`, then again with
`--writable-disk <fixture>` so completion performs an initial read, one write,
one flush, and a reopen persistence check on a writable host-file backing, and
continue to `HVC #0`.
The default CLI/runner smokes also verify that `--disk`, `--iso`, and
`--writable-disk` are recorded without opening a backend when live opt-in is absent. This proves only
the first live synthetic, host-file-backed, and read-only ISO-backed request
completion paths, not persistent boot disk lifecycle, firmware boot, installer
boot, or Windows boot support.

The Windows Arm HVF platform-description smokes are metadata-safe FDT tests,
not live HVF entry tests. Run
`tests/integration/windows-arm-hvf-platform-description-cli-smoke.sh` and
`tests/integration/windows-arm-hvf-platform-description-runner-smoke.sh` when
you want to verify that BridgeVM can build a QEMU-free, Apple-VZ-free FDT
platform description with magic `0xd00dfeed`, guest RAM at `0x40000000`,
requested CPU nodes, and PL011/PL031 plus VirtIO-MMIO installer ISO
(`0x10002000`) and target disk (`0x10003000`) nodes inside the
`0x10000000..0x20000000` Windows device window. This proves only device
discovery metadata, not DTB handoff into UEFI, GIC emulation, installer boot,
or Windows boot support.
