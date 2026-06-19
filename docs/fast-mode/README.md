# Fast Mode

Fast Mode is the narrow, optimized engine path.

Initial supported guests:

- Ubuntu Arm64
- Fedora Arm64
- Debian Arm64
- macOS Arm guests

Windows is deliberately not listed here: the current Apple Virtualization.framework
Fast Mode path targets Linux/macOS Arm guests. Windows 11 Arm uses the restricted
QEMU/HVF Compatibility Mode path today, with any dedicated non-QEMU Windows fast
path treated as long-term custom VMM R&D.

That distinction is a product boundary, not branding. QEMU/HVF is useful for
Windows installer reachability and compatibility evidence, but keeping Windows
on QEMU means BridgeVM remains in the same broad performance class as other
QEMU/HVF frontends. The Parallels-like Windows goal requires a BridgeVM-owned
VMM on Apple Hypervisor.framework plus custom device, display, integration, and
guest-tools layers.

Fast Mode intentionally rejects Windows guests, x86 guests, legacy BIOS,
arbitrary QEMU devices, and custom CPU models.

## Parallels-class scope

Fast Mode is the BridgeVM path that is allowed to chase Parallels-like
lightness. Compatibility Mode can be polished, but it is still the QEMU/HVF
expert path.

The four product axes are tracked separately:

