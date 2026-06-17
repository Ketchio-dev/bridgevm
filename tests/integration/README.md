# Integration Tests

Executable smoke tests for cross-crate CLI/API contracts live here. They are
intentionally small and metadata-focused so they can run without booting a VM.

Run the current smoke coverage from the repository root:

```sh
tests/integration/template-create-cli-smoke.sh
tests/integration/store-doctor-cli-smoke.sh
tests/integration/mode-recommendation-cli-smoke.sh
tests/integration/windows-arm-qemu-args-cli-smoke.sh
tests/integration/port-forward-cli-smoke.sh
tests/integration/clone-cli-smoke.sh
tests/integration/metadata-repair-cli-smoke.sh
tests/integration/manifest-migration-cli-smoke.sh
tests/integration/manifest-schema-cli-smoke.sh
tests/integration/export-import-cli-smoke.sh
tests/integration/delete-cli-smoke.sh
tests/integration/lifecycle-restart-cli-smoke.sh
tests/integration/lifecycle-suspend-resume-cli-smoke.sh
tests/integration/lifecycle-plan-cli-smoke.sh
tests/integration/snapshot-metadata-create-list-cli-smoke.sh
tests/integration/snapshot-disk-create-cli-smoke.sh
tests/integration/snapshot-list-restore-cli-smoke.sh
tests/integration/suspend-snapshot-cli-smoke.sh
tests/integration/diagnostics-cli-smoke.sh
tests/integration/boot-media-download-cli-smoke.sh
tests/integration/boot-media-download-fake-curl-smoke.sh
tests/integration/guest-tools-file-drop-cli-smoke.sh
tests/integration/guest-tools-shared-folder-cli-smoke.sh
tests/integration/guest-tools-clipboard-cli-smoke.sh
tests/integration/guest-tools-display-resize-cli-smoke.sh
tests/integration/guest-tools-handshake-cli-smoke.sh
tests/integration/displayd-plan-cli-smoke.sh
tests/integration/networkd-plan-cli-smoke.sh
tests/integration/guest-tools-app-window-cli-smoke.sh
tests/integration/guest-tools-time-sync-cli-smoke.sh
tests/integration/guest-tools-command-tracker-cli-smoke.sh
tests/integration/guest-tools-agent-update-cli-smoke.sh
tests/integration/guest-tools-metrics-cli-smoke.sh
tests/integration/shared-folder-manifest-cli-smoke.sh
tests/integration/performance-cli-smoke.sh
tests/integration/qemu-host-only-cli-smoke.sh
tests/integration/qmp-control-cli-smoke.sh
tests/integration/qmp-supervisor-cli-smoke.sh
tests/sleep-wake/metadata-baseline-smoke.sh
tests/integration/ssh-plan-cli-smoke.sh
tests/integration/disk-create-inspect-cli-smoke.sh
tests/integration/disk-verify-cli-smoke.sh
tests/integration/disk-compact-cli-smoke.sh
tests/integration/snapshot-active-disk-maintenance-cli-smoke.sh
tests/integration/log-viewer-cli-smoke.sh
tests/integration/application-consistent-snapshot-cli-smoke.sh
tests/integration/application-consistent-freeze-thaw-cli-smoke.sh
tests/integration/application-consistent-fsfreeze-backend-smoke.sh
tests/integration/resource-profile-readiness-smoke.sh
tests/integration/readiness-report-cli-smoke.sh
tests/integration/fast-mode-readiness-smoke.sh
tests/integration/fast-mode-readiness-unsupported-smoke.sh
tests/integration/fast-mode-readiness-template-matrix-smoke.sh
tests/integration/fast-mode-template-boot-media-smoke.sh
tests/integration/apple-vz-live-opt-in-skip-smoke.sh
tests/integration/apple-vz-live-evidence-verifier-smoke.sh
tests/integration/qemu-live-evidence-verifier-smoke.sh
tests/integration/macos-metadata-overrides-smoke.sh
tests/integration/macos-bundle-helper-verify-smoke.sh
tests/integration/macos-bundled-daemon-supervisor-smoke.sh
tests/integration/macos-settings-defaults-smoke.sh
tests/integration/macos-app-start-path-smoke.sh
tests/integration/macos-release-candidate-dry-run-smoke.sh
tests/integration/macos-release-verifier-custom-app-smoke.sh
tests/integration/macos-artifact-manifest-apple-vz-runner-smoke.sh
tests/integration/macos-debug-dmg-custom-app-name-smoke.sh
```

To run the current metadata/socket-safe subset without a real VM, real QEMU
backend, Apple VZ launch, host network mutation, real host mount freezing,
network download, or live opt-in:

```sh
tests/integration/metadata-safe-smoke-suite.sh
```

To run the local release-readiness lane that combines Rust formatting, Rust
workspace tests, macOS Swift tests, local `.app` bundle build/signature
verification including the default bundled AppleVzRunner helper, clean app
rebuild coverage, the macOS metadata override smoke, bundled AppleVzRunner
helper verification smoke, bundled `bridgevmd` supervisor smoke, local debug
DMG build/mounted-content verification, custom app-name DMG coverage that
rejects stale default app bundles, release-candidate command dry-run coverage,
release verifier custom app bundle coverage, AppleVzRunner artifact-manifest
coverage, Apple VZ and QEMU preserved live-evidence verifier coverage,
artifact manifest generation, and a
debug-vs-public-release boundary check:

```sh
tests/integration/local-release-readiness-suite.sh
```

This lane verifies local debug artifacts only. It does not replace the public
notarized release checklist or the default public-gate mode of
`packaging/macos/verify-release-candidate.sh`.

For stronger local app usability evidence without mounting or launching the
DMG, add the metadata-safe smoke suite, skip DMG packaging gates, and require
the bundled app executable to show a main window, supervise the bundled
`bridgevmd` child, and answer a daemon-backed `store doctor` request. This is
the credential-free local `.app` proof; it does not need DMG packaging,
notarization, or a Developer ID identity:

```sh
tests/integration/local-release-readiness-suite.sh --app-only --with-metadata-smokes --locally-usable-app
```

Use `--app-only` when you want the fastest local `.app` readiness pass and do
not need to build, mount, verify, or launch a DMG. It still writes an app-only
artifact manifest with bundle/helper hashes and signing diagnostics. Omit it
for full local debug packaging coverage.

For the fullest interactive packaging path, add the LaunchServices GUI window
checks for the app, mounted DMG app, and quarantined DMG app:

```sh
tests/integration/local-release-readiness-suite.sh --with-metadata-smokes --with-gui-launch
```

The suite runs `template-create-cli-smoke.sh`, `store-doctor-cli-smoke.sh`,
`mode-recommendation-cli-smoke.sh`, `windows-arm-qemu-args-cli-smoke.sh`,
`clone-cli-smoke.sh`, `metadata-repair-cli-smoke.sh`,
`manifest-migration-cli-smoke.sh`, `manifest-schema-cli-smoke.sh`,
`export-import-cli-smoke.sh`,
`delete-cli-smoke.sh`, the snapshot metadata/disk/list-restore/suspend/active
disk maintenance smokes, `displayd-plan-cli-smoke.sh`,
`networkd-plan-cli-smoke.sh`, the guest-tools MVP and application/window
smokes, the disk create/inspect/verify/compact smokes,
`fast-mode-template-boot-media-smoke.sh`,
`boot-media-download-fake-curl-smoke.sh`, the Fast Mode readiness smokes,
`fast-mode-readiness-template-matrix-smoke.sh`,
`readiness-report-cli-smoke.sh`,
`shared-folder-manifest-cli-smoke.sh`, `port-forward-cli-smoke.sh`,
`qemu-host-only-cli-smoke.sh`, the QMP control/supervisor smokes,
`ssh-plan-cli-smoke.sh`, `lifecycle-plan-cli-smoke.sh`,
`lifecycle-restart-cli-smoke.sh`, `lifecycle-suspend-resume-cli-smoke.sh`,
`application-consistent-snapshot-cli-smoke.sh`,
`application-consistent-freeze-thaw-cli-smoke.sh`,
`application-consistent-fsfreeze-backend-smoke.sh`, `log-viewer-cli-smoke.sh`,
`diagnostics-cli-smoke.sh`, `performance-cli-smoke.sh`,
`resource-profile-readiness-smoke.sh`,
`tests/sleep-wake/metadata-baseline-smoke.sh`,
`apple-vz-live-evidence-verifier-smoke.sh`,
`qemu-live-evidence-verifier-smoke.sh`, and
the Apple VZ and QEMU live opt-in skip smokes sequentially and stops at the
first failure.

The promoted clone/import/delete, disk, display, lifecycle, guest-tools, QMP,
SSH planning, diagnostics, performance, resource profile, readiness report,
application-consistent, sleep/wake metadata baseline, and log coverage stay
inside the same safety boundary.

It uses disposable stores, local Unix sockets, the Linux tools scaffold, fake
backend stubs or presence markers where daemon-owned runner metadata is
required, fake QMP/guest-tools sockets, fake/shadowed host helpers for backend
primitives such as `fsfreeze`, bounded log tails, redacted diagnostic bundles,
validate-only Apple VZ config planning, and metadata-only or bounded host-side
disk/performance artifacts. It exercises boot-media download execution only
through a shadow `curl` that copies a local fixture for a fixed
`example.invalid` URL and refuses unexpected invocations, so the metadata-safe
suite records download/result metadata without a real network fetch. It also
exercises the Apple VZ and QEMU live evidence verifiers
against synthetic evidence only. It does not start a real VM, launch real QEMU
or Apple VZ, open a graphical console or host browser/SSH client, run guest
benchmarks, freeze real host mounts, claim live guest OS state changes, mutate
live networking, or fetch network artifacts.

The macOS metadata override smoke runs in the default local release-readiness
lane. It is credential-free and builds a disposable debug app bundle with
ad-hoc signing. It sets
`BRIDGEVM_MACOS_APP_NAME`, `BRIDGEVM_BUNDLE_DISPLAY_NAME`,
`BRIDGEVM_BUNDLE_NAME`, `BRIDGEVM_BUNDLE_IDENTIFIER`,
`BRIDGEVM_BUNDLE_SHORT_VERSION`, `BRIDGEVM_BUNDLE_VERSION`, and
`BRIDGEVM_BUNDLE_COPYRIGHT`, then verifies that the generated `.app` path and
`Info.plist` metadata reflect those override values. It also supplies a
disposable `BRIDGEVM_MACOS_ICON_FILE` fixture and verifies that the icon is
copied into `Contents/Resources` and recorded as `CFBundleIconFile`.

The macOS debug app clean build smoke is credential-free. It seeds stale helper
and resource files into a disposable app bundle and verifies that the next
debug app build removes old `Contents` before installing the expected
executable, resources, and bundled AppleVzRunner helper.

The macOS bundle helper verify smoke is credential-free. It builds a disposable
debug app fixture with a bundled `Contents/Helpers/AppleVzRunner` that lacks the
required virtualization entitlement and verifies that bundle `--verify-only`
rejects it before release packaging can treat the helper as valid.

