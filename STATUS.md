# BridgeVM — Current Status

Concise "where are we" snapshot. Full plan/roadmap/scaffold log lives in
[PLAN.md](PLAN.md); this file is the fast scan.

_Last updated: 2026-06-16._

## What this is
Open-source, Parallels-class Mac virtualization app with two engines:
- **Fast Mode (`LightVM`)** = Apple Virtualization.framework (NOT QEMU) — lightweight path for modern guests.
- **Compatibility Mode (`FullVM`)** = QEMU + HVF — runs everything else (Windows, older/x86 guests).

## Phase 0 live-boot evidence (the headline milestone)
All three "live boot proof" criteria are now demonstrated on Apple Silicon:

| Criterion | Status | Evidence |
| --- | --- | --- |
| QEMU/HVF Linux Arm64 (Compatibility) | ✅ proven + recorded | `~/bridgevm-live-evidence/qemu-arm64-2026-06-16/` |
| Apple VZ Linux Arm64 (Fast) | ✅ proven + recorded | `~/bridgevm-live-evidence/apple-vz-arm64-2026-06-16/` |
| Windows 11 Arm installer reachability | ✅ proven (graphical) | `~/bridgevm-live-evidence/windows-arm64-2026-06-16/` |

Live evidence is captured via opt-in smokes (`tests/integration/qemu-live-boot-opt-in-smoke.sh`,
`apple-vz-live-boot-opt-in-smoke.sh`) and recorded with `bridgevm readiness --record-live-evidence`.

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
`bridgevm run <vm> --spawn` now **really boots** a Fast Mode (Apple VZ) Linux VM (was dry-run only) when `BRIDGEVM_APPLE_VZ_RUNNER` is set — records a real pid, `dry_run:false`, state `running`; without the env it preserves the legacy dry-run/not-implemented behavior (back-compat). `bridgevm suspend <vm>` / `bridgevm resume <vm>` work end-to-end, wired runner → `lightvm-runner` → `bridgevm-api`/daemon/CLI → macOS app:
- AppleVzRunner does VZ `saveMachineState`/`restoreMachineState` (`--save-state`/`--restore-state`); machine identifier + NAT MAC persisted per bundle (required for restore to match).
- `suspend` boots the Fast VM, pauses, saves to `metadata/suspend-images/<vm>.bin`, marks `suspended`; `resume` restores + runs detached, marks `running`. Needs `BRIDGEVM_APPLE_VZ_RUNNER` (path to a signed AppleVzRunner).
- macOS app pause/resume send `suspend_backend`/`resume_backend` daemon requests.
- Verified: real Debian arm64 VZ guest suspended (98 MB state) → resumed to a running guest.
- `bridgevm stop <vm>` now reliably terminates the running VM process (SIGTERM→grace→SIGKILL) for both Fast (AppleVzRunner) and Compat (qemu) — no orphan left (release gate). The daemon supervises resumed children like cold-start.
- Compat (QEMU) suspend: `bridgevm suspend` does a QMP `snapshot-save` internal qcow2 snapshot (`bridgevm-suspend`). **Compat resume is not supported on Apple Silicon under HVF** — QEMU aborts in `cpu_pre_load` restoring an HVF arm64 guest; resume reports this honestly and preserves the snapshot. Fast Mode is the supported suspend/resume path.
- Follow-ups: pause an already-running Fast VM via IPC (current model boots→saves); Compat live resume (needs a non-HVF path or a future QEMU fix).

## Networking
Compatibility Mode (QEMU) NAT + port forwarding works at launch: manifest `network.forwards` become QEMU `hostfwd=tcp::HOST-:GUEST` in the launch command, so the host port is actually bound when the VM runs (verified: host port LISTENs). `bridgevm port add/remove` edit the forwards. Bridged networking still needs the `com.apple.vm.networking` entitlement (user resource).

## Verification lanes
- **Safe app lane:** `tests/integration/local-release-readiness-suite.sh --app-only --locally-usable-app`
- **Rust:** `cargo test --workspace`
- **Live boot (opt-in, heavy):** the `*-live-boot-opt-in-smoke.sh` scripts (need a real disk/ISO + `*_ALLOW_REAL_START=1`).