| Axis | Fast Mode target | Current implementation |
| --- | --- | --- |
| Native macOS integration | Clipboard, folders, drag/drop, dynamic resize, app/window control, and eventually host-side proxy windows. | Clipboard and dynamic resize effects are live-proven through guest tools; VZ shared-folder device is wired; Linux app/window control has real `.desktop`/`gio`/`gtk-launch`/`wmctrl` backends, including a preserved Ubuntu arm64 QEMU/HVF live pass of `guest-tools-app-window-live-gui-opt-in-smoke.sh`. Daemon-produced RGBA crop artifacts can be attached to real window payloads for the macOS proxy shell, and the shell can forward pointer/key input through the guest-tools `WindowInput` path to `xdotool` when present. The proxy shell also refreshes the crop summary as well as the raw crop file, debounces host move/resize changes into `SetWindowBounds`, backed by `wmctrl -ir ... -e ...` on X11, and maps user-closing the proxy shell to guest `CloseWindow`. Apple VZ display launches can now export a whole-view raw RGBA file, and daemon-owned, CLI, or app-direct Show Display runner metadata can auto-supply that file as the crop source once it exists; the real-backend socket smoke verifies the env-unset app-direct metadata fallback against the default `metadata/apple-vz-display-framebuffer.rgba` path. Visible live app-direct crop evidence and true per-window streaming are still separate gates. |
| Apple Silicon path | Prefer Apple Virtualization.framework for Linux/macOS Arm guests and avoid QEMU on the high-performance path. | Apple VZ helper can live boot/suspend/resume supported Linux Arm64 fixtures, open a real `VZVirtualMachineView` display window, and optionally export that view as raw RGBA bytes. |
| Intelligent resources | Automatic CPU/RAM/display pacing based on profile, foreground/background, battery, thermal, and workload signals. | Launch policy and runtime policy metadata exist; display pacing and Apple VZ display policy IPC can consume the recorded policy, but live CPU/RAM reapply is still blocked. |
| Graphics acceleration | Native display pipeline with Metal frame pacing; Windows 3D acceleration only after dedicated graphics R&D. | The first native VZ GUI display is proven; a full Metal compositor and Direct3D-to-Metal/WDDM layer are not implemented. |
| Windows 11 Arm lightness | Replace the QEMU Windows core path with a custom Apple Hypervisor.framework VMM when the device and driver stack exists. | QEMU/HVF installer reachability exists as Compatibility evidence only. The no-QEMU Windows path now has a metadata-only HVF machine-plan gate plus signed opt-in empty HVF VM, vCPU lifecycle, pre-canceled vCPU run/cancel, pending IRQ/vtimer control, a signed opt-in HVF VTimer exit probe that programs Apple Hypervisor.framework virtual timer state, observes `HV_EXIT_REASON_VTIMER_ACTIVATED`, and handles the mask/pending IRQ boundary, 16 KiB guest RAM map/unmap, one-instruction guest-entry, two-exit PC-advance, unmapped MMIO read, injected MMIO read-emulation, captured MMIO write-emulation, a tiny PL011 UART-style device, a two-device PL011+PL031 RTC MMIO bus probe, VirtIO-MMIO block identity register probe, a live opt-in VirtIO-MMIO block queue/config/address/notify probe that seeds and completes one synthetic in-guest-memory read request after `queue_notify`, optional signed opt-in `--disk`, `--iso`, and `--writable-disk` variants that complete that same live `queue_notify` path from host-file, read-only installer-media, and writable read/write/flush/reopen backings, one default MMIO-bus/device-bus in-memory VirtIO block read request model probe, one default host-file-backed block read model probe, one default writable host-file-backed VirtIO block write/flush/reopen persistence model probe, one default read-only ISO-backed block read/write-rejection model probe, a sparse raw GPT/ESP/MSR/Windows boot-disk layout writer/verifier, an AArch64 UEFI FD/vars pflash handoff verifier, an AArch64 UEFI code/vars pflash memory-image mapper, an opt-in pflash HVF map/unmap boundary for the prepared firmware and vars slots, an opt-in UEFI reset-vector first-entry/first-exit boundary with exception-class reporting, and an opt-in bounded UEFI firmware run-loop boundary that maps pflash plus guest RAM, populates the generated FDT platform DTB at `0x40010000` with PL011/PL031 plus VirtIO-MMIO installer ISO at `0x10002000` read-only and target disk at `0x10003000` writable, seeds `X0` with that DTB IPA, seeds `SP_EL1`, can map low pflash aliases, can optionally wire HVF pending IRQ/vtimer controls with `--wire-interrupt-timer` and report vtimer offset/exits, pending IRQ injection counts, and timer/IRQ status names, can seed pflash, guest RAM, or executable-candidate diagnostic VBAR/vector slots, and reports watchdog timeout plus abort/mapped-region details, `PC` instruction word/hint, `X0`-`X4`/`CPSR`, EL1 exception/vector sysregs, EL1 MMU translation sysreg snapshots, PC stage-1 leaf descriptor/XN bits, stage-1 descriptor samples and walk entries for low-vector/pflash/guest-RAM/executable-vector/PC/VBAR/ELR/FAR/SP addresses, an EL1-executable leaf candidate scan with vector-sync VA/PA/instruction/hint telemetry plus 2 KiB-aligned vector-base scan/suppression/limit telemetry, passive recommended-vector-base selection, opt-in one-shot recommended-vector-base `VBAR_EL1` set and follow-up-exit telemetry via `--try-recommended-vector-base-vbar`, and an automatic `diagnosis=` classifier. A real edk2 pflash smoke now maps the low pflash aliases, observes PC progress beyond the reset vector to final PC `0x200`, and stops at watchdog-driven `HV_EXIT_REASON_CANCELED` with VA/PA classified inside the low firmware pflash alias while capturing the final-PC instruction, registers, EL1 exception/vector sysregs, and MMU translation sysregs from the canceled vCPU state; the current default pflash observation is `instruction=0xffffffff` / `instruction_hint=erased-pflash` with `VBAR_EL1=0x0`, `ELR_EL1=0x200`, `ESR_EL1=0x86000007` (`instruction abort same EL`, `translation fault level 3`), and `FAR_EL1=0x200`. The descriptor sample set plus walk trace shows low vector `0x0`/`0x200` as invalid L3 descriptors, firmware reset/vector addresses in XN L2 block `0x60000008000c01`, guest RAM vector addresses in XN L2 block `0x60000040000f0d`, and seeded `SP_EL1=0x43fffff0` in XN L2 block `0x60000043e00f0d`; the executable scan finds the low firmware pflash alias at `0x200000` as a 2 MiB executable block candidate (`descriptor=0x200f8d`, `PXN=false`, `UXN=false`), the recommended-vector-base VBAR run records requested/attempted/set/source-exit/target/status plus diagnostic-vector-populated and follow-up-exit telemetry, seeds the selected base before setting `VBAR_EL1`, reaches `PC=0x200204` with `diagnosis=executable-diagnostic-vector-hvc-exit`, routes through `ERET`, and stops at `PC=0x20020c` with `diagnosis=executable-diagnostic-vector-eret-landing-hvc-exit`; the executable-candidate diagnostic-vector run sets `VBAR_EL1=0x200000`, reaches a real `HVC AArch64` exit at `PC=0x200204` with `diagnosis=executable-diagnostic-vector-hvc-exit`, handles that exit, rewrites `ELR_EL1` to the executable landing pad, resumes through `ERET`, and stops cleanly at `PC=0x20020c` with `diagnosis=executable-diagnostic-vector-eret-landing-hvc-exit` and no unsupported-exit blocker. The low-vector repair run now patches the real low-vector L3 stage-1 descriptor at entry IPA `0xc000` to `descriptor=0xf8f`, reaches `PC=0x204` with `diagnosis=low-vector-diagnostic-page-hvc-exit`, routes through `ERET`, and stops at `PC=0x20c` with `diagnosis=low-vector-diagnostic-page-eret-landing-hvc-exit`; the combined continuation plus repair proof now keeps the repaired low-vector diagnostic page installed, arms the captured `ELR_EL1`/`SPSR_EL1` context through diagnostic `ERET`, records `Low vector diagnostic page resume CPSR set status name: not attempted`, and stops at `PC=0x20c` after 5 observed exits with `VTimer exit count: 1`; the pre-ERET target snapshot now proves that `ELR_EL1=0x200` points at BridgeVM's installed diagnostic `HVC #1` (`0xd4000022`) on descriptor `0xf8f`, while the preserved original slot remains erased pflash. A separate `--restore-low-vector-slot-before-eret` opt-in uses an executable pflash `ERET` trampoline, restores the preserved low-vector slot before the original-context `ERET`, and proves the target becomes `0xffffffff` / `erased-pflash` with `Low vector diagnostic page slot restored: true`, `Observed exits: 4`, `VTimer exit count: 2`, and `Final PC: 0x200`. This is timer/vector telemetry rather than MMIO device discovery. The pflash diagnostic-vector run proves `VBAR_EL1=0x08000000` reaches `PC=0x08000200`, and the guest RAM diagnostic-vector run proves `VBAR_EL1=0x40000000` reaches `PC=0x40000200`, but both remain execute-never under the live stage-1 tables. The current next blocker is the VMM/firmware bootstrap above the repaired vector path: a Windows/UEFI-credible GIC path, firmware device discovery, and UEFI Boot Manager handoff. It still has no UEFI Boot Manager execution, no installer boot, no installed Windows persistence, no Windows boot, no live Windows fast path, no GUI/network/TPM/Secure Boot support in the custom HVF path, and remains outside Apple VZ Fast Mode. |

This means Fast Mode is already structurally different from Compatibility Mode,
but "Parallels-like" remains a staged goal rather than a finished claim.
It also means BridgeVM does not currently claim a Parallels-like Windows
no-QEMU fast path: Windows remains outside the Apple VZ Fast Mode support
boundary.

The current custom Windows HVF frontier is now named as
`windows-firmware-device-discovery-probe`: a no-QEMU wrapper around the bounded
UEFI firmware run-loop that forces the low-vector repair/continue and
interrupt/timer policies needed to stop at the first post-repair device
interaction. The default safe smokes still report that boundary as not reached,
so this remains telemetry and gating work rather than a usable Windows fast
path.

