# Compatibility Mode

Compatibility Mode is the broad QEMU-backed engine path.

It is for legacy operating systems, x86 emulation, research OSes, custom kernels, manual hardware settings, serial consoles, and advanced debugging.

It may be slower than Fast Mode, but its product promise is that more things are possible.

## Implemented scaffold

The Rust `bridgevm-qemu` crate can build a QEMU command preview from a Compatibility Mode manifest. The CLI exposes this through:

```bash
bridgevm qemu-args legacy-linux
bridgevm disk prepare legacy-linux
bridgevm disk create legacy-linux
bridgevm disk inspect legacy-linux
bridgevm disk compact legacy-linux
bridgevm port list legacy-linux
bridgevm port add legacy-linux 2222:22
bridgevm port remove legacy-linux 2222:22
bridgevm share list legacy-linux
bridgevm share add legacy-linux Projects ~/Projects --read-only
bridgevm share remove legacy-linux Projects
bridgevm ssh legacy-linux --user ubuntu
bridgevm prepare-run legacy-linux
bridgevm runner-status legacy-linux
bridgevm snapshot create legacy-linux before-upgrade --kind disk
bridgevm snapshot disk-create legacy-linux before-upgrade
bridgevm snapshot chain legacy-linux
bridgevm snapshot list legacy-linux
bridgevm snapshot restore legacy-linux before-upgrade
```

The `fullvm-runner` binary can also print the generated QEMU arguments:

```bash
fullvm-runner legacy-linux --print-qemu-args
```

Running `bridgevm disk prepare legacy-linux` prepares only the primary disk metadata: BridgeVM resolves the manifest's primary disk, creates the disk directory, writes `metadata/primary-disk.json`, and prints the disk path, format, size, readiness, creation flag, and any suggested creation command. Running `bridgevm prepare-run legacy-linux`, `bridgevm run legacy-linux` without spawn, or `fullvm-runner legacy-linux` performs the same disk preparation before recording dry-run runner metadata in the VM bundle.

For the default `qcow2` disk format, disk preparation does not create or verify the image. This is deliberate: `bridgevm disk prepare legacy-linux` is the safe planning path, so it builds and records the `qemu-img create` command without running it. If the disk file is missing, metadata records `exists: false`, `created: false`, and a command such as:

```bash
qemu-img create -f qcow2 ~/.bridgevm/vms/legacy-linux.vmbridge/disks/root.qcow2 80GiB
```

Run `bridgevm disk create legacy-linux` to explicitly execute the recorded `qemu-img create -f <format> <path> <size>` command for a missing non-raw primary disk. If `qemu-img` is not installed, is not on `PATH`, or exits unsuccessfully, `disk create` fails and leaves the caller with the same safe command information from preparation. `disk prepare` remains the lowest-risk way to refresh disk preparation state without running `qemu-img` or building a runner command; `runner-status` shows what BridgeVM last prepared, including the disk path, format, existence flag, sparse-file creation result, and suggested command. For `raw` primary disks, preparation can create the missing disk directly as a sparse file and records `created: true`, so `disk create` commonly reports that the disk is already ready.

Run `bridgevm disk inspect legacy-linux` after creation to verify and record the primary disk metadata. For an existing non-raw disk, BridgeVM runs `qemu-img info --output=json <path>`, prints the JSON metadata, and writes the latest result to `metadata/last-disk-inspect.json`. Inspection can fail safely when the primary disk is raw, the disk file is missing, `qemu-img` is not installed, or `qemu-img info` exits unsuccessfully; in those cases BridgeVM reports the boundary instead of treating the disk as verified.

Run `cargo run -p bridgevm-cli -- disk compact legacy-linux` to explicitly compact an existing non-raw primary or active disk. BridgeVM uses `qemu-img convert` to write a compacted replacement, renames the previous active disk to a `.precompact-<timestamp>` backup path, moves the compacted image into the active disk location, and writes the latest compaction result to `metadata/last-disk-compact.json`. If the disk is raw, missing, or `qemu-img convert` is unavailable or unsuccessful, the command fails at that explicit boundary instead of silently changing storage state.