## Remaining work to fully "complete" the app (with blockers)
The VM lifecycle (create/run/suspend/resume/stop) + networking + boot evidence are done. The remaining features each have a concrete blocker:

**Guest tools — transport PROVEN, effects remaining:**
- The `bridgevm-tools-linux` agent now cross-compiles to Linux-arm64 (`scripts/build-guest-agent-linux.sh`, via zig + cargo-zigbuild) and the **full transport is verified end-to-end**: the agent runs inside a booted Debian guest (cloud-init NoCloud seed), sends `GuestHello` over `/dev/virtio-ports/org.bridgevm.guest-tools.0` → QEMU virtio-serial → host `guest-tools.sock`, and the host `accept-hello` validates token + capabilities (tampered token rejected). Smoke: `tests/integration/guest-tools-live-handshake-opt-in-smoke.sh`. Gotcha: the guest agent must advertise a capability subset matching the manifest `AgentPolicy` (default manifest disables drag-drop/agent-update).
- **Guest-tools effects (real, verified):** `time-sync` actually sets the guest clock (`settimeofday`, capability-gated) — verified in-guest (wall clock jumped 2001→now) via `tests/integration/guest-tools-effects-opt-in-smoke.sh`; `guest-metrics` now reports real `/proc` values; `fs-freeze`/`fs-thaw` have Real backends. Note: macOS AF_UNIX paths are capped at 104 bytes, so the guest-tools.sock path must stay short.
- **Remaining guest-tools effects:** clipboard sync + dynamic resolution need a GUI guest (Xorg/Wayland + tooling) to verify; shared folders need virtiofs (`virtiofsd` is Linux-only/unavailable on macOS for QEMU, so it requires VZ-native `VZVirtioFileSystemDevice` + a mountable VZ guest). Their command-backends are unit-tested but not headlessly assertable here.
- **Application-consistent snapshots (freeze/thaw): VERIFIED live.** The daemon orchestrates guest `fs-freeze` → disk snapshot → `fs-thaw`, with a structural **always-thaw** guarantee (a thaw-dispatch error can no longer leave the guest frozen). The live e2e smoke (`application-consistent-snapshot-live-opt-in-smoke.sh`) now **passes on-device** (Apple Silicon, QEMU/HVF, Debian 12 arm64 cloud image): a daemon-owned guest boots, the daemon receives the agent's GuestHello, runs a real `fsfreeze -f`, takes the qcow2 snapshot, runs a real `fsfreeze -u`, and the snapshot is recorded. Getting it green required a daemon fix: the daemon now connects to the guest-tools socket **host-first and HOLDS the connection** across reconcile ticks (reconcile_guest_tools_session), so it catches the agent's one-shot GuestHello instead of reconnecting each tick and racing past it (regression-tested by `reconcile_holds_connection_and_catches_delayed_guest_hello`).
- **Daemon shutdown reaps its children (FIXED).** `bridgevmd` now installs SIGTERM/SIGINT handlers and, on shutdown, tears down every backend it spawned (QMP `quit` → `SIGTERM`/`SIGKILL`, with a force-kill fallback if the graceful path bails). Previously a killed daemon orphaned its QEMU/AppleVzRunner children, which kept running and holding their ports. Verified end-to-end (SIGTERM the daemon → QEMU reaped, port freed) and unit-tested (`shutdown_reaps_supervised_children_so_none_orphan`). The daemon has no pid re-adoption path, so reaping-on-exit is the correct behavior.
- **Concurrent Compat VMs get distinct VNC displays (FIXED).** Compatibility Mode launches with a VNC display; it used to pin `-display vnc=:0` (TCP 5900) for every VM, so a second Compat VM failed to start. Spawn paths now call `assign_free_vnc_display`, which picks the lowest free display, avoiding both bound ports and displays already handed to the daemon's other live children (a bare port probe races because QEMU binds its VNC port late in startup, so two back-to-back launches would both grab `:0`). The macOS app's viewer endpoint already parses `vnc=:N` → port 5900+N, so no app change was needed. Verified e2e (two daemon-owned Compat VMs → `:0`/`:1`, both bound + alive) and unit-tested. Daemon-less CLI launches use a port-probe only (best-effort).
- **Embedded graphical display (Fast/VZ): structural implementation landed; window needs GUI verification.** Architecture decision made: the `AppleVzRunner` helper hosts the display window itself (it already runs the VM in-process). New, isolated path so the verified headless boot + save/restore is untouched:
  - `AppleVzConfigurationBuilder.buildLinuxKernelConfigurationWithDisplay` adds a Virtio GPU scanout + USB keyboard + USB pointing device (macOS 14+); the headless builder is unchanged (a graphics device disables VZ save/restore, so the display path deliberately has no suspend/resume).
  - `AppleVzVirtualMachineLauncher.launchLinuxKernelVirtualMachineWithDisplay` creates the VM on the main queue and hosts it in a resizable `NSWindow` + `VZVirtualMachineView` via an AppKit run loop.
  - `AppleVzRunner --display` flag (threads `AppleVzLaunchOptions.displayWindow`).
  - `lightvm-runner --apple-vz-display` forwards `--display` to the AppleVzRunner helper (unit-tested: `launch_handoff_forwards_display_to_helper`).
  - api `display_fast_backend` + `fast_runner_args(..., display)` push `--apple-vz-display` (unit-tested: `fast_runner_args_display_appends_display_flag`).
  - **CLI `bridgevm display <vm>`** drives the whole chain (`display <vm>` → display_fast_backend → lightvm-runner `--apple-vz-display` → AppleVzRunner `--display` → graphics config + window). Local-GUI only (rejects `--socket`; requires `BRIDGEVM_APPLE_VZ_RUNNER`).
  - **The graphics config is PROVEN to boot a real guest** (headless): `AppleVzRunner --graphics` boots the with-graphics config without a window (a verification path the windowed form can't do on a host with no window server). Verified on-device — a signed AppleVzRunner booted the Debian arm64 fixture with the Virtio GPU attached and reached the installer menu on the serial console. The only part that needs a GUI is the on-screen pixels.
  - **One-command demo:** `scripts/run-vz-display-demo.sh` builds+signs the runner, fetches the Debian fixture, stages a bundle + handoff, and opens the window (`--check` runs the headless graphics boot instead, for CI/SSH — verified passing). So the embedded display is end-to-end runnable today; a user runs the demo in a GUI session to see the window.
  - Optional polish: a macOS app "Show display" button (the app bundles `lightvm-runner` + `AppleVzRunner` but not `bridgevm`, so it would spawn the runner directly).

**Need user resources:**
- **Developer ID / notarization** — user's paid Apple Developer cert + notarytool profile (only blocks public signed distribution; local dev uses ad-hoc signing).
- **Bridged networking** — `com.apple.vm.networking` entitlement (NAT + port-forward already work without it).
- **Full Windows install** — Windows license/ISO + TPM 2.0 (swtpm) + Secure Boot (reaching Setup is already proven).

**Resource manager (§14) — battery-adaptive Fast Mode resources DONE:** Fast Mode cold starts (`cold_start_fast_backend`, `display_fast_backend`) now expand `auto` memory/cpu using the host power state at launch (`bridgevm-resource-manager::read_on_battery` parses `pmset -g batt`, honoring `BRIDGEVM_FORCE_ON_BATTERY` for tests/demos). Policy: on battery, `auto` Automatic/Office VMs step down to conserve power (4096/2 → 2048/1); Performance/Developer keep their headroom; explicit per-VM values are always respected. Unit-tested (`parses_pmset_battery_state`, `launch_decision_steps_down_auto_profiles_on_battery`, `power_aware_resolution_only_affects_auto_values`). Not applied to resume (must match saved state) or Compat (heavyweight mode). Remaining §14 work: runtime re-apply while running + foreground/background signal.

**Smaller follow-ups (implementable now):** real displayd frame sampling, in-guest perf benchmarks (needs the guest agent), readiness graphical boot-progress recording (release-gate semantics — needs sign-off), Compat live resume (needs a non-HVF path).

## Where to look
- `PLAN.md` — full plan, roadmap, §20 "Current scaffold progress" running log.
- `crates/` — Rust engine (config, qemu, apple-vz, api, daemon, cli, …).
- `apps/macos/` — SwiftUI app + AppleVzRunner.
- `tests/integration/` — smokes (the de-facto behavior record; see its README).
- `docs/` — per-mode/feature docs.