The macOS bundled daemon supervisor smoke is credential-free. It builds a
disposable debug app bundle, isolates `HOME` and `BRIDGEVM_HOME`, and launches
the bundled app executable to verify that `BundledDaemonSupervisor` starts the
bundled `Contents/Helpers/bridgevmd`. It waits for the isolated daemon socket,
checks that the app spawned the bundled helper, and sends a daemon-backed
`store doctor` request through that socket. To make that local `.app`
executable/window plus bundled socket-doctor proof explicit, without any DMG,
notarization, or Developer ID dependency, run:

```sh
BRIDGEVM_MACOS_BUNDLED_DAEMON_REQUIRE_GUI=1 \
BRIDGEVM_MACOS_BUNDLED_DAEMON_REQUIRE_WINDOW=1 \
  tests/integration/macos-bundled-daemon-supervisor-smoke.sh
```

If the GUI app launch path is not available in the current macOS session, the
smoke falls back to the strongest non-GUI coverage: starting the bundled
`bridgevmd` directly with the same helper environment that the supervisor
provides, then verifying the isolated socket and daemon response. Set
`BRIDGEVM_MACOS_BUNDLED_DAEMON_REQUIRE_GUI=1` to make the GUI supervisor path
mandatory, set `BRIDGEVM_MACOS_BUNDLED_DAEMON_REQUIRE_WINDOW=1` to also require
an app-owned main window, or
`BRIDGEVM_MACOS_BUNDLED_DAEMON_FORCE_HELPER_ONLY=1` to run only the non-GUI
helper environment path. The force-helper-only mode conflicts with required
GUI/window proof and fails fast if both are requested.

The macOS settings defaults smoke is credential-free and GUI-free. It runs the
focused `AppSettingsTests` coverage that proves persisted defaults select real
daemon mode, keep mock inventory disabled, preserve the default local socket,
and pass `BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1` to the bundled daemon supervisor
only when the Apple VZ live-start setting is enabled:

```sh
tests/integration/macos-settings-defaults-smoke.sh
```

The macOS app start path smoke is credential-free and metadata-safe. It ties
together the focused tests that prove the persisted Apple VZ live-start setting
reaches the bundled daemon supervisor environment, the dashboard primary Start
action calls the app client with `.start` only after launch readiness is ready,
and the daemon client serializes that action as `run_backend` with
`spawn=true`:

```sh
tests/integration/macos-app-start-path-smoke.sh
```

This smoke does not boot a live VM. It proves the app-to-daemon start handoff
up to the existing runner spawn request; live boot proof remains covered by the
manual Apple VZ/QEMU opt-in harnesses.

The macOS release-candidate dry-run smoke is also credential-free. It verifies
that `packaging/macos/build-release-candidate.sh --dry-run` requires release
inputs and plans the expected Developer ID signing, hardened runtime,
app and DMG notarization/stapling, artifact manifest, and public release gate
commands without running those credentialed commands.

The macOS release verifier custom app smoke builds a disposable `BridgeVM.app`
debug bundle, packages it into a disposable DMG under that same basename, and
verifies that `verify-release-candidate.sh --expect-debug-boundary` checks the
app name supplied by the caller rather than assuming `BridgeVMApp.app`.

The macOS artifact manifest AppleVzRunner smoke is credential-free. It writes a
disposable app fixture with `Contents/Helpers/AppleVzRunner` and verifies that
the artifact manifest records helper path, presence, executability, size,
SHA-256, codesign, and entitlement command sections.

Manifest schema coverage asserts that `bridgevm metadata manifest-schema`
prints the VM manifest JSON Schema v1 contract and that
`bridgevm metadata validate-manifest <path>` accepts a local valid YAML
manifest while rejecting malformed YAML, future schema versions, and empty VM
names. It reads only disposable local manifest fixtures and does not create,
launch, migrate, or mutate a VM bundle.

Readiness report coverage specifically locks the `bridgevm readiness` CLI
contract as a metadata-only preflight report. It asserts that the output says
`Metadata only: true`, `Live E2E required: true`, and keeps the live boot,
console, and guest-tools proof requirement visible even when metadata blockers
are gone. When a VM has no runner metadata yet, the report may include the
optional `pre_run_launch_readiness` object from the Rust preflight path; the CLI
renders that fallback as `Pre-run launch readiness:`. Fast Mode and
Compatibility Mode both use that section now: Compatibility Mode reports
structured QEMU launch blockers such as a missing primary disk without starting
QEMU. That section is still metadata-safe launch readiness only. It must not be
interpreted as evidence that a runner was spawned, Apple VZ or QEMU was
launched, QMP or guest tools were contacted, networking was changed, or a VM
booted.

Manual opt-in live boot coverage is intentionally separate from that default
list:

```sh
tests/integration/apple-vz-live-boot-opt-in-smoke.sh
tests/integration/qemu-live-boot-opt-in-smoke.sh
```

The default skip boundary for that live smoke is covered by
`apple-vz-live-opt-in-skip-smoke.sh` and
`qemu-live-opt-in-skip-smoke.sh`. They clear the live `BRIDGEVM_LIVE_VZ_*` or
`BRIDGEVM_LIVE_QEMU_*` inputs for the child process and verify that the live
smokes print `SKIP` and exit successfully without starting Apple VZ, QEMU, a
VM, a GUI, or a network download.

To prepare the recommended Debian arm64 netboot fixture for that smoke:

```sh
tests/integration/prepare-apple-vz-debian-fixture.sh
```

The Debian helper downloads the stable arm64 netboot `linux` and `initrd.gz`
from Debian's installer tree, creates a sparse `root.raw`, and prints the
shell-safe `BRIDGEVM_LIVE_VZ_KERNEL`, `BRIDGEVM_LIVE_VZ_INITRD`,
`BRIDGEVM_LIVE_VZ_RAW_DISK`, `BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE`, and
`BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED` exports needed by the live smoke. The Debian
`linux` file is a raw Linux kernel arm64 boot executable Image that works with
VZ LinuxBootLoader. It does not start Apple VZ by itself and does not set
`BRIDGEVM_LIVE_VZ_ALLOW_REAL_START`; keep that opt-in separate from fixture
preparation. Pass `--dry-run` to print the planned paths without downloading
fixtures. The default raw disk size is `64m`, which is enough for the bounded
netboot smoke and keeps readiness evidence hashing fast; set
`BRIDGEVM_LIVE_VZ_RAW_DISK_SIZE` when a larger disposable disk is needed. After
files exist, the helper also prints
`BRIDGEVM_LIVE_VZ_DEBIAN_KERNEL_SHA256` and
`BRIDGEVM_LIVE_VZ_DEBIAN_INITRD_SHA256`; set those values on later runs to pin
and verify the downloaded kernel/initrd bytes.

An older Alpine arm64 netboot fixture helper is also available:

```sh
tests/integration/prepare-apple-vz-alpine-fixture.sh
```

The helper downloads `vmlinuz-virt` and `initramfs-virt` from Alpine's
`latest-stable` aarch64 netboot directory by default, creates a sparse raw disk,
and prints the `BRIDGEVM_LIVE_VZ_*` assignments needed by the live smoke. Set
`BRIDGEVM_LIVE_VZ_ALPINE_FLAVOR=lts` to prepare the larger LTS flavor instead.
The default raw disk size is `64m`; set `BRIDGEVM_LIVE_VZ_RAW_DISK_SIZE` for a
larger disposable disk.
Pass `--dry-run` to print the planned paths and compatibility warning without
downloading fixtures. It does not start Apple VZ by itself and does not set
`BRIDGEVM_LIVE_VZ_ALLOW_REAL_START`; keep that opt-in separate from fixture
preparation. Alpine `vmlinuz` artifacts may be PE32+ EFI applications, which
are not accepted by VZ LinuxBootLoader; use the Debian helper when preparing a
known-good live boot fixture.

By default it prints `SKIP` and exits successfully. It only attempts a real
Apple VZ start when `BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1`,
`BRIDGEVM_LIVE_VZ_KERNEL`, `BRIDGEVM_LIVE_VZ_RAW_DISK`, and
`BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED` are set. The serial sentinel is required so
the smoke proves guest boot progress, not only successful start/stop calls.
Optional inputs include `BRIDGEVM_LIVE_VZ_INITRD`,
`BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE`, `BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS`,
`BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS`,
`BRIDGEVM_LIVE_VZ_MEMORY_MIB`, `BRIDGEVM_LIVE_VZ_CPU_COUNT`,
`BRIDGEVM_LIVE_VZ_RUNNER`,
`BRIDGEVM_LIVE_VZ_VIEWER_FRAME`, `BRIDGEVM_LIVE_VZ_VIEWER_FRAME_WIDTH`,
`BRIDGEVM_LIVE_VZ_VIEWER_FRAME_HEIGHT`, and
`BRIDGEVM_LIVE_VZ_GUEST_TOOLS_EFFECTS_JSON`. The raw disk is
copied into a temporary VM bundle before launch so disposable fixture inputs do
not have to be mutated in place. The bounded timing controls must be positive
integers, and the script requires `BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED` to appear
in the serial log after launch.

Successful live runs print the preserved temporary store, an `evidence`
directory, and `evidence/SUMMARY.txt`. The evidence directory records the input
environment, source and copied fixture sizes plus SHA-256 digests, the copied
Apple VZ launch spec, the handoff JSON consumed by the helper, the selected
`AppleVzRunner` path, validate/config-check output, live-launch command output,
and pointers to the runner and serial logs. This keeps the manual proof
auditable without changing the default no-live behavior. The configured serial
sentinel must be found before the harness records readiness as verified live
boot evidence. If a live run also captures a real viewer frame outside
the harness, provide it with `BRIDGEVM_LIVE_VZ_VIEWER_FRAME` plus positive
width and height values; the harness copies it into the evidence directory and
writes verifier-checked `viewer-evidence.json`. If separate guest-tools effect
evidence was produced from observable guest-side results, provide that JSON via
`BRIDGEVM_LIVE_VZ_GUEST_TOOLS_EFFECTS_JSON`; the harness copies it into the
bundle for the verifier rather than fabricating guest effects. When that JSON
uses artifact-based effect records, referenced artifact files are copied into
the evidence directory, SHA-256 checked, and rewritten as relative evidence
paths before verification.

Validation evidence is intentionally stricter than a successful helper exit:
`apple-vz-validate.output` must retain the `--validate-only
--print-config-plan --validate-vz-config` transcript and the config-plan markers
that show the limited Apple VZ shape was constructed and checked. Those markers
prove only the handoff/configuration boundary for `linux-kernel` + `raw` + NAT;
they do not prove a live boot, guest console progress, or guest-tools effects
unless the separate live-launch transcript and required serial sentinel evidence
come from an explicitly opted-in Apple VZ run.

The preserved `$STORE/evidence` directory from a live smoke can be checked
afterward with:

```sh
tests/integration/verify-apple-vz-live-evidence.sh "$STORE/evidence"
```

