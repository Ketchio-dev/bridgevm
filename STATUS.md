# BridgeVM — Current Status

Concise "where are we" snapshot. Full plan/roadmap/scaffold log lives in
[PLAN.md](PLAN.md); this file is the fast scan.

_Last updated: 2026-07-07._

## Current usability judgment
Local/debug BridgeVM is usable for the verified Phase 0 flows: the app bundle
builds and opens, the dashboard talks to the bundled daemon, safe metadata flows
are covered end to end, Compatibility Mode can launch QEMU NAT/port-forwarded
VMs with supervised process cleanup, and Fast Mode can cross into the signed
Apple VZ helper for live start/suspend/resume/display only for the current
`linux-kernel` + `raw` disk + NAT Linux fixture shape when the required helper,
entitlement, and explicit opt-in are present. That live-ready shape is now an
official CLI creation path via `bridgevm create ... --boot-mode linux-kernel
... --disk-format raw`, so it no longer requires hand-editing a manifest. Linux
installer and `qcow2` Fast Mode plans remain boot-media/planning flows and now
surface structured live readiness blockers instead of being reported as
launch-ready. `scripts/stage-vz-linux-demo-vm.sh` now wraps the official
create/stage/prepare path for a tryable Apple VZ Linux demo bundle without
starting Apple VZ. A local June 19, 2026 recheck also proves the daemon-less
CLI `bridgevm display` wrapper can launch that staged Ubuntu Noble Arm64 Fast
Mode VM through `lightvm-runner` + `AppleVzRunner`, keep the detached runner
alive after the CLI returns, answer the Apple VZ display runtime-control
`status`/`stop` socket, export the 1440x900 RGBA framebuffer, and boot the guest
to systemd network/SSH socket targets before clean stop. The latest local
readiness suite proves this local app boundary.

Public/distribution-ready completion is still blocked by external resources and
live fixtures: Developer ID/notarization, scriptable in-guest shared-folder
mount assertions, full Fast/VZ GUI desktop fixture validation beyond the
installer screen, real Apple VZ CPU/RAM live-apply, real suspend-image memory
serialization/deserialization for metadata suspend
snapshots, QEMU vmnet privilege/entitlement for host-only/bridged launch, and a
full Windows install/license/TPM/Secure Boot validation pass. Practical status
is now tracked by product gates rather than percentage estimates: local Phase 0
app usability is proven by the local readiness lane, while public distribution,
full live desktop validation, and Parallels-style integration remain separate
open gates.

For a current evidence-backed gate report, run:

```sh
tests/integration/product-gates-report.sh
```

## Current P3 Windows 3D direction
The active 3D-engine path is now Windows ARM64 `viogpu3d` bring-up rather than
guest-agent polish. The next concrete gate is **driver bind + first GPU trace**:
BridgeVM should boot the test-signed Windows driver far enough to observe
virtio-gpu feature negotiation and at least one capset/blob/context/fence event
from the host. BridgeVM keeps the default virtio-gpu PCI id at `DEV_1050` for
the proven 2D `viogpudo` path. P3 runs still default to the experimental
`BRIDGEVM_VIRTIO_GPU_3D_BIND_ID=1` alias (`DEV_10F7`), but the installed boot
wrapper can now expose `DEV_1050` explicitly with `--virtio-gpu-device-id 1050`
for PR #943-style packages. To support the same gate, the HVF virtio-gpu device
now has a thin, env-gated JSONL recorder:

```sh
BRIDGEVM_VIRTIO_GPU_TRACE_JSONL=/tmp/bridgevm-virtio-gpu.jsonl ...
```

The recorder captures device shape, common-config feature reads/writes, queue
notify state, control command request/response names, capset/blob/context/submit
fields, and fence create/complete/deliver events. This is intentionally not a
full replay system yet; full replay waits until a real Windows driver command
stream exists. The matching gate report is now:

```sh
bridgevm hvf virtio-gpu-trace-report \
  --trace /tmp/bridgevm-virtio-gpu.jsonl \
  --protocol auto \
  --require-p3-gate
```

It fails the command when the trace is missing the P3 bring-up shape: 3D backend,
virtio feature acceptance, queue notify, a coherent `venus` or `virgl` capset
identity, matching `context_init`, successful blob/non-empty submit, and a
backend-parked fenced command with fence delivery. Use `--protocol venus` or
`--protocol virgl` to pin the expected driver path; `auto` accepts either
complete identity and prints the selected protocol. A synthetic host preflight
now exercises the same device-model host-visible blob map/unmap, non-empty
submit, and renderer-fence callback path without booting Windows:

```sh
bridgevm hvf virtio-gpu-3d-host-preflight
bridgevm hvf virtio-gpu-3d-host-preflight --protocol virgl
```

The default remains the current `venus` contract. The `virgl` mode is a
synthetic device-model check for capset 1/context-init/submit/fence plumbing;
it does not by itself mark the Windows end-to-end 3D gate as passed.

The host renderer probe now separates that device-model result from the real
renderer backend. A live `scripts/run-venus-host-probe.sh` still reports
`host_renderer_venus=AVAILABLE` and `VENUS_CAPSET_OK ver=0 size=160`; a live
`scripts/run-virgl-host-probe.sh` reports `host_renderer_virgl=AVAILABLE` using
macOS CGL/OpenGL callbacks (`gl_context_callbacks=cgl-opengl`) and
`VIRGL_CAPSET_OK ver=1 size=308`. PR #943 therefore no longer appears blocked by
host VirGL renderer creation or HVF runtime selection: the installed boot path
can select the CGL-backed VirGL backend with
`BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=virgl`, and the installed boot wrapper sets that
when `--gpu-trace-protocol virgl` is requested.

The no-VM P3 readiness script can now include that live renderer evidence when
requested:

```sh
scripts/check-hvf-windows-p3-gpu-readiness.sh \
  --driver-dir /path/to/viogpu3d \
  --probe-host-renderer
```

Without `--probe-host-renderer`, it keeps the deterministic static result
(`host_renderer_virgl=NOT_PROBED`) for a VirGL package. With the live probe
enabled and `BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=virgl`, the same package records the
real probe result, e.g. `host_renderer_virgl=AVAILABLE`, reports
`host_backend_virgl_runtime=WIRED`, and can pass the no-VM readiness gate. The
default installed runtime remains `venus`, so a `virgl` package still fails fast
unless the VirGL runtime is explicitly selected.

A standalone package checker plus P3-specific injector wrapper now preflight a
test-signed `viogpu3d` package, reject missing catalogs or non-ARM64 PE
`.sys`/`.dll` binaries, accept audited `DEV_1050` or `DEV_10F7` INF HWIDs,
require protocol identification (`venus`/`virgl` by auto scan or
`VIOGPU3D_PROTOCOL=...` override), auto-load
`bridgevm-package-provenance.env` when an external build includes it, stage it
into the WinPE injector, and plant an offline BCD test-signing marker:

```sh
scripts/check-hvf-windows-viogpu3d-package.sh /path/to/viogpu3d

scripts/check-hvf-windows-viogpu3d-package.sh \
  --manifest /tmp/viogpu3d-package-manifest.txt \
  --pci-device-id 1050 \
  /path/to/viogpu3d

scripts/find-hvf-windows-viogpu3d-packages.sh \
  --root "$HOME/BridgeVM" \
  --out-dir /tmp/bridgevm-viogpu3d-inventory \
  --require-found

scripts/check-hvf-windows-p3-gpu-readiness.sh \
  --driver-dir /path/to/viogpu3d \
  --pci-device-id 1050 \
  --require-driver-package

VIOGPU3D_DIR=/path/to/viogpu3d \
  scripts/build-hvf-windows-viogpu3d-injector.sh

scripts/prepare-hvf-windows-viogpu3d-build-kit.sh \
  --source-dir "$HOME/BridgeVM/viogpu3d-pr943" \
  --out-dir /tmp/bridgevm-viogpu3d-pr943-build-kit \
  --no-fetch
```

The installed Windows boot harness has a matching `--virtio-gpu-3d
--gpu-trace PATH --gpu-trace-protocol auto|venus|virgl --require-gpu-trace-gate`
policy that builds the probe with the selected host runtime, exposes `DEV_10F7`
by default or an explicit `--virtio-gpu-device-id`, writes the trace, and stores
`p3-gpu-readiness.txt`, `viogpu3d-package-manifest.txt`,
`virtio-gpu-trace-report.txt`, and `virtio-gpu-trace-gate.txt` in the evidence directory when
`--viogpu3d-dir DIR --require-viogpu3d-readiness` is supplied. The readiness
script blocks a `virgl` package against the default `venus` host path, but passes
that package when `--gpu-trace-protocol virgl` selects the VirGL runtime. We
checked out PR #943 source at `/Users/user/BridgeVM/viogpu3d-pr943`;
the build-kit report identifies it as `protocol=virgl`,
`hwids=PCI\VEN_1AF4&DEV_1050`, with ARM64 configuration present and Mesa DLLs
required. A live inventory scan on 2026-07-07 now finds that source directory as
one rejected candidate (`candidate_count=1`, `ready_count=0`,
`candidate_reject_reason=FAIL: no .inf found ...`), so the external artifact
blocker is still real. The Windows
virtio-console/vioser agent path is useful for automation but is no longer
treated as a hard dependency for P3: if revisited, it should be a short KD/WPP or
QEMU byte-comparison diagnostic spike.

## BridgeVM HVF Windows engine — boots Windows 11 ARM64 to an interactive desktop
The from-scratch VMM (`crates/bridgevm-hvf`, directly on Apple Hypervisor.framework,
QEMU-independent) now boots a real, installed **Windows 11 ARM64 desktop** and drives
it with keyboard and pointer. Progress against the completion-plan milestone ladder
([docs/hvf-windows-install-completion-plan.md](docs/hvf-windows-install-completion-plan.md)):

| Milestone | State |
| --- | --- |
| **M1** Windows Setup boots from NVMe alone (no ISO/keyboard) | ✅ done |
| **M2** Scripted install completes (WIM applied, bootable) | ✅ done |
| **M3** Installed Windows reaches the desktop (OOBE auto-skip, `bridge` autologon) | ✅ done |
| **M4** Interactive desktop (keyboard + pointer + display) | ✅ substantively done — visible typing into apps + pointer move/click, all ramfb-proven with xHCI enabled |
| **M5** Connected · persistent · fast enough | 🟡 partial — **networking live-proven 2026-07-09**: `--virtio-net` (NAT) boots the installed desktop with the in-box netkvm bound ("Red Hat VirtIO Ethernet Adapter"), DHCP lease 10.0.2.15/gw 10.0.2.2, gateway ping 3/3, DNS resolves, HTTP GET example.com → 200 (guest internet works; outbound ICMP is NAT-dropped as with QEMU slirp — known cosmetic gap). Remaining: SMP (single vCPU today), suspend/resume |
| **M6** Integration polish (clipboard / resize / shared folders) | ✅ substantively done — M6-1 clipboard verbs, M6-2 file transfer (LS / chunked GET / PUT), M6-3 resident service loop + macOS pasteboard auto-sync + control-file injection, M6-4 bidirectional shared-folder sync (incl. guest-agent self-update over its own channel), all live-proven over virtio-console with an 11-min zero-timeout soak. Resize is formally BLOCKED on a real WDDM driver (ramfb + Basic Display enumerates zero display modes — probed in-guest). Gotchas fixed along the way: idle-guest vCPU-exit starvation (ServiceWake 250ms heartbeat) and the guest power plan sleeping the VM at desktop+5min (powercfg, persisted; bake into inject flow for fresh images) |