## Implemented scaffold

The Rust `bridgevm-apple-vz` crate builds a dry-run Apple Virtualization Framework launch spec from a Fast Mode manifest. This keeps Fast Mode on a separate execution path instead of routing it through QEMU.

Apple VZ launch preflight runs at the `build_fast_plan` boundary. It accepts only Fast Mode manifests with an Arm guest arch (`arm64` or `aarch64`), unset or `apple-vz` preferred backend, `nat` networking, a supported Apple VZ guest family, and a primary disk format of `raw` or `qcow2` for dry-run planning. The live readiness gate is intentionally stricter today: the signed Swift runner can actually start only `linux-kernel` boot with a `raw` primary disk, so installer/macOS-restore boot modes and `qcow2` disks are surfaced as structured launch blockers rather than being reported as live-ready.

A launch-readiness/preflight gate belongs immediately after that dry-run spec is built and before any default Apple VZ process start. Its job is still not to launch the guest. It converts the dry-run plan into structured readiness output: missing installer/kernel/initrd/restore media, missing active disk files, disk formats that cannot be launched, and platform or backend support blockers are visible as named blockers that CLI, daemon, dashboard, and tests can consume. The same readiness object travels with dry-run runner metadata so `prepare-run`, Fast Mode `run` without spawn, and daemon `runner-status` all report the same blocked or ready state.

Shared networking policy is being staged in `bridgevm-network`. That crate can
build a backend-neutral `NetworkPlan` with mode capabilities, hostname, and
validated port-forward rules. Fast Mode still accepts only NAT at the Apple VZ
launch preflight boundary today, but the shared planner now records the intended
host-only and isolated boundaries separately from launcher integration.

Safe readiness and metadata commands do not start a live VM, Apple VZ, QEMU, or
a GUI:

```bash
bridgevm templates
bridgevm create ubuntu-dev --template ubuntu-arm64-installer
bridgevm boot-media ubuntu-dev
bridgevm media import ubuntu-dev --source ~/Downloads/ubuntu-arm64.iso
bridgevm boot-media ubuntu-dev
bridgevm media status ubuntu-dev
bridgevm media download-plan ubuntu-dev --url https://example.invalid/ubuntu.iso --sha256 <digest>
bridgevm media download ubuntu-dev
bridgevm media verify ubuntu-dev --sha256 <digest>
bridgevm prepare-run ubuntu-dev
bridgevm run ubuntu-dev
bridgevm runner-status ubuntu-dev
bridgevm ssh ubuntu-dev --user ubuntu
lightvm-runner ubuntu-dev --print-plan
lightvm-runner ubuntu-dev --require-ready --print-plan
lightvm-runner --launch-spec .vmbridge/metadata/apple-vz-launch.json --print-handoff
lightvm-runner --launch-spec .vmbridge/metadata/apple-vz-launch.json --require-ready --launch
```

For the current limited live Linux Apple VZ shape, create the VM with a raw
primary disk up front instead of editing the manifest after creation:

```bash
bridgevm create live-vz-linux \
  --os debian \
  --arch arm64 \
  --mode fast \
  --boot-mode linux-kernel \
  --kernel-path boot/vmlinuz \
  --initrd-path boot/initrd \
  --kernel-command-line "console=hvc0 root=/dev/vda" \
  --disk 64MiB \
  --disk-format raw
```

`prepare-run` will create the missing sparse `disks/root.raw` file directly and
can report the launch spec as ready once the kernel/initrd paths exist.

For a first-class "try the Apple engine Linux path" staging flow, use the demo
stager. It uses `bridgevm create ... --disk-format raw`, copies the Debian
kernel/initrd/raw-disk fixtures into the real VM bundle, and runs
`prepare-run`. It does not launch Apple VZ:

```bash
scripts/stage-vz-linux-demo-vm.sh --prepare-fixture --name vz-linux-demo
```

When staging succeeds, the VM has a ready
`metadata/apple-vz-launch.json` and runner metadata built from
`lightvm-runner --launch-spec <bundle>/metadata/apple-vz-launch.json`. The
manual GUI step is then:

```bash
export BRIDGEVM_APPLE_VZ_RUNNER="$(apps/macos/scripts/build-sign-apple-vz-runner.sh)"
cargo run --quiet -p bridgevm-cli -- display vz-linux-demo --width 1280 --height 800
```

For Ubuntu, the first artifact-preparation gate uses official Ubuntu Arm64 cloud
image pieces and produces a stager-compatible fixture directory:

```bash
eval "$(scripts/prepare-vz-ubuntu-cloudimg-fixture.sh)"
scripts/stage-vz-ubuntu-desktop-vm.sh \
  --kernel "$BRIDGEVM_UBUNTU_VZ_KERNEL" \
  --initrd "$BRIDGEVM_UBUNTU_VZ_INITRD" \
  --raw-disk "$BRIDGEVM_UBUNTU_VZ_RAW_DISK" \
  --kernel-command-line "$BRIDGEVM_UBUNTU_VZ_KERNEL_CMDLINE" \
  --name ubuntu-cloudimg-vz
```

This path builds `root.raw` as a whole-disk ext4 image, so its kernel command
line is `console=hvc0 root=/dev/vda rw`, not the older convenience assumption
`root=/dev/vda2`. It also writes `artifacts.json` with source digests, raw-disk
format, root layout, kernel command line, detected module version, and whether a
desktop stack was seen. The default Ubuntu cloud image is server-first; a ready
Apple VZ launch spec is still not a claim that Ubuntu Desktop GUI packages are
installed.