The verifier checks the summary status, fixture manifest hashes and sizes, the
launch spec and handoff artifacts, validation and live-launch output, the
selected runner path, and required serial sentinel evidence from the opted-in
live run. Validation output is expected to keep the validate-only
config-plan transcript and markers described above. The recorded
`environment.txt` is not only preserved; it is cross-checked against fixture
manifest source paths, launch-spec kernel command line and resource values, and
the selected runner path recorded in `apple-vz-runner.path`. If
`apple-vz-runner.artifact` is present, the verifier uses that relative
evidence-bundle copy of `AppleVzRunner` for executable and SHA-256 checks so the
bundle remains reviewable after the original helper path disappears. The bounded live
launch controls, `BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS` and
`BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS`, must also match the summary and
live-launch transcript. Artifact path lines in `SUMMARY.txt` are cross-checks
against the preserved
evidence bundle, not just human-readable labels: the verifier expects `Store`,
`Bundle`, `Launch spec`, `Handoff JSON`, output path, runner/serial log,
`Fixture manifest`, and `Environment` lines to resolve to the evidence fields
and artifacts they describe. The recorded
`apple-vz-runner.path` must name the selected helper; if no copied artifact is
present, that original path must still point at an existing executable. The
`apple-vz-live-launch.output` must include the expected handoff-ready,
diagnostics, start, and finished transcript markers.
`apple-vz-live-evidence-verifier-smoke.sh` covers that verifier against a
synthetic evidence bundle. The verifier and its smoke do not start a live VM,
QEMU, Apple VZ, or a GUI; the actual live proof still comes only from the
separate opt-in live smoke with real fixtures and
`BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1`.

Template list/create integration coverage should exercise:

- `bridgevm templates`
- `bridgevm create <vm> --template <template-id>`
- `bridgevm list`
- The same template listing, template-backed VM creation, and VM listing
  through `bridgevm --socket <sock> ...`
- Rejection of unknown template IDs before creating a bundle
- Rejection of duplicate VM names without mutating the existing bundle

The expected contract is that boot templates are visible through local and
daemon-backed CLI paths, template-backed creation writes the expected manifest
guest and boot metadata, and the flow remains metadata-only until an explicit
disk or run command is issued.

Current executable coverage: `template-create-cli-smoke.sh` covers local and
socket-backed template listing, Ubuntu/Fedora/Debian ARM64 and macOS restore
template VM creation, manifest boot metadata, VM list visibility,
unknown-template rejection, and duplicate name rejection.

Store Doctor integration coverage should exercise:

- `bridgevm doctor`
- `bridgevm --socket <sock> doctor`
- Store root and VM bundle directory readiness
- Host capability audit rendering for discovered QEMU, runner, and network
  helper candidates

The expected contract is that Store Doctor prepares and reports metadata store
readiness through local and daemon-backed CLI paths without launching a VM,
running QEMU, invoking Apple VZ, mutating networking, or executing discovered
host helper tools.

Current executable coverage: `store-doctor-cli-smoke.sh` covers local and
socket-backed doctor output with fake executable candidates on `PATH`, and
fails if any fake QEMU, runner, or network helper is executed instead of merely
being discovered.

Fast Mode template boot-media integration coverage should exercise:

- A Linux Arm64 VM created from `ubuntu-arm64-installer`
- `boot-media`, `media status`, `media import`, `media verify`, and
  `media download-plan` through local CLI and daemon socket paths
- Missing installer readiness blockers in `prepare-run` and metadata-only
  `run` output before local media is imported
- Missing source, missing media, checksum mismatch, and missing media-kind
  errors that do not print launch or spawn claims
- Download-plan metadata using an inert URL without executing a download

The expected contract is that template-created Fast Mode VMs report structured
boot-media paths and readiness blockers while installer media is absent,
importing a local file makes the boot-media entry ready without creating a disk,
verification and download planning remain metadata-only, and failure paths do
not imply that BridgeVM launched QEMU, Apple VZ, or a VM.

Current executable coverage: `fast-mode-template-boot-media-smoke.sh` covers
the local and socket-backed halves of this template boot-media contract without
starting QEMU, Apple VZ, or fetching network artifacts.

`port-forward-cli-smoke.sh` creates a disposable Compatibility Mode VM store
under `/tmp` so the daemon's Unix socket path stays below platform length
limits. It adds and removes a NAT port forward through the local CLI, verifies
that the VM manifest records the forward, checks that `qemu-args` renders or
removes the matching `hostfwd` argument, verifies metadata-only
`bridgevm open <vm> <guest-port> --scheme <scheme>` planning output, then
starts `bridgevmd` against the same store and repeats the
list/add/open/remove/`qemu-args` flow through the socket API. The smoke prepends
a fake `open` executable to `PATH` and fails if it is invoked, so the coverage
checks the browser command boundary without opening a browser or network
target. Both local and socket paths also assert that missing forwarded-port
plans fail without invoking that fake `open` command.

Port forwarding integration coverage should exercise both the local CLI and
socket API paths for:

- `bridgevm port list <vm>`
- `bridgevm port add <vm> <host:guest>`
- `bridgevm open <vm> <guest-port> --scheme <scheme>`
- `bridgevm port remove <vm> <host:guest>`

The expected contract is that add/remove update manifest `network.forwards`,
Compatibility Mode QEMU planning renders those entries as `hostfwd`, open-port
planning prints the selected guest port, host port, host, URL, scheme, and host
`open` command without executing that command, and the changed manifest affects
subsequent runner plans and spawns rather than an already-running backend.

Current executable coverage: `port-forward-cli-smoke.sh` covers the local CLI
and socket-backed halves of this contract, including lowest-host selection when
multiple forwards target the requested guest port and invalid open-plan inputs.

QEMU host-only networking integration coverage should exercise:

- A Compatibility Mode VM whose manifest has `network.mode: host-only`
- `bridgevm qemu-args <vm>`
- `bridgevm --socket <sock> qemu-args <vm>`
- `bridgevm port add <vm> <host:guest>`
- `bridgevm --socket <sock> port add <vm> <host:guest>`

The expected contract is that QEMU planning renders a host-only netdev for both
the local CLI and daemon socket paths, does not render NAT `hostfwd` entries,
and continues to reject port forwarding outside NAT before rewriting the
manifest.

Current executable coverage: `qemu-host-only-cli-smoke.sh` covers local and
socket-backed host-only `qemu-args` planning plus local and socket-backed
port-forward rejection.

QMP supervisor integration coverage should exercise:

- `bridgevm --socket <sock> run <vm> --spawn` with a fake QEMU backend
- Daemon-owned child supervision without starting a real VM
- Supervisor QMP negotiation against a fake Unix socket
- `metadata/qmp-supervisor.json` recording drained QMP events
- Status, runner-status, or readiness diagnostics reusing that latest metadata
  without opening a new QMP connection
- Terminal QMP event cleanup that clears runner metadata and marks the VM stopped

The expected contract is that the daemon can supervise a spawned Compatibility
Mode backend through QMP using only disposable local metadata and fake sockets.
The recorded supervisor data is a metadata-safe diagnostic cache of the latest
bounded drain. It is useful for explaining recent daemon-observed QMP events,
but it is not a live console proof, a guest boot transcript, or an append-only
event log. The smoke must not launch real QEMU, boot a guest, or touch
networking.

Current executable coverage: `qmp-supervisor-cli-smoke.sh` covers the
socket-backed spawn path with a fake `qemu-system-x86_64`, validates the
recorded `RESUME` and terminal `SHUTDOWN` events, verifies that CLI diagnostic
surfaces expose the cached supervisor metadata after cleanup, and verifies
daemon cleanup after the terminal event.

Windows 11 Arm restricted QEMU planning coverage should exercise:

- A Compatibility Mode VM with `guest.os: windows`, `guest.version: 11`, and
  `guest.arch: arm64`
- `bridgevm qemu-args <vm>`
- `bridgevm --socket <sock> qemu-args <vm>`
- Default display renderer mapping from manifest `spice` to the restricted
  Apple Silicon-friendly `cocoa,gl=on` QEMU display argument
- Preservation of an explicit VNC display renderer as the external-viewer
  handoff (`-display vnc=:0`)

The expected contract is that Windows 11 Arm still travels through the
restricted Compatibility Mode backend until a dedicated Windows Fast Mode
exists. QEMU planning must select `qemu-system-aarch64`, `-machine virt`,
`-accel hvf`, `-cpu host`, and the restricted-profile `virtio-rng-pci` device
without spawning QEMU or claiming that Windows booted.

Current executable coverage: `windows-arm-qemu-args-cli-smoke.sh` covers local
and socket-backed Windows 11 Arm restricted `qemu-args` planning, including
default display remapping, explicit VNC external-viewer handoff preservation,
and fake backend launch guards for QEMU and Apple VZ binaries.

Networkd public CLI plan coverage should exercise:

- `cargo run -p networkd -- --print-plan --backend qemu --mode nat --forward <host:guest>`
- `cargo run -p networkd -- --print-plan --backend apple-vz --mode host-only`
- `cargo run -p networkd -- --print-plan --backend qemu --mode isolated`
- `cargo run -p networkd -- --print-plan --backend qemu --mode bridged`
- Human-readable summary output when `--print-plan` is omitted
- Rejection for malformed or zero-valued `--forward` inputs
- Rejection for duplicate host ports
- Rejection for port forwards outside NAT, including isolated networking
- Rejection for Apple VZ bridged networking until backend support exists

The expected contract is that `networkd` exposes the shared network planner as
a public metadata-only runner surface. JSON output should include backend,
mode, hostname, validated port-forward rules, capability flags, requirements,
and notes for higher-level runners and smoke tests. Summary output should
remain concise for operator-facing readiness checks. This smoke must not start
a VM, start QEMU, launch Apple VZ, create a host-only interface, attach a
bridge, or modify live networking; it validates planning metadata and rejection
paths only.

Current executable coverage: `networkd-plan-cli-smoke.sh` covers public
`cargo run -p networkd` invocations for QEMU NAT forwards, Apple VZ host-only
planning, QEMU isolated planning, QEMU bridged blocker metadata, summary
output, malformed, zero-valued, and duplicate host-port forward rejection,
isolated-plus-forward rejection, and Apple VZ bridged rejection without
starting QEMU, Apple VZ, or a VM.

QMP control integration coverage should exercise:

- `bridgevm qmp-status <vm>`
- `bridgevm qmp-stop <vm>`
- `bridgevm qmp-cont <vm>`
- `fullvm-runner <vm> --qmp-status`
- The same commands through `bridgevm --socket <sock>`
- QMP negotiation with `qmp_capabilities` before `query-status`, `stop`, or
  `cont`
- Status parsing from fake QMP `query-status` responses on local and
  socket-backed paths
- Async QMP events received before a command return, so command callers still
  consume the matching return value
- Explicit failure when the QMP socket is unavailable

The expected contract is a backend command boundary only: tests use a fake QMP
Unix socket and must not start QEMU. `qmp-status` maps to QEMU's
`query-status` command and reports the fake QMP status response; the
`fullvm-runner --qmp-status` diagnostic consumes that same boundary without
spawning QEMU. `qmp-stop` maps to QEMU's `stop` command and `qmp-cont` maps to
QEMU's `cont` command. These do not yet serialize or restore suspend images.

Current executable coverage: `qmp-control-cli-smoke.sh` covers local and
socket-backed `qmp-status`/`qmp-stop`/`qmp-cont` against a fake QMP server,
including `query-status` responses, an async event before the status command
return, `fullvm-runner --qmp-status`, and status/control missing-socket
reporting.

SSH plan integration coverage should exercise:

- `bridgevm ssh <vm> --user <user>`
- `bridgevm --socket <sock> ssh <vm> --user <user>`
- Compatibility Mode port-forward preference for guest port 22
- Connected guest-tools runtime IP fallback when no SSH port forward exists
- Rejection when no reachable target metadata is available

