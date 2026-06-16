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
- **Two robustness follow-ups surfaced during that work:** (a) Compatibility Mode pins `-display vnc=:0` (TCP 5900), so two Compat VMs can't run at once and a leftover QEMU squatting on 5900 blocks a spawn — needs per-VM VNC port allocation (note: the Swift viewer endpoint parser is coupled to the `vnc=:` form). (b) When `bridgevmd` is killed (SIGTERM) it does not tear down its supervised QEMU children, so they orphan and keep holding their ports — `bridgevm stop` works, but daemon shutdown should reap children too.
- **Embedded graphical display** — `VZVirtioGraphicsDeviceConfiguration` exists, but `VZVirtualMachineView` needs the VM *in-process* while VMs run in the `AppleVzRunner` helper. Needs an architecture decision (helper hosts the display window vs. stream a framebuffer) and a GUI session to verify (can't verify headless). Adding a graphics device may also disable save/restore — must be conditional.

**Need user resources:**
- **Developer ID / notarization** — user's paid Apple Developer cert + notarytool profile (only blocks public signed distribution; local dev uses ad-hoc signing).
- **Bridged networking** — `com.apple.vm.networking` entitlement (NAT + port-forward already work without it).
- **Full Windows install** — Windows license/ISO + TPM 2.0 (swtpm) + Secure Boot (reaching Setup is already proven).

**Smaller follow-ups (implementable now):** resource manager (§14 auto CPU/RAM/battery), real displayd frame sampling, in-guest perf benchmarks (needs the guest agent), readiness graphical boot-progress recording (release-gate semantics — needs sign-off), Compat live resume (needs a non-HVF path).

## Where to look
- `PLAN.md` — full plan, roadmap, §20 "Current scaffold progress" running log.
- `crates/` — Rust engine (config, qemu, apple-vz, api, daemon, cli, …).
- `apps/macos/` — SwiftUI app + AppleVzRunner.
- `tests/integration/` — smokes (the de-facto behavior record; see its README).
- `docs/` — per-mode/feature docs.