Port forwarding is configured through the manifest before runner planning.
`bridgevm port list legacy-linux` prints the current `network.forwards`
entries, `bridgevm port add legacy-linux <host:guest>` appends a forwarding
pair such as `2222:22`, and `bridgevm port remove legacy-linux <host:guest>`
removes that exact pair. The daemon socket API exposes the same list/add/remove
operations. These commands update the VM manifest's `network.forwards` field;
they do not patch an already-running QEMU process. Compatibility Mode QEMU
argument generation already renders those manifest forwards as `hostfwd`
options, so changes affect subsequent `qemu-args`, `prepare-run`, dry-run
runner metadata, and later `run --spawn` attempts.

Shared-folder approvals are also configured through the manifest before any
guest-tools mount action. `bridgevm share list legacy-linux` prints approved
manifest entries, `bridgevm share add legacy-linux <name> <host-path>
[--read-only] [--host-path-token <token>]` records an approved host path,
read-only flag, and opaque token, and `bridgevm share remove legacy-linux
<name>` removes that approval. The daemon socket API exposes the same
list/add/remove operations, and the macOS dashboard should use that boundary for
its shared-folder management UI. These commands change durable VM policy only:
they do not mount a folder inside an already-running guest, create a guest path,
or update transient guest-tools session state by themselves.

Compatibility Mode networking supports NAT, isolated, and host-only QEMU
plans. NAT renders QEMU user networking and consumes manifest port forwards as
`hostfwd` options. Isolated renders restricted user networking. Host-only
renders the QEMU host-only netdev plan without guest outbound internet and
without port forwarding; because macOS `vmnet-host` requires root or the
`com.apple.vm.networking` entitlement, `network-plan`, `prepare-run`, and
spawn readiness report a `qemu-host-only-requires-privilege` blocker before a
live launch. Attempts to add `network.forwards` while the manifest is in
`host-only` mode are rejected by both the local CLI and daemon socket API.
Bridged networking similarly renders a `vmnet-bridged` QEMU netdev but reports
`qemu-bridged-requires-privilege` until the QEMU process has the required
privilege. Advanced network mode is still accepted as a planning intent, but
QEMU argument generation reports an explicit launch blocker until advanced
network launcher wiring exists.

`bridgevm ssh legacy-linux [--user USER]` uses metadata to print an SSH command
plan, but does not execute `ssh`. Compatibility Mode prefers a manifest forward
whose guest port is `22`; for example, `2222:22` produces
`ssh -p 2222 USER@127.0.0.1`. If no such forward is present, a connected
guest-tools runtime with a valid guest IP can produce `ssh USER@<guest-ip>`.

Compatibility Mode resource planning uses the shared resource-manager scaffold before QEMU arguments are generated. Manifest `resources.memory` and `resources.cpu` values of `auto` are resolved deterministically from `resources.profile`; explicit memory or CPU values remain unchanged. The resolved values are then rendered into the QEMU `-m` and `-smp` arguments used by `qemu-args`, `prepare-run`, dry-run runner metadata, and later spawn attempts.

The recommended startup sequence is `disk prepare`, `disk create`, `disk inspect`, then `run --spawn`. Passing `--spawn` attempts to start QEMU only after the selected active disk exists. If the disk is still missing, BridgeVM exits with the recorded creation command instead of spawning. That makes `disk prepare`, `disk create`, `disk inspect`, `prepare-run`, `qemu-args`, `runner-status`, and `run --spawn` safe to use in automation: planning can report missing qcow2 work, `disk create` can perform the explicit image creation step, `disk inspect` can verify the resulting non-raw image metadata, and QEMU argument generation plus spawning remain gated on active disk metadata. When spawn succeeds, it redirects output to `logs/qemu.log`, records the child PID in `metadata/runner.json`, and marks the VM running.