The expected contract is that `bridgevm ssh` is metadata-only: it prints the
SSH command BridgeVM would use without executing `ssh` or starting a backend.
Compatibility Mode should prefer the lowest manifest forward to guest port 22;
otherwise a connected guest-tools runtime with a valid non-loopback guest IP can
produce a direct guest IP command.

Current executable coverage: `ssh-plan-cli-smoke.sh` covers local and
socket-backed SSH planning, missing-target errors, port-forward preference, and
guest-tools IP fallback from a disposable Compatibility Mode VM bundle. It also
checks lowest-host SSH forward selection and empty-user rejection. The smoke
prepends a fake `ssh` executable to `PATH` and fails if any local or
socket-backed SSH plan or missing-target error invokes it.

Primary disk create/inspect integration coverage should exercise:

- `cargo run -p bridgevm-cli -- disk prepare <vm>`
- `cargo run -p bridgevm-cli -- disk create <vm>`
- `cargo run -p bridgevm-cli -- disk inspect <vm>`
- The same prepare/create/inspect operations through
  `bridgevm --socket <sock> ...`
- The `qemu-img create -f qcow2` and `qemu-img info --output=json` execution
  boundaries for non-raw primary disks
- Raw primary disk preparation without invoking `qemu-img`
- Recording of `metadata/primary-disk.json`,
  `metadata/last-disk-create.json`, and `metadata/last-disk-inspect.json`

The expected contract is that primary disk preparation records the intended
disk boundary without creating qcow2 files, creation crosses only the explicit
qemu-img boundary, inspection reads an existing primary disk without rewriting
it, and raw disks are prepared directly without qemu-img. Failures must not
invent success metadata.

Current executable coverage: `disk-create-inspect-cli-smoke.sh` covers local
and socket-backed prepare/create/inspect success paths, raw preparation and
create skip behavior, missing-disk inspect rejection, fake qemu-img create
failure handling, and fake qemu-img info failure handling.

Disk compaction integration coverage should exercise:

- `cargo run -p bridgevm-cli -- disk compact <vm>`
- `bridgevm --socket <sock> disk compact <vm>`
- The `qemu-img convert` execution boundary for an existing non-raw active disk
- Replacement of the active disk while keeping a `.precompact-<timestamp>`
  backup
- Recording of `metadata/last-disk-compact.json`

The expected contract is that compaction is explicit, preserves the previous
active disk as a timestamped backup before replacing it, and reports failures
without silently rewriting disk state when the disk or `qemu-img convert`
boundary is unavailable.

Current executable coverage: `disk-compact-cli-smoke.sh` covers the local CLI
and socket-backed halves of this contract for primary active disks, plus
missing active disk rejection and fake `qemu-img convert` failure handling
without writing success metadata.
`snapshot-active-disk-maintenance-cli-smoke.sh` covers the same compaction
boundary after snapshot disk creation has selected a snapshot overlay as the
active disk, proving compaction follows `metadata/active-disk.json` rather than
falling back to the manifest primary disk.

Disk verification integration coverage should exercise:

- `cargo run -p bridgevm-cli -- disk verify <vm>`
- `bridgevm --socket <sock> disk verify <vm>`
- The `qemu-img check --output=json` execution boundary for an existing
  non-raw active disk
- Recording of `metadata/last-disk-verify.json`
- Reporting the active disk path and parsed JSON check report

The expected contract is that verification is explicit, reads the active disk
without rewriting it, records the qemu-img check report, and reports failures
without silently changing disk or snapshot-chain state.

Current executable coverage: `disk-verify-cli-smoke.sh` covers the local CLI
and socket-backed success paths plus raw-disk rejection, missing active disk
rejection, and fake `qemu-img check` failure handling for primary active disks.
`snapshot-active-disk-maintenance-cli-smoke.sh` covers verification after
snapshot disk creation has selected a snapshot overlay as the active disk,
proving verification follows `metadata/active-disk.json` rather than falling
back to the manifest primary disk.

Snapshot list/restore integration coverage should exercise:

- `bridgevm snapshot create <vm> <snapshot> --kind disk`
- `bridgevm --socket <sock> snapshot create <vm> <snapshot> --kind disk`
- `bridgevm snapshot disk-create <vm> <snapshot>`
- `bridgevm --socket <sock> snapshot disk-create <vm> <snapshot>`
- `bridgevm snapshot list <vm>`
- `bridgevm --socket <sock> snapshot list <vm>`
- `bridgevm snapshot chain <vm>`
- `bridgevm --socket <sock> snapshot chain <vm>`
- `bridgevm snapshot restore <vm> <snapshot>`
- `bridgevm --socket <sock> snapshot restore <vm> <snapshot>`
- Listing snapshot name, kind, captured VM state, and creation timestamp
- Reporting dashboard-facing snapshot chain metadata: active disk source,
  active snapshot name when present, selected disk path, overlay readiness, and
  backing readiness
- Creating the explicit qcow2 snapshot overlay through the
  `qemu-img create -f qcow2 -F <format> -b <backing> <overlay>` execution
  boundary without launching a VM backend
- Recording `metadata/snapshot-disks/<snapshot>-create.json` and switching
  `metadata/active-disk.json` to the snapshot overlay
- Keeping metadata-only snapshot create/list paths from executing `qemu-img`,
  launching QEMU, launching Apple VZ, writing disk-create execution metadata, or
  creating primary/overlay disk files
- Restoring disk snapshot metadata to the recorded backing disk boundary
- Recording `metadata/last-restore.json` with restored state and active disk
  source metadata

The expected contract is metadata-only: restore switches BridgeVM's active disk
and runtime-state metadata to the snapshot boundary, but it does not boot,
rewind, or execute a guest. The macOS dashboard may surface the same daemon
metadata, but must keep the restore action framed as this metadata boundary.

Current executable coverage: `snapshot-metadata-create-list-cli-smoke.sh`
covers local and socket-backed disk snapshot metadata creation/listing,
snapshot-chain metadata, duplicate snapshot rejection, absent primary/overlay
disk files, absent disk-create/restore metadata, and fake `qemu-img`/backend
launch guards. `snapshot-disk-create-cli-smoke.sh` covers local and
socket-backed fake `qemu-img` snapshot overlay creation, disk-create output,
create metadata, active-disk metadata, snapshot-chain status, and backend launch
guards. `snapshot-list-restore-cli-smoke.sh` covers local and socket-backed disk
snapshot list output, snapshot-chain metadata before and after overlay creation,
active-disk metadata, restore output, restored status, and last-restore
metadata, plus missing-snapshot restore rejection on both paths.

Suspend snapshot integration coverage should exercise:

- `bridgevm snapshot create <vm> <snapshot> --kind suspend`
- `bridgevm --socket <sock> snapshot create <vm> <snapshot> --kind suspend`
- Recording `metadata/suspend-images/<snapshot>.json`
- Reporting the planned `suspend-images/<snapshot>.bin` image path, format,
  readiness, and preparation timestamp when restoring
- Rejecting restore while the planned suspend image file is absent
- Recording restore metadata once the fake planned image marker exists,
  including `suspend_image` in `metadata/last-restore.json`

The expected contract is still metadata-only: BridgeVM records and verifies the
planned suspend image marker, but it does not serialize, deserialize, or restore
guest memory yet. Tests should use fake image files and must not boot a VM or
claim real suspend/resume performance.

Current executable coverage: `suspend-snapshot-cli-smoke.sh` covers local and
socket-backed suspend snapshot metadata creation, missing-image restore
rejection, fake-image restore success, CLI status output, and last-restore
metadata, with negative assertions that snapshot creation does not write the
planned suspend image, missing-image restore does not write last-restore
metadata, and neither path launches a backend.

Application-consistent snapshot integration coverage should exercise:

- `bridgevm snapshot create <vm> <snapshot> --kind application-consistent`
- `bridgevm --socket <sock> snapshot create <vm> <snapshot> --kind application-consistent`
- Recording `metadata/application-consistent-snapshots/<snapshot>.json`
- Reporting guest-tools connection state, required capabilities, missing
  capabilities, and planned freeze/thaw semantics from the preflight record
- Reporting that local execution requires daemon socket access
- Exercising the daemon-owned `snapshot execute-application-consistent` scaffold
  with a live fake guest-tools harness that can wait for correlated command
  results
- Exercising the Linux tools `--real-fsfreeze` opt-in path with a fake
  `fsfreeze` backend shadowed on `PATH`, including allowlisted mount ordering
  and thaw rollback after failure

The expected contract is that this snapshot kind is an honest preflight
metadata and execution scaffold/boundary today. `snapshot create` does not
freeze or thaw the guest. The daemon-owned execution path may send correlated
freeze/thaw protocol commands, and `bridgevm-tools-linux` defaults to simulated
freeze/thaw acknowledgements. Linux `fsfreeze` dispatch is available only when the
guest tools process is explicitly started with `--real-fsfreeze` and one or
more `--fsfreeze-mount <path>` allowlist entries. In the metadata-safe suite
that dispatch is exercised only through a fake executable shadowed on `PATH`;
it must not freeze real host mounts. Even against a real guest, that mode
should still avoid claiming application consistency: it does not flush
databases, quiesce applications, or coordinate app writes, and it may require
root or `CAP_SYS_ADMIN` or fail on unsupported filesystems. The capability
names are `fs-freeze` and `fs-thaw`; socket-level protocol scaffolds may
acknowledge matching freeze/thaw messages.

Current executable coverage: `application-consistent-snapshot-cli-smoke.sh`
covers the local CLI and socket-backed preflight halves of this contract,
duplicate snapshot rejection, partial connected-runtime capability metadata,
and the local CLI guard that requires daemon socket access for execution.
`application-consistent-freeze-thaw-cli-smoke.sh` covers the daemon-owned live
guest-tools socket scaffold with request-correlated freeze and thaw
`CommandResult` frames around snapshot metadata creation, and asserts that the
default Linux tools path reports a simulated scaffold boundary where no OS
`fsfreeze` was executed.
`application-consistent-fsfreeze-backend-smoke.sh` covers only the
`bridgevm-tools-linux --real-fsfreeze` command path through a fake `fsfreeze`
binary shadowed on `PATH`, asserting allowlisted temporary-directory order,
reverse thaw order, and rollback thaw after a partial freeze failure without
freezing host mounts, requiring privileges, or proving application consistency.

Freeze/thaw protocol smoke coverage reuses the existing live guest-tools socket
pattern from `guest-tools-file-drop-cli-smoke.sh` and
`guest-tools-shared-folder-cli-smoke.sh`. The current smoke starts a real
`bridgevm-tools-linux --socket` session advertising `fs-freeze` and `fs-thaw`,
drives `snapshot execute-application-consistent` through the daemon-owned
snapshot command path, and asserts request-correlated freeze and thaw
`CommandResult` frames. A failure case where the disk snapshot step fails but
thaw is still attempted and recorded remains covered by daemon unit tests.
`fsfreeze` command-path coverage should prefer a fake backend for call order
and rollback assertions. The current fake backend smoke shadows the `fsfreeze`
command, verifies that fake command is first on `PATH`, and uses ordinary
temporary directories inside its disposable store only; any live mount test
must be separately opt-in and document its privilege and filesystem
requirements.

VM bundle export/import integration coverage should exercise:

