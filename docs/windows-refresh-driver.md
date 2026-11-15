# Windows display refresh development

The installed Red Hat display-only driver does not implement selectable refresh
timings. The experimental patch in `scripts/patches/virtio-win-viogpudo-refresh.patch`
adds 30, 60, 75, 90, 120, 144, 165, 180, 240, 360, 480, 540, 600 and 720Hz
virtual modes for each resolution. This is source under development, **not a
signed or boot-validated driver**. General Preview still uses the upstream driver.

## Device contract

The standard virtio-gpu configuration prefix (offsets 0–15) is unchanged.
The BridgeVM-specific extension uses aligned little-endian 32-bit registers:

| Offset | Register | Behaviour |
| --- | --- | --- |
| 16 | `BVR1` magic | Read-only, protocol version 1 |
| 20 | Virtual refresh Hz | Read/write, integer 30–720, initial 120 |
| 24 | Retrace interrupt enable | Read/write, 0 or 1, initial/reset 0 |
| 28 | Maximum virtual Hz | Read-only, 720 |

`events_read` bit 31 means a virtual retrace occurred. The guest acknowledges
it through the standard `events_clear` register. Delivery uses the config MSI-X
vector or the config ISR bit for INTx. DISPLAY events remain independent.
The host's deadline waker remains idle until enabled; it uses nanosecond
intervals and an absolute schedule, resynchronizing after long gaps without
issuing a burst of missed frames. Reset disables retrace interrupts.

The patched DOD checks `BVR1` before starting, enumerates explicit pixel/H/V
timings, commits the pinned target's rate to offset 20, and implements both
ControlInterrupt and GetScanLine. Its ISR reports `DISPLAYONLY_VSYNC` only for
the retrace event. Ordinary queue completions do not become vsync notifications.
Power-down/reset stop the host source; no guest timer or DPC rearm loop is used.

These are **virtual** timings. Actual presentation is bounded by the Mac's
screen, compositor, frame production and scheduling. Selecting 720Hz does not
make a 120Hz panel display 720 distinct frames each second.

## Build an unsigned candidate

The upstream virtio-win source is pinned to commit `47e330f26153619255c3190c6aa8810cd9e7b7fd`.
Export it without local edits or credentials:

```sh
scripts/prepare-hvf-windows-refresh-build-kit.sh \
  --source-dir /path/to/kvm-guest-drivers-windows --out-dir /path/to/new-kit
```

On a Windows builder with VS2022 ARM64 C++ tools and an already installed WDK,
run the kit's `build.ps1`. It builds VirtioLib and viogpudo, verifies the ARM64
PE machine type, and copies unsigned SYS/INF plus the upstream licence.

An optional `-UseNuGet` route pins Microsoft's WDK and SDK packages to
10.0.26100.6584. Those packages require Microsoft licence acceptance; authorize
that installation before using the option. The script neither creates a signing
certificate nor installs the driver nor changes Secure Boot/test-signing settings.

## Validation and signing still required

Kit preparation compiles and runs the actual portable GetScanLine raster helper,
checking the 720Hz blank interval, frame wrap and a late query after 24 hours.
Host unit tests cover 2D config-vector delivery at every listed rate, the deadline
boundary, stop/reset, invalid guest rates, and independence from the 3D backend.
They do not establish Windows compatibility or measured frame throughput.

Before installation: compile with the WDK, verify the INF/catalog, submit through
the owner's Microsoft Hardware Developer account, and obtain a valid production
signature. The upstream Red Hat catalog cannot sign a modified binary. Apple's
app certificate cannot sign a Windows kernel driver.

Validate the signed candidate in a disposable VM clone: boot, mode enumeration,
all rate switches, dynamic resolution, enable/disable, power transitions, stop and
restart. Measure guest retrace and presentation intervals separately from physical
screen output. Do not replace the working signed driver in a user's VM first.

Primary references: [Microsoft DOD initialization](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/dispmprt/ns-dispmprt-_kmddod_initialization_data),
[interrupt notification fields](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/d3dkmddi/ns-d3dkmddi-_dxgkargcb_notify_interrupt_data),
[WDK NuGet installation](https://learn.microsoft.com/en-us/windows-hardware/drivers/install-the-wdk-using-nuget).