The old "late-DXE stall / firmware won't bind NVMe" wall is **resolved**: root cause
was the stale Homebrew `edk2-aarch64-code.fd`; a current tianocore/edk2 ArmVirtQemu
firmware is now vendored in-repo (`crates/bridgevm-hvf/firmware/`) and boots Windows
Setup from NVMe. The platform models a `virt`-shaped machine (`fw_cfg`, PCIe ECAM,
PL011/PL031, Apple in-kernel GICv3) plus device models: NVMe (2 namespaces), xHCI
(HID keyboard DCI3 + absolute-pointer DCI5), ramfb display, and a `VirtPlatform::reset()`
reboot loop that survives Windows' install/OOBE reboots while preserving disks + UEFI
vars. The whole install→desktop pipeline is reproducible from the Win11 ARM64 ISO via
`scripts/build-hvf-windows-scripted-source.sh` + `scripts/run-hvf-windows-scripted-install.sh`
+ `scripts/run-hvf-windows-installed-boot.sh`.

M5 status (the remaining work to "usable"):
- **Network (D3)** — a `virtio-net-pci` device model + an in-process userspace slirp-style
  NAT (ARP/DHCP/DNS/ICMP + host-socket TCP/UDP proxy) are implemented and unit-tested; the
  in-box `netkvm` ARM64 driver is injected offline via WinPE+DISM and **binds + completes
  virtio feature negotiation** against our device. Live IP is not yet up — the guest driver
  doesn't finish programming the virtqueues (two virtio-1.0 common-config bugs are precisely
  diagnosed and in progress). No modelled NIC works driverless on Windows-ARM, so virtio-net +
  driver injection is the chosen path.
- **Performance (E1 / SMP)** — the probe runs a **single vCPU**, so first-boot OOBE takes tens
  of minutes and the desktop is sluggish. This is the single largest usability gap; a staged
  multi-vCPU design (per-vCPU threads, PSCI `CPU_ON`, a global platform lock per MMIO exit,
  `BRIDGEVM_SMP_CPUS`) is planned but not started.
- **Persistence (D4)** — NVMe write-back to the host image is proven (installed changes survive
  reboot); a clean in-guest shutdown + flush proof is pending.

Honest framing: this is an impressive from-scratch VMM result, but it is **not yet a usable
daily product** — no working network, single-core, and it still needs the scripted harness +
injected drivers rather than a clean UX. The project's own strategy note flags this as the
highest-cost / lowest-user-value track (QEMU+HVF already boots Windows 11 ARM today). Full
history + reproduction recipes live in the assistant memory status file; strategy/gap context
in [docs/hvf-windows-engine-strategy.md](docs/hvf-windows-engine-strategy.md) and
[docs/hvf-windows-platform-contract-gap.md](docs/hvf-windows-platform-contract-gap.md).

## Parallels-class scope check
The product plan explicitly targets the four Parallels-like axes, but they are
not all at the same maturity:

| Axis | Current BridgeVM state | Honest next gate |
| --- | --- | --- |
| macOS-native integration / Coherence | Clipboard and display-resize effects are live-proven; Linux app/window command backends now reach `.desktop`, `gio`/`gtk-launch`, and `wmctrl` boundaries, including a preserved live Ubuntu arm64 QEMU/HVF pass of `guest-tools-app-window-live-gui-opt-in-smoke.sh` on June 17, 2026. The Apple VZ display helper can optionally export its `VZVirtualMachineView` as a raw RGBA file at `<bundle>/metadata/apple-vz-display-framebuffer.rgba`; the daemon can attach host-side `metadata/proxy-windows/<id>.json` crop summaries to real `wmctrl` window payloads when given a host RGBA framebuffer source, cache those window crop targets, and refresh the `.rgba` artifacts when that framebuffer file changes; when no explicit framebuffer env is set, daemon-owned, CLI, and app-direct Show Display runner metadata can now supply that Apple VZ framebuffer path and dimensions automatically once the file exists. The real-backend socket smoke now verifies that env-unset app-direct metadata fallback against the default `metadata/apple-vz-display-framebuffer.rgba` path, including crop refresh after the framebuffer file changes. The macOS app can open a proxy shell sized from guest bounds, render the refreshable raw RGBA crop artifact, re-read the crop summary on refresh so changed crop dimensions/output paths do not break the proxy, forward pointer/key events through guest-tools to the Linux `xdotool` boundary when available, send debounced host proxy move/resize changes back through the tested `SetWindowBounds` path (`wmctrl -ir ... -e ...` on X11), map user-closing the proxy shell to guest `CloseWindow` while suppressing that command for internal proxy replacement, keep one background refresh loop alive while proxy shells are open so it dispatches `ListWindows` and reconciles changed/missing windows, surface tracked proxy count/refresh state, tracked-window summaries, and crop-backed proxy count plus a host-only "Close Proxies" cleanup action in the Guest Tools panel, refresh an already-open proxy shell when a later `windows` payload changes its bounds/crop-summary path or reports it closed, and reconcile tracked proxy shells on inventory refresh so removed/stopped VMs close their host shells and renamed VMs reopen under the new title/key. Proxy retention is keyed by VM + guest window id to avoid collisions across VMs. The demo script now has a preserved local `--prove-proxy-crop` pass for the visible app-direct framebuffer-to-crop path at `~/bridgevm-live-evidence/apple-vz-proxy-crop-2026-06-18-auto-verified/`, including verifier output. This is **Coherence-lite plumbing**, not real host-window Coherence. | Drive the crop path from a real guest desktop window and move beyond the current file-backed proxy loop toward compositor-grade host-window integration. |
| Apple Silicon hypervisor optimization | Fast Mode uses Apple Virtualization.framework through the signed `AppleVzRunner` helper for Linux/macOS Arm paths. The live Linux helper path is intentionally narrow today: `linux-kernel` boot, `raw` primary disk, NAT networking, entitlement, and explicit opt-in. Known Linux Arm64 boot/suspend/resume/display fixtures are proven under that shape; installer ISO and `qcow2` plans are not live-ready. It is not just a QEMU preset, but it is also not a Windows fast path. | Broaden supported VZ Linux/macOS boot shapes, tighten app-runner IPC, and keep Windows in Compatibility/restricted QEMU-HVF paths until a custom Windows VMM exists. |
| Intelligent resources / battery | Launch-time `auto` CPU/RAM policy and runtime foreground/background/battery policy metadata are implemented and tested. `displayd` can consume `metadata/runtime-resources.json` as a file-backed display pacing policy, and the windowed Apple VZ display process now exposes a Unix control socket for `status`/`stop`/`policy`/`pacing` that local CLI, daemon socket, and the macOS app can drive. Runtime reapply also records `runtime_control_acknowledged` when that live helper reads the refreshed policy. | Live Apple VZ CPU/RAM control must apply the policy to a running VM; today `live_applied` remains false. |
| Graphics acceleration / Metal | Fast/VZ display now renders real GUI pixels in a `VZVirtualMachineView` and can optionally export that AppKit view to a raw RGBA file for proxy-crop experiments. The long-term plan still calls for a Metal compositor/display pipeline. | Metal/displayd integration and frame pacing first; Direct3D-to-Metal or WDDM work remains long-term R&D, not a current claim. |

So the answer to "are we considering those Parallels details?" is yes. The
current build has real foundations for the first three and a proven native VZ
display window, but it is **not** yet Parallels-level Coherence, resource live
tuning, or Windows 3D acceleration.

`bridgevm doctor` now prints the same Parallels-class scope as product-facing
status, so the CLI distinguishes proven, partial, and planned tracks instead of
letting users infer Parallels-level support from the project ambition.

