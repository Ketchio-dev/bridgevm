# Windows display refresh development

The installed Red Hat display-only driver does not implement selectable refresh
timings. The experimental patch in `scripts/patches/virtio-win-viogpudo-refresh.patch`
adds 30, 60, 75, 90, 120, 144, 165, 180, 240, 360, 480, 540, 600 and 720Hz
virtual modes for each resolution. This remains experimental source, **without
a Microsoft production signature**. A development-signed ARM64 candidate has
booted in a disposable Windows clone. General Preview still uses the upstream driver.

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

The patched DOD checks `BVR1` before starting. Like the stock driver, it reports
target timings that do not depend on the source size, but with one target per
rate whose only specified timing is the vertical rate; 120Hz is preferred. Each
target also has a matching monitor mode, so a new host size should not need a
fresh monitor approval (not yet validated in Windows). It commits the pinned target's rate to offset 20, takes the
raster height from the pinned source, and implements both ControlInterrupt and
GetScanLine. Its ISR reports `DISPLAYONLY_VSYNC` only for
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

## Development-only test signing

`scripts/sign-hvf-windows-refresh-test-package.ps1` can prepare a temporary
self-signed package on an elevated disposable Windows builder. It signs the PE before
creating/signing the catalog, exports only the public certificate, checks the
signatures, and removes its temporary builder certificate/key. This does not
produce a Microsoft production signature or change any VM's boot settings.
The hosted workflow accepts an explicit `test_sign` dispatch input, default false;
ordinary PR builds remain unsigned. Only the public test certificate is exported.

Installing that package requires separate authorization to use a disposable VM
with Secure Boot disabled and TESTSIGNING enabled. Never apply that change to
the working VM as part of routine setup or updates.

## Validation and signing still required

Kit preparation compiles and runs the actual portable GetScanLine raster helper,
checking the 720Hz blank interval, frame wrap and a late query after 24 hours.
It also compiles the patched monitor and target enumeration against WDDM
callback fixtures: a source size missing from the monitor snapshot gets the same
per-rate targets with 120Hz preferred, each target has exactly one matching
monitor mode, and a real add-mode failure must survive releasing the unowned
mode record.
Host unit tests cover 2D config-vector delivery at every listed rate, the deadline
boundary, stop/reset, invalid guest rates, and independence from the 3D backend.
They do not establish Windows compatibility or measured frame throughput.

A development-signed ARM64 candidate has enumerated and switched all fourteen
rates in a disposable Windows 11 guest. At 1280x800, eight virtual VBlank waits
per rate gave median intervals of 8.40ms at 120Hz, 7.07ms at 144Hz and 1.40ms
at 720Hz. These samples do not measure unique rendered frames or physical output.
The guest resize helper now retains the selected rate across geometry changes.

The experimental DOD reserves 64MiB of framebuffer headroom, but attaches only
the pages used by each resource. Attaching the entire reserve needs 66 direct
queue descriptors and exceeds the 64-entry control queue; the host receives no
backing command and displays black pixels. Validate actual visible updates as
well as successful mode API results. After the resource-page fix, a single run at
3600x2260 passed all fourteen mode changes and VBlank waits (eight samples/rate;
medians 120=8.34ms, 144=6.86ms, 720=1.43ms). Earlier builds had black frames. A later native-app run after Windows Update
passed all fourteen mode changes but its first 30Hz VBlank wait returned timeout;
the other 13 rates waited successfully. Idle/power transitions still need broader
stability testing.

Before production distribution: compile with the WDK, verify the INF/catalog, submit through
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