- `bridgevm export <vm> --output <bundle>`
- `bridgevm import <bundle> [--name <name>]`
- The same import operation through `bridgevm --socket <sock> import ...`
- Directory bundle paths such as `<name>.vmbridge`
- Archive bundle paths ending in `.tar`

The expected contract is that export preserves the portable VM bundle, including
manifest and metadata, either as a `.vmbridge` directory bundle or as a `.tar`
archive when the output path ends with `.tar`. Import should accept the matching
directory or tar archive input, copy the bundle into a fresh store, optionally
rewrite the VM identity and hostname, preserve snapshot and manifest-managed
network/share metadata, reject duplicate destination names, and write import
metadata. Export/import must exclude transient live artifacts such as socket
and lock files from directory bundles, tar archives, and imported copies.

The macOS dashboard export/import surface should be tested against this same
daemon/socket contract when UI coverage exists. It should present the copied
bundle metadata and file-copy result for directory and tar bundles, while
preserving manifest metadata such as snapshots, port forwards, and shared
folders. The expected UI/API boundary remains copy-only: no VM boot, QMP
connection, guest-tools attachment, live socket copy, or live guest state
migration should be implied.

Current executable coverage: `export-import-cli-smoke.sh` covers local export,
local import into a fresh store with rename, duplicate import rejection, and
malformed tar import rejection without creating a bundle. It also verifies
socket-backed import into another fresh store using both the `.vmbridge`
directory bundle path and a `.tar` archive path, socket-backed duplicate import
rejection, and direct preservation of manifest/import/export/snapshot metadata,
port forwards, and shared-folder tokens. It seeds `.sock` and `.lock` files in
the source bundle and verifies that they are absent from exported directories,
exported tar archives, and local/socket imported bundles.

VM clone integration coverage should exercise:

- `bridgevm clone <vm> <new-name>`
- `bridgevm clone <vm> <new-name> --linked`
- The same clone operation through `bridgevm --socket <sock> clone ...`
- Manifest identity and hostname rewriting for the cloned VM
- Preservation of snapshot, port-forward, and shared-folder metadata
- Linked clone overlay creation through the `qemu-img create -f qcow2 -F
  <format> -b <source-active-disk> <destination-overlay>` execution boundary
- Duplicate destination rejection
- Linked clone rejection when the source active disk is missing, without
  creating linked-clone success metadata or an overlay disk

The expected contract is that clone creates a new local VM bundle from an
existing bundle without launching a backend, rewrites the manifest identity for
the destination name, writes `metadata/clone.json`, and preserves portable
manifest/metadata state needed by subsequent lifecycle, networking, sharing, and
snapshot commands. A linked clone creates a fresh destination `disks/root.qcow2`
overlay backed by the source VM's active disk, records the backing path and
creation command in `metadata/clone.json`, and starts with an empty snapshot
list so copied snapshot disk metadata cannot point at stale destination state.

Current executable coverage: `clone-cli-smoke.sh` covers local full clone,
local linked clone, duplicate destination rejection, local linked-clone
missing-active-disk rejection, socket-backed full clone, socket-backed linked
clone, and socket-backed linked-clone missing-active-disk rejection from
disposable Compatibility Mode VM bundles.

Metadata-only delete integration coverage should exercise:

- `bridgevm delete <vm> --metadata-only`
- The same operation through `bridgevm --socket <sock> delete <vm> --metadata-only`
- Refusal to delete a running VM, including metadata-only delete
- Rejection of duplicate metadata-only delete after tombstoning
- Tombstone metadata creation under the preserved VM bundle
- Deleted VM filtering from `bridgevm list`

The expected contract is a dashboard-safe delete boundary. Metadata-only delete
does not remove the `.vmbridge` bundle or `manifest.yaml`; it writes
`metadata/deletion.json` plus `metadata/deleted-manifest.yaml` so future repair,
audit, export, or recovery workflows can identify the deleted VM without
silently destroying disk/media artifacts. Local and socket-backed lists hide
tombstoned VMs. Running VMs are refused before tombstone metadata is written,
and their manifests remain in place. A VM that has already been tombstoned is
treated as not found on repeat metadata-only delete, while the preserved bundle,
manifest, and tombstone artifacts remain in place. Destructive bundle removal
remains a separate explicit CLI/storage boundary and must not be used by
dashboard delete controls.

Current executable coverage: `delete-cli-smoke.sh` covers local and
socket-backed metadata-only delete, list filtering, tombstone artifacts,
preserved bundles/manifests, duplicate tombstone rejection, and running-VM
refusal without starting QEMU, Apple VZ, or a real guest.

Metadata repair integration coverage should exercise:

- `bridgevm metadata repair <vm>`
- The same repair operation through `bridgevm --socket <sock> metadata repair ...`
- The daemon/API path that handles `BridgeVmRequest::RepairMetadata`
- Missing core metadata repair for runtime state, active disk, guest-tools token,
  and primary disk preparation metadata
- Missing disk snapshot, suspend snapshot, and application-consistent snapshot
  metadata repair
- Idempotent no-op behavior after metadata is healthy

The expected contract is that repair is conservative and metadata-only. Missing
repairable metadata can be recreated from the manifest and snapshot list, and
existence flags can be refreshed, but VM disks, runner history, and operation
receipts are not invented. Repair must not create disks or replace corrupt JSON
that cannot be parsed safely.

The macOS dashboard repair surface invokes the daemon `repair_metadata` action
and presents the same metadata-only result: repaired vs no-op state, repair
actions, timestamp, and bundle path. It does not imply disk creation, JSON
replacement, or guest/backend execution.

Current executable coverage: `metadata-repair-cli-smoke.sh` covers local and
socket-backed metadata repair plus idempotent no-op repair from disposable
Compatibility Mode VM bundles.

Manifest migration integration coverage should exercise:

- `bridgevm metadata migrate-manifest <vm> --dry-run`
- `bridgevm metadata migrate-manifest <vm>`
- The same dry-run and execution paths through
  `bridgevm --socket <sock> metadata migrate-manifest ...`
- Current-schema no-op migration as the first stable migration boundary
- Backup and receipt metadata under `metadata/manifest-before-migration.yaml`
  and `metadata/manifest-migration.json`
- Rejection of unsupported future schemas before rewriting or writing receipts
- Rejection of malformed YAML before writing receipts
- Post-migration readability by `list`, `status`, `qemu-args`, export, and
  import without starting QEMU, Apple VZ, a VM, or guest tools

The expected contract is conservative and metadata-only. Until an older
manifest schema exists, migration validates the current `bridgevm.io/v1`
manifest, records a no-op receipt on execution, and leaves dry-runs read-only.
Future-schema and malformed manifests fail without backup or receipt output, so
schema-upgrade tooling does not silently rewrite unknown input.

Current executable coverage: `manifest-migration-cli-smoke.sh` covers local and
socket-backed dry-run/current-schema no-op migration, manifest backup/receipt
metadata, future-schema rejection, malformed YAML rejection, and migrated VM
readability through list/status/QEMU planning/export/import.

Lifecycle restart integration coverage should exercise:

- `bridgevm restart <vm>` after a running VM metadata state
- `bridgevm restart <vm>` after a suspended VM metadata state
- The same restart operation through `bridgevm --socket <sock> restart <vm>`

The expected contract is Phase-0 lifecycle metadata control: restart first uses
the same safe stop path as `bridgevm stop`, clears runner metadata, then returns
the VM to the running state. It does not claim a real guest reboot unless a
future backend-specific restart path starts a real backend.

Current executable coverage: `lifecycle-restart-cli-smoke.sh` covers local
running and suspended restarts, socket-backed running and suspended restarts,
status reporting after each restart path, and backend launch guards.

Lifecycle suspend/resume metadata integration coverage should exercise:

- `bridgevm suspend <vm>` after a running VM metadata state
- `bridgevm resume <vm>` after a suspended VM metadata state
- Rejection of `bridgevm suspend <vm>` after a stopped VM metadata state
- `bridgevm stop <vm>` after a suspended VM metadata state
- The same suspend and resume operations through
  `bridgevm --socket <sock> suspend <vm>` and
  `bridgevm --socket <sock> resume <vm>`

The expected contract is Phase-0 lifecycle metadata control: suspend records a
suspended runtime state without starting a backend, resume records running state
metadata, stopped VMs cannot be suspended, and suspended VMs can still be
stopped through the safe metadata stop path.

Current executable coverage: `lifecycle-suspend-resume-cli-smoke.sh` covers
local and socket-backed suspend/resume metadata transitions, stopped-to-suspend
rejection, suspended-to-stopped lifecycle cleanup, and backend launch guards.

Lifecycle plan integration coverage should exercise:

- `bridgevm lifecycle-plan <vm> --action suspend`
- `bridgevm lifecycle-plan <vm> --action resume`
- `bridgevm --socket <sock> lifecycle-plan <vm> --action suspend|resume`
- Compatibility Mode mapping to QMP `stop`/`cont` without connecting to QMP
- Fast Mode reporting the Apple VZ suspend/resume backend as not wired yet

The expected contract is metadata-only planning for UI/API command readiness.
The plan reports current and target lifecycle states, the backend boundary, the
planned QMP command for Compatibility Mode, QMP socket path availability, and
blockers. Tests may create a marker at the QMP socket path to exercise readiness
state, but must not start QEMU, Apple VZ, perform a QMP handshake, or claim that
real guest suspend/resume occurred. This is the contract the macOS lifecycle
controls should display.

Current executable coverage: `lifecycle-plan-cli-smoke.sh` covers local and
socket-backed suspend/resume plan output, missing and present QMP socket markers,
negative assertions that plan inspection does not create socket markers,
and Fast Mode unsupported-backend blockers.

Diagnostics bundle integration coverage should exercise:

- `bridgevm diagnostics bundle <vm> --output <dir>`
- `bridgevm --socket <sock> diagnostics bundle <vm> --output <dir>`
- Dashboard-facing summary metadata such as output path, source bundle,
  creation time, and copied file list

The expected contract is that the output includes `manifest.yaml`, `logs/`,
`metadata/`, and `diagnostic-bundle.json`; excludes disks, installer or restore
media, sockets, and lock files; and redacts the guest-tools token plus sensitive
JSON keys and URL query strings before writing bundled JSON. Dashboard cards or
panels should display that copied-file metadata only; they must not open live
backend endpoints or imply guest inspection.

Current executable coverage: `diagnostics-cli-smoke.sh` covers the local CLI
and socket-backed halves of this contract, copied-file summary metadata,
redaction/exclusion boundaries, and missing-VM failure handling.

Boot media download execution coverage should exercise:

- `bridgevm media download-plan <vm> --url <url> --sha256 <digest>`
- `bridgevm media download <vm>`
- `bridgevm media status <vm>`
- The same planning, download execution, and status flow through
  `bridgevm --socket <sock>`

The expected contract is that download planning only records intent, while
download execution is explicit: it fetches the planned URL to the resolved boot
media path, verifies the planned SHA-256 before replacing the destination,
records `metadata/boot-media/<kind>-download-result.json`, removes the
temporary download file on success, and updates status with the latest download
result. This smoke serves a tiny fixture over loopback HTTP, so it exercises
the curl execution boundary without depending on external network access or
starting a VM.

