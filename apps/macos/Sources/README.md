# BridgeVM macOS App

This directory is reserved for the SwiftUI/AppKit shell described in `PLAN.md`.

The first app milestone should be a native dashboard that calls the Rust daemon instead of duplicating VM lifecycle logic.

## Current prototype

`BridgeVMApp/` contains a SwiftUI dashboard shell with:

- a VM list with search, status indicators, and mode labels
- Store Doctor/source/refresh status in the sidebar, including manual inventory
  refresh and daemon store-root readiness
- detail metrics for guest OS, engine mode, resources, and network state
- a first-pass VM creation sheet backed by daemon boot templates
- a first-pass Boot Media panel for status, local import, SHA256 verification,
  no-download planning, and planned download execution
- an aggregate Readiness card that loads the daemon `readiness_report`,
  reuses its boot-media, snapshot-chain, and runner metadata to warm the
  detail panels, and summarizes user-facing state as metadata ready, live
  evidence pending, or a blocker count without exposing raw blocker paths in
  VM list cards
- a first-pass Guest Tools status panel that reads daemon-backed capabilities,
  runtime state, guest IP, heartbeat, metrics readiness, and the latest command
  result when available, without treating command/status UI as proof of live
  guest-side effects
- basic lifecycle controls wired through a `VirtualMachineClient` protocol,
  including Fast Mode Apple VZ suspend/resume that sends the daemon
  `suspend_backend`/`resume_backend` requests (suspend saves machine state to
  disk synchronously; resume restores it and runs the VM detached), alongside
  suspend/resume plan readiness surfaces for inspecting control before sending
- `MockVirtualMachineClient` sample data for UI development
- `DaemonVirtualMachineClient` with a newline-delimited JSON Unix socket
  transport for the Rust daemon
- daemon-backed Guest Tools command-send DTOs for the typed
  `guest_tools_send_command` boundary, including safe alpha application/window
  list, launch, focus, and close controls
- a Console action that queries daemon `qmp_status`/QMP socket readiness,
  reports backend diagnostic status, and keeps Compatibility Mode external VNC
  viewer handoff distinct from embedded macOS console work
- a Logs panel that requests bounded QEMU and serial log tails through the
  daemon `view_logs` API and renders the latest text without starting a backend
- a Port Forwards panel for daemon-backed manifest `list_ports`, `add_port`,
  and `remove_port` without live networking changes
- a Network Plan panel that requests daemon `network_plan` metadata and renders
  backend mode, executable/dry-run status, capabilities, blockers, notes, and
  planned port forwards without creating live network interfaces
- a Portable Bundle panel for daemon-backed VM bundle export/import metadata,
  including archive format, copied bundle file counts/paths, preserved manifest
  and metadata status, and import rename/manifest rewrite results
- a Runtime Resources panel for Fast Mode VMs that sends the daemon
  `reapply_runtime_resources` request, records foreground/background policy
  metadata, and honestly shows `live_apply_blockers` until live Apple VZ/display
  control IPC exists
- persisted Settings controls for the daemon socket path, mock-inventory mode,
  and local Apple VZ live-start opt-in