Compatibility Mode QEMU plans include the scaffold guest-tools transport. The
generated QEMU arguments add a virtio-serial port named
`org.bridgevm.guest-tools.0` backed by `metadata/guest-tools.sock`.
`runner-status` prints the guest-tools transport, channel, socket path, token
metadata file, and token creation time. The token value stays in
`metadata/guest-tools-token.json` and is not written into QEMU process
arguments. When `bridgevmd` owns a spawned Compatibility Mode backend, its
supervisor connects to `metadata/guest-tools.sock`, authenticates the first
guest frame as a `GuestHello`, and keeps the stream open. Bounded supervisor
drains now record heartbeat, guest IP, and guest metrics updates in
`metadata/guest-tools-runtime.json`, and `bridgevm guest-tools status` prints
that runtime state. For daemon-owned running backends,
`bridgevm --socket <sock> guest-tools set-clipboard <vm> --text <text>`,
`bridgevm --socket <sock> guest-tools resize-display <vm> --width <w> --height
<h> --scale <scale>`, and `bridgevm --socket <sock> guest-tools time-sync
<vm>` authorize and write host commands to the authenticated guest-tools
stream. Shared-folder dispatch is available through `bridgevm --socket <sock>
guest-tools mount-share <vm> --name <share> [--request-id <id>]` and
`bridgevm --socket <sock> guest-tools unmount-share <vm> --name <share>
[--request-id <id>]`; the host resolves `<share>` through the manifest-approved
shared-folder entries before dispatching the opaque host path token to the
guest-tools protocol. That dispatch is separate from `bridgevm share
list/add/remove`: manifest approval records what may be shared, while
`mount-share` is the explicit attempt to ask an authenticated guest tools
session to use that approved share. High-level wrappers are also available for
drag-and-drop, application metadata, and window metadata:

```bash
bridgevm guest-tools file-drop-start <vm> \
  --transfer-id <id> \
  --file-name <name> \
  --size-bytes <bytes> \
  [--request-id <id>]

bridgevm guest-tools file-drop-chunk <vm> \
  --transfer-id <id> \
  --chunk-index <index> \
  --data-base64 <base64> \
  [--request-id <id>]

bridgevm guest-tools file-drop-complete <vm> \
  --transfer-id <id> \
  [--request-id <id>]

bridgevm guest-tools list-applications <vm> [--request-id <id>]
bridgevm guest-tools launch-application <vm> --id <application-id> [--request-id <id>]
bridgevm guest-tools list-windows <vm> [--request-id <id>]
bridgevm guest-tools focus-window <vm> --id <window-id> [--request-id <id>]
bridgevm guest-tools close-window <vm> --id <window-id> [--request-id <id>]
```

`guest-tools send-command` remains available for raw envelope testing.
Commands with request IDs remain pending until the guest returns a matching
`CommandResult`.

These Compatibility Mode command surfaces are still alpha protocol scaffolds in
the current Linux guest tools runner. Application and window metadata use static
or in-memory scaffold entries rather than real desktop inventory; launch,
focus, and close acknowledgements do not start processes or control real
windows. Drag-and-drop commands track transfer state in memory but do not
write files unless the Linux tools scaffold is explicitly started with
`--file-drop-dir`; in that opt-in path it base64-decodes chunks and materializes
the completed payload under that configured directory after validating the
declared size and file name.

Disk snapshot chains follow the same explicit-storage boundary. Running `bridgevm snapshot create legacy-linux before-upgrade --kind disk` records the normal snapshot entry and writes an additional chain description under `metadata/snapshot-disks/before-upgrade.json`. That file is the scaffold for a qcow2 overlay chain: it records the backing disk selected from the active disk metadata, the backing format, the planned overlay location, whether the overlay currently exists, and a command of the form:

```bash
qemu-img create -f qcow2 -F <backing-format> -b <backing-file> <overlay>
```

Run `bridgevm snapshot chain legacy-linux` to inspect this metadata, including the active disk source, snapshot name when present, and selected disk path. Run `bridgevm snapshot disk-create legacy-linux before-upgrade` to explicitly execute the recorded overlay command. The command fails safely if the backing disk is missing, if `qemu-img` is unavailable, or if `qemu-img create` exits unsuccessfully. Creation attempts are recorded under `metadata/snapshot-disks/before-upgrade-create.json` or similar metadata, so callers can inspect the last overlay creation boundary. BridgeVM records the selected chain member in `metadata/active-disk.json`: successful overlay creation switches future `qemu-args`, runner plans, and spawns to the overlay, and restoring a disk snapshot rewinds the active disk to that snapshot's backing image.

Suspend snapshots use a different metadata boundary rather than a qcow2 chain. Running `bridgevm snapshot create legacy-linux paused --kind suspend` records the snapshot and writes `metadata/suspend-images/paused.json` with the planned image path `suspend-images/paused.bin`, the image format marker, whether the image currently exists, and the preparation timestamp. BridgeVM does not write real guest memory into that file yet. Restoring the suspend snapshot only requires the planned image marker to exist, records the image metadata as `suspend_image` in `metadata/last-restore.json`, and prints the suspend image status in the CLI restore output. It does not deserialize memory or resume a guest.