If the local input is an existing Ubuntu Arm64 qcow2 image, use the boot-artifact
preparer instead. It converts qcow2 to raw with `qemu-img` as an offline file
operation, uses a Docker extraction boundary to locate the Ubuntu root
filesystem, copies the kernel/initrd from that same root, normalizes gzip
`vmlinuz-*` kernels into the uncompressed arm64 kernel Image shape accepted by
Apple VZ, derives a `root=UUID=...` kernel command line, and writes
`artifacts.json`:

```bash
scripts/prepare-vz-ubuntu-arm64-boot-artifacts.sh \
  --source-image target/live-images/noble-server-cloudimg-arm64.img \
  --output-dir target/vz-ubuntu-arm64-artifacts/noble \
  --disk-size 32G

scripts/stage-vz-ubuntu-desktop-vm.sh \
  --fixture-dir target/vz-ubuntu-arm64-artifacts/noble \
  --kernel-command-line "$(jq -r .kernel_command_line target/vz-ubuntu-arm64-artifacts/noble/artifacts.json)" \
  --name ubuntu-qcow2-vz
```

This still does not run QEMU as a VM backend. `qemu-system-*`, `AppleVzRunner`,
and GUI launch are outside the preparation gate. The actual live Apple VZ proof
remains an explicit operator step after the bundle is staged. The
`docker-offline` backend requires the selected Docker image to already exist
locally; pass `--allow-docker-pull` only when a network pull is intentional.

Manual live proof commands are intentionally separate and require explicit
operator opt-in plus auditable evidence:

```bash
eval "$(tests/integration/prepare-apple-vz-debian-fixture.sh)"
export BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1
tests/integration/apple-vz-live-boot-opt-in-smoke.sh
```

The live smoke prints the actual preserved evidence path. Treat a live run as
proof only after reviewing the smoke output and verifying the evidence bundle.

`bridgevm prepare-run ubuntu-dev` and Fast Mode `bridgevm run ubuntu-dev` without spawn currently record dry-run `lightvm` runner metadata. They also prepare primary disk metadata: BridgeVM resolves the manifest's primary disk, creates the disk directory, writes `metadata/primary-disk.json`, writes the dry-run Apple VZ launch spec to `metadata/apple-vz-launch.json`, and embeds the disk preparation result plus `launch_spec_path` in `metadata/runner.json`. That runner metadata includes the `AppleVzLaunchSpec.readiness` result so a missing installer, missing active disk, unsupported disk format, or unsupported host/guest/backend capability is visible before any launcher boundary. Runner readiness blockers preserve an affected path when the blocker belongs to a file, or an affected capability when the blocker belongs to host/runtime support. `bridgevm runner-status ubuntu-dev` reads back the same readiness state from RunnerStatus instead of inventing a separate status shape.

The macOS dashboard creation sheet uses the same daemon-backed template flow. It loads boot templates with `list_templates`, sends `create_vm` for the selected template, and leaves the new VM stopped. From there the real next step is boot media readiness: inspect `media status`, import a local installer or restore image, verify an existing file, or record a download plan.

`bridgevm boot-media ubuntu-dev` is a focused inspection command for the same boot portion of the Apple VZ dry-run plan. It resolves manifest/template boot media against the `.vmbridge` bundle and prints the installer, kernel, initrd, or macOS restore path with its `exists` state. It does not download media, prepare disks, or require users to inspect the full `prepare-run` or `lightvm-runner --print-plan` JSON.

`bridgevm media import ubuntu-dev --source <path>` is the local media handoff for files the user already has. It uses the same Apple VZ dry-run boot media resolution as `boot-media`, then copies the provided installer, kernel, initrd, or macOS restore file into the expected path inside the `.vmbridge` bundle. This is a safe intermediate step for OS download/template flows: a template-created VM can report `exists: false`, accept a user-provided local file, and then report `exists: true` on the next `boot-media` check. It does not download installer or restore media.

`bridgevm media status ubuntu-dev` summarizes the Fast Mode boot media contract. For each installer, kernel, initrd, or macOS restore entry, it reports the resolved bundle path, whether the file exists, its current file size when present, the latest local import record, the latest verification result, the latest download plan, and the latest download result. Import records are stored under `.vmbridge/metadata/boot-media/<kind>.json`, verification records under `<kind>-verify.json`, download plans under `<kind>-download.json`, and download results under `<kind>-download-result.json`.

`bridgevm media download-plan ubuntu-dev --url <url> [--sha256 <hex>]` records a remote media download intent without performing the network download. It resolves the same installer, kernel, initrd, or macOS restore destination inside the `.vmbridge` bundle, reports the caller-provided URL, optional expected SHA-256, current file existence and size, and the latest import/verify status, and writes that plan under `.vmbridge/metadata/boot-media/<kind>-download.json`. When more than one boot media path exists, use `--kind installer-image|kernel|initrd|macos-restore-image` to choose the planned destination.

`bridgevm media download ubuntu-dev` executes the recorded download plan. It does not accept or choose a URL itself; it reads `.vmbridge/metadata/boot-media/<kind>-download.json`, confirms the current resolved destination still matches the plan, fetches the planned URL to a temporary file, verifies the optional planned SHA-256 digest, moves the file into place, and records the result under `.vmbridge/metadata/boot-media/<kind>-download-result.json`. When more than one boot media path exists, use `--kind installer-image|kernel|initrd|macos-restore-image` to select the planned download to execute.

`bridgevm media verify ubuntu-dev --sha256 <hex>` verifies a resolved Fast Mode boot media file against an expected SHA-256 digest supplied by the user. It computes the digest of the already-resolved installer, kernel, initrd, or macOS restore file, reports whether the digest matched, and writes the result under `.vmbridge/metadata/boot-media/<kind>-verify.json`. It does not download media. When more than one boot media path exists, use `--kind installer-image|kernel|initrd|macos-restore-image` to select the entry to verify.