The source is intentionally thin: VM lifecycle state should continue to live in
`bridgevmd`, while the macOS app presents inventory and sends user actions.
The daemon client currently speaks the same wire format as `bridgevmd` for
`store_doctor`, `list_vms`, `list_templates`, `create_vm`, `run_backend`,
`suspend_backend`, `resume_backend`, `stop_backend`, and
`reapply_runtime_resources`;
dashboard Start requests route through daemon-owned backend launch with
`spawn=true` rather than only marking metadata as running, and dashboard
Suspend/Resume now route through `suspend_backend`/`resume_backend` (Fast Mode
Apple VZ save/restore) rather than metadata-only transitions;
the Boot Media panel also speaks `inspect_boot_media_status`,
`import_boot_media`, `verify_boot_media`, `plan_boot_media_download`, and
`download_boot_media`.
The aggregate Readiness card speaks `readiness_report` through the same daemon
boundary. It is a compact pre-launch summary only: loading it may refresh cached
boot-media, snapshot-chain, and runner metadata already returned by the daemon,
but it must not prepare disks, write launch specs, connect to QMP, start QEMU,
launch Apple VZ, or touch a guest. When the daemon reports no blockers but still
requires live E2E evidence, the dashboard should keep that distinction visible
as metadata-ready/live-evidence-pending rather than presenting a completed live
boot.
The Portable Bundle panel speaks `export_vm` and `import_vm` through the same
daemon boundary. It renders stopped bundle copy metadata only: source/output
paths, directory-or-tar format, relative copied file paths, manifest/metadata
preservation, and optional import identity rewrites.
The Port Forwards panel speaks `list_ports`, `add_port`, and `remove_port` as
manifest metadata operations. The separate Open Port panel only plans a host URL
or command for an existing forward; neither panel starts a VM, opens a browser,
or mutates a running backend.
The Network Plan panel speaks `network_plan` through the daemon. It is a
planning surface only: it can show NAT/isolated/unsupported mode capability
metadata, executable status, blockers, notes, and planned forwards, but it does
not create host-only or bridged interfaces, attach live port forwards, or start
a backend.
The Console button includes a status diagnostic. It asks the daemon whether the
Compatibility Mode QMP socket is known and ready, then shows that result to the
user. The Logs panel is a separate diagnostic surface for `logs/qemu.log` and
`logs/serial.log` tails. Neither diagnostic path attaches to a framebuffer or
streams display output. Compatibility Mode can still plan explicit VNC display
as a QEMU `-display vnc=:0` dry-run template, and daemon-owned spawn remaps that
template to a free `vnc=:N` display before launch for an external-viewer
handoff. What remains future work is embedding that graphical viewer directly in
the macOS app.
The Guest Tools panel uses the daemon-backed guest tools status boundary for
readiness visibility instead of owning transport behavior in the app. For
daemon-owned Compatibility Mode backends it can show authenticated runtime
state, heartbeat, guest IP, metrics, time-sync capability/readiness, passive
agent-update metadata, and the latest correlated command result. Provisioning
remains a host/guest installation concern: the app can surface the generated
token metadata/status and command readiness, but it does not install or update
guest tools inside the VM. Command/status UI should be described as a typed
daemon boundary until a live guest run preserves observable guest-side effects
for those commands.
The lifecycle controls should use the same daemon/API metadata boundaries as the
CLI. Suspend and resume planning can surface `lifecycle-plan` readiness,
Compatibility Mode QMP `stop`/`cont` intent, socket availability, and blockers.
Fast Mode Apple VZ suspend/resume is now wired end-to-end: Suspend sends
`suspend_backend` (the daemon boots the Fast VM, runs it briefly, pauses, and
saves VZ machine state synchronously) and Resume sends `resume_backend` (restores
the saved state and runs the VM detached). Requires a signed AppleVzRunner via
`BRIDGEVM_APPLE_VZ_RUNNER`. Compatibility Mode suspend/resume remains a
follow-up; pausing an already-running VM over IPC is also still out of scope.
The macOS client can also model the daemon-backed `guest_tools_send_command`
request/response boundary for safe alpha command dispatch, including
list-applications and list-windows actions that can update status/result
surfaces. That is a typed client path rather than proof that guest tools UI
controls are complete or that the app can control real guest applications and
windows.
It can execute a recorded boot media download plan, but it does not choose remote
media URLs or launch Apple Virtualization.framework guests through the dashboard
or daemon path.
Actual guest tools transport and commands remain behind the Compatibility Mode
daemon-owned backend boundary, and Fast Mode process launch exists only through
the separate `AppleVzRunner` helper for the limited supported shape described
below.
By default the dashboard uses the same daemon store convention as the CLI:
`$BRIDGEVM_HOME/run/bridgevmd.sock` when `BRIDGEVM_HOME` is set, otherwise
`~/.bridgevm/run/bridgevmd.sock`. Point Settings at a development socket such
as `target/bridgevm-dev/run/bridgevmd.sock` when running a separate local
daemon store. If the configured socket is unavailable, the dashboard keeps the
daemon error visible instead of silently replacing the inventory with sample
data; enable mock inventory explicitly in Settings when demo data is intended.
Settings are stored in
`UserDefaults`; applying settings rebuilds the dashboard client and reloads the
VM list. The Apply button is enabled only when the edited settings differ from
the values currently applied to the dashboard.
The Apple VZ live-start setting is off by default. When enabled for the bundled
daemon, the app passes `BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1`; changing that
setting causes the bundled daemon to be relaunched so the environment matches
the visible Settings state.