## Adversarial review round 2 (3 parallel reviews: api lifecycle / agent protocol / storage-config-qemu)
A deeper read-only sweep of the correctness-critical subsystems found a large set
of real bugs. **All HIGH + most MED are FIXED + tested:**
- **agent protocol/security:** F1 (HIGH host-OOM: unbounded frame → MAX_FRAME_BYTES cap), F2 (HIGH: non-constant-time token compare → constant-time), F3 (MED: oversized-handshake spin → reset), F4 (MED: auto-detected `xclip`/`xrandr` resolved via guest PATH → pinned PATH for effect commands), F5 (MED: agent's own clipboard spawn could hang it via daemonized-child fd-hold/stdin-deadlock → null fds + threaded stdin + bounded wait).
- **storage/config/qemu:** HIGH-1 (manifest path bundle-escape → arbitrary file write → `ensure_bundle_relative`), HIGH-2 (QEMU option-string comma injection → escaped), MED-3/MED-4 (memory/size `checked_mul`), MED-5 (empty-slug name → rejected), MED-6 (non-atomic runner.json/state.json → atomic temp+rename).
- **api lifecycle:** HIGH-1 (PID-reuse → could SIGKILL an unrelated recycled pid; now validates `ps -o etime` start-time vs recorded launch), HIGH-3 (atomic metadata, = MED-6).
- **Also fixed:** `wait_for_job` (api #5) now requires OBSERVING the snapshot job before treating its disappearance as success (a failed/reaped-before-seen snapshot-save no longer reads as a successful snapshot → no resume into a missing snapshot); agent **F6** (file-drop now rejects a chunk the moment accumulated bytes would exceed the declared size); agent **F7** (`UnexpectedEof` no longer classified as idle/retry — it's terminal).
- **Lifecycle divergence cluster — FIXED:** after an irreversible action (snapshot committed / process killed), `stop`/`suspend` now `force_transition_state` (a new unconditional state write) so an unexpected prior state can no longer strand a dead backend recorded as `Running`/`Suspended` (closes #2 Compat-suspend + #6 Fast-suspend). Compat **resume** (#4) now polls over a 5s window so a `-loadvm` abort is caught whenever it exits, and confirms via QMP `query-status` (rejecting a terminal status, killing the half-up QEMU) before consuming the irreplaceable suspend marker.
- **Accepted residuals:** true two-file atomicity across `runner.json` + `state.json` on a mid-spawn crash (#8) isn't achievable, but is mitigated by the now-atomic per-file writes + the daemon's dead-pid reconciliation + the PID-identity guard; and a per-VM lock against concurrent daemon-less `stop`+`suspend` is unneeded on the daemon path (requests are serialized) and a rare operator scenario daemon-less. Both documented rather than over-engineered.

## Adversarial review round 1 (this session) — all 7 findings FIXED
A read-only review of this session's commits surfaced 7 findings; ALL are now
fixed + tested. (#1 HIGH) the held guest-tools connection lost a GuestHello split
across reads → now peeks (MSG_PEEK) for a whole frame before consuming
(`reconcile_reassembles_a_guest_hello_split_across_reads`). (#2 HIGH)
`assign_free_vnc_display` silently fell back to colliding `vnc=:0` on exhaustion →
now errors. (#3 MED) battery-adaptive resources never ran on the daemon/app path →
now applied there too. (#4 MED) `pmset` could hang the launch → now timeout-
bounded. (#5 LOW) `cleanup_owned_backend` could drop a child without reaping if
`Child::kill()` errored → now always `wait()`s (treats already-exited as success),
so a child can never orphan. (#6 LOW) AppleVzRunner accepted conflicting mode
flags (`--display`/`--graphics`/`--save-state`/`--restore-state`) → `parse` now
rejects more than one (`testConflictingModeFlagsAreRejected`). (#7 LOW) the
windowed display launcher ignored `--stop-after-seconds` and had no graceful stop
→ now honors the timer and installs SIGINT/SIGTERM handlers that `requestStop()`
the guest (then force-stop after the grace period) before AppKit terminates.

## What this is
Open-source, Parallels-class Mac virtualization app with three execution tracks:
- **Compatibility Engine (`FullVM`)** = QEMU + HVF/TCG — compatibility path for legacy OSes, x86 emulation, unusual hardware, and current Windows 11 Arm installer evidence.
- **Apple VZ Engine (`LightVM`)** = Apple Virtualization.framework (NOT QEMU) — lightweight path for Linux/macOS Arm guests.
- **BridgeVM HVF Engine (R&D)** = BridgeVM-owned Apple Hypervisor.framework VMM — target path for Windows 11 Arm without QEMU.

QEMU remains useful and supported for compatibility, but it is not the final
Windows 11 Arm performance architecture. The Parallels-like Windows goal now
means: keep QEMU as the fallback/expert engine, while building a separate
BridgeVM HVF VMM/device/display/driver stack for Windows.

The first BridgeVM HVF boundary is now present: `bridgevm hvf windows-plan` and
`hvf-runner --windows-plan` print the blocked no-QEMU Windows 11 Arm plan,
`bridgevm hvf machine-plan` / `hvf-runner --machine-plan` print the structured
machine/device/readiness metadata gate, `bridgevm hvf vm-probe` /
`hvf-runner --vm-probe` default to a no-create opt-in boundary for empty HVF VM
create/destroy, `bridgevm hvf vcpu-probe` / `hvf-runner --vcpu-probe` extend
that boundary through one empty vCPU create/destroy lifecycle without
`hv_vcpu_run`, `bridgevm hvf vcpu-run-probe` /
`hvf-runner --vcpu-run-probe` add a stronger opt-in run/cancel boundary by
pre-canceling the vCPU before one immediate `hv_vcpu_run` return,
`bridgevm hvf interrupt-timer-probe` /
`hvf-runner --interrupt-timer-probe` add an opt-in no-guest-entry boundary for
HVF pending IRQ injection and virtual timer control (`set/get_pending_interrupt`,
`set/get_vtimer_mask`, and `set/get_vtimer_offset`),
`bridgevm hvf memory-map-probe` / `hvf-runner --memory-map-probe` add an
opt-in 16 KiB guest RAM map/unmap boundary,
`bridgevm hvf guest-entry-probe` / `hvf-runner --guest-entry-probe` add an
opt-in one-instruction guest entry boundary with `HVC #0` and a watchdog, and
`bridgevm hvf guest-exit-loop-probe` /
`hvf-runner --guest-exit-loop-probe` add an opt-in two-exit loop boundary with
explicit PC read/advance between `HVC #0` and `HVC #1`, and
`bridgevm hvf mmio-read-probe` / `hvf-runner --mmio-read-probe` add an opt-in
unmapped `LDR X0, [X1]` MMIO/data-abort exit boundary at IPA `0x50000000`, and
`bridgevm hvf mmio-read-emulation-probe` /
`hvf-runner --mmio-read-emulation-probe` add an opt-in minimal MMIO read
emulation boundary that injects `X0=0x123456789abcdef0`, advances PC, and
continues to `HVC #0`, and
`bridgevm hvf mmio-write-emulation-probe` /
`hvf-runner --mmio-write-emulation-probe` add an opt-in minimal MMIO write
emulation boundary that captures `X0=0xfedcba987654321`, advances PC, and
continues to `HVC #0`, and
`bridgevm hvf mmio-serial-device-probe` /
`hvf-runner --mmio-serial-device-probe` add an opt-in tiny serial-style MMIO
device boundary that captures data `X0=0x41`, injects status `X0=0x90`,
advances PC across both MMIO exits, and continues to `HVC #0`, and
`bridgevm hvf mmio-block-device-probe` /
`hvf-runner --mmio-block-device-probe` add an opt-in VirtIO-MMIO block identity
boundary that routes magic/version/device/vendor register reads through the
BridgeVM MMIO device bus, injects `0x74726976`, `0x2`, `0x2`, and
`0x4252564d`, advances PC across all four MMIO exits, and continues to
`HVC #0`, and
`bridgevm hvf mmio-block-queue-probe [--disk <path>|--iso <path>|--writable-disk <path>]` /
`hvf-runner --mmio-block-queue-probe [--disk <path>|--iso <path>|--writable-disk <path>]` add an opt-in VirtIO-MMIO block
queue/config/address/notify boundary that routes device/driver feature, queue
select/size/ready, descriptor/driver/device ring addresses, status, queue
notify, interrupt status, config generation, and capacity registers through
the same BridgeVM MMIO device bus, seeds one synthetic read descriptor chain in
guest RAM, completes it immediately after `queue_notify`, writes
data/status/used-ring state, raises the used-buffer interrupt status, and, when
`--disk <path>` is supplied on the signed opt-in path, completes that same live
HVF `queue_notify` path from a host-file backing at byte offset `0xe00` instead
of the synthetic sector pattern, when `--iso <path>` is supplied, from a
read-only installer-media backing at the same byte offset, or, when
`--writable-disk <path>` is supplied, completes the same signed live path through
an initial read, one `VIRTIO_BLK_T_OUT` write, one `VIRTIO_BLK_T_FLUSH`, and a
reopen persistence check. The Windows firmware two-device path now advertises
per-backing capacity sectors for the installer ISO and target disk, rejects
unexpected `queue_notify` IPAs/values before draining queue 0, and clears queue,
interrupt, feature, and avail-index state on VirtIO status-zero reset. It
advances PC across the mixed read/write exits and
continues to `HVC #0`, and
`bridgevm hvf virtio-block-request-model-probe` /
`hvf-runner --virtio-block-request-model-probe` add a default, no-HVF-entered
in-memory VirtIO block request model boundary that completes one synthetic
`VIRTIO_BLK_T_IN` descriptor chain after VirtIO-MMIO queue setup writes through
the MMIO bus and queue notify through the device bus, writes data/status/used
ring state, and raises interrupt status without QEMU or Apple VZ, and
`bridgevm hvf virtio-block-file-backing-probe --disk <path>` /
`hvf-runner --virtio-block-file-backing-probe --disk <path>` add a default,
no-HVF-entered host-file storage boundary that completes one
`VIRTIO_BLK_T_IN` descriptor chain by reading sector data from a host disk-image
file at byte offset `0xe00`, writes data/status/used ring state, and raises
interrupt status without QEMU or Apple VZ, and
`bridgevm hvf virtio-block-writable-file-backing-probe --disk <path>` /
`hvf-runner --virtio-block-writable-file-backing-probe --disk <path>` add a
default, no-HVF-entered writable host-file storage boundary that completes an
initial `VIRTIO_BLK_T_IN` read, then completes one `VIRTIO_BLK_T_OUT` write
plus one `VIRTIO_BLK_T_FLUSH` at byte offset `0xe00`, reopens the host file,
and verifies the written bytes persisted without QEMU or Apple VZ, and
`bridgevm hvf virtio-block-iso-backing-probe --iso <path>` /
`hvf-runner --virtio-block-iso-backing-probe --iso <path>` add a default,
no-HVF-entered read-only installer-media storage boundary that completes one
`VIRTIO_BLK_T_IN` descriptor chain by reading sector data from an ISO backing at
byte offset `0xe00`, then rejects one `VIRTIO_BLK_T_OUT` write request with
`S_IOERR` while writing status/used-ring state and raising interrupt status
without QEMU or Apple VZ, and
`bridgevm hvf host-capabilities` /
`hvf-runner --host-capabilities` query the native Apple Hypervisor.framework
host metadata without creating a VM. The machine plan is not firmware boot or
Windows boot proof yet, and the live VM probe is only a substrate create/destroy
proof when explicitly allowed. The vCPU create probe is lifecycle proof; the
run/cancel probe proves only the host API run-return boundary; the memory probe
proves only host allocation plus guest IPA map/unmap; the interrupt/timer probe
proves only the host-side pending IRQ and virtual timer control API boundary
needed for later firmware wait-state handling; the guest-entry probe
proves only a bounded mapped-instruction exit; the guest-exit-loop probe proves
only a minimal BridgeVM-owned run/exit/PC-advance loop; the MMIO read probe
proves only that a guest unmapped read reaches BridgeVM as an exit; the MMIO
read emulation probe proves only the first injected-read continuation loop; the
MMIO write emulation probe proves only the matching captured-write continuation
loop; the MMIO serial device probe proves only the first PL011 UART-style
multi-register device emulation loop; the MMIO RTC device probe proves the
first two-device BridgeVM MMIO bus dispatch with PL011 UART plus PL031 RTC; the
MMIO block device probe proves only VirtIO-MMIO block identity register
handling; the MMIO block queue/config/address/notify probe now proves one live
HVF MMIO `queue_notify` can trigger synthetic in-guest-memory request
completion through the BridgeVM block device model, and the signed opt-in
`--disk` variant can complete that live path by reading a host-file-backed
sector into guest RAM, the signed opt-in `--iso` variant can complete that same
live path by reading a read-only installer-media-backed sector into guest RAM,
and the signed opt-in `--writable-disk` variant can complete read/write/flush and
reopen persistence on that same live path; `bridgevm hvf
windows-boot-disk-layout-probe --disk <path> --create` and `hvf-runner
--windows-boot-disk-layout-probe --disk <path> --create` can now create a
sparse raw Windows Arm target disk, write a protective MBR plus primary/backup
GPT, model ESP/MSR/Windows Basic Data partitions, reopen the disk, and verify
MBR/GPT/partition-entry CRC/name/range metadata without QEMU, Apple VZ, GUI
launch, or HVF entry; `bridgevm hvf windows-firmware-handoff-probe --firmware
<fd> --vars-template <fd> --vars <fd> --create-vars` and `hvf-runner
--windows-firmware-handoff-probe --firmware <fd> --vars-template <fd> --vars
<fd> --create-vars` can now validate AArch64 UEFI FD and vars-template
firmware-volume headers, verify FV checksums, seed a mutable vars store from the
template, reopen it, and report planned code/vars pflash IPA slots without
QEMU, Apple VZ, GUI launch, or HVF entry; `bridgevm hvf
windows-pflash-map-probe --firmware <fd> --vars-template <fd> --vars <fd>
--create-vars` and `hvf-runner --windows-pflash-map-probe --firmware <fd>
--vars-template <fd> --vars <fd> --create-vars` can now load verified code/vars
inputs into planned 64 MiB pflash memory images, verify copied prefixes, zero
padding, non-overlapping IPA ranges, guest RAM separation, and device MMIO
separation without QEMU, Apple VZ, GUI launch, or HVF entry; `bridgevm hvf
windows-pflash-hvf-map-probe --firmware <fd> --vars-template <fd> --vars <fd>
--create-vars` and `hvf-runner --windows-pflash-hvf-map-probe --firmware <fd>
--vars-template <fd> --vars <fd> --create-vars` now validate those pflash
images and default to an opt-in blocker, while the signed live path can map the
firmware slot read/execute and vars slot read/write into an empty HVF VM and
unmap them again without creating a vCPU or entering guest code, but these still
do not prove firmware reset-vector entry, UEFI Boot Manager execution,
installer boot, installed Windows persistence, reboot persistence, or Windows boot;
`bridgevm hvf windows-reset-vector-entry-probe --firmware <fd> --vars-template
<fd> --vars <fd> --create-vars` and `hvf-runner
--windows-reset-vector-entry-probe --firmware <fd> --vars-template <fd> --vars
<fd> --create-vars` now validate those pflash images and default to an opt-in
blocker, while the signed live path can map the pflash slots, create one vCPU,
set PC/CPSR to the UEFI reset-vector entry state, run once under a watchdog,
observe the first exit, classify the Arm exception class, report whether PC
progressed beyond the reset vector, and clean up without QEMU, Apple VZ, GUI
launch, UEFI Boot Manager execution, installer boot, or Windows boot claims;
`BRIDGEVM_HVF_ALLOW_REAL_EDK2_RESET_VECTOR_ENTRY=1
tests/integration/windows-arm-hvf-real-edk2-reset-vector-live-opt-in-smoke.sh`
can additionally prove that a real Homebrew/QEMU AArch64 edk2 pflash image is
accepted by the same no-QEMU HVF path and progresses PC beyond the reset vector
before the first unhandled exception exit; `bridgevm hvf
windows-firmware-run-loop-probe --firmware <fd> --vars-template <fd> --vars
<fd> --create-vars` and `hvf-runner --windows-firmware-run-loop-probe
--firmware <fd> --vars-template <fd> --vars <fd> --create-vars` now validate
the same pflash images and default to an opt-in blocker, while the signed live
path can map code read/execute pflash, vars read/write pflash, and guest RAM
read/write/execute, populate the generated FDT platform DTB in guest RAM at
`0x40010000`, create one vCPU, set PC/X0-DTB/CPSR, run a bounded firmware
exit-classification loop, optionally map low pflash aliases, optionally wire
HVF pending IRQ/vtimer controls with `--wire-interrupt-timer`, program/report
vtimer offset plus a future `CNTV_CVAL_EL0` deadline and `CNTV_CTL_EL0=1`,
route the VTimer event as PPI 11 / INTID 27 through the single-vCPU GIC
CPU-interface path, report vtimer-exit count, pending-IRQ injection count, the
VTimer auto-mask observation, the per-exit deadline rearm status, and the last timer/IRQ status
names, handled ICC read/write counts, per-`ICC_IAR1`/`ICC_EOIR1`/`ICC_DIR`
counts, and last `ICC_IAR1`/`ICC_EOIR1`/`ICC_DIR` INTIDs, report the watchdog
timeout, ESR abort ISS/fault-status details,
mapped-region hints, and the `PC` instruction word/hint plus `X0`-`X4`, `CPSR`,
EL1 exception/vector, and EL1 MMU translation sysreg snapshots at each firmware
exit, and clean up without QEMU, Apple VZ, GUI launch, UEFI Boot Manager
execution, installer boot, network, TPM, Secure Boot, or Windows boot claims.
The firmware loop now mirrors the proven timer programming boundary when
`--wire-interrupt-timer` is requested, including minimal timer PPI-to-GIC
CPU-interface delivery. The signed real-edk2 check with that
flag now avoids the previous immediate-CVAL VTimer storm at the reset vector
and reaches the same low-vector `PC=0x200` blocker as the non-timer run. A
separate signed opt-in HVF VTimer exit probe programs Apple Hypervisor.framework
virtual timer state, unmasks the timer, observes
`HV_EXIT_REASON_VTIMER_ACTIVATED`, validates the automatic VTimer mask, then
handles the pending-IRQ/re-unmask boundary. This is
still timer/interrupt substrate evidence, not UEFI Boot Manager execution,
installer boot, GUI, network, TPM, Secure Boot, persistence, drivers, or
Windows boot. The real-edk2 firmware run-loop now distinguishes raw timer
events from deliverable guest interrupts: repair/continue paths record
`vtimer_ppi_pending_recorded=true`, `vtimer_irq_line_assertable=false`,
`vtimer_gic_group1_enabled=false`, `vtimer_gic_priority_mask=0xff`,
`vtimer_gic_running_priority=0xff`, `vtimer_gic_priority_threshold=0xff`,
`vtimer_gic_pending_intid=spurious`, `vtimer_pending_irq=not attempted`,
`vtimer_unmask=not attempted` after the post-repair VTimer boundary, and
`Last pending IRQ set status name: not attempted`, meaning the pending PPI is
modeled but the guest GIC/ICC state has not yet enabled Group1 delivery or
selected INTID 27 as a deliverable interrupt.
Initial low-vector repair still permits `vtimer_unmask=HV_SUCCESS` so the
diagnostic repair sequence can complete, but post-repair timer exits now stop
cleanly instead of spinning until `--max-exits` or watchdog cancellation;
`BRIDGEVM_HVF_ALLOW_REAL_EDK2_FIRMWARE_RUN_LOOP=1
tests/integration/windows-arm-hvf-real-edk2-firmware-run-loop-live-opt-in-smoke.sh`
can additionally prove that the real edk2 pflash path reaches the bounded
run-loop with `--map-low-pflash-alias` and a 2000 ms watchdog. The previous
low-PA `translation fault level 2` and watchdog-cancel frontier are gone in
the repair/continue live checks: they now finish with `Blockers: none`, the
continue/remap paths reduce repeated post-repair VTimer exits from six/eight
exit loops down to two VTimer exits and four observed exits, and the combined
recommended-vector/repair path stops cleanly at five observed exits. This is
still not a firmware boot or Windows boot claim. The run-loop now also reads
the 32-bit AArch64 instruction at the
final PC from the mapped pflash image and renders a hint for WFI/WFE/NOP-style
wait instructions or erased pflash words when recognized. The current default
real edk2 observation is `instruction=0xffffffff` with
`instruction_hint=erased-pflash` at `PC=0x200`; the live run-loop now also
seeds `SP_EL1` to the top of guest RAM and captures `VBAR_EL1`, `ELR_EL1`,
`ESR_EL1`, `FAR_EL1`, and `SPSR_EL1`. The run-loop also captures `SCTLR_EL1`,
`TCR_EL1`, `TTBR0_EL1`, `TTBR1_EL1`, `MAIR_EL1`, `SP_EL1`, a stage-1 leaf
descriptor for the final PC, and per-address stage-1 walk entries
(`table_ipa`, index, entry IPA, descriptor, next-table/output metadata) so the
guest virtual-address mapping state can be diagnosed without guessing. The
rendered exit line also includes an automatic `diagnosis=` classifier for the
currently observed vector/MMU fault pattern. The
current default observed sysreg state is `VBAR_EL1=0x0`, `ELR_EL1=0x200`,
`ESR_EL1=0x86000007` (`instruction abort same EL`,
`translation fault level 3`), and `FAR_EL1=0x200`. A diagnostic-vector run can
seed `VBAR_EL1=0x08000000` and patch the current-EL/SPx vector slot at
`0x08000200`; that proves the slot is reached but still cannot execute because
the live stage-1 L2 block descriptor is `0x60000008000c01` with `PXN=true` and
`UXN=true`, producing `diagnosis=diagnostic-vector-stage1-xn-permission-fault`.
The run-loop can also seed the same diagnostic vector into guest RAM at
`VBAR_EL1=0x40000000`; that reaches `PC=0x40000200` with the HVC instruction
present, but the live descriptor is `0x60000040000f0d` with `PXN=true` and
`UXN=true`, producing
`diagnosis=guest-ram-diagnostic-vector-stage1-xn-permission-fault`.
The run-loop now renders a stage-1 descriptor sample set and a full stage-1
walk trace for low-vector, pflash, guest-RAM, PC, VBAR, ELR, FAR, executable
diagnostic-vector, and SP addresses. In the current real edk2 run, `0x0`/`0x200`
walk to an invalid L3 descriptor, the firmware reset and
pflash diagnostic vector addresses share the XN L2 block
`0x60000008000c01`, the guest RAM diagnostic vector uses XN L2 block
`0x60000040000f0d`, and the seeded `SP_EL1=0x43fffff0` lands in XN L2 block
`0x60000043e00f0d`. The same exit now scans known pflash/guest-RAM ranges for
EL1-executable stage-1 leaf candidates and records each candidate's
`vector_sync_va`, `vector_sync_pa`, `vector_sync_instruction`, and
`vector_sync_hint` at the current-EL/SPx sync slot, then scans 2 KiB-aligned
vector-base candidates inside each executable leaf while filtering zero/erased
slots, reporting scanned/suppressed/limit telemetry, and passively selecting a
recommended vector base that can feed the opt-in one-shot
`--try-recommended-vector-base-vbar` redirect experiment; the current real edk2
run finds the low firmware pflash alias at `0x200000` as a 2 MiB executable
block candidate (`descriptor=0x200f8d`, `PXN=false`, `UXN=false`). The opt-in
redirect experiment records requested/attempted/set/source-exit/target/status
plus follow-up-exit telemetry and only claims the `VBAR_EL1` set and
follow-up-exit observation boundary, not boot; the current experiment now seeds
the selected recommended vector base with a diagnostic vector before setting
`VBAR_EL1`, so the follow-up reaches `PC=0x200204` with
`diagnosis=executable-diagnostic-vector-hvc-exit`, routes through `ERET`, and
stops at `PC=0x20020c` with
`diagnosis=executable-diagnostic-vector-eret-landing-hvc-exit`. The opt-in
`--continue-after-recommended-vector-base-vbar` experiment captures the source
`ELR_EL1`/`SPSR_EL1`, arms an `ERET` resume with `HV_SUCCESS` status names, and
still reports the no-repair blocker: the restored `ELR_EL1=0x200` returns to
the still-faulting low-vector path, so exit 3 repeats the recommended-vector
diagnostic HVC instead of advancing to UEFI boot. The combined
`--continue-after-recommended-vector-base-vbar --repair-low-vector-diagnostic-page --continue-after-low-vector-repair --wire-interrupt-timer`
live proof now keeps the repaired low-vector diagnostic page installed, records
the recommended-vector VBAR redirect as requested but not armed, classifies the
follow-up as `low-vector-diagnostic-page-hvc-exit`, arms the original
`ELR_EL1`/`SPSR_EL1` context through the diagnostic `ERET`, and stops at the
low-vector diagnostic landing `PC=0x20c` with `Blockers: none`. Its post-repair
first-exit telemetry records exit 4 as `HV_EXIT_REASON_EXCEPTION` at `PC=0x204`
with `diagnosis=low-vector-diagnostic-page-hvc-exit` and
`interaction=exception:non-mmio`; because this combined path already set the
recommended vector base, that first-exit context keeps `VBAR_EL1=0x200000`
while still returning through the low-vector diagnostic page. This is still diagnostic-vector
continuation evidence, not firmware device discovery; its separate first
post-repair device-interaction telemetry skips diagnostic continuation and raw
VTimer exits, so the first post-repair device interaction remains
`not observed`. The VTimer exit is still recorded as timer telemetry with
`vtimer_ppi_pending_recorded=true` and `vtimer_gic_pending_intid=spurious`; it is
not counted as MMIO/ICC device discovery while firmware has not enabled the
guest-visible GIC/ICC Group1 delivery path. A fourth diagnostic-vector run can also seed that executable candidate, set `VBAR_EL1=0x200000`, reach a
real `HVC AArch64` exit at `PC=0x200204` with
`diagnosis=executable-diagnostic-vector-hvc-exit`, handle that HVC, rewrite
`ELR_EL1` to the executable landing pad, resume through `ERET`, and stop cleanly
at `PC=0x20020c` with
`diagnosis=executable-diagnostic-vector-eret-landing-hvc-exit` and no
unsupported-exit blocker. A fifth repair run now wires the firmware VTimer
deadline path, handles the first `HV_EXIT_REASON_VTIMER_ACTIVATED` boundary,
patches the real low-vector L3 stage-1 descriptor at entry IPA `0xc000` from
previous descriptor `0x0` to `0xf8f`, records whether a repeated low-vector
fault appears after repair, reaches the low-vector diagnostic `HVC` at
`PC=0x204`, routes that exit through `ERET`, reaches the landing `HVC` at
`PC=0x20c`, then arms a one-shot low-vector `ERET` resume back to the captured
original `ELR_EL1`/`SPSR_EL1` context with explicit `HV_SUCCESS` status
telemetry. The non-continue proof still stops at the synthetic landing path,
but `--continue-after-low-vector-repair` now keeps the diagnostic page patched
instead of restoring the original low-vector bytes, avoids direct `CPSR` resume
(`Low vector diagnostic page resume CPSR set status name: not attempted`), and
arms the original context through `ELR_EL1`/`SPSR_EL1` plus the diagnostic
`ERET`. The signed live smoke records `Low vector diagnostic page slot restored:
false`, `Observed exits: 5`, `VTimer exit count: 1`, and `Final PC: 0x20c`; the
current frontier is the repeated low-vector diagnostic HVC/ERET landing
(`PC=0x204`/`PC=0x20c`), not the old restored-erased-pflash `PC=0x200` path.
The pre-ERET target snapshot now proves why: the captured `ELR_EL1=0x200`
currently points at BridgeVM's installed low-vector diagnostic `HVC #1`
(`0xd4000022`) on stage-1 descriptor `0xf8f`, while the preserved original
slot bytes were still erased pflash (`0xffffffffffffffffffffffff` /
`0xffffffff`). A separate `--restore-low-vector-slot-before-eret` opt-in now
uses an executable pflash `ERET` trampoline, restores the preserved low-vector
slot before the original-context `ERET`, and proves that target becomes
`0xffffffff` / `erased-pflash` with `Low vector diagnostic page slot restored:
true`, `Observed exits: 4`, `VTimer exit count: 2`, and `Final PC: 0x200`.
That is useful loop-cause evidence, not boot progress.
This is repair-and-resume timer/vector telemetry, not UEFI Boot Manager,
installer, or Windows boot. The `--remap-low-vector-to-recommended-vector` opt-in path now separates
the low-vector descriptor patch primitive from the remap candidate policy: it
only attempts the remap when the recommended target has a populated,
non-BridgeVM Current EL/SPx sync slot. The current real-edk2 recommendation is
still the fallback empty vector scan (`vector_sync_instruction=0x00000000`), so
the run-loop rejects it for remap and falls back to diagnostic-page repair plus
the same ERET continuation evidence. The remap telemetry currently reports
`Low vector recommended-vector remap succeeded: false`, target PA `not observed`,
descriptor `not observed`, and the same `PC=0x204`/`PC=0x20c` diagnostic
frontier while first device interaction remains `not observed`. The run-loop now also accepts installer ISO plus
writable target disk paths as first-class no-QEMU metadata, verifies the
generated FDT magic before handoff, reports the platform DTB byte count and
`X0` DTB set status, and has a first firmware data-abort MMIO routing path for
the Windows device window through the BridgeVM PL011/PL031, GICv3
distributor/redistributor MMIO register skeletons, plus VirtIO-MMIO installer
ISO (`0x10002000`, read-only) and target disk (`0x10003000`, writable) skeleton
bus. The GICv3 skeletons are
wired to absorb common firmware MMIO accesses, including status and group
modifier registers, and the live run-loop now has a single-vCPU Group1 `ICC_*`
CPU-interface sysreg skeleton for `ICC_SRE_EL1`, `ICC_CTLR_EL1`,
`ICC_PMR_EL1`, `ICC_BPR1_EL1`, `ICC_IGRPEN1_EL1`, `ICC_HPPIR1_EL1`,
`ICC_IAR1_EL1`, `ICC_EOIR1_EL1`, and `ICC_DIR_EL1`, plus conservative
firmware-tolerant `ICC_BPR0_EL1`, `ICC_IGRPEN0_EL1`, `ICC_RPR_EL1`,
`ICC_AP0R*`/`ICC_AP1R*`, Group0 spurious, and `ICC_SGI1R_EL1` stubs. The live
run-loop also has a conservative device IRQ line
boundary: successful VirtIO block `queue_notify` completion raises the
used-buffer interrupt status, mirrors that status into the matching GICD FDT SPI
pending bit, gates HVF IRQ line assertion on GICD `EnableGrp1NS`,
GICD/GICR `IGROUPR` Group1 bits, SPI/PPI enable/pending state, and
`ICC_IGRPEN1` plus PMR/current-running-priority threshold state, lets
`ICC_HPPIR1_EL1`/`ICC_IAR1_EL1` choose the highest-priority pending Group1
interrupt across redistributor PPI and distributor SPI candidates, moves the
acknowledged INTID active, treats
`ICC_EOIR1_EL1` as priority drop plus deactivate when `ICC_CTLR_EL1` EOImode is
clear, requires `ICC_DIR_EL1` for deactivate when EOImode is set, refreshes and
re-pends level VirtIO sources after actual deactivation, and can deassert the
line when VirtIO ACK/status reset clears the source. It reports device IRQ line
assert/deassert counts and status names, handled MMIO read/write counts,
per-device MMIO counts for PL011/PL031/GICD/GICR/installer ISO/target disk,
VirtIO `queue_notify` and request-completion counts, handled ICC read/write
counts, per-`ICC_IAR1`/`ICC_EOIR1`/`ICC_DIR` counts, and last
`ICC_IAR1`/`ICC_EOIR1`/`ICC_DIR` INTIDs. This proves only the
VirtIO-status-to-GICD-SPI-to-priority-selected-ICC-IAR/EOIR/DIR-to-HVF-line
skeleton boundary plus minimal timer PPI-to-GIC CPU-interface delivery, not
full GIC delivery beyond the minimal single-vCPU SPI/PPI paths, complete
nested preemption, binary-point/List Register behavior, multi-vCPU routing,
complete deactivation-stack semantics, UEFI Boot Manager handoff, or Windows
boot.
BridgeVM now also has a metadata-only
FDT platform-description boundary exposed through `bridgevm hvf
windows-platform-description-probe` and
`hvf-runner --windows-platform-description-probe`; it builds and inspects an FDT
blob with magic `0xd00dfeed`, guest RAM at `0x40000000`, requested CPU nodes,
PL011/PL031 plus VirtIO-MMIO installer ISO (`0x10002000`) and target disk
(`0x10003000`) nodes inside the `0x10000000..0x20000000` Windows device window,
root `interrupt-parent` phandle `0x1`, GICv3 distributor/redistributor ranges,
four ARM arch timer interrupts, and PL011/PL031/VirtIO FDT SPI interrupt cells
`0..3`, while reporting `ACPI: not implemented`, `fw_cfg: not used`,
`GIC: described/not emulated`, and `GIC emulated: false`, without entering HVF
or launching QEMU, Apple VZ, or GUI tooling. The firmware run-loop now uses that
FDT shape for the guest-RAM DTB handoff at `0x40010000` and seeds UEFI entry
`X0` with that IPA when live execution is explicitly allowed. The next blocker
is therefore no longer the metadata shape, DTB pointer, or bare GIC MMIO window
wiring itself; it is completing real interrupt-controller behavior beyond the
current single-vCPU Group1 skeleton and using the described devices for real
firmware discovery before UEFI Boot Manager handoff.
This is still firmware execution evidence, not UEFI Boot Manager execution,
installer boot, GUI/network/TPM/Secure Boot support, or Windows boot;
the VirtIO block request model proves the same
synthetic read completion in metadata-safe no-HVF form, not live block IO,
persistent boot disk, firmware boot, or Windows boot; the VirtIO block
file backing probe proves the first host-file read backend for the same
descriptor model without entering HVF; the VirtIO block writable file backing
probe proves the first metadata-safe host-file write/flush/reopen persistence
backend for the same descriptor model without entering HVF; the VirtIO block ISO
backing probe proves the first read-only installer-media read backend for the
same descriptor model plus write rejection without entering HVF, but none of
these prove full persistent boot disk lifecycle, partition install state,
firmware boot, installer boot, or Windows boot.
None of these prove firmware or Windows boot.
`apps/macos/scripts/build-sign-hvf-runner.sh`
signs the runner with `com.apple.security.hypervisor`; with that signature, the
empty HVF VM, vCPU create/destroy, pre-canceled vCPU run/cancel, memory
map/unmap, interrupt/timer control, VTimer exit, one-instruction guest-entry,
two-exit guest-loop, MMIO read exit,
MMIO read emulation, MMIO write emulation, MMIO serial device, and MMIO RTC
device probes pass on this host; the VirtIO-MMIO block identity and
queue/config/address/notify probes are wired as signed HVF storage-facing
boundaries, with the queue probe completing one synthetic in-memory read request
after `queue_notify`, while the request model probe is a default in-memory
device-model test and does not enter HVF. The live guest-entry probe exits with
`HV_EXIT_REASON_EXCEPTION` and syndrome `0x5a000000` without watchdog
cancellation; the live guest-exit-loop probe observes PC `0x40000004` plus
`0x5a000000` then `0x5a000001` exception syndromes across the two HVC exits.
The live MMIO read probe targets IPA `0x50000000` and exits without watchdog
cancellation, reporting syndrome `0x93c08006` with virtual and physical address
`0x50000000`; the live MMIO read emulation probe then injects
`0x123456789abcdef0`, advances PC, continues to `HVC #0`, and reads the same
value back from `X0`. The live MMIO write emulation probe captures
`0xfedcba987654321`, advances PC, continues to `HVC #0`, and reads the same
value back from `X0`. The live MMIO serial device probe captures a PL011 UART
data-register write `0x41`, routes the write plus flag-register read through a
one-device BridgeVM MMIO bus, injects flags `0x90`, advances PC twice, continues
to `HVC #0`, and reads the same flag value back from `X0`. The live MMIO RTC
device probe attaches PL011 UART plus PL031 RTC to the same
BridgeVM MMIO bus, reads RTC IPA `0x50001000`, injects `0x20260618`, advances
PC, continues to `HVC #0`, and reads the same value back from `X0`. The
capability query reports default IPA bits `36`, max IPA bits `40`, and EL2
support `true`.
The live VTimer exit probe observes `HV_EXIT_REASON_VTIMER_ACTIVATED`,
confirms the timer mask boundary, injects the pending IRQ, and unmasks the
timer again. The firmware run-loop timer wiring now uses a future deadline
instead of immediate expiry and routes the event through the minimal timer
PPI-to-GIC CPU-interface path. The single-vCPU GICv3 path now also does
priority-based selection across pending PPI/SPI candidates and honors the
`ICC_CTLR_EL1` EOImode split between `ICC_EOIR1_EL1` priority drop and
`ICC_DIR_EL1` deactivation, so the signed real-edk2 timer check no longer spins
on VTimer exits at the reset vector. In the real-edk2 firmware run-loop,
BridgeVM records the timer PPI as pending but does not call
`hv_vcpu_set_pending_interrupt` when `vtimer_irq_line_assertable=false`; the
per-exit snapshot now shows `vtimer_gic_group1_enabled=false` and
`vtimer_gic_pending_intid=spurious`, so the current timer-delivery blocker is
the guest-visible GIC/ICC enable/selection path rather than the PMR threshold.
BridgeVM now defers post-repair `hv_vcpu_set_vtimer_mask(false)` as
`vtimer_unmask=not attempted` instead of creating repeated synthetic timer
exits. First MMIO/ICC device interaction is still not observed. The signed
repair run proves that VTimer service can fall through into low-vector repair
and reach the clean `PC=0x20c` landing without a watchdog, while the signed
continue/remap runs now keep the repaired diagnostic page installed, arm the
captured `ELR_EL1`/`SPSR_EL1` context through diagnostic `ERET`, and stop at the
same low-vector diagnostic landing (`VTimer exit count: 1`, `Observed exits: 5`,
`Blockers: none`); it is still not firmware boot, UEFI Boot Manager handoff,
installer boot, or Windows boot.
The no-QEMU Windows path now also has a named
`windows-firmware-device-discovery-probe` wrapper in both `bridgevm hvf` and
`hvf-runner`: it forces the run-loop policy needed for device discovery
diagnosis (low pflash alias, low-vector repair, post-repair continue,
interrupt/timer wiring, and stop-at-first-post-repair-device-boundary), reports
`Device discovery boundary reached/status/ready`, and has CLI/runner smokes that
prove the default opt-in-blocked path without launching QEMU, Apple VZ, or GUI
tooling. Current metadata-safe evidence still reports the boundary as not
reached, so this is a product gate and telemetry surface, not firmware boot or
Windows boot.

## Phase 0 live-boot evidence (the headline milestone)
All three "live boot proof" criteria are now demonstrated on Apple Silicon:

| Criterion | Status | Evidence |
| --- | --- | --- |
| QEMU/HVF Linux Arm64 (Compatibility) | ✅ proven + recorded | `~/bridgevm-live-evidence/qemu-arm64-2026-06-16/` |
| Apple VZ Linux Arm64 (Fast) | ✅ proven + recorded | `~/bridgevm-live-evidence/apple-vz-arm64-2026-06-16/` |
| Windows 11 Arm installer reachability | ✅ proven (graphical) | `~/bridgevm-live-evidence/windows-arm64-2026-06-16/` |

Fast/VZ display and Coherence-lite crop evidence now has its own preserved
artifact bundle:
`~/bridgevm-live-evidence/apple-vz-proxy-crop-2026-06-18-auto-verified/`.
It proves a real `VZVirtualMachineView` window was captured (`viewer-frame.png`,
`2696x1800`), a whole-view app-direct raw RGBA framebuffer was exported
(`1280x800`, 4,096,000 bytes), and `displayd` materialized a `640x400` crop
artifact from it. The proof script also ran the verifier and preserved
`app-direct-proxy-crop-verifier.output`. Verify it again with:

```sh
tests/integration/verify-vz-proxy-crop-evidence.sh \
  ~/bridgevm-live-evidence/apple-vz-proxy-crop-2026-06-18-auto-verified
```

Live evidence is captured via opt-in smokes (`tests/integration/qemu-live-boot-opt-in-smoke.sh`,
`apple-vz-live-boot-opt-in-smoke.sh`) and recorded with `bridgevm readiness --record-live-evidence`.
Readiness ingestion now accepts verifier-bound graphical
`boot-progress-evidence.json` PNG artifacts as live-boot progress proof, while
ordinary viewer/QMP evidence remains console-only unless a separate progress
artifact or serial sentinel is present.

## Windows installer support (product feature)
`bridgevm` can build and launch the Windows 11 Arm installer in Compatibility Mode:
```sh
bridgevm create win11 --os windows --version 11 --arch arm64 \
  --mode compatibility --boot-mode windows-installer \
  --installer-image /path/to/Win11_Arm64.iso
bridgevm run win11 --spawn   # boots to Windows Setup
```
Wiring: `BootMode::WindowsInstaller` (bridgevm-config) + installer media in
`build_compatibility_command` (bridgevm-qemu): edk2 + ramfb + qemu-xhci/usb-kbd + ISO
as usb-storage cdrom (bootindex 0) + virtio-rng. Covered by unit tests, the CLI
flag, and `tests/integration/windows-arm-qemu-args-cli-smoke.sh`. See
[docs/compatibility-mode/README.md](docs/compatibility-mode/README.md).

## Fast Mode lifecycle: run / suspend / resume (product feature)
`bridgevm run <vm> --spawn` now **really boots** a Fast Mode (Apple VZ) Linux VM (was dry-run only) when `BRIDGEVM_APPLE_VZ_RUNNER` is set — records a real pid, `dry_run:false`, state `running`; without the env it preserves the dry-run metadata fallback and reports the concrete `apple-vz-runner-unavailable` blocker so callers know a signed runner is required. `bridgevm suspend <vm>` / `bridgevm resume <vm>` work end-to-end, wired runner → `lightvm-runner` → `bridgevm-api`/daemon/CLI → macOS app:
- `bridgevm create ... --disk-format raw` now creates the supported live Linux
  kernel manifest shape directly (`disks/root.raw`, `format: raw`), and
  `prepare-run` creates the sparse raw disk while recording
  `metadata/apple-vz-launch.json` plus `runner.json.launch_spec_path`.
- `scripts/stage-vz-linux-demo-vm.sh --prepare-fixture --name vz-linux-demo`
  stages the Debian Apple VZ Linux fixtures into a real BridgeVM VM bundle using
  that official create path, copies the kernel/initrd/raw disk, runs
  `prepare-run`, and stops before launch. The metadata-safe smoke
  `vz-linux-demo-stage-smoke.sh` locks this path with fake fixtures and proves
  `Launch ready: true` without opening Apple VZ.
- AppleVzRunner does VZ `saveMachineState`/`restoreMachineState` (`--save-state`/`--restore-state`); machine identifier + NAT MAC persisted per bundle (required for restore to match).
- `suspend` boots the Fast VM, pauses, saves to `metadata/suspend-images/<vm>.bin`, marks `suspended`; `resume` restores + runs detached, marks `running`. Needs `BRIDGEVM_APPLE_VZ_RUNNER` (path to a signed AppleVzRunner).
- macOS app pause/resume send `suspend_backend`/`resume_backend` daemon requests. Daemon/app Start requires the app's Allow Apple VZ live starts setting (or `BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1`) in addition to a signed `BRIDGEVM_APPLE_VZ_RUNNER`; Show Display sets its own helper opt-in for the local GUI window path.
- Verified: real Debian arm64 VZ guest suspended (98 MB state) → resumed to a running guest.
- `bridgevm stop <vm>` now reliably terminates the running VM process (SIGTERM→grace→SIGKILL) for both Fast (AppleVzRunner) and Compat (qemu) — no orphan left (release gate). The daemon supervises resumed children like cold-start.
- Compat (QEMU) suspend: `bridgevm suspend` does a QMP `snapshot-save` internal qcow2 snapshot (`bridgevm-suspend`). **Compat resume is not supported on Apple Silicon under HVF** — QEMU aborts in `cpu_pre_load` restoring an HVF arm64 guest; resume reports this honestly and preserves the snapshot. Fast Mode is the supported suspend/resume path.
- Follow-ups: pause an already-running Fast VM via IPC (current model boots→saves); Compat live resume (needs a non-HVF path or a future QEMU fix).

## Networking
Compatibility Mode (QEMU) NAT + port forwarding works at launch: manifest `network.forwards` become QEMU `hostfwd=tcp::HOST-:GUEST` in the launch command, so the host port is actually bound when the VM runs (verified: host port LISTENs). `bridgevm port add/remove` edit the forwards. QEMU host-only and bridged planning now render `vmnet-host` / `vmnet-bridged` but live launch is blocked until the QEMU process runs as root or carries the `com.apple.vm.networking` entitlement (user resource).

## Verification lanes
- **Safe app lane:** `tests/integration/local-release-readiness-suite.sh --app-only --locally-usable-app`
- **Latest local app readiness pass (2026-06-17):** `tests/integration/local-release-readiness-suite.sh --app-only --with-metadata-smokes --locally-usable-app` passed end to end: Rust workspace tests with default features disabled, Swift app tests, debug `.app` bundle build/signature verification, bundled helper verification, LaunchServices-free app startup with a detected BridgeVM window, bundled daemon supervisor/socket doctor smoke, release credential/dry-run smokes, live opt-in default skip, preserved live-evidence verifiers, app-only artifact manifest verification, and the metadata-safe smoke suite.
- **Rust:** `cargo test --workspace`
- **Live boot (opt-in, heavy):** the `*-live-boot-opt-in-smoke.sh` scripts (need a real disk/ISO + `*_ALLOW_REAL_START=1`).

## Remaining work to fully "complete" the app (with blockers)
The VM lifecycle (create/run/suspend/resume/stop) + networking + boot evidence are done. The remaining features each have a concrete blocker:

**Guest tools — transport PROVEN, effects remaining:**
- The `bridgevm-tools-linux` agent now cross-compiles to Linux-arm64 (`scripts/build-guest-agent-linux.sh`, via zig + cargo-zigbuild) and the **full transport is verified end-to-end**: the agent runs inside a booted Debian guest (cloud-init NoCloud seed), sends `GuestHello` over `/dev/virtio-ports/org.bridgevm.guest-tools.0` → QEMU virtio-serial → host `guest-tools.sock`, and the host `accept-hello` validates token + capabilities (tampered token rejected). Smoke: `tests/integration/guest-tools-live-handshake-opt-in-smoke.sh`. Gotcha: the guest agent must advertise a capability subset matching the manifest `AgentPolicy` (default manifest disables drag-drop/agent-update).
- **Guest-tools effects (real, verified):** `time-sync` actually sets the guest clock (`settimeofday`, capability-gated) — verified in-guest (wall clock jumped 2001→now) via `tests/integration/guest-tools-effects-opt-in-smoke.sh`; `guest-metrics` now reports real `/proc` values; `fs-freeze`/`fs-thaw` have Real backends. Note: macOS AF_UNIX paths are capped at 104 bytes, so the guest-tools.sock path must stay short.
- **In-guest perf benchmark attachment — DONE:** the Linux guest agent exposes a bounded `RunBenchmark` command, the VM policy now advertises the `benchmark` capability, and daemon/socket `performance sample` attaches `guest_benchmark_*` CPU/disk micro-benchmark measurements when the daemon owns the running backend and has a connected benchmark-capable guest-tools session. Local/offline samples remain host-side only.
- **Shared folders (Fast/VZ): device wired + boots; in-guest mount is the interactive step.** `AppleVzConfigurationBuilder` adds a VZ-native `VZVirtioFileSystemDeviceConfiguration` (single-directory share + validated tag, macOS 13+) when a share is requested — no `virtiofsd` (that is QEMU-only/unavailable on macOS). Driven by `AppleVzRunner --share-dir PATH [--share-tag TAG] [--share-read-only]`, threaded through both the headless and windowed launch paths and the demo script. Verified: the config validates with the device and a real guest **boots with the share + graphics attached** (demo `--check`). The guest mounts it with `mount -t virtiofs <tag> <dir>` (the demo prints the command); a non-interactive mount assertion needs a scriptable VZ guest (the netboot-installer fixture isn't). Unit-tested (`testBuildsVirtioSharedDirectoryDeviceWhenRequested`, `testShareDirFlagsThreadSharedDirectoryToLauncher`). Compat (QEMU) shared folders remain blocked (no macOS `virtiofsd`).
- **Clipboard sync + dynamic resolution — VERIFIED live with the real tools (headless).** No GUI desktop is needed: a headless X server (`Xvfb`) lets `xclip`/`xrandr` run and be checked. `tests/integration/guest-tools-clipboard-resize-effects-opt-in-smoke.sh` boots a guest, apt-installs Xvfb/xclip/x11-xserver-utils, and drives the host→guest `SetClipboard` and `ResizeDisplay` commands end to end — it PASSES, asserting the guest's X clipboard reads back exactly the host's text (real `xclip`) and the agent ran `xrandr` with the host geometry, over the live virtio-serial transport. The agent also now **auto-detects** the right tool out of the box (`wl-copy` on Wayland, `xclip` on X11, `xrandr` for resize) when no explicit command is configured (unit-tested). (Gotcha found + fixed while writing the smoke: `xclip -i` daemonizes holding the agent's stdout/stderr pipe, hanging `wait_with_output`; the wrapper redirects its fds to /dev/null.)
- **Coherence-lite foundation (Linux apps/windows): implemented + tested through the real desktop-tool boundary; live GUI harness passes.** `bridgevm-tools-linux` no longer has to answer `applications`/`windows` commands only from static scaffold state: when the guest has desktop tools, it now lists visible `.desktop` applications and launches them through `gio`/`gtk-launch`, and uses `wmctrl` to list/focus/close real X11 windows. Real `wmctrl` window payloads now prefer `wmctrl -l -p -G`, preserving `pid`, `desktop`, and `bounds` metadata for the later host-proxy-window path; if those tools are absent it falls back to the previous in-memory scaffold, so CI/headless behavior stays stable. Unit-tested (`desktop_controller_detection_uses_real_tools_when_available`, `desktop_file_parser_filters_visible_applications`, `wmctrl_window_backend_lists_focuses_and_closes_real_tool_output`) and smoke-tested over the daemon/socket/CLI path with fake `.desktop`, `gio`, and `wmctrl` tools (`guest-tools-app-window-real-backend-cli-smoke.sh`). The heavy opt-in harness (`guest-tools-app-window-live-gui-opt-in-smoke.sh`) has a preserved pass on this machine: it booted a real Ubuntu Noble arm64 cloud image under QEMU/HVF, installed Xvfb/openbox/xterm/wmctrl/gio over guest networking, launched a `.desktop` app through the agent, and then listed/focused/closed the real X11 window through `wmctrl` over live virtio-serial transport. The harness code now also records a live-window crop proof bundle (`live-window-payload.json`, `live-window-crop.json`, `live-window-crop.rgba`, `live-window-proxy-crop-proof.json`) by feeding the real `wmctrl` bounds into `displayd` with a synthetic host RGBA framebuffer on the next live opt-in run. The default metadata-safe lane still runs only the skip guard, and this remains proof of the Linux desktop-tool/crop boundary rather than Parallels-style host-window Coherence.
  - macOS app observability now surfaces the last command's real backend source when the guest reports it, e.g. `linux-desktop-file` for application inventory/launch or `wmctrl`/`xdotool` for X11 window list/focus/close/input. The VM detail Guest Tools panel also turns `applications`/`windows` command-result payloads into actionable Launch/Focus/Close rows, deduping repeated IDs across result/metadata and showing window PID/bounds when available. Window rows with bounds can open a host-side proxy shell whose `NSWindow` size is planned from the guest bounds; an AppKit capture layer maps/clamps mouse coordinates into guest window coordinates and forwards pointer/key events through the existing guest-tools command path. Already-open proxy shells are tracked by VM + guest window id; while any proxy is open, a single background app loop dispatches `ListWindows`, reloads guest-tools status, replaces the shell plan when bounds/crop-summary metadata changes, and closes the host shell when the guest reports that window closed or an authoritative window list omits it. The same Guest Tools panel now surfaces tracked proxy count, tracked-window summaries, crop-backed proxy count, and auto-refresh/in-flight state, plus a host-only cleanup button for closing tracked proxy shells without sending guest close commands. Successful inventory refresh also closes tracked shells for removed/stopped VMs and reopens renamed VM shells under the new title/key. This is Coherence-lite command/proxy UX, not true host-window Coherence.
  - `displayd --window-id/--window-x/--window-y/--window-width/--window-height` can now emit a `window_region` crop contract for those proxy windows: it validates complete geometry, clips the guest rect to the framebuffer, derives Retina backing pixels, records host presentation size, and exposes host-to-guest input scale fractions. It can also consume a raw RGBA framebuffer file via `--framebuffer-rgba-file` and materialize the clipped guest window pixels to `--window-crop-rgba-file`, with source length/mtime and refresh timestamp metadata in `window_crop_frame`. The daemon now has the matching host-side artifact bridge for `windows` command results: when `BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_RGBA_FILE` plus framebuffer dimensions are configured, it writes `metadata/proxy-windows/<window-id>.json/.rgba`, injects `window_crop_frame_summary_path` into the real `wmctrl` window payload, caches the window crop target, and refreshes the cached artifact on reconcile when the framebuffer file metadata changes. If those env vars are absent, daemon-owned, CLI, and app-direct Show Display Apple VZ runner metadata can supply the default `metadata/apple-vz-display-framebuffer.rgba` source and display dimensions automatically once the helper has written the file; the real-backend socket smoke now asserts that default-path fallback and refresh behavior without setting framebuffer env vars. The macOS proxy shell can decode that `window_crop_frame` JSON artifact, validate/load the raw RGBA bytes, convert them into a host image, refresh from the artifact file, re-read the crop summary on each refresh so output path/dimension changes are applied instead of causing stale byte-count failures, send input events back to the guest-tools `WindowInput` command, debounce host proxy move/resize events into `SetWindowBounds { id, x, y, width, height }`, send `CloseWindow` when the user closes the proxy shell, and now keeps a tracked-shell refresh loop running so later `ListWindows` payloads replace stale geometry/crop paths or close missing guest windows. Linux real desktop sessions execute those bounds updates through `wmctrl -ir <id> -e 0,x,y,width,height`, while scaffold sessions record the requested bounds. The new Apple VZ display export can act as a whole-view RGBA file producer for this path, but this is still file-backed AppKit capture, not true per-window streaming or Parallels-style Coherence.
- **Application-consistent snapshots (freeze/thaw): VERIFIED live.** The daemon orchestrates guest `fs-freeze` → disk snapshot → `fs-thaw`, with a structural **always-thaw** guarantee (a thaw-dispatch error can no longer leave the guest frozen). The live e2e smoke (`application-consistent-snapshot-live-opt-in-smoke.sh`) now **passes on-device** (Apple Silicon, QEMU/HVF, Debian 12 arm64 cloud image): a daemon-owned guest boots, the daemon receives the agent's GuestHello, runs a real `fsfreeze -f`, takes the qcow2 snapshot, runs a real `fsfreeze -u`, and the snapshot is recorded. Getting it green required a daemon fix: the daemon now connects to the guest-tools socket **host-first and HOLDS the connection** across reconcile ticks (reconcile_guest_tools_session), so it catches the agent's one-shot GuestHello instead of reconnecting each tick and racing past it. The held connection **peeks (MSG_PEEK) for a complete newline-terminated frame before consuming anything**, so a GuestHello split across host reads (virtio-serial chunks it) can't be partially consumed + lost when the read timeout fires mid-frame — the leftover stays in the kernel socket buffer for the drain reader. Regression-tested by `reconcile_holds_connection_and_catches_delayed_guest_hello` + `reconcile_reassembles_a_guest_hello_split_across_reads`.
- **Daemon shutdown reaps its children (FIXED).** `bridgevmd` now installs SIGTERM/SIGINT handlers and, on shutdown, tears down every backend it spawned (QMP `quit` → `SIGTERM`/`SIGKILL`, with a force-kill fallback if the graceful path bails). Previously a killed daemon orphaned its QEMU/AppleVzRunner children, which kept running and holding their ports. Verified end-to-end (SIGTERM the daemon → QEMU reaped, port freed) and unit-tested (`shutdown_reaps_supervised_children_so_none_orphan`). The daemon has no pid re-adoption path, so reaping-on-exit is the correct behavior.
- **Concurrent Compat VMs get distinct VNC displays (FIXED).** Compatibility Mode launches with a VNC display; it used to pin `-display vnc=:0` (TCP 5900) for every VM, so a second Compat VM failed to start. Spawn paths now call `assign_free_vnc_display`, which picks the lowest free display, avoiding both bound ports and displays already handed to the daemon's other live children (a bare port probe races because QEMU binds its VNC port late in startup, so two back-to-back launches would both grab `:0`). The macOS app's viewer endpoint already parses `vnc=:N` → port 5900+N, so no app change was needed. Verified e2e (two daemon-owned Compat VMs → `:0`/`:1`, both bound + alive) and unit-tested. Daemon-less CLI launches use a port-probe only (best-effort). If the whole display range is exhausted `assign_free_vnc_display` now returns a hard error (the spawn fails loudly) instead of silently falling back to the colliding `:0`.
- **Embedded graphical display (Fast/VZ): verified live, including GUI pixels.** Architecture decision made: the `AppleVzRunner` helper hosts the display window itself (it already runs the VM in-process). New, isolated path so the verified headless boot + save/restore is untouched:
  - `AppleVzConfigurationBuilder.buildLinuxKernelConfigurationWithDisplay` adds a Virtio GPU scanout + USB keyboard + USB pointing device (macOS 14+); the headless builder is unchanged (a graphics device disables VZ save/restore, so the display path deliberately has no suspend/resume).
  - `AppleVzVirtualMachineLauncher.launchLinuxKernelVirtualMachineWithDisplay` creates the VM on the main queue and hosts it in a resizable `NSWindow` + `VZVirtualMachineView` via an AppKit run loop; the initial window frame now matches the requested Virtio GPU scanout size instead of staying hard-coded.
  - `AppleVzRunner --display` flag (threads `AppleVzLaunchOptions.displayWindow`); `--display-width PX --display-height PX` selects the Virtio GPU scanout size (default `1280x800`).
  - `lightvm-runner --apple-vz-display` forwards `--display` to the AppleVzRunner helper; `--apple-vz-display-width/--apple-vz-display-height` forward the scanout dimensions (unit-tested: `launch_handoff_forwards_display_to_helper`).
  - api `display_fast_backend` + `fast_runner_args(..., display)` push `--apple-vz-display`; `display_fast_backend_with_size` carries optional dimensions through to the runner (unit-tested: `fast_runner_args_display_appends_display_flag`, `fast_runner_args_display_appends_display_dimensions`).
  - **CLI `bridgevm display <vm> [--width PX --height PX]`** drives the whole chain (`display <vm>` → display_fast_backend → lightvm-runner `--apple-vz-display` → AppleVzRunner `--display` → graphics config + window). Local-GUI only (rejects `--socket`; requires `BRIDGEVM_APPLE_VZ_RUNNER`).
  - **The graphics config is PROVEN to boot a real guest** (headless): `AppleVzRunner --graphics` boots the with-graphics config without a window. Verified on-device — a signed AppleVzRunner booted the Debian arm64 fixture with the Virtio GPU attached and reached the installer menu on the serial console (`scripts/run-vz-display-demo.sh --check --width 1440 --height 900` passed).
  - **The on-screen window is PROVEN too:** `scripts/run-vz-display-demo.sh --prove-window --width 1280 --height 800 --proof-seconds 16 --capture-delay 6 --evidence-dir /tmp/bridgevm-vz-display-window-proof` opened a real AppKit `VZVirtualMachineView`, found the `BridgeVM — vz-display-demo` window by CGWindow ID, captured that exact window with `screencapture -l`, and wrote `viewer-frame.png` + `viewer-evidence.json` + `display-window-proof.json`. The preserved frame shows the Debian installer main menu rendered inside the Fast/VZ window (Retina capture: `2696x1800`, requested scanout/window: `1280x800`).
  - **One-command demo/proof:** `scripts/run-vz-display-demo.sh --preflight` now reports local fixture/helper/tool readiness without downloading, signing, launching Apple VZ, opening a GUI window, or running `displayd`. `scripts/run-vz-display-demo.sh [--width PX --height PX]` builds+signs the runner, fetches the Debian fixture, stages a bundle + handoff, and opens the window. `--check` runs the same graphics configuration headless for CI/SSH; `--prove-window` opens the GUI window and preserves display-window proof artifacts; `--prove-proxy-crop` additionally enables app-direct raw RGBA export, materializes a `displayd` crop artifact (`app-direct-framebuffer.rgba`, `app-direct-window-crop.json`, `app-direct-window-crop.rgba`, `app-direct-proxy-crop-proof.json`), and runs `verify-vz-proxy-crop-evidence.sh` before reporting PASS. Preserved local crop pass: `scripts/run-vz-display-demo.sh --prove-proxy-crop --width 1280 --height 800 --proof-seconds 18 --capture-delay 6 --evidence-dir ~/bridgevm-live-evidence/apple-vz-proxy-crop-2026-06-18-auto-verified` captured `viewer-frame.png` (`2696x1800`) and wrote a `640x400` crop artifact from a `1280x800` app-direct framebuffer plus `app-direct-proxy-crop-verifier.output`.
  - **Optional proxy framebuffer export:** `AppleVzRunner --display` accepts `--proxy-framebuffer-rgba-file PATH` and `--proxy-framebuffer-capture-interval-ms N`. When enabled, it periodically captures the `VZVirtualMachineView` through AppKit, converts it to raw RGBA bytes, and atomically writes the file; runtime-control `status` reports `framebuffer_export.enabled`, `path`, and `interval_millis`. `lightvm-runner`, `bridgevm-api`, and the macOS app thread this through as `--apple-vz-proxy-framebuffer-rgba-file`, defaulting display launches to `<bundle>/metadata/apple-vz-display-framebuffer.rgba` with the helper's 500 ms interval. This is a pragmatic whole-view file producer for the proxy-crop bridge, not a Metal compositor or DirectX/Metal acceleration layer.
  - Display launch metadata is locked by smoke coverage: display runner
    metadata now records `launch_spec_path`, uses the real
    `lightvm-runner --launch-spec <bundle>/metadata/apple-vz-launch.json`
    command shape, and carries the default proxy framebuffer export argument
    (`--apple-vz-proxy-framebuffer-rgba-file
    <bundle>/metadata/apple-vz-display-framebuffer.rgba`) instead of stale fake
    planner flags.
  - **macOS app "Show Display" button DONE:** the VM diagnostics panel now shows Width/Height fields plus a "Show Display" button for Fast Mode VMs (`ConsoleDiagnosticsPanel` → `VMDetailView` → `DashboardView` → `DashboardViewModel.showDisplay`). It validates the requested size, then spawns the bundled `lightvm-runner` with `--apple-vz-display --apple-vz-display-width <w> --apple-vz-display-height <h> --apple-vz-runtime-control-socket /tmp/bvm-vz-<stable-bundle-hash>.sock --apple-vz-proxy-framebuffer-rgba-file <bundle>/metadata/apple-vz-display-framebuffer.rgba` (no `--store`, so it uses the same default store as the bundled daemon) via `EmbeddedDisplayLauncher`, which opens the window outside the daemon path (local GUI session). After a successful launch, the app refreshes runner status and records a foreground runtime policy so the diagnostics/resource panels follow the visible display session without changing the success alert. The Launch Readiness panel now also exposes daemon-backed display-control Status/Policy/Pacing/Stop Display buttons when runner metadata advertises `runtime_control`, and it shows the latest JSON response. Unit-tested (`EmbeddedDisplayLauncherTests`: arg-builder, custom display size, runtime control socket arg/path, helper resolution, missing-helper error, proxy framebuffer export args/default path; `DashboardViewModelTests`: invalid display dimensions, post-launch foreground cache refresh, runtime-control status/policy/pacing/stop result handling; `DaemonDTOTests`: `runtime_control` request/response wire format + daemon client name-cache path); app tests green.

**Need user resources:**
- **Developer ID / notarization** — user's paid Apple Developer cert + notarytool profile (only blocks public signed distribution; local dev uses ad-hoc signing).
- **Host-only / bridged QEMU networking** — `com.apple.vm.networking` entitlement or root-run QEMU for macOS vmnet backends (NAT + port-forward already work without it).
- **Full Windows install** — Windows license/ISO + TPM 2.0 (swtpm) + Secure Boot (reaching Setup is already proven).

**Resource manager (§14) — battery-adaptive Fast Mode resources + runtime policy signal DONE; display pacing consumer + display control socket started:** Fast Mode cold starts — the api `cold_start_fast_backend`/`display_fast_backend` (daemon-less CLI) AND the daemon's own `spawn_fast_backend_with_restore` (the app's primary path) — now expand `auto` memory/cpu using the host power state at launch (`bridgevm-resource-manager::read_on_battery` parses `pmset -g batt`, honoring `BRIDGEVM_FORCE_ON_BATTERY` for tests/demos). Policy: on battery, `auto` Automatic/Office VMs step down to conserve power (4096/2 → 2048/1); Performance/Developer keep their headroom; explicit per-VM values are always respected. Runtime `bridgevm resources reapply <vm> --visibility foreground|background` and the live-control-oriented alias `bridgevm runtime-control reapply <vm> --visibility foreground|background` now record a foreground/background + power-aware policy in `metadata/runtime-resources.json` over both local CLI and daemon socket, preserving explicit memory/CPU and exposing `display_fps_cap`, rationale, `runtime_control_acknowledged`, and `live_apply_blockers`. If a windowed Apple VZ display helper is alive and advertises `policy`, reapply asks that helper to read the refreshed policy and records the acknowledgement without claiming CPU/RAM hot-apply. `bridgevm display`/Show Display also records a foreground runtime policy when it starts the windowed Fast/VZ path, passes a runtime control socket to AppleVzRunner, and records `runtime_control` metadata (`apple-vz-display`, `status`, `stop`, `policy`, `pacing`) beside runner metadata. `bridgevm runner-status <vm>` shows that socket, and `bridgevm runtime-control status|stop|policy|pacing <vm>` can now send live JSON commands to the recorded Apple VZ display socket locally or through the daemon socket; the macOS app exposes the same display-control actions in Launch Readiness. The `policy` command reads the current `metadata/runtime-resources.json` from inside the live display helper, while `pacing` summarizes the policy-derived display visibility and FPS cap seen by that helper. `displayd --runtime-policy-file metadata/runtime-resources.json` consumes that file for display pacing: it uses policy visibility and caps `max_fps` when `display_fps_cap` is numeric, while preserving `adaptive` as visibility-derived pacing; its `--window-*` arguments lock the proxy-window crop/input mapping contract, and `--framebuffer-rgba-file` + `--window-crop-rgba-file` can materialize a tested raw RGBA window crop. The AppleVzRunner display process now exposes live runtime control IPC for status/stop/policy/pacing, but this is not CPU/RAM hot-apply or live per-window display streaming. Unit/integration-tested (`runtime_decision_uses_foreground_background_signal`, `handler_reapplies_runtime_resources_for_background_fast_vm`, `handler_acknowledges_runtime_policy_when_display_control_reads_it`, `display_runtime_policy_uses_foreground_visibility`, `fast_runner_args_display_appends_runtime_control_socket`, `runtime_control_status_uses_recorded_socket_metadata`, `AppleVzDisplayRuntimeControlServerTests`, `runtime-resource-policy-cli-smoke.sh`, `displayd-plan-cli-smoke.sh`). Not applied to resume (must match saved state) or Compat (heavyweight mode). Remaining §14 work: real live Apple VZ CPU/RAM control plus policy application to a running VM (`live_applied` is still false with `runtime-control-unavailable`).

**Smaller follow-ups (implementable now):** Compat live resume (needs a non-HVF path).

## Where to look
- `PLAN.md` — full plan, roadmap, §20 "Current scaffold progress" running log.
- `crates/` — Rust engine (config, qemu, apple-vz, api, daemon, cli, …).
- `apps/macos/` — SwiftUI app + AppleVzRunner.
- `tests/integration/` — smokes (the de-facto behavior record; see its README).
- `docs/` — per-mode/feature docs.