`lightvm-runner ubuntu-dev --print-plan` prints the structured `AppleVzLaunchSpec` JSON that the Apple VZ launcher boundary consumes. With `--write-metadata`, it writes the same spec to `metadata/apple-vz-launch.json` and records that path in `metadata/runner.json` as `launch_spec_path`. With `--require-ready`, it writes the launch spec artifact and exits with a readiness error if any blocker remains; this is a launcher-boundary gate, not a real Apple VZ process start by itself. The spec includes guest identity, boot mode, installer/kernel/restore-image inputs with resolved paths and existence flags, active disk path and format, resources, device flags, integration flags, log paths, and launch readiness. Readiness covers both bundle inputs and host launch capability: missing disk/media paths remain path blockers, while a non-macOS host or non-Apple-Silicon host is reported as a capability blocker. A ready result means the dry-run inputs and host capability look launchable, while a blocked result preserves each blocker with its kind, affected path or capability, and a caller-facing remediation hint. The same readiness result is copied into dry-run runner metadata rather than being recomputed differently by the CLI, daemon, or dashboard.

`lightvm-runner --launch-spec <path>` is the first artifact-consumer boundary for
the Apple VZ backend. Instead of rebuilding the plan from a VM manifest, it
reads an existing `metadata/apple-vz-launch.json` file and can run the same
`--require-ready` gate against that artifact. `--print-handoff` emits a concise
`AppleVzLaunchHandoff` JSON with the backend name, VM identity, bundle path,
launch spec path, guest, boot mode, disk, resources, log paths, integration
flags, and readiness object. `--launch` now consumes that same handoff through
the Apple VZ launcher interface after the readiness gate, so the handoff is the
stable boundary between artifact preparation and `VZVirtualMachine`
construction.

The macOS Swift package also contains an `AppleVzRunner` helper that decodes and
validates the same handoff JSON in `--validate-only` mode and can print a
configuration plan with `--print-config-plan`. Passing
`--apple-vz-runner <path>` makes the Rust launcher send the handoff JSON to that
helper over stdin instead of using the in-process unsupported launcher. Helper
stdout/stderr from a successful launch is relayed by `lightvm-runner`, so a
bounded manual live run can preserve the helper start/finish transcript in
`apple-vz-live-launch.output` for later evidence review. Without
`--apple-vz-runner`, the default Rust launcher returns a signed-helper-required
error instead of starting Apple VZ itself. The Swift helper now has a limited
real launch path for the supported
`linux-kernel` + `raw` disk + NAT shape; unsupported shapes, such as Linux
installer mode, fail before `VZVirtualMachine.start()` with a clear
unsupported-input error. Even for the supported shape, the helper requires the
explicit `--allow-real-vz-start` opt-in before calling
`VZVirtualMachine.start()`, so a ready handoff sent to the Swift helper remains
validation/configuration work by default rather than an accidental live VM
start. For manual live boot E2E work, `AppleVzRunner` also accepts
`--stop-after-seconds <N>`; `lightvm-runner` forwards this as
`--apple-vz-stop-after-seconds <N>` so a successful test fixture can request a
guest stop instead of waiting forever. If the guest ignores that request,
`--force-stop-grace-seconds <N>` (or
`lightvm-runner --apple-vz-force-stop-grace-seconds <N>`) force-stops the VM
after the grace period.

The SwiftPM-built helper is not automatically signed with the virtualization
entitlement. For local live E2E, build and sign the helper explicitly:

```sh
APPLE_VZ_RUNNER_BIN="$(apps/macos/scripts/build-sign-apple-vz-runner.sh)"
codesign -d --entitlements :- "$APPLE_VZ_RUNNER_BIN"
```

The Swift configuration and launch boundary is intentionally narrower than dry-run planning. `AppleVzRunner` now treats `linux-kernel` boot with a `raw` primary disk as the only handoff shape it can mark ready, and it can print or validate the limited Apple VZ configuration only for that shape. Starting a `VZVirtualMachine` also requires NAT networking plus the explicit `--allow-real-vz-start` opt-in. `qcow2`, Linux installer, macOS restore, non-NAT networking, and other disk formats/modes remain visible in plans but are marked with launch blockers before any live helper is asked to start.

Fast Mode resource planning now runs through the shared resource-manager scaffold before the Apple VZ launch spec is emitted. When manifest memory or CPU is `auto`, BridgeVM resolves those values deterministically from `resources.profile`. Explicit manifest memory or CPU values are preserved. The launch spec also includes the selected `display_fps_cap` and `rationale` from the resource decision so callers can inspect the planning policy without starting the default Apple VZ launcher path.

For running Fast Mode VMs, `bridgevm resources reapply <vm> --visibility foreground|background` re-evaluates the policy using the current host battery state and records `metadata/runtime-resources.json`. The same operation is also exposed as `bridgevm runtime-control reapply <vm> --visibility foreground|background` for callers working from the live display-control surface. Both forms are available over local CLI and the daemon socket. If a windowed Apple VZ display helper is alive and advertises `policy`, reapply asks that helper to read the refreshed policy and records `runtime_control_acknowledged: true`; a missing or stale helper leaves the policy recorded with acknowledgement false. `bridgevm display` and the macOS app's Show Display path also record a foreground policy after starting the windowed Fast/VZ display, so visible display sessions have an immediate policy file for pacing consumers; the app also refreshes its runner/resource caches after Show Display succeeds so diagnostics reflect the foreground display session. The display path now passes `--apple-vz-runtime-control-socket /tmp/bvm-vz-<stable-bundle-hash>.sock` through `lightvm-runner` to `AppleVzRunner`, records `runtime_control` metadata, and lets the display process answer `status`, `stop`, `policy`, and `pacing` JSON commands over that short Unix socket. `bridgevm runner-status <vm>` prints the latest runtime control socket, supported commands, runtime policy visibility, display FPS cap, runtime-control acknowledgement, live-applied state, and blocker codes beside the runner metadata. `bridgevm runtime-control status|stop|policy|pacing <vm>` works locally and through `bridgevm --socket <sock> ...`; the macOS Launch Readiness panel exposes the same daemon-backed display-control status/policy/pacing/stop actions and shows the latest JSON response. The policy record still includes `live_applied` and `live_apply_blockers`; today it reports `runtime-control-unavailable` because live Apple VZ CPU/RAM hot-apply has not been implemented yet.