The macOS dashboard surfaces this Compatibility Mode snapshot metadata through
the daemon: snapshot list entries with kind and recorded runtime state,
disk-chain status, suspend-image readiness, the latest disk-create result
metadata, and the latest restore metadata. Its disk-create action executes only
the explicit `qemu-img create` overlay boundary, and its restore action rewinds
recorded metadata such as active-disk/runtime state. That dashboard restore
surface must not claim to restore guest memory, roll back a live QEMU process,
or provide full application consistency.

Application-consistent snapshots have a conservative preflight record plus a
daemon-owned execution path. Running `bridgevm snapshot create legacy-linux
app-safe --kind application-consistent` records the snapshot and writes
`metadata/application-consistent-snapshots/app-safe.json` with guest-tools
connection state, required `fs-freeze`/`fs-thaw` capabilities, advertised and
missing capabilities, readiness, backend freeze/thaw support, and planned
freeze/thaw semantics. The daemon-owned `bridgevm --socket <sock> snapshot
execute-application-consistent legacy-linux app-safe` path requires a
daemon-owned backend and authenticated guest-tools stream, dispatches
request-correlated freeze/thaw commands around snapshot creation, always
attempts thaw after the snapshot boundary, and records the execution result.
The Linux tools runner defaults to simulated freeze/thaw acknowledgements for
socket-safe tests. Its explicit `--real-fsfreeze --fsfreeze-mount <path>` mode
calls the Linux `fsfreeze` backend on allowlisted mounts; fake-backend smokes
cover ordering and rollback, and the heavy live opt-in smoke verifies the real
QEMU/HVF guest path against a safe loopback ext4 mount. That proves the
filesystem freeze/thaw boundary, not database flushing, application quiescing,
or complete application-level consistency.

`bridgevm stop legacy-linux` now uses the backend stop path. If QEMU is running and its QMP socket is available, BridgeVM sends QMP `quit`, marks the VM stopped, and clears runner metadata. Dry-run runner metadata is cleared without requiring QMP.

Compatibility Mode lifecycle suspend uses QMP `stop` plus `snapshot-save` to write an internal qcow2 snapshot tagged `bridgevm-suspend`, then quits QEMU and records a suspend marker. Resume relaunches QEMU with `-loadvm bridgevm-suspend` and only consumes that marker after QEMU survives the restore readiness window and, when reachable, QMP does not report a terminal status. If `-loadvm` exits quickly (the known Apple Silicon HVF arm64 failure mode), both local and daemon/socket resume report the failure and preserve the suspend marker and qcow2 snapshot.

QMP diagnostics are scaffolded:

```bash
bridgevm qmp-socket legacy-linux
bridgevm qmp-status legacy-linux
bridgevm logs qemu legacy-linux --bytes 16384
bridgevm logs serial legacy-linux --bytes 16384
bridgevm diagnostics bundle legacy-linux --output target/bridgevm-diagnostics
fullvm-runner legacy-linux --qmp-status
```

If QEMU is not running, these commands report that the QMP socket is unavailable. Once QEMU is started with `--spawn`, the QMP client can negotiate capabilities and issue `query-status`. Async QMP events received before a command response are skipped so command callers can still read the matching return value. The QMP client also exposes low-level envelope/event reading, bounded event drains, idle-socket classification, and terminal event classification so longer-lived supervisors can consume QMP events continuously without depending only on request-time status checks. During daemon supervision, the latest drained QMP event batch is recorded in `metadata/qmp-supervisor.json` whenever the drain observes events, reaches its bound, or sees a terminal event. This file is a latest supervisor snapshot, not an append-only QMP event log.

`bridgevm logs qemu <vm>` tails `logs/qemu.log`, which daemon and local spawn paths use for QEMU stdout/stderr. `bridgevm logs serial <vm>` tails `logs/serial.log`, the file passed to QEMU through `-serial file:<bundle>/logs/serial.log`. Both commands accept `--bytes <n>`, clamp the daemon payload to a bounded size, and report path, file existence, byte count, returned byte count, and truncation state. The same `view_logs` API backs the macOS dashboard Logs panel, so users can inspect backend text diagnostics without starting a graphical console.