`apps/macos/Package.swift` builds the prototype as Swift Package executables for
quick local validation. `BridgeVMApp` remains the daemon-backed UI shell.
`AppleVzRunner` is a separate helper boundary for the limited Fast Mode backend:
it decodes `AppleVzLaunchHandoff` JSON from a file or stdin, validates the
handoff/readiness contract, links against Virtualization.framework when
available, and can run in `--validate-only` mode. It can also print a
configuration plan and construct or validate only the limited Apple VZ
configuration shape for `linux-kernel` boot with a `raw` primary disk and NAT
networking. `qcow2` remains valid for dry-run readiness and plan output, but
actual VZ configuration construction supports raw disks only.
Validation/config-plan output is evidence for that narrow construction boundary
only. Smoke tests and evidence verifiers should preserve the config-plan markers
from `--validate-only --print-config-plan --validate-vz-config` so reviewers can
see the helper decoded the handoff, selected the supported boot/disk/network
shape, and validated the constructible VZ configuration. Those markers remain
metadata/synthetic-safe because they are emitted before any allowed call to
`VZVirtualMachine.start()`; real boot proof still requires the explicit live
opt-in path and its preserved launch plus serial or graphical boot-progress
evidence.
Reviewer evidence should keep the helper output tied to the exact artifacts that
fed it: the launch spec, handoff JSON, fixture manifest, environment source
paths, selected resources, kernel command line, runner path, validation output,
live-launch output, bounded stop/grace controls, any serial sentinel, and any
verifier-bound `boot-progress-evidence.json` artifact. The
synthetic verifier smoke can exercise those cross-checks without starting Apple
VZ, QEMU, a GUI console, or a networked guest. A dashboard or daemon status of
metadata-ready therefore means the app has enough metadata to explain readiness;
it is not a substitute for a separately preserved and verified live evidence
bundle.
When Rust invokes the helper through `lightvm-runner --apple-vz-runner <path>`,
the helper receives the handoff over stdin. For the supported
`linux-kernel`/`raw`/NAT shape it can construct, validate, start, and wait for a
real `VZVirtualMachine` only when the caller also passes the explicit
`--allow-real-vz-start` opt-in. Without that opt-in, even a supported handoff
fails before `VZVirtualMachine.start()`. Unsupported shapes fail before launch
with clear input errors. Live boot E2E coverage therefore needs real
kernel/initrd/raw disk fixtures, the required Apple virtualization entitlement,
and the live-start opt-in; ordinary smoke scripts should stay on validate-only,
configuration-validation, unsupported-input, or missing-opt-in paths. For local
manual live E2E, run `apps/macos/scripts/build-sign-apple-vz-runner.sh` after
each rebuild to sign the helper with `apps/macos/AppleVzRunner.entitlements`.
`AppleVzRunner --stop-after-seconds N` is available for bounded live fixtures
that should request guest shutdown after startup. A future Xcode project can
wrap the same `Sources/BridgeVMApp` target once signing, app icons,
entitlements, and release packaging are ready.