`displayd --runtime-policy-file <bundle>/metadata/runtime-resources.json` is the first file-backed consumer for that runtime policy. It treats the policy visibility as the display visibility and applies a numeric `display_fps_cap` as a max FPS cap; `display_fps_cap: "adaptive"` preserves the normal visibility-based pacing. The AppleVzRunner display socket is the first live status/stop/policy/pacing IPC for the display process: `policy` reads the current runtime policy file from the live helper process, and `pacing` returns the helper-visible display visibility plus FPS cap derived from that policy. `runtime_control_acknowledged` only means that read happened successfully during reapply. Neither piece is live VM CPU/RAM hot-plug, and neither makes `live_applied` true. The same display path can pass `--apple-vz-proxy-framebuffer-rgba-file <bundle>/metadata/apple-vz-display-framebuffer.rgba` through `lightvm-runner` to `AppleVzRunner`; the helper captures the `VZVirtualMachineView` through AppKit, converts it to raw RGBA, writes it atomically on a timer, and reports `framebuffer_export` in runtime-control `status`. This is whole-view file capture, not a Metal compositor.

For Coherence-lite display planning, `displayd --window-*` can emit a
`window_region` object for a guest window with known bounds. The plan validates
complete geometry, clips the requested guest rectangle to the framebuffer,
computes the Retina backing rectangle, records the host proxy size, and exposes
host-to-guest input scale fractions. With `--framebuffer-rgba-file` and
`--window-crop-rgba-file`, it can also crop a raw RGBA framebuffer into a raw
RGBA window artifact and record source length/mtime plus refresh timestamp
metadata. The macOS proxy shell can decode that crop-frame artifact, load the
raw RGBA bytes as a host image, and refresh both the summary JSON and artifact
file so changed crop output paths or dimensions do not leave the proxy stuck on
stale metadata.
The daemon can now produce the same artifact shape from successful `windows`
command results when a host RGBA framebuffer file is explicitly configured with
`BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_RGBA_FILE`,
`BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_WIDTH`, and
`BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_HEIGHT`; it writes
`metadata/proxy-windows/<window-id>.json/.rgba` and injects
`window_crop_frame_summary_path` into the result payload. It also caches those
window crop targets and refreshes their `.rgba` artifacts on daemon reconcile
when the configured framebuffer file changes. If no explicit framebuffer env is
set, daemon-owned, CLI, or app-direct Show Display Apple VZ runner metadata can
auto-supply the default `metadata/apple-vz-display-framebuffer.rgba` path and
display dimensions once the helper has written the file. The same proxy shell
now has
an AppKit input capture layer that maps host pointer coordinates back to guest
window coordinates and sends pointer/key events through the capability-gated
guest-tools `WindowInput` command; Linux guests execute that path through
`xdotool` when it is available, otherwise scaffold tests record an accepted
input payload. The same guest-tools window family now includes
`SetWindowBounds`, backed by `wmctrl -ir ... -e ...` on X11, and the macOS proxy
shell debounces host-window movement/resizing into that command so the guest
window manager can be asked to keep the real surface aligned with the proxy.
This does not yet provide true per-window framebuffer streaming or a Metal
compositor. The current proof is a preserved app-direct whole-view proxy-crop
path, which is useful evidence but not the final per-window compositor.

`bridgevm ssh ubuntu-dev [--user USER]` is a metadata-only SSH planner. It does
not execute `ssh`; when connected guest-tools runtime metadata reports a valid
guest IP, it can print `ssh USER@<guest-ip>`.

The optional manifest `boot` section is the dry-run contract for Apple VZ launch planning:

```yaml
boot:
  mode: linux-installer
  installerImage: installers/ubuntu-arm64.iso
```

Supported Fast Mode boot modes are `existing-disk`, `linux-installer`, `linux-kernel`, and `macos-restore`. Linux installer mode requires `installerImage`; Linux kernel mode requires `kernelPath` and can include `initrdPath` plus `kernelCommandLine`; macOS restore mode requires `macosRestoreImage`. Relative paths are resolved against the `.vmbridge` bundle. Missing media is reported through the launch spec's `exists` flag and through `bridgevm boot-media <vm>` so template/download flows can be planned before the file is present. `bridgevm media import <vm> --source <path>` stays on the local-file side of that boundary: it copies a caller-supplied file to the resolved expected path, records metadata in `.vmbridge/metadata/boot-media/<kind>.json`, but does not fetch or choose OS downloads. `bridgevm media status <vm>` reads the same resolved entries and metadata back as a concise status view with paths, existence states, file sizes, last import details, last verification result, last download plan, and last download result. `bridgevm media verify <vm> --sha256 <hex>` also stays inside this local boundary: it hashes the resolved file, compares it with the caller-provided expected digest, and records the verification result in `.vmbridge/metadata/boot-media/<kind>-verify.json`. `bridgevm media download-plan <vm> --url <url> [--sha256 <hex>]` records remote download intent metadata under `.vmbridge/metadata/boot-media/<kind>-download.json` with the provided URL, resolved destination, optional expected digest, current file existence and size, and latest import/verify state; it does not perform the network download. `bridgevm media download <vm>` executes that recorded plan by fetching the stored URL to the stored destination, checking the optional expected digest, and recording the outcome under `.vmbridge/metadata/boot-media/<kind>-download-result.json`.