### Live boot evidence review (opt-in)

`tests/integration/qemu-live-boot-opt-in-smoke.sh` is the manual opt-in harness
for the real QEMU live path and skips unless `BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START=1`
and `BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED` are set and a `qemu-system-*` binary is on
`PATH`. `tests/integration/prepare-qemu-live-fixture.sh` only prints shell-safe
`BRIDGEVM_LIVE_QEMU_*` exports and creates the store/evidence directories; it does
not create a disk image, start QEMU, or set the real-start opt-in. The operator
must supply a bootable qcow2 via `BRIDGEVM_LIVE_QEMU_QCOW2_DISK`.

On Apple Silicon the preferred fixture is a UEFI-bootable Debian arm64
`genericcloud` qcow2 (`BRIDGEVM_LIVE_QEMU_ARCH=arm64`): the runner boots it with
`-machine virt -accel hvf -cpu host -bios edk2-aarch64-code.fd`, so HVF
accelerates the guest, and the image's serial console reaches the PL011 UART that
the runner captures through `-serial file:<bundle>/logs/serial.log`. A captured,
readiness-verified example sequence (Debian 12 bookworm arm64):

```sh
curl -fL -o /tmp/debian-arm64-genericcloud.qcow2 \
  https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-arm64.qcow2
export BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START=1
export BRIDGEVM_LIVE_QEMU_ARCH=arm64
export BRIDGEVM_LIVE_QEMU_QCOW2_DISK=/tmp/debian-arm64-genericcloud.qcow2
export BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED='Linux version 6.1.0-49-cloud-arm64'
export BRIDGEVM_LIVE_QEMU_STORE=/tmp/bridgevm-live-qemu-run
export BRIDGEVM_LIVE_QEMU_TIMEOUT_SECONDS=180
tests/integration/qemu-live-boot-opt-in-smoke.sh
```

The smoke creates a disposable Compatibility Mode VM, copies the qcow2 to
`disks/root.qcow2`, spawns QEMU with `run --spawn` (VNC viewer endpoint on
`-display vnc=:0`), waits up to the timeout for the required serial sentinel plus
a `query-status` `running` QMP reply, writes the evidence bundle, and then records
it with `bridgevm readiness <vm> --live-evidence "$STORE/evidence"
--record-live-evidence`. A pass means readiness reports `live-boot` proven, the
serial sentinel proven, `console` proven (serial counts as console evidence), and
`QMP` proven; `guest-tools-effects` stays unproven because a stock cloud image has
no BridgeVM guest agent. The opt-in run preserves `$STORE/evidence` with:

- `SUMMARY.txt` status and artifact path lines
- `qemu-live-evidence.json` with `proven`, backend, boot mode, the full QEMU
  command, the QMP `running` snapshot, the serial sentinel, and per-artifact
  SHA-256 digests
- `serial.log` and `qemu.log` copies plus the `qmp-transcript.jsonl` capabilities
  and `query-status` exchange
- `fixture-manifest.json` source/bundle disk paths with sizes and digests, and
  `environment.txt` with the `BRIDGEVM_LIVE_QEMU_*` inputs used

Later plain `bridgevm readiness <vm>` re-verifies
`.vmbridge/metadata/live-evidence/latest`; `--clear-live-evidence` removes the
preserved evidence metadata and copied bundle. As with the Apple VZ path, a live
proof needs more than successful process start/stop output: the serial sentinel is
the QEMU harness's default guest-boot-progress contract, while viewer/QMP state
only supports console diagnostics. Preserved bundles may also include
verifier-bound `boot-progress-evidence.json` graphical artifacts when a separate
QEMU viewer capture proves boot progress.

### Windows 11 Arm installer boot

Compatibility Mode supports booting a Windows 11 Arm installer ISO through the
`windows-installer` boot mode. Set it in the VM manifest:

```yaml
guest:
  os: windows
  version: "11"
  arch: arm64
display:
  renderer: vnc            # headless capture; omit for a normal app window
boot:
  mode: windows-installer
  installerImage: /path/to/Win11_Arm64.iso
```

Or create it directly from the CLI (no manifest editing):

```sh
bridgevm create win11 --os windows --version 11 --arch arm64 \
  --mode compatibility --boot-mode windows-installer \
  --installer-image /path/to/Win11_Arm64.iso
```