Current executable coverage: `boot-media-download-cli-smoke.sh` covers the
local CLI and socket-backed halves of this contract for a Fast Mode Linux
installer image, plus a local CLI checksum-mismatch path that verifies an
existing destination is preserved while failed download metadata and status are
recorded.

Log viewer integration coverage should exercise:

- `bridgevm logs qemu <vm> --bytes <n>`
- `bridgevm logs serial <vm> --bytes <n>`
- `bridgevm --socket <sock> logs qemu <vm> --bytes <n>`
- `bridgevm --socket <sock> logs serial <vm> --bytes <n>`

The expected contract is that log viewing reads bounded tails from
`logs/qemu.log` or `logs/serial.log`, reports missing log files without
starting a backend, preserves the path/byte/truncation metadata, and works
through the daemon socket with the same shape used by the macOS dashboard.

Current executable coverage: `log-viewer-cli-smoke.sh` covers local QEMU and
serial log tailing, socket-backed QEMU and serial log tailing, local and
socket-backed missing log-file metadata, and local and socket-backed missing-VM
failure from disposable VM bundles. It also verifies the full log byte count,
returned byte count, truncation flag, tail marker, and absence of older fixture
lines so runner and serial log views stay bounded to the requested tail.

Guest-tools handshake smoke coverage should exercise:

- Local CLI guest-tools status from a VM bundle with a guest-tools token and
  socket metadata
- Socket-backed `guest-tools status` exposing the same connected token,
  capability, and socket metadata
- Socket-backed `guest-tools linux-command <vm> accept-hello` for a valid
  `GuestHello`
- Wrong-token rejection before accepting the hello
- Disallowed-capability rejection before accepting the hello

The expected contract is that guest-tools handshake validation can be exercised
through local and daemon socket paths with a local socket fixture only. The
smoke must not start a real VM, start QEMU, launch Apple VZ, or run a
`bridgevm-tools-linux` process.

Current executable coverage: `guest-tools-handshake-cli-smoke.sh` covers
local/socket guest-tools token and socket metadata reporting, `linux-command`
generation for the MVP policy capability set, `accept-hello` success across
clipboard, display-resize, shared-folders, metrics, agent-update, and
time-sync capabilities, wrong-token rejection, and disallowed-capability
rejection without starting QEMU, Apple VZ, a VM, or Linux tools. File-drop
protocol coverage remains in `guest-tools-file-drop-cli-smoke.sh` because the
default disposable VM manifest does not currently expose `drag-drop`.

Linux guest-tools file-drop live socket coverage should exercise:

- A real `bridgevm-tools-linux --socket <sock> --file-drop-dir <dir>` process
- `FileDropStart`, one or more `FileDropChunk` frames, and `FileDropComplete`
- Successful payload materialization in the configured drop directory
- Rejection of unsafe file names and declared-size mismatches

Current executable coverage: `guest-tools-file-drop-cli-smoke.sh` covers a
live Unix socket session against the Linux tools scaffold and verifies the
normal write path plus unsafe-name and size-mismatch failures.

Linux guest-tools shared-folder live socket coverage should exercise:

- A real `bridgevm-tools-linux --socket <sock>` process
- `MountShare` and `UnmountShare` command frames
- Successful mount/unmount acknowledgements
- Rejection of an unmount request for a share that is not mounted

Current executable coverage: `guest-tools-shared-folder-cli-smoke.sh` covers a
live Unix socket session against the Linux tools scaffold and verifies normal
shared-folder command acknowledgements plus a missing-share failure.

Linux guest-tools clipboard live socket coverage should exercise:

- A real `bridgevm-tools-linux --socket <sock> --clipboard-text <text>`
  process
- Guest-origin scaffold `ClipboardChanged` telemetry after `GuestHello`
- Host-origin `SetClipboard` command frames
- Request-correlated `CommandResult` metadata for successful clipboard command
  dispatch
- Request-correlated failure metadata when the opt-in clipboard command backend
  rejects a payload

The expected contract is that clipboard commands travel through the
Unix socket protocol and Linux tools scaffold without starting a real VM.
Successful results prove only that the alpha clipboard command plumbing
accepted and processed scaffold payloads; they do not claim that a real guest
OS clipboard was read or written.

Current executable coverage: `guest-tools-clipboard-cli-smoke.sh` covers a
live Unix socket session against the Linux tools scaffold, then verifies
guest-origin clipboard telemetry, host-origin clipboard command dispatch, and
request-correlated success and failure acknowledgements from the opt-in
clipboard command backend.

Linux guest-tools display-resize live socket coverage should exercise:

- A real `bridgevm-tools-linux --socket <sock> --display-resize-command <path>`
  process
- Host-origin `ResizeDisplay` command frames
- Request-correlated `CommandResult` metadata for successful display resize
  command dispatch and opt-in backend failures
- The opt-in backend receiving width, height, and scale argv values

The expected contract is that display resize commands travel through the Unix
socket protocol and Linux tools scaffold without starting a real VM. Successful
results prove only that the alpha dynamic-resolution command plumbing accepted
and processed scaffold payloads; they do not claim that a real guest display
mode was changed.

Current executable coverage: `guest-tools-display-resize-cli-smoke.sh` covers a
live Unix socket session against the Linux tools scaffold, then verifies
host-origin display resize command dispatch, request-correlated
acknowledgement, opt-in backend argv propagation, and
`display-resize-failed` metadata when the backend rejects a resize request.

Displayd public CLI plan coverage should exercise:

- `cargo run -p displayd -- --print-plan`
- Foreground, background, and hidden visibility frame-pacing policy
- Display pipeline metadata
- Dynamic resize event JSON and Retina backing-size calculation
- Cursor movement JSON with host-overlay state and framebuffer clamping
- Dirty-region update strategy and full-frame fallback metadata
- File-backed frame timing samples through `--frame-sample-file`
- Human-readable summary output when `--print-plan` is omitted

The expected contract is that `displayd` exposes a metadata-only public runner
surface for Fast Mode display planning without starting QEMU, Apple VZ, Metal,
or VNC. JSON output should remain suitable for higher-level planning and smoke
tests; summary output should remain concise for operator-facing readiness
checks.

Current executable coverage: `displayd-plan-cli-smoke.sh` covers public
`cargo run -p displayd` invocations for JSON plan output, visibility pacing,
display pipeline metadata, resize/cursor event serialization, cursor clamping,
dirty-region update and full-frame fallback metadata, Metal/VNC boundary
metadata, file-backed frame timing samples, invalid sample rejection, and
non-JSON summary output.

Linux guest-tools application/window live socket coverage should exercise:

- A daemon-owned fake Compatibility Mode backend with local
  `bridgevm-tools-linux --socket <sock>` connected through the guest-tools
  socket path
- `bridgevm --socket <sock> guest-tools list-applications <vm>`
- `bridgevm --socket <sock> guest-tools launch-application <vm> --id <id>`
- `bridgevm --socket <sock> guest-tools list-windows <vm>`
- `bridgevm --socket <sock> guest-tools focus-window <vm> --id <id>`
- `bridgevm --socket <sock> guest-tools close-window <vm> --id <id>`
- Request-correlated `CommandResult` metadata for successful application/window
  commands, a missing-application launch failure, and a closed-window failure

The expected contract is that application and window commands travel through
the socket-backed CLI, daemon-owned guest-tools session, and Linux tools
scaffold without starting a real VM. Successful results prove only that the
alpha command plumbing accepted and processed scaffold entries; they do not
claim that a real guest application was launched or a real desktop window was
controlled.

Current executable coverage: `guest-tools-app-window-cli-smoke.sh` covers a
fake QEMU backend plus local Linux tools process, then verifies application and
window command dispatch, result correlation, missing-application launch
failure metadata, and the closed-window failure case.

Linux guest-tools time-sync live socket coverage should exercise:

- A daemon-owned fake Compatibility Mode backend with local
  `bridgevm-tools-linux --socket <sock>` connected through the guest-tools
  socket path
- `bridgevm --socket <sock> guest-tools time-sync <vm> --unix-epoch-millis <ms>`
- Rejection before dispatch when no guest-tools session is connected
- Protocol validation rejection for invalid timestamps on a connected session
- Request-correlated command/result metadata for a valid `TimeSync` command

The expected contract is that host-origin time-sync commands travel through the
socket-backed CLI, daemon-owned guest-tools session, and Linux tools scaffold
without changing a real guest clock. Invalid timestamps are rejected at the
protocol boundary before dispatch.

Current executable coverage: `guest-tools-time-sync-cli-smoke.sh` covers a fake
QEMU backend plus local Linux tools process, no-session rejection, connected
`InvalidTimestamp` rejection for `--unix-epoch-millis 0`, valid command frame
validation, and request-correlated runtime result metadata.

Guest-tools command tracker negative-path coverage should exercise:

- A daemon-owned fake Compatibility Mode backend and fake guest-tools Unix
  socket session advertising a command capability
- Rejection of duplicate pending request IDs before a second command is sent
  to the guest-tools session
- Ignoring stray `CommandResult` frames whose request ID is not pending
- Recording only the expected command result in runtime metadata once the
  matching request ID completes

The expected contract is that pending request IDs are unique, command results
must match a tracked host-origin request, and unexpected guest-origin results
must not satisfy or overwrite the pending command.

Current executable coverage: `guest-tools-command-tracker-cli-smoke.sh` covers
the fake QEMU/socket path, duplicate pending request rejection,
`UnexpectedCommandResult` logging for a stray result, and honest recording of
the original matching clipboard command result.

Guest-tools agent-update passive metadata coverage should exercise:

- `bridgevm guest-tools status <vm>` exposing `agent-update` only when
  manifest `security.signedAgentUpdates` enables that policy capability
- A daemon-owned fake Compatibility Mode backend and fake guest-tools Unix
  socket session advertising `agent-update`
- A guest-origin `AgentUpdateAvailable` protocol frame with current version,
  available version, download URL, and signature metadata
- Recording those fields plus an observed timestamp in
  `metadata/guest-tools-runtime.json`
- Socket-backed `guest-tools status` exposing the same passive update metadata
  without claiming a download, install, execution, or completed auto-update

The expected contract is that `AgentUpdateAvailable` is status metadata only.
The daemon may authenticate, authorize, record, and report the notice, but it
must not fetch the URL, verify or install a package, execute an updater, mutate
the guest tools binary, or treat the notice as command completion.

Current executable coverage: `guest-tools-agent-update-cli-smoke.sh` covers the
no-real-VM fake backend/socket path for the signed-update policy gate in both
directions: manifests with `security.signedAgentUpdates` expose `agent-update`,
while manifests with that policy disabled do not. It also verifies passive
runtime metadata recording, socket status visibility, and the no-execution
claim boundary.

Linux guest-tools metrics live socket coverage should exercise:

- `bridgevm guest-tools status <vm>` exposing the `guest-metrics` capability
  from diagnostics policy before a guest session is connected
- A daemon-owned fake Compatibility Mode backend and fake guest-tools Unix
  socket bridged to `bridgevm-tools-linux --socket <sock>`
- `GuestMetrics` protocol frames published by the Linux tools scaffold without
  starting real QEMU or Apple VZ
- Runtime `metadata/guest-tools-runtime.json` metrics fields including CPU,
  memory, and update timestamp
- Socket-backed `guest-tools status` exposing the same passive guest metrics
  metadata