`bridgevm-core` also exposes the first template hint layer. `bridgevm recommend --os ubuntu --arch arm64` reports a stable hint id, source, and default Linux installer path, `bridgevm templates` lists the same metadata-only entries through the CLI or daemon socket, and `bridgevm create <name> --template <id>` can fill omitted guest OS, arch, and boot media metadata from a chosen template. The macOS dashboard uses the daemon `list_templates`/`create_vm` form of this same flow instead of duplicating template logic in Swift. `bridgevm --socket <sock> boot-media <vm>` is available for the same direct inspection path over the daemon socket, `bridgevm --socket <sock> media import <vm> --source <path>` is available for the same local import operation, `bridgevm --socket <sock> media status <vm>` is available for the same status summary, `bridgevm --socket <sock> media verify <vm> --sha256 <hex>` is available for the same SHA-256 comparison, `bridgevm --socket <sock> media download-plan <vm> --url <url> [--sha256 <hex>]` is available for the same no-download intent record, and `bridgevm --socket <sock> media download <vm>` is available for the same recorded-plan download execution. When creating from explicit OS/arch instead, `bridgevm create` applies a matching hint automatically when no explicit boot flags are provided. Listing or using templates never downloads installer or restore media. Current defaults are:

| Hint id | Guest | Boot mode | Media path |
| --- | --- | --- | --- |
| `ubuntu-arm64-installer` | Ubuntu Arm64 | `linux-installer` | `installers/ubuntu-arm64.iso` |
| `ubuntu-arm64-apple-vz-linux-kernel-raw` | Ubuntu Arm64 | `linux-kernel` | `boot/vmlinuz`, `boot/initrd`, `disks/root.raw` |
| `fedora-arm64-installer` | Fedora Arm64 | `linux-installer` | `installers/fedora-arm64.iso` |
| `debian-arm64-installer` | Debian Arm64 | `linux-installer` | `installers/debian-arm64.iso` |
| `debian-arm64-apple-vz-linux-kernel-raw` | Debian Arm64 | `linux-kernel` | `boot/vmlinuz`, `boot/initrd`, `disks/root.raw` |
| `macos-restore` | macOS Arm | `macos-restore` | `installers/macos-restore.ipsw` |

For `qcow2` primary disks, the scaffold does not create or validate the image yet. Missing disks are reported with a `qemu-img create -f qcow2 <path> <size>` command in metadata/output, and `qcow2` remains a dry-run plan format only for the Apple VZ path; it now carries an `unsupported-live-disk-format` blocker in launch readiness. For `raw` primary disks, the missing disk can be created directly as a sparse file and raw is the only disk format currently supported by limited Swift VZ configuration construction. Planned media download execution now exists. Fast Mode spawn reports the runner-required `apple-vz-runner-unavailable` blocker when no signed Swift helper is configured: `BRIDGEVM_APPLE_VZ_RUNNER` must point at a signed `AppleVzRunner`. With the helper configured, `run --spawn` can cross into the limited Apple VZ helper boundary with the same explicit opt-in requirements as the helper itself. A spawn-facing readiness check may fail before that boundary with structured blockers, and `run --spawn` includes a concise blocker summary in its failure message while still writing dry-run runner metadata for blocked launches. Only an explicit helper launch with `--allow-real-vz-start` can attempt the limited real launch shape today. A live boot E2E also needs a launch-ready `linux-kernel` spec with real kernel/initrd/raw disk fixtures and the required Apple virtualization entitlement; ordinary smoke coverage should stay on validate-only, config-validation, unsupported-input, missing-opt-in paths, or synthetic evidence verification.

For the Ubuntu desktop-target Apple VZ path, `scripts/stage-vz-ubuntu-desktop-vm.sh` wraps the same `ubuntu-arm64-apple-vz-linux-kernel-raw` template and `prepare-run` contract. It expects an already bootable Ubuntu Arm64 `root.raw` plus a matching `vmlinuz` and `initrd`, then writes a launch-ready bundle without installing Ubuntu or starting Apple VZ. Use `--fixture-dir <dir>` when that directory contains `vmlinuz`, `initrd`, and `root.raw`; use `--kernel-command-line` if the root device is not `root=/dev/vda2`.

### Live evidence review

`tests/integration/apple-vz-live-boot-opt-in-smoke.sh` is the manual opt-in harness for the live path and skips unless the required fixture environment variables are present. `tests/integration/prepare-apple-vz-debian-fixture.sh` prepares a Debian arm64 netboot `linux` Image, `initrd.gz`, and sparse `root.raw` for that harness; Debian is the preferred live fixture because its netboot `linux` is a raw arm64 Linux kernel image accepted by VZ LinuxBootLoader.

Concise manual live proof sequence:

```sh
bash scripts/run-vz-display-demo.sh --preflight
eval "$(tests/integration/prepare-apple-vz-debian-fixture.sh)"
export BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1
tests/integration/apple-vz-live-boot-opt-in-smoke.sh | tee /tmp/bridgevm-live-vz-smoke.out
EVIDENCE_DIR="$(awk -F': ' '/^Evidence: / {print $2}' /tmp/bridgevm-live-vz-smoke.out | tail -n 1)"
STORE="$(awk -F': ' '/^PASS: Apple VZ live boot opt-in smoke [(]/ {gsub(/[)]$/, "", $2); print $2}' /tmp/bridgevm-live-vz-smoke.out | tail -n 1)"
tests/integration/verify-apple-vz-live-evidence.sh "$EVIDENCE_DIR"
bridgevm --store "$STORE" readiness live-vz-linux --live-evidence "$EVIDENCE_DIR" --record-live-evidence
bridgevm --store "$STORE" readiness live-vz-linux
```