For that mode, `build_compatibility_command` extends the arm64 `virt`/HVF/edk2
command with the device shape Windows on Arm needs: a `ramfb` GOP framebuffer the
installer renders to, a USB HID stack (`qemu-xhci` + `usb-kbd` + `usb-tablet`) so
the firmware "Press any key to boot from CD or DVD" prompt can be answered, and
the installer ISO presented as a bootable USB CD-ROM
(`usb-storage,...,bootindex=0`; WinPE includes USB mass-storage drivers). The
`virtio-rng-pci` device comes from the restricted Windows-Arm planning profile
(guest `version: "11"`). `bridgevm run <vm> --spawn` then launches it.

Verified: Windows 11 25H2 Arm64 booted to the Setup "Select language settings"
screen under the product-generated command, confirmed by a QMP `screendump` with
`query-status` running. The "Press any key" prompt has a short timeout, so a key
must be sent early (over QMP `send-key`, or pressed in the viewer). Reaching the
Setup screen does not require the target disk; a real install still needs an
NVMe-backed target the WinPE `stornvme` driver can see rather than the default
`virtio` primary disk.

`bridgevm diagnostics bundle legacy-linux --output <dir>` packages the
Compatibility Mode state that is useful for support without copying large or
live backend artifacts. It collects `manifest.yaml`, `logs/`, and `metadata/`,
writes `diagnostic-bundle.json`, and excludes disks, installer or restore media,
sockets, and lock files. JSON copied into the bundle is redacted, including the
guest-tools token, sensitive JSON keys, and URL query strings. The bundle
metadata reports only relative copied-file paths.
Daemon QMP supervisor snapshots in `metadata/qmp-supervisor.json` are included
automatically through the copied metadata directory.

The macOS dashboard can use the same daemon diagnostics response as a
support/export metadata surface for Compatibility Mode VMs. It may show the
bundle output path, source VM bundle, creation time, and copied file list, but
creating the bundle still does not connect to QMP, start QEMU, attach guest
tools, or copy excluded live backend artifacts.

Portable VM export/import is a broader copy boundary than diagnostics, and it
is also available to Compatibility Mode VMs through the Rust CLI/API/socket
surface. `bridgevm export legacy-linux --output <path>` writes either a
portable `.vmbridge` directory bundle or a `.tar` archive depending on the
output path, and `bridgevm import <bundle> [--name <name>]` copies that bundle
into a fresh destination while preserving manifest and metadata entries such as
snapshots, port forwards, and approved shared folders. The macOS dashboard
should surface the same daemon operation and show file-copy metadata rather
than inventing a separate importer. Export/import must not start QEMU, connect
to QMP, attach guest tools, copy live sockets, or claim live guest state
migration. Any disk or installer/media inclusion must remain exactly whatever
the existing Rust export/import behavior copies; the dashboard should not add
extra storage artifacts on its own.

`bridgevmd` can also run dry-run preparation, explicit disk creation, disk inspection, active-disk verification, disk compaction, metadata-only repair, spawn Compatibility Mode backends, stop dry-run backends, and report runner/QMP status through its socket API. The macOS dashboard Storage Maintenance panel uses those same `verify_disk` and `compact_disk` responses to show the active disk path, qemu-img command, check report or backup path, duration, and refreshed chain metadata without inventing a separate storage manager. The dashboard metadata repair panel/action invokes the daemon `repair_metadata` path and may show repaired/no-op status, actions, timestamp, and VM bundle path. That repair surface is metadata-only: it does not create disks or replace corrupt JSON. Daemon spawn follows the same disk rule as the CLI: missing `qcow2` disks are reported with the `qemu-img create` command and QEMU is not started. Spawned QEMU children started through the daemon are kept in an in-memory supervisor registry. The daemon polls that registry on a short interval, so exited child processes are reconciled even when no client request arrives. When a QMP socket is available, the daemon attaches a short-timeout QMP client to the supervised backend, drains a bounded batch of async events on each reconcile tick, consumes terminal events such as `SHUTDOWN`, and falls back to status polling when no event stream is attached or the stream has to be reset.

The remaining Compatibility Mode storage/supervision work is now narrower: make
QMP supervision fully event-driven across longer backend lifetimes, add real
suspend memory serialization/restoration, and add higher-level application
quiescing on top of the verified guest filesystem freeze/thaw boundary.