The expected contract is that guest metrics are passive runtime telemetry.
Recording metrics must authenticate through the guest-tools session and update
status metadata only; it must not start a backend, benchmark the guest, or treat
the metrics frame as a host command completion.

Current executable coverage: `guest-tools-metrics-cli-smoke.sh` covers the
no-real-VM fake backend/socket path for publishing `GuestMetrics` through
`bridgevm-tools-linux`, recording runtime metrics metadata, and reporting those
values through the socket-backed CLI status path.

Shared-folder manifest integration coverage should exercise:

- `bridgevm share list <vm>`
- `bridgevm share add <vm> <name> <host-path> [--read-only] [--host-path-token <token>]`
- `bridgevm share remove <vm> <name>`
- The same operations through `bridgevm --socket <sock> ...`

The expected contract is that the CLI manages the manifest `sharedFolders`
approval list without exposing raw host paths to guest commands, returns the
resolved opaque host path token, rejects empty or whitespace share fields,
rejects duplicate share names and tokens through manifest validation, and makes
approved shares visible through guest-tools status. These operations change VM
manifest policy only; they do not live-update an existing guest-tools session,
perform a guest mount, or create a guest filesystem path. Guest-side mount
behavior remains covered by the separate guest-tools mount/unmount command path.

Current executable coverage: `shared-folder-manifest-cli-smoke.sh` covers local
CLI and socket-backed manifest management, empty/whitespace field rejection,
duplicate name/token rejection, read-only/token preservation, plus guest-tools
approved-share status reporting from a disposable VM bundle.

Performance baseline/sample integration coverage should exercise:

- `bridgevm performance baseline <vm> --output <dir>`
- `bridgevm --socket <sock> performance baseline <vm> --output <dir>`
- `bridgevm performance sample <vm> --output <dir> --artifact-bytes 4096 --iterations 1`
- `bridgevm --socket <sock> performance sample <vm> --output <dir> --artifact-bytes 4096 --iterations 1`
- `bridgevm performance sample <vm> --output <dir> --artifact-bytes 4096 --iterations 3 --sync`
- Invalid bounded-sample inputs, including zero iterations, excessive
  iterations, and oversized artifacts.
- Missing-VM rejection for baseline and sample paths on local and socket
  transports without creating output artifacts.

The expected contract is that baseline writes a metadata-only artifact with the
current VM state, guest-tools status, available runtime metrics, and notes, and
that sample writes a bounded host-side sample artifact, leaves the probe file or
per-iteration probe files in the artifact directory, reports non-metadata write
measurements and aggregate latency metadata, and does not boot, resume, or
benchmark the guest. Dashboard cards should surface artifact paths, timestamps,
byte counts, iteration counts, and latency fields as metadata only.

Current executable coverage: `performance-cli-smoke.sh` covers the local CLI
and socket-backed halves of the baseline and sample contracts,
sync/multi-iteration metadata, invalid bound rejection, missing-VM rejection
without persisted output, and fake host-side `qemu-img info` timing metadata.

Fast Mode Apple VZ launch-readiness coverage should exercise the Rust planner,
dry-run runner metadata, and CLI/daemon surfaces for the preflight gate without
starting Apple VZ:

- A template-created Linux Arm64 VM with missing installer media reports a
  structured missing-boot-media blocker that includes the media kind and
  resolved bundle path.
- `prepare-run`, Fast Mode `run` without spawn, and daemon `runner-status`
  expose the same readiness object from runner metadata, including the overall
  state and blocker list.
- Aggregate `bridgevm readiness` reports expose the same preflight object in
  optional `pre_run_launch_readiness` when runner metadata is absent, and the
  CLI prints that fallback under `Pre-run launch readiness:` without creating
  runner metadata or starting a backend.
- Dry-run metadata writes `.vmbridge/metadata/apple-vz-launch.json`, and
  `metadata/runner.json` records the same path as `launch_spec_path`.
- `lightvm-runner --require-ready` fails with named readiness blockers while
  blocked, and passes without spawning Apple VZ when the dry-run inputs are
  ready.
- `lightvm-runner --launch-spec <path> --print-handoff` consumes the persisted
  launch spec artifact directly and emits the Apple VZ backend handoff
  JSON without rebuilding the manifest plan or spawning Apple VZ.
- `AppleVzRunner --handoff-json <path> --validate-only` decodes that handoff
  JSON through the Swift helper boundary and validates the ready backend input
  without starting a `VZVirtualMachine`. With `--print-config-plan`, it also
  proves the handoff carries enough resource data for the limited
  configuration-construction boundary.
- Swift Apple VZ configuration construction/validation is limited to
  `linux-kernel` boot, a `raw` primary disk, and NAT networking. `qcow2`
  remains acceptable for dry-run readiness/plan coverage, but it must not be
  treated as constructible Apple VZ disk configuration.
- `lightvm-runner --launch-spec <path> --require-ready --launch` reaches the
  launcher interface for ready handoffs. The default in-process Rust launcher
  still returns the explicit unimplemented Apple Virtualization.framework
  launch error.
- Passing `--apple-vz-runner <path>` to that same launch path sends the handoff
  JSON to the Swift helper over stdin, proving the Rust-to-helper process
  boundary. The Swift helper now owns the limited real launch path for
  `linux-kernel` + `raw` + NAT specs, but it must still require
  `--allow-real-vz-start` before `VZVirtualMachine.start()` is called. Smoke
  tests should therefore exercise validate-only, config-validation,
  unsupported-input, or missing-opt-in paths when they intend to prove the
  process boundary without starting Apple VZ.
- Fast Mode `run --spawn` still fails before Apple VZ process launch, but its
  error summarizes the current readiness blockers, including the spawn-only
  `fast-mode-spawn-unimplemented` blocker and any missing disk/media blockers.
- After local media import or a test-created placeholder at the resolved path,
  the missing-boot-media blocker clears while launch remains a readiness result,
  not a spawned Apple VZ process.
- A Fast Mode VM whose active primary disk is missing reports a structured
  missing-disk blocker with the active disk path and format.
- Unsupported primary disk formats remain preflight failures instead of
  falling through to launch.
- Unsupported launch inputs, such as x86 guests, non-Apple-VZ preferred backend,
  or non-NAT networking, remain preflight failures with stable blocker names.
- Unsupported host launch capabilities, such as a non-macOS host or non-Apple-
  Silicon host, remain launch-readiness capability blockers rather than path
  blockers.
- Linux kernel mode covers both required `kernelPath` and optional `initrdPath`
  so a missing initrd is reported separately from a missing kernel.
- macOS restore mode reports a missing restore image as boot media readiness,
  while unsupported host or guest capability checks remain support blockers.
- CLI and socket tests should assert the JSON shape is stable enough for the
  dashboard: RunnerStatus readiness, optional pre-run launch readiness fallback,
  overall readiness state, blocker kind, affected path or capability, and a
  concise remediation hint.

Current executable coverage: `fast-mode-readiness-smoke.sh` covers the CLI
`prepare-run`, local dry-run `run`, and socket-backed `runner-status`/`run`
surfaces for missing disk/media blockers and readiness clearing when test
placeholder media is present. `fast-mode-readiness-unsupported-smoke.sh` covers
CLI and socket preflight failures for x86 guests, unsupported primary disk
formats, non-Apple-VZ preferred backends, and non-NAT networking without
spawning Apple VZ. It also covers missing active disk readiness, Linux
kernel/initrd readiness separation, missing macOS restore image readiness, and
the qcow2 boundary where dry-run handoff/config-plan output remains valid but
Swift VZ configuration validation refuses to treat qcow2 as constructible.
`fast-mode-readiness-template-matrix-smoke.sh` covers the Ubuntu/Fedora/Debian
Arm64 installer template matrix on both local and socket transports, asserting
the template note plus the metadata-safe `readiness`/`prepare-run` blockers for
missing installer media, missing primary disks, and launch-readiness blocker
propagation.
The smoke scripts do not pass the supported Swift-helper live-start opt-in, so
they must not start a real VM. A separate live boot E2E would need real
kernel/initrd/raw disk fixtures, Apple virtualization entitlement coverage, and
the explicit `--allow-real-vz-start` opt-in.

Readiness evidence requirements are stricter than these metadata-safe smokes.
The current required-but-unproven categories are `live-boot`, `console`, and
`guest-tools-effects`:

- `live-boot` needs an opt-in evidence bundle proving verified guest boot
  progress, not only an empty blocker list, valid handoff JSON, or helper
  start/stop transcript.
- `console` needs verifier-bound console evidence such as a preserved
  graphical viewer artifact, accepted serial sentinel output, or accepted QEMU
  QMP running evidence. Plain QMP socket readiness, bounded log tail APIs, or
  unverified loose logs are diagnostics only.
- `guest-tools-effects` needs observable effects inside the guest from
  guest-tools commands, not only authenticated protocol dispatch, pending-count
  tracking, or `last_command_result` metadata. The preserved-evidence path can
  mark this category proven only when the evidence bundle includes
  verifier-checked guest-tools result artifacts for those effects. Each effect
  must include either matching `expected_value`/`observed_value` fields or an
  artifact plus SHA-256 digest that the verifier can read from the preserved
  evidence directory.

Synthetic verifier tests may assert that these requirement names are preserved.
The Rust `readiness_report` request and `bridgevm readiness <vm>
--live-evidence <dir>` CLI can ingest a preserved Apple VZ evidence directory
without launching a VM, validate the same bounded metadata/text/JSON markers,
and then mark `live-boot` plus accepted serial, viewer, or QMP console evidence
proven in that report. `--record-live-evidence` first verifies the supplied directory and then
copies it into the VM bundle at `metadata/live-evidence/latest`, records
`metadata/live-evidence.json`, and lets later plain `bridgevm readiness <vm>`
re-run the verifier against that preserved path. `--clear-live-evidence` must
remove both the metadata JSON and copied bundle, after which plain readiness
returns to pending live evidence unless a new `--live-evidence` directory is
provided. They must still leave `guest-tools-effects` unproven unless
corresponding live guest-tools effect artifacts are added to the evidence
contract and checked by the verifier. Ingesting preserved evidence is still a
metadata/report path; it does not run guest-tools commands, launch Apple VZ,
start QEMU, or touch a guest by default.

`resource-profile-readiness-smoke.sh` covers profile-derived Fast Mode resource
handoff without starting Apple VZ: `performance` with automatic memory/CPU
resolves to 6144 MiB and 4 vCPUs, manual memory/CPU overrides are preserved,
and both local and socket-backed launch specs pass through `lightvm-runner`
handoff JSON plus Swift `AppleVzRunner --validate-only --print-config-plan`.