The display demo has its own operator proof path. `--preflight` is
metadata-safe and does not download fixtures, build/sign helpers, launch Apple
VZ, open a GUI window, or run `displayd`; the other modes deliberately cross
the live Apple VZ boundary:

```sh
bash scripts/run-vz-display-demo.sh --preflight --width 1440 --height 900
bash scripts/run-vz-display-demo.sh --check --width 1440 --height 900
bash scripts/run-vz-display-demo.sh --prove-window --evidence-dir /tmp/vz-display-proof
bash scripts/run-vz-display-demo.sh --prove-proxy-crop --evidence-dir /tmp/vz-proxy-crop-proof
```

`--prove-window` captures the actual `VZVirtualMachineView` window with
`screencapture`; `--prove-proxy-crop` additionally proves the app-direct
whole-view RGBA export can feed `displayd` into a crop artifact. It still does
not prove per-window guest app streaming or a final Coherence-style compositor.

The Debian fixture helper only prepares kernel/initrd/raw-disk inputs and prints
shell-safe exports; it does not set
`BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1`. The smoke prints the actual evidence
directory it created, and the verifier must accept that directory before
`bridgevm readiness --record-live-evidence` preserves it in the harness-created
temporary `live-vz-linux` VM bundle. Recording the smoke evidence onto an
arbitrary existing VM is unsupported unless its name and bundle path match the
preserved launch spec. A live
proof needs more than successful process start/stop output: keep serial sentinel
evidence when `BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED` is set, or provide a
verifier-bound graphical boot-progress artifact with
`BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_FRAME` plus width, height, stage, and marker
metadata. Ordinary viewer frames and QMP state can support console diagnostics,
but they should not be treated as guest boot progress by themselves.

Live attempts preserve `$STORE/evidence` with:

- `SUMMARY.txt` status and artifact path lines
- fixture manifest source and bundle paths, sizes, and SHA-256 digests
- `environment.txt` with the source fixture paths, selected resources, kernel command line, and runner path used for the attempt
- the launch spec and handoff JSON consumed at the Apple VZ boundary
- the selected helper path, copied `AppleVzRunner` artifact, validation output,
  and live-launch output
- runner and serial log pointers, plus serial sentinel evidence when configured
- optional `boot-progress-evidence.json` plus copied graphical boot-progress frame when
  `BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_FRAME`, width, height, stage, and marker values
  are provided
- optional `viewer-evidence.json` plus copied viewer frame when
  `BRIDGEVM_LIVE_VZ_VIEWER_FRAME`, width, and height are provided
- optional `guest-tools-effects.json` copied from
  `BRIDGEVM_LIVE_VZ_GUEST_TOOLS_EFFECTS_JSON` when separate observable
  guest-tools effect evidence was produced

After `tests/integration/verify-apple-vz-live-evidence.sh "$STORE/evidence"`
accepts the bundle, `bridgevm readiness <vm> --live-evidence "$STORE/evidence"
--record-live-evidence` can preserve the verified evidence inside the VM bundle
for later metadata-only readiness review. Later plain `bridgevm readiness <vm>`
re-runs the verifier against `.vmbridge/metadata/live-evidence/latest`;
`bridgevm readiness <vm> --clear-live-evidence` removes that preserved evidence
metadata and copied bundle.

The live smoke runs `tests/integration/verify-apple-vz-live-evidence.sh "$STORE/evidence"` before printing `PASS`. Reviewers can rerun the same verifier against a preserved bundle before treating a live smoke result as proof. The verifier checks the summary, fixture manifest, environment, launch spec, handoff, selected runner path or copied runner artifact, validation output, launch output, configured serial sentinel evidence, and optional graphical boot-progress artifact as a connected set for opted-in real Apple VZ runs. It cross-checks `environment.txt` against the fixture manifest source paths, the launch spec kernel command line and resources, and the selected runner path. It also treats artifact path lines in `SUMMARY.txt` as assertions that must resolve to the preserved evidence artifacts, not just as human-readable labels. The `Store`, `Bundle`, `Launch spec`, `Handoff JSON`, output path, runner/serial log, `Fixture manifest`, and `Environment` lines must resolve to the evidence fields and artifacts they name.

That live evidence bundle is not guest-tools-effects proof by default. A
future/current preserved-evidence path may prove `guest-tools-effects` only when
the bundle includes guest-tools result artifacts that the verifier explicitly
checks against observable guest-side effects. Authenticated command dispatch,
pending-count tracking, or `last_command_result` metadata alone remain protocol
or status evidence, not proof that a guest-side file, clipboard, display,
application, shared-folder mount, or other requested effect actually changed.
The opt-in harness copies a provided `BRIDGEVM_LIVE_VZ_GUEST_TOOLS_EFFECTS_JSON`
file into the evidence directory, but the verifier still decides whether it is
valid proof. If that JSON references effect artifacts, the harness copies those
files into the evidence directory, checks or fills their SHA-256 digests, and
rewrites artifact paths to relative evidence paths before verification.

The same verifier cross-checks the bounded live controls:
`BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS` and
`BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS` must be positive integers and match
the values recorded in `SUMMARY.txt` and the live-launch transcript.

`tests/integration/apple-vz-live-evidence-verifier-smoke.sh` covers the verifier with synthetic evidence only; it does not start a live VM, QEMU, Apple VZ, or a GUI, and the actual live proof still requires the separate opt-in smoke. Set `BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED` to a known fixture sentinel or provide `BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_FRAME` with matching metadata so the smoke proves guest boot progress, not just successful VM start/stop calls.
