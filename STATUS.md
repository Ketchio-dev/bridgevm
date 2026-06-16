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

**Implementable but infra-gated (need a one-time setup):**
- **Guest tools running *in* the guest** (clipboard, shared folders/virtiofs, dynamic resolution, drag-drop) — the host-side virtio-serial channel is already in the launch command; the gap is the `bridgevm-tools-linux` agent running inside the guest. Blocked on: (a) a Linux-arm64 cross-toolchain (not installed: no `aarch64-*-gcc`/`cross`/`zig`, no rust `aarch64-unknown-linux-gnu` target), and (b) injecting the built agent into a booting guest (cloud-init seed ISO) to verify effects.
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
