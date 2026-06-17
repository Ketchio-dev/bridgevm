# BridgeVM — Current Status

Concise "where are we" snapshot. Full plan/roadmap/scaffold log lives in
[PLAN.md](PLAN.md); this file is the fast scan.

_Last updated: 2026-06-16._

## Adversarial review round 2 (3 parallel reviews: api lifecycle / agent protocol / storage-config-qemu)
A deeper read-only sweep of the correctness-critical subsystems found a large set
of real bugs. **All HIGH + most MED are FIXED + tested:**
- **agent protocol/security:** F1 (HIGH host-OOM: unbounded frame → MAX_FRAME_BYTES cap), F2 (HIGH: non-constant-time token compare → constant-time), F3 (MED: oversized-handshake spin → reset), F4 (MED: auto-detected `xclip`/`xrandr` resolved via guest PATH → pinned PATH for effect commands), F5 (MED: agent's own clipboard spawn could hang it via daemonized-child fd-hold/stdin-deadlock → null fds + threaded stdin + bounded wait).
- **storage/config/qemu:** HIGH-1 (manifest path bundle-escape → arbitrary file write → `ensure_bundle_relative`), HIGH-2 (QEMU option-string comma injection → escaped), MED-3/MED-4 (memory/size `checked_mul`), MED-5 (empty-slug name → rejected), MED-6 (non-atomic runner.json/state.json → atomic temp+rename).
- **api lifecycle:** HIGH-1 (PID-reuse → could SIGKILL an unrelated recycled pid; now validates `ps -o etime` start-time vs recorded launch), HIGH-3 (atomic metadata, = MED-6).
- **Also fixed:** `wait_for_job` (api #5) now requires OBSERVING the snapshot job before treating its disappearance as success (a failed/reaped-before-seen snapshot-save no longer reads as a successful snapshot → no resume into a missing snapshot); agent **F6** (file-drop now rejects a chunk the moment accumulated bytes would exceed the declared size); agent **F7** (`UnexpectedEof` no longer classified as idle/retry — it's terminal).
- **Remaining MED/LOW backlog (documented):** the api lifecycle metadata/runtime *divergence-on-partial-failure* cluster (suspend's `transition_state` running after the irreversible kill, resume's fixed-2s `-loadvm` survival heuristic, fast-suspend transition gating, the two-file spawn crash window). These share one fix — **a per-VM lock around the suspend/resume/stop orchestration + idempotent ordering that commits the state transition (force-write) only with the irreversible action, reconciling against pid liveness** — best done as one focused change rather than piecemeal.

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
- **Shared folders (Fast/VZ): device wired + boots; in-guest mount is the interactive step.** `AppleVzConfigurationBuilder` adds a VZ-native `VZVirtioFileSystemDeviceConfiguration` (single-directory share + validated tag, macOS 13+) when a share is requested — no `virtiofsd` (that is QEMU-only/unavailable on macOS). Driven by `AppleVzRunner --share-dir PATH [--share-tag TAG] [--share-read-only]`, threaded through both the headless and windowed launch paths and the demo script. Verified: the config validates with the device and a real guest **boots with the share + graphics attached** (demo `--check`). The guest mounts it with `mount -t virtiofs <tag> <dir>` (the demo prints the command); a non-interactive mount assertion needs a scriptable VZ guest (the netboot-installer fixture isn't). Unit-tested (`testBuildsVirtioSharedDirectoryDeviceWhenRequested`, `testShareDirFlagsThreadSharedDirectoryToLauncher`). Compat (QEMU) shared folders remain blocked (no macOS `virtiofsd`).
- **Clipboard sync + dynamic resolution — VERIFIED live with the real tools (headless).** No GUI desktop is needed: a headless X server (`Xvfb`) lets `xclip`/`xrandr` run and be checked. `tests/integration/guest-tools-clipboard-resize-effects-opt-in-smoke.sh` boots a guest, apt-installs Xvfb/xclip/x11-xserver-utils, and drives the host→guest `SetClipboard` and `ResizeDisplay` commands end to end — it PASSES, asserting the guest's X clipboard reads back exactly the host's text (real `xclip`) and the agent ran `xrandr` with the host geometry, over the live virtio-serial transport. The agent also now **auto-detects** the right tool out of the box (`wl-copy` on Wayland, `xclip` on X11, `xrandr` for resize) when no explicit command is configured (unit-tested). (Gotcha found + fixed while writing the smoke: `xclip -i` daemonizes holding the agent's stdout/stderr pipe, hanging `wait_with_output`; the wrapper redirects its fds to /dev/null.)
- **Application-consistent snapshots (freeze/thaw): VERIFIED live.** The daemon orchestrates guest `fs-freeze` → disk snapshot → `fs-thaw`, with a structural **always-thaw** guarantee (a thaw-dispatch error can no longer leave the guest frozen). The live e2e smoke (`application-consistent-snapshot-live-opt-in-smoke.sh`) now **passes on-device** (Apple Silicon, QEMU/HVF, Debian 12 arm64 cloud image): a daemon-owned guest boots, the daemon receives the agent's GuestHello, runs a real `fsfreeze -f`, takes the qcow2 snapshot, runs a real `fsfreeze -u`, and the snapshot is recorded. Getting it green required a daemon fix: the daemon now connects to the guest-tools socket **host-first and HOLDS the connection** across reconcile ticks (reconcile_guest_tools_session), so it catches the agent's one-shot GuestHello instead of reconnecting each tick and racing past it. The held connection **peeks (MSG_PEEK) for a complete newline-terminated frame before consuming anything**, so a GuestHello split across host reads (virtio-serial chunks it) can't be partially consumed + lost when the read timeout fires mid-frame — the leftover stays in the kernel socket buffer for the drain reader. Regression-tested by `reconcile_holds_connection_and_catches_delayed_guest_hello` + `reconcile_reassembles_a_guest_hello_split_across_reads`.
- **Daemon shutdown reaps its children (FIXED).** `bridgevmd` now installs SIGTERM/SIGINT handlers and, on shutdown, tears down every backend it spawned (QMP `quit` → `SIGTERM`/`SIGKILL`, with a force-kill fallback if the graceful path bails). Previously a killed daemon orphaned its QEMU/AppleVzRunner children, which kept running and holding their ports. Verified end-to-end (SIGTERM the daemon → QEMU reaped, port freed) and unit-tested (`shutdown_reaps_supervised_children_so_none_orphan`). The daemon has no pid re-adoption path, so reaping-on-exit is the correct behavior.
- **Concurrent Compat VMs get distinct VNC displays (FIXED).** Compatibility Mode launches with a VNC display; it used to pin `-display vnc=:0` (TCP 5900) for every VM, so a second Compat VM failed to start. Spawn paths now call `assign_free_vnc_display`, which picks the lowest free display, avoiding both bound ports and displays already handed to the daemon's other live children (a bare port probe races because QEMU binds its VNC port late in startup, so two back-to-back launches would both grab `:0`). The macOS app's viewer endpoint already parses `vnc=:N` → port 5900+N, so no app change was needed. Verified e2e (two daemon-owned Compat VMs → `:0`/`:1`, both bound + alive) and unit-tested. Daemon-less CLI launches use a port-probe only (best-effort). If the whole display range is exhausted `assign_free_vnc_display` now returns a hard error (the spawn fails loudly) instead of silently falling back to the colliding `:0`.
- **Embedded graphical display (Fast/VZ): structural implementation landed; window needs GUI verification.** Architecture decision made: the `AppleVzRunner` helper hosts the display window itself (it already runs the VM in-process). New, isolated path so the verified headless boot + save/restore is untouched:
  - `AppleVzConfigurationBuilder.buildLinuxKernelConfigurationWithDisplay` adds a Virtio GPU scanout + USB keyboard + USB pointing device (macOS 14+); the headless builder is unchanged (a graphics device disables VZ save/restore, so the display path deliberately has no suspend/resume).
  - `AppleVzVirtualMachineLauncher.launchLinuxKernelVirtualMachineWithDisplay` creates the VM on the main queue and hosts it in a resizable `NSWindow` + `VZVirtualMachineView` via an AppKit run loop.
  - `AppleVzRunner --display` flag (threads `AppleVzLaunchOptions.displayWindow`).
  - `lightvm-runner --apple-vz-display` forwards `--display` to the AppleVzRunner helper (unit-tested: `launch_handoff_forwards_display_to_helper`).
  - api `display_fast_backend` + `fast_runner_args(..., display)` push `--apple-vz-display` (unit-tested: `fast_runner_args_display_appends_display_flag`).
  - **CLI `bridgevm display <vm>`** drives the whole chain (`display <vm>` → display_fast_backend → lightvm-runner `--apple-vz-display` → AppleVzRunner `--display` → graphics config + window). Local-GUI only (rejects `--socket`; requires `BRIDGEVM_APPLE_VZ_RUNNER`).
  - **The graphics config is PROVEN to boot a real guest** (headless): `AppleVzRunner --graphics` boots the with-graphics config without a window (a verification path the windowed form can't do on a host with no window server). Verified on-device — a signed AppleVzRunner booted the Debian arm64 fixture with the Virtio GPU attached and reached the installer menu on the serial console. The only part that needs a GUI is the on-screen pixels.
  - **One-command demo:** `scripts/run-vz-display-demo.sh` builds+signs the runner, fetches the Debian fixture, stages a bundle + handoff, and opens the window (`--check` runs the headless graphics boot instead, for CI/SSH — verified passing). So the embedded display is end-to-end runnable today; a user runs the demo in a GUI session to see the window.
  - **macOS app "Show Display" button DONE:** the VM diagnostics panel now shows a "Show Display" button for Fast Mode VMs (`ConsoleDiagnosticsPanel` → `VMDetailView` → `DashboardView` → `DashboardViewModel.showDisplay`). It spawns the bundled `lightvm-runner` with `--apple-vz-display` (no `--store`, so it uses the same default store as the bundled daemon) via `EmbeddedDisplayLauncher`, which opens the window outside the daemon path (local GUI session). Unit-tested (`EmbeddedDisplayLauncherTests`: arg-builder, helper resolution, missing-helper error); 335 app tests green. The window itself still needs a GUI session + a VZ-bootable Fast VM to render.

**Need user resources:**
- **Developer ID / notarization** — user's paid Apple Developer cert + notarytool profile (only blocks public signed distribution; local dev uses ad-hoc signing).
- **Bridged networking** — `com.apple.vm.networking` entitlement (NAT + port-forward already work without it).
- **Full Windows install** — Windows license/ISO + TPM 2.0 (swtpm) + Secure Boot (reaching Setup is already proven).

**Resource manager (§14) — battery-adaptive Fast Mode resources DONE:** Fast Mode cold starts — the api `cold_start_fast_backend`/`display_fast_backend` (daemon-less CLI) AND the daemon's own `spawn_fast_backend_with_restore` (the app's primary path) — now expand `auto` memory/cpu using the host power state at launch (`bridgevm-resource-manager::read_on_battery` parses `pmset -g batt`, honoring `BRIDGEVM_FORCE_ON_BATTERY` for tests/demos). Policy: on battery, `auto` Automatic/Office VMs step down to conserve power (4096/2 → 2048/1); Performance/Developer keep their headroom; explicit per-VM values are always respected. Unit-tested (`parses_pmset_battery_state`, `launch_decision_steps_down_auto_profiles_on_battery`, `power_aware_resolution_only_affects_auto_values`). Not applied to resume (must match saved state) or Compat (heavyweight mode). Remaining §14 work: runtime re-apply while running + foreground/background signal.

**Smaller follow-ups (implementable now):** real displayd frame sampling, in-guest perf benchmarks (needs the guest agent), readiness graphical boot-progress recording (release-gate semantics — needs sign-off), Compat live resume (needs a non-HVF path).

## Where to look
- `PLAN.md` — full plan, roadmap, §20 "Current scaffold progress" running log.
- `crates/` — Rust engine (config, qemu, apple-vz, api, daemon, cli, …).
- `apps/macos/` — SwiftUI app + AppleVzRunner.
- `tests/integration/` — smokes (the de-facto behavior record; see its README).
- `docs/` — per-mode/feature docs.