`apple-vz-live-boot-opt-in-smoke.sh` is that separate manual harness. It creates
a temporary ready `linux-kernel` + `raw` + NAT launch spec, validates the Swift
configuration boundary, proves the missing opt-in failure, then invokes
`lightvm-runner --apple-vz-allow-real-start` with
`--apple-vz-stop-after-seconds` so successful fixtures do not leave the helper
waiting indefinitely. It also passes `--apple-vz-force-stop-grace-seconds`, so a
guest that ignores the graceful stop request is force-stopped after the grace
period. If no signed helper is supplied through
`BRIDGEVM_LIVE_VZ_RUNNER`, the script builds `AppleVzRunner` with SwiftPM and
ad-hoc signs it using `apps/macos/scripts/build-sign-apple-vz-runner.sh`.
Each non-skipped live attempt preserves `$STORE/evidence` with
`SUMMARY.txt`, `fixture-manifest.json`, `environment.txt`,
`apple-vz-launch.json`, `live-vz-handoff.json`, `apple-vz-runner.path`,
`apple-vz-runner.artifact`, `apple-vz-runner.sha256`,
`apple-vz-validate.output`, `apple-vz-live-launch.output`, the selected runner
path, a copied runner artifact, and copied runner and serial logs referenced by
the launch spec. The fixture manifest records source and
bundle file size/hash pairs so reviewers can tell exactly which kernel, initrd,
and copied raw disk were booted. The summary, environment file, and live-launch
transcript also retain the configured stop-after and force-stop grace seconds,
so the manual proof records the bounded lifecycle window used for the run. The
serial log remains the strongest guest progress proof: configure
`BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED` with a known sentinel for every non-skipped
live proof. The harness skips real Apple VZ starts without that sentinel rather
than treating a serial capture alone as proof. Optional
`BRIDGEVM_LIVE_VZ_VIEWER_FRAME` and
`BRIDGEVM_LIVE_VZ_GUEST_TOOLS_EFFECTS_JSON` inputs are copied into the evidence
directory and validated by the verifier when present. A non-skipped successful
live smoke invokes `verify-apple-vz-live-evidence.sh "$STORE/evidence"` before
printing `PASS`, so the preserved evidence contract is checked immediately.

`verify-apple-vz-live-evidence.sh <evidence-dir>` validates that preserved
bundle after the live run. It checks `SUMMARY.txt` status, fixture manifest
hashes and sizes, launch spec and handoff JSON, validation and live-launch
output, the selected runner path, and required serial sentinel evidence from the
opted-in live run. The Rust readiness ingestion path performs a bounded
subset of those checks so local and daemon-backed readiness reports can reflect
an already-preserved Apple VZ bundle without invoking Swift or starting Apple
VZ. The shell verifier also cross-checks `environment.txt` against the fixture manifest's
source paths, the launch spec's kernel command line and resources, and the
selected runner path recorded in `apple-vz-runner.path`.
If `apple-vz-runner.artifact` exists, it is checked as a relative executable
artifact inside the evidence directory and matched against
`apple-vz-runner.sha256`.
Path lines recorded in `SUMMARY.txt` are treated as
artifact cross-checks against the preserved bundle, so the `Store`, `Bundle`,
`Launch spec`, `Handoff JSON`, output path, runner/serial log,
`Fixture manifest`, and `Environment` lines must resolve to the evidence fields
and files they name. Runner and serial log paths referenced by the launch spec
must be regular files inside the preserved evidence directory, not external
bundle paths or symlinks.
The selected runner path comes from `apple-vz-runner.path`; when
`apple-vz-runner.artifact` is present, that relative evidence file is the
executable/SHA-256 proof, otherwise the original selected runner path must still
resolve to an existing executable. The
live-launch transcript must retain the expected handoff-ready, diagnostics,
start, and finished markers. `apple-vz-live-evidence-verifier-smoke.sh` tests
that verifier with synthetic evidence and must remain metadata-only: it does
not start Apple VZ, QEMU, a VM, or a GUI, and it does not replace the opt-in
live smoke as the source of real live proof. Even when the verifier accepts a
synthetic bundle, that acceptance is coverage of the evidence artifact contract;
it is not proof of `live-boot`, `console`, or `guest-tools-effects`. Future or
current `guest-tools-effects` proof must come from verifier-checked
guest-tools result artifacts or matching expected/observed values inside the
preserved bundle, not from live boot metadata, command dispatch metadata, or the
default metadata-safe smoke lane.

Compatibility Mode can use the same readiness ingestion path with a preserved
QEMU evidence bundle. A QEMU bundle is selected by `qemu-live-evidence.json`
and must prove `backend: "qemu"`, VM identity, compatibility boot mode, disk
format, network, a supported `qemu-system-*` command with the exact emitted
`-qmp unix:<socket>,server=on,wait=off` shape, `qmp.running: true`,
`qmp.status: "running"`, a QMP socket path, and SHA-256-checked QEMU, serial
log, and QMP transcript artifacts. The transcript must contain the QMP
greeting, `query-status` command, and a running response. A configured serial
sentinel must appear in the preserved serial log. Accepted QEMU evidence sets
`qmp_evidence_proven`, so the console requirement can be proven by QMP even
when no graphical viewer artifact is present. The readiness smoke uses
synthetic QEMU artifacts to verify this
schema and record/reuse/clear behavior without starting QEMU; it is still
contract coverage rather than real live proof.

The QEMU verifier also binds the bundle to the readiness target rather than
trusting a self-consistent artifact alone. The preserved `vm_name` must match
the requested VM, the target must be Compatibility Mode, `disk_format` must be
`qcow2` and match the active disk metadata, `network` must be `nat`, the QMP
socket must match the VM bundle's expected `metadata/qmp.sock`, command
`-name` must match `vm_name`, command `-qmp` must point at the same socket, and
`qmp.status` must be `running`. The dedicated
`qemu-live-evidence-verifier-smoke.sh` covers these metadata-safe rejection
cases plus SHA/path/sentinel tampering without launching QEMU.

The optional `qemu-live-boot-opt-in-smoke.sh` is the real Compatibility Mode
counterpart to the Apple VZ live smoke. By default it prints `SKIP` and exits
successfully. It only attempts a real QEMU start when
`BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START=1` and
`BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED` are set. Point it at an existing prepared
Compatibility Mode VM with `BRIDGEVM_LIVE_QEMU_STORE` and
`BRIDGEVM_LIVE_QEMU_VM`, or provide `BRIDGEVM_LIVE_QEMU_QCOW2_DISK` to create a
disposable VM in a temporary store. The host must have either
`qemu-system-x86_64` or `qemu-system-aarch64` on `PATH`.

`prepare-qemu-live-fixture.sh` is the metadata-safe prep companion for that
manual live path. It does not require QEMU or `qemu-img`, does not create a
disk image, does not start a VM, and does not set
`BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START`. It only validates an operator-supplied
bootable qcow2 when one is provided, creates the chosen store/evidence
directories, and prints shell-safe exports for review:

```sh
eval "$(tests/integration/prepare-qemu-live-fixture.sh \
  --qcow2 /path/to/bootable-root.qcow2 \
  --arch arm64 \
  --sentinel bridgevm-qemu-ready \
  --timeout 120)"
export BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START=1
tests/integration/qemu-live-boot-opt-in-smoke.sh
```

Use `--dry-run` to preview the exports without touching the filesystem.

Use an existing VM store when you want to preserve the operator-created VM
bundle:

```sh
BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START=1 \
BRIDGEVM_LIVE_QEMU_STORE=/path/to/bridgevm-store \
BRIDGEVM_LIVE_QEMU_VM=ubuntu-compat \
BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED=bridgevm-qemu-ready \
BRIDGEVM_LIVE_QEMU_EVIDENCE_DIR=/path/to/qemu-live-evidence \
BRIDGEVM_LIVE_QEMU_TIMEOUT_SECONDS=120 \
  tests/integration/qemu-live-boot-opt-in-smoke.sh
```

Use a disposable VM when you already have a bootable qcow2 fixture and want the
harness to create the Compatibility Mode bundle:

```sh
BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START=1 \
BRIDGEVM_LIVE_QEMU_QCOW2_DISK=/path/to/root.qcow2 \
BRIDGEVM_LIVE_QEMU_ARCH=arm64 \
BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED=bridgevm-qemu-ready \
  tests/integration/qemu-live-boot-opt-in-smoke.sh
```

Optional QEMU live inputs are `BRIDGEVM_LIVE_QEMU_STORE`,
`BRIDGEVM_LIVE_QEMU_VM`, `BRIDGEVM_LIVE_QEMU_QCOW2_DISK`,
`BRIDGEVM_LIVE_QEMU_ARCH`, `BRIDGEVM_LIVE_QEMU_EVIDENCE_DIR`, and
`BRIDGEVM_LIVE_QEMU_TIMEOUT_SECONDS`. The timeout must be a positive integer
and defaults to `60`.

The harness runs `bridgevm run --spawn`, waits for the configured serial
sentinel and a QMP `query-status` running response, writes
`qemu-live-evidence.json` plus hashed QEMU, serial, and QMP transcript
artifacts, records that bundle through
`bridgevm readiness --live-evidence --record-live-evidence`, and preserves
audit sidecars in the evidence directory: `SUMMARY.txt`, `environment.txt`,
`fixture-manifest.json`, `bridgevm-run.output`, and
`bridgevm-readiness-record.output`. A successful run ends with:

```text
PASS: QEMU live boot opt-in smoke (<store>)
Evidence directory: <evidence-dir>
Summary: <evidence-dir>/SUMMARY.txt
```

The readiness record must include `Live evidence: verified (<evidence-dir>)`,
`Live evidence QMP: proven=true`, and
`Live evidence serial sentinel: required=true proven=true`. The summary and
fixture manifest are also refreshed on harness failures so failed live attempts
still leave the selected store, bundle, timing, fixture disk, active disk, and
partial evidence paths reviewable. Common blockers are missing
`BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START=1`, missing
`BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED`, no supported `qemu-system-*` executable
on `PATH`, a missing `BRIDGEVM_LIVE_QEMU_QCOW2_DISK`, a non-running QMP status,
or the expected serial sentinel never appearing before the timeout. This
remains manual opt-in live evidence; metadata-safe suites cover only its
default skip boundary and the synthetic verifier.

## Application-consistent snapshot live opt-in smoke

`application-consistent-snapshot-live-opt-in-smoke.sh` proves the REAL
application-consistent snapshot orchestration end to end against a
daemon-owned, booted Compatibility (QEMU/HVF) Linux guest. `bridgevmd` spawns
and OWNS the QEMU backend (holding the live `guest-tools.sock` session), the
guest agent boots with a Real `fsfreeze` backend bound to a SAFE, dedicated
loopback ext4 mount (`/mnt/bridgevm-fsfreeze`, never the rootfs), and
`bridgevm --socket <bridgevmd> snapshot execute-application-consistent` drives
the daemon `FsFreeze -> disk snapshot -> FsThaw` orchestration over the real
virtio-serial channel.

It asserts the agent's `FsFreeze`/`FsThaw` `CommandResult` frames are
`ok:true`, that the result messages reference the real fsfreeze boundary on the
safe mount (so a simulated ack could not pass), and that the disk snapshot was
recorded between the freeze and the thaw.

This is heavy manual opt-in (it boots a real VM). It SKIPS unless
`BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START=1` and
`BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK=<bootable arm64 Linux cloud qcow2>` are
set, and requires macOS `hdiutil`, `qemu-system-aarch64` with `hvf`, `python3`,
and a cross-compiled agent (build via `scripts/build-guest-agent-linux.sh`). The
guest image must provide `fsfreeze` (util-linux), `mkfs.ext4`, `losetup`/`mount`,
and `base64`/`gunzip`. The test-only `BRIDGEVM_COMPAT_EXTRA_QEMU_ARGS` daemon
seam attaches the NoCloud cidata seed ISO to the daemon-spawned QEMU command
without changing the product command builder. It is excluded from the
metadata-safe suite; only its default skip boundary is covered there.
