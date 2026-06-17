# Phase 0 Architecture

BridgeVM currently implements the foundation from `PLAN.md` as a Rust workspace.

The important Phase 0 boundary is intentional: Fast Mode and Compatibility Mode are separate concepts from the beginning, with separate planner, runner, and verification paths. This keeps Fast Mode from becoming a thin QEMU settings screen.

## Implemented components

- `bridgevm-config`: readable VM manifests.
- `bridgevm-core`: mode recommendation and engine traits.
- `bridgevm-resource-manager`: deterministic resource profile decisions shared by runner planners.
- `bridgevm-storage`: `.vmbridge` bundle creation, listing, metadata-only delete tombstones, primary disk preparation metadata, explicit disk image creation, disk inspection metadata, active-disk verification/compaction metadata, disk snapshot chain metadata/inspection, and suspend snapshot image metadata.
- `bridgevm-storage`: VM runtime state and snapshot metadata.
- `bridgevm-cli`: developer CLI for create/list/status/lifecycle/snapshot flows.
- `BridgeVMApp`: SwiftUI macOS dashboard prototype with VM inventory, settings,
  Store Doctor/source readiness, lifecycle controls backed by lifecycle-plan
  readiness plus daemon suspend/resume/stop requests, a
  template-backed creation sheet, boot media readiness, a daemon-backed clone
  sheet with full/linked clone options and linked overlay metadata, and
  daemon-backed guest tools status/readiness visibility, including latest
  command-result status, plus VM bundle export/import metadata/file-copy status,
  metadata/planning-only network/open-port/SSH plan visibility, manifest-level
  port-forward list/add/remove controls, manifest-level approved shared folder
  list/add/remove controls, a Console action that
  reports daemon QMP socket readiness as a diagnostic instead of opening a
  graphical console, a Logs panel for bounded QEMU/serial log tails, and
  snapshot list/restore controls plus primary-disk prepare/create/inspect and
  active-disk verify/compact maintenance controls plus a metadata-only repair
  action that surfaces daemon metadata boundaries rather than live VM launch,
  implicit disk creation, or memory restore.
- `bridgevm-api`: typed request/response protocol for daemon operations.
- `bridgevm-daemon`: Unix socket metadata service using the typed API protocol.
- `bridgevm-qemu`: Compatibility Mode QEMU command builder, QMP client/control helpers, suspend/resume snapshot helpers, and supervisor metadata support.
- `bridgevm-apple-vz`: Fast Mode Apple VZ planner plus helper handoff support for opt-in live Linux kernel launch, save/restore, and display-window flows.
- `bridgevm-api` and `bridgevm-cli`: metadata-safe readiness evidence ingestion for preserved Apple VZ live evidence bundles through `readiness_report` and `bridgevm readiness --live-evidence`, without launching Apple VZ or invoking the Swift helper.
- `bridgevmd` supervisor: daemon-owned backend child registry for `run --spawn`, periodic exit reconciliation, QMP/guest-tools reconciliation for Compatibility Mode, and stop/shutdown cleanup.
- `bridgevm-api`: service contracts for daemon/socket callers.
- `bridgevm-lightvm` and `bridgevm-fullvm`: separate engine adapters.
- `bridgevm-agent-protocol`: first typed guest-tools messages.
- `bridgevm-agentd`: host-side guest-tools session policy and authenticated
  `GuestHello` validation.
- `bridgevm-api` and `bridgevm-cli`: guest-tools policy/status and explicit
  `GuestHello` acceptance requests backed by `bridgevm-agentd`.
- `bridgevm-api`, `bridgevm-cli`, and the macOS client: daemon-backed
  guest-tools command-send typed boundary for already-formed agent envelopes and
  safe alpha inline file-drop plus application/window list, launch, focus, and
  close dispatch.
- `bridgevm-cli`: diagnostics bundle export for VM manifests, logs, and
  metadata with redaction and artifact exclusions.
- `bridgevm-cli`: metadata-only performance baseline export for VM state,
  runner metadata, guest-tools runtime metrics, and notes.

## Store Doctor Boundary

`bridgevm store doctor` is the metadata-store readiness check used by the CLI,
daemon, and macOS dashboard sidebar. It reports the configured store root,
`vms` directory path, and readiness status, creating or confirming the
directory structure needed for VM bundle discovery. The same information is
available through the daemon `store_doctor` request, so the dashboard can show
store/source health before or alongside inventory refresh. Store Doctor is not a
VM repair, migration, disk, network, or backend-start action: it does not modify
VM manifests, create disks, repair bundle metadata, connect to QMP, attach guest
tools, or launch a VM.

## Disk preparation boundary

`disk prepare`, `prepare-run`, and runner startup now call `VmStore::prepare_primary_disk`. The method resolves the manifest's primary disk path, creates the containing directory, and writes `metadata/primary-disk.json`. `disk prepare` prints that result directly without building a runner plan or running `qemu-img`. Runner metadata also carries the disk preparation result so callers can see the path, format, size, existence state, whether BridgeVM created anything, and any suggested creation command.

Preparation creates missing `raw` primary disks directly, using a sparse file sized from the manifest. For `qcow2` and other non-raw formats, preparation does not create, inspect, repair, or compact the image during planning; those operations stay behind explicit commands. That boundary keeps disk planning side-effect-light: if the file is missing, BridgeVM records a command of the form:

```bash
qemu-img create -f <format> <path> <size>
```

`disk create` is the explicit execution path for that recorded command. It runs `qemu-img create -f <format> <path> <size>` for a missing non-raw primary disk, reports an already-ready result when the disk already exists, and fails if `qemu-img` is missing or exits unsuccessfully. This is a disk-file creation boundary only: it does not start QEMU, launch Apple VZ, or start a VM. Raw disks usually have nothing extra to create because preparation already made the sparse file.

`disk inspect` is the explicit inspection path after creation. For an existing non-raw primary disk, it runs:

```bash
qemu-img info --output=json <path>
```

The command's JSON output is printed for callers and recorded as the latest inspection in `metadata/last-disk-inspect.json`. Inspection can fail safely when the primary disk is raw, the disk file is missing, `qemu-img` is unavailable, or `qemu-img info` exits unsuccessfully.

`disk verify` is the explicit check boundary for an existing non-raw active disk:

```bash
cargo run -p bridgevm-cli -- disk verify <vm>
```

The command uses `qemu-img check --output=json` against the selected active
disk, records the parsed report in `metadata/last-disk-verify.json`, and does
not rewrite disk or snapshot-chain state. It fails safely when the selected disk
is raw or missing, when `qemu-img` is unavailable, or when the check command
fails.

`disk compact` is the explicit compaction boundary for an existing non-raw active disk:

```bash
cargo run -p bridgevm-cli -- disk compact <vm>
```

The command uses `qemu-img convert` to write a compacted replacement image, renames the current active disk to a `.precompact-<timestamp>` backup path, replaces the active disk with the compacted image, and records the latest attempt in `metadata/last-disk-compact.json`. It fails safely when the selected disk is raw or missing, when `qemu-img` is unavailable, or when conversion/replacement fails.

`metadata repair` is the conservative metadata-only repair boundary:

```bash
cargo run -p bridgevm-cli -- metadata repair <vm>
```

The command recreates missing repairable metadata from the manifest and
snapshot list, refreshes existence flags, and reports the bundle path,
timestamp, repaired/no-op status, and action list. The same
`RepairMetadata` request is available through the daemon. The macOS dashboard
repair action should show that daemon result directly and must not claim to
create disks, replace corrupt JSON files, invent runner history, or repair a
live backend.

`metadata migrate-manifest` is the conservative schema-upgrade boundary:

```bash
cargo run -p bridgevm-cli -- metadata migrate-manifest <vm> --dry-run
cargo run -p bridgevm-cli -- metadata migrate-manifest <vm>
```

The current `bridgevm.io/v1` schema migrates as a no-op: dry-run validates
without writing, while execution records `metadata/manifest-migration.json` and
backs up the manifest to `metadata/manifest-before-migration.yaml`. Future
schemas and malformed YAML are rejected before receipts or backups are written.
The same `MigrateManifest` request is available through the daemon socket.

Compatibility Mode spawn uses this metadata as a gate. Dry-run preparation can complete with a missing `qcow2` disk, but `run --spawn` refuses to start QEMU until the primary disk exists and reports the recorded `qemu-img create` command.

The macOS dashboard Storage Maintenance panel exposes the same daemon-backed
sequence: prepare records primary-disk metadata and the suggested command,
Create Disk runs the explicit `qemu-img create` disk-file boundary, inspect reads
`qemu-img info` metadata, verify checks the active disk, and compact performs the
explicit non-raw active-disk replacement flow. These dashboard controls surface
the same metadata and command results as the CLI/socket API; they do not start
QEMU, launch Apple VZ, or start a VM.

## Metadata-only delete boundary

Dashboard delete uses the non-destructive tombstone path:

```bash
cargo run -p bridgevm-cli -- delete <vm> --metadata-only
```

The local CLI and daemon socket path both refuse running VMs before writing any
delete metadata. For stopped or suspended VMs, metadata-only delete preserves
the `.vmbridge` bundle, `manifest.yaml`, disks, boot media, logs, and existing
metadata. It writes `metadata/deletion.json` as the tombstone/audit record and
`metadata/deleted-manifest.yaml` as the manifest snapshot, then list operations
hide the VM from normal inventory results. This gives the dashboard a safe
default that cannot accidentally destroy disk images or installer media while
still making deleted VMs disappear from the main VM list.

Destructive bundle removal remains a separate explicit storage/CLI boundary for
developer cleanup or future advanced flows. It should not be wired to dashboard
delete controls without an additional confirmation and recovery story.

This means the current safe path is:

1. `disk prepare` prepares directories and records primary disk metadata without touching runner state.
2. `disk create` explicitly runs `qemu-img create` for a missing non-raw disk, while keeping `disk prepare` as a safe command-reporting path.
3. `disk inspect` explicitly runs `qemu-img info --output=json` for an existing non-raw disk and records inspection metadata.
4. `disk verify` explicitly runs `qemu-img check --output=json` for the selected non-raw active disk and records check metadata without changing storage state.
5. `disk compact` explicitly runs `qemu-img convert`, keeps a `.precompact-<timestamp>` backup, replaces the active disk, and records compaction metadata.
6. `metadata repair` repairs missing metadata only and reports any no-op/action details without creating disks or replacing corrupt JSON.
7. `delete --metadata-only` tombstones a stopped VM while preserving its bundle and hiding it from inventory.
8. `prepare-run` builds a dry-run runner plan after the same disk preparation step.
9. `runner-status` exposes the latest runner and disk preparation state, including the suggested `qemu-img create` command when one is needed.
10. `run --spawn` starts QEMU only when the selected active disk already exists.

## Resource profile boundary

Fast Mode and Compatibility Mode now share the same resource profile resolver before their runner-specific plans are emitted. The resolver maps the manifest's `resources.profile` to deterministic default memory, vCPU, display FPS cap, and rationale values, and Fast Mode launch planning can use the current host battery state when `memory` or `cpu` are `auto`. Runtime reapply remains a metadata-backed policy signal until a live Apple VZ/display control channel consumes it.

Manifest `resources.memory` and `resources.cpu` values are only replaced when they are set to `auto`. Explicit memory or CPU values in the manifest are preserved and passed into the Fast Mode Apple VZ plan or Compatibility Mode QEMU arguments. Fast Mode also exposes the selected `display_fps_cap` and `rationale` in its launch spec so callers can inspect why the automatic values were chosen.

Running Fast Mode VMs can also receive a metadata-backed runtime policy signal through `bridgevm resources reapply <vm> --visibility foreground|background` or the matching daemon request. That command requires running Fast Mode runner metadata, re-reads the host battery state, applies the foreground/background policy, and records `metadata/runtime-resources.json` with the chosen memory, CPU, display FPS cap, rationale, and `live_apply_blockers`. Until a live Apple VZ/display control channel exists, the record is honest: `live_applied` is `false` with `runtime-control-unavailable`, so it is a UI/display pacing contract rather than CPU or memory hot-plug.

## Compatibility Mode port forwards

Compatibility Mode port forwarding is a manifest-editing boundary. The CLI
commands are:

```bash
bridgevm port list <vm>
bridgevm port add <vm> <host:guest>
bridgevm port remove <vm> <host:guest>
```

`port list` reads `network.forwards`, `port add` appends a host-to-guest pair,
and `port remove` removes the matching pair from the VM manifest. The typed API
and daemon socket expose the same operations, so callers do not need to edit
manifest YAML directly. QEMU Compatibility Mode command generation already
renders manifest forwards as `hostfwd` options. Because the commands update the
manifest rather than a live QEMU process, the new forwarding set is consumed by
subsequent runner plans, `qemu-args`, dry-run metadata, and later spawns.

## Network Planning Boundary

`bridgevm-network` now owns the shared planning scaffold for backend networking.
It can produce a `NetworkPlan` for Apple VZ or QEMU with a selected mode,
hostname, validated port-forward rules, capability flags, and human-readable
notes. The typed API now uses that planner when adding manifest port forwards,
so duplicate host ports, zero-valued ports, unsupported network mode names, and
port-forwards outside NAT are rejected before the manifest is rewritten. Fast
Mode Apple VZ preflight and Compatibility Mode QEMU command generation also
consume the same planner: Apple VZ still accepts only NAT, while QEMU maps NAT
to user networking with `hostfwd` entries and maps isolated networking to
`restrict=on`. Host-only and bridged modes remain explicit unsupported launch
boundaries until their backend wiring exists.

`bridgevm network-plan <vm>` and the daemon `PlanNetwork` request expose the
manifest-derived plan for a VM without performing live networking. `networkd`
exposes the same planner as a public runner CLI:
`cargo run -p networkd -- --print-plan` emits the JSON `NetworkPlan` for
selected backend, mode, hostname, forwards, capability flags, and notes;
omitting `--print-plan` emits only a concise readiness summary. These CLIs
reject malformed forward syntax, zero-valued ports, forwards outside NAT, and
unsupported backend/mode combinations such as Apple VZ bridged networking before
any backend runner can consume the plan. This boundary is metadata-only: it does
not start QEMU, launch Apple VZ, create or attach host-only/bridged interfaces,
modify live port forwarding, or start a VM.

`bridgevm ssh <vm> [--user USER]` is also a planning boundary. It does not start
an SSH client. For Compatibility Mode, the planner first looks for a manifest
forward with guest port `22` and emits `ssh -p <host> USER@127.0.0.1`. If that
is unavailable, a connected guest-tools runtime with a valid guest IP can emit
`ssh USER@<guest-ip>`. The macOS dashboard surfaces this same SSH plan metadata:
command, target, port, and source only. That surface must not execute `ssh`,
start an SSH process, connect to the guest, start a backend, open a browser, or
start a VM.

The dashboard open-port and SSH plan surfaces belong to this same
metadata/planning boundary. They may present daemon-derived plans for forwarded
guest ports and SSH targets, such as the host port, URL, command, target, and
source BridgeVM would use, but they must stay honest about what happened: no
browser is opened by these status surfaces, no SSH process is started, no
network connection is attempted, no live port-forward is modified, no backend is
started, and no VM is started.

The macOS dashboard also surfaces the daemon `network_plan` response directly:
backend, mode, hostname, dry-run and executable flags, capability flags,
blockers, notes, and planned port forwards. It also surfaces the manifest
port-forward list/add/remove boundary through the daemon, so it edits the
recorded policy consumed by later plans rather than attaching to QMP or
modifying a running backend.

VM shared-folder approval uses the same manifest-editing pattern as port
forwarding, though unlike port forwarding it is also part of guest-tools policy
for Fast Mode and Compatibility Mode VMs. The CLI commands are:

```bash
bridgevm share list <vm>
bridgevm share add <vm> <name> <host-path> [--read-only] [--host-path-token <token>]
bridgevm share remove <vm> <name>
```

`share list` reads the manifest `sharedFolders` approval list, `share add`
records an approved host path, read-only flag, and opaque host path token, and
`share remove` removes the named approval entry. The typed API, daemon socket,
and macOS dashboard share-management UI expose the same manifest policy
boundary so callers do not edit YAML directly. These operations do not mount a
folder in a running guest, create a guest filesystem path, or modify an
already-running backend. A live guest mount remains a separate guest-tools
action: after an authenticated session exists, `guest-tools mount-share`
resolves a manifest-approved share name to its opaque host path token and sends
that protocol command to the guest tools stream.

## Fast Mode boot contract

Fast Mode now has a dry-run boot/install media contract for the default CLI, daemon, and Rust runner path, separate from the narrow Swift helper launch path. Manifests may include an optional `boot` section with `existing-disk`, `linux-installer`, `linux-kernel`, or `macos-restore` mode. `bridgevm-core` recommends default metadata-only boot templates for supported Fast Mode guests, the CLI and daemon socket can list those templates without downloading media, `bridgevm create <name> --template <id>` can fill omitted guest OS, arch, and boot media metadata from a chosen template, and explicit OS/arch creates still apply matching templates when no explicit boot flags are supplied. `bridgevm-apple-vz` validates the guest/mode combination, required fields, and empty path inputs at the `build_fast_plan` boundary. A launch-readiness/preflight gate sits after this dry-run plan and before any default process launch attempt so missing boot media, missing disks, unsupported disk formats, and unsupported Apple VZ host/guest/backend combinations become structured blockers rather than implicit launch failures. Host launch blockers are part of the same spec: a non-macOS host reports `unsupported-host-os`, and a non-Apple-Silicon host reports `unsupported-host-arch`, both as capability blockers rather than path blockers. That readiness metadata belongs to the same runner boundary as the dry-run launch spec: local and socket-backed `prepare-run`, Fast Mode `run` without spawn, and daemon `runner-status` expose the same RunnerStatus readiness state. Those dry-run paths write the launch spec artifact to `.vmbridge/metadata/apple-vz-launch.json` and record the same path in `metadata/runner.json` as `launch_spec_path`.

The aggregate Rust readiness report also carries an optional
`pre_run_launch_readiness` field. This field is the same metadata-safe
preflight result computed before a runner is launched, but it is available even
when no runner metadata exists yet. CLI callers should render it as a
`Pre-run launch readiness:` section only when runner metadata is absent, and
the macOS dashboard may use the decoded field as a UI fallback for newly
created or not-yet-prepared Fast Mode VMs. The field is deliberately not live
evidence: computing or displaying it must not start QEMU, launch Apple VZ,
spawn a runner, perform a QMP handshake, attach guest tools, mutate networking,
or start a VM.

The macOS dashboard creation wizard is now on that same daemon boundary. Its creation sheet asks `bridgevmd` for `list_templates`, lets the user choose a boot template, and sends `create_vm` to create a stopped `.vmbridge` bundle. It does not choose installer media itself. After a template-backed VM exists, the intended next flow is boot media status, local import, verification, download-plan metadata, or planned download execution.

Relative boot media paths are resolved against the `.vmbridge` bundle and emitted in `AppleVzLaunchSpec` with an `exists` flag. `bridgevm boot-media <vm>` exposes that boot slice directly, including installer, Linux kernel/initrd, or macOS restore paths and their existence states, without downloading media or requiring callers to inspect the full runner-plan JSON. The same inspection is available through `bridgevm --socket <sock> boot-media <vm>`. `bridgevm media import <vm> --source <path>` is the local media boundary for this contract: it uses the dry-run plan's expected path, copies a caller-provided installer, kernel, initrd, or macOS restore file into the `.vmbridge` bundle so the next plan reports `exists: true`, and records import metadata under `.vmbridge/metadata/boot-media/<kind>.json`. It deliberately does not download, select, or verify remote OS media. The same import path is available through `bridgevm --socket <sock> media import <vm> --source <path>`. `bridgevm media status <vm>` inspects the resolved Fast Mode boot media entries after planning/import: each installer, kernel, initrd, or macOS restore entry includes its resolved path, existence state, file size, latest import metadata, latest verification result, and latest download plan when present. The same status inspection is available through `bridgevm --socket <sock> media status <vm>`.

Download intent is another metadata boundary, separate from network transfer. `bridgevm media download-plan <vm> --url <https-url> [--sha256 <hex>]` records the caller-provided remote media URL, resolved bundle destination, optional expected SHA-256, current file existence and size, and latest import/verify state under `.vmbridge/metadata/boot-media/<kind>-download.json`, then prints the same plan. It records what BridgeVM should fetch later; it does not fetch the URL. Boot modes with multiple media paths require `--kind installer-image|kernel|initrd|macos-restore-image` to select the planned destination. The same no-download planning path is available through `bridgevm --socket <sock> media download-plan <vm> --url <https-url> [--sha256 <hex>]`.

`bridgevm media download <vm>` is the explicit execution boundary for the recorded download plan. It re-resolves the current Fast Mode boot destination, requires the stored plan destination to match, downloads the planned URL to a temporary file, checks the optional planned SHA-256 before replacing the destination, and records success or failure under `.vmbridge/metadata/boot-media/<kind>-download-result.json`. The same planned-download execution path is available through `bridgevm --socket <sock> media download <vm>`. It does not choose OS media or invent a URL; it only executes the already recorded plan.

SHA-256 verification is a second local metadata boundary, not a download policy. `bridgevm media verify <vm> --sha256 <hex>` hashes the already-resolved Fast Mode boot media file, compares it with the caller-provided expected digest, and records the success or failure under `.vmbridge/metadata/boot-media/<kind>-verify.json`. Boot modes with multiple media paths, such as Linux kernel mode, require `--kind installer-image|kernel|initrd|macos-restore-image` to select the file to verify. The same verification path is available through `bridgevm --socket <sock> media verify <vm> --sha256 <hex>`. That keeps template and OS-download flows plannable without pretending the default VZ launcher starts a VM: `boot-media`, `media import`, `media status`, `media download-plan`, `media download`, `media verify`, `prepare-run`, Fast Mode `run` without spawn, `runner-status`, `lightvm-runner --print-plan`, `lightvm-runner --require-ready`, and `lightvm-runner --launch-spec <path> --print-handoff` can prove which media the Apple VZ boundary should consume and whether the current dry-run launch inputs are ready or blocked. The launch-spec handoff path reads the persisted artifact instead of rebuilding from a manifest, so the Apple VZ backend has a stable artifact-consumer boundary.

Real Apple VZ process launch now exists at the Swift helper boundary for supported `linux-kernel` Fast Mode VMs with a `raw` primary disk and NAT networking, and it remains guarded by explicit helper configuration and live-start opt-in. Local and socket-backed planning commands still stop at metadata creation, media readiness, verification, recorded-plan download execution, and structured launch-readiness/preflight reporting; `bridgevm run <vm> --spawn`, daemon-backed Start, suspend, and resume cross into the signed `AppleVzRunner` only when `BRIDGEVM_APPLE_VZ_RUNNER` and the required opt-in are present. Without that helper boundary, the legacy dry-run/not-implemented behavior remains. Manual live E2E can pass `--apple-vz-stop-after-seconds <N>` so the helper requests guest shutdown after startup, and the SwiftPM-built helper must be signed after build with `apps/macos/AppleVzRunner.entitlements` or an equivalent virtualization entitlement. The separate display path (`bridgevm display <vm>` and the app's Show Display action) launches the helper in a local GUI session with Apple VZ graphics; the headless launch and save/restore path remains separate because a graphics device disables VZ save/restore.

## Snapshot chain scaffold

The regular snapshot flow records logical snapshot metadata and runtime state. Disk snapshots add a second metadata layer for Compatibility Mode storage planning: `snapshot create <vm> <name> --kind disk` writes `metadata/snapshot-disks/<name>.json` alongside the existing snapshot metadata.

That disk snapshot metadata is a qcow2 chain plan with a separate execution boundary. It is derived from the current active disk record and captures the backing file, backing format, planned overlay path, overlay existence state, and the command BridgeVM can run to create the overlay:

```bash
qemu-img create -f qcow2 -F <backing-format> -b <backing-file> <overlay>
```

This keeps the snapshot path aligned with `disk prepare`, `disk create`, and `disk inspect`: BridgeVM records exact storage intent first, then `snapshot disk-create <vm> <name>` executes the recorded overlay command behind an explicit boundary. The execution path fails safely when the backing disk is missing, when `qemu-img` is unavailable, or when `qemu-img create` exits unsuccessfully. Creation attempts are recorded under `metadata/snapshot-disks/<name>-create.json` or equivalent metadata. `snapshot chain <vm>` inspects the recorded chain metadata and reports the active disk source, snapshot name when present, and selected disk path. Successful overlay creation also updates `metadata/active-disk.json`, and `qemu-args`, runner planning, and spawn resolve the manifest through that active disk record before building QEMU arguments.

Suspend snapshots add a separate planned-image metadata boundary. `snapshot create <vm> <name> --kind suspend` writes `metadata/suspend-images/<name>.json` with the planned image path `suspend-images/<name>.bin`, the image format marker, image existence state, and preparation timestamp. This records where a future suspend image should live, but it does not serialize guest memory yet. Restoring a suspend snapshot requires only that planned image marker to exist, updates the image existence state, restores the recorded runtime-state metadata, and writes the image metadata into `metadata/last-restore.json` as `suspend_image`. The CLI prints the suspend image path, format, readiness, and preparation timestamp when that metadata restore succeeds; no memory is deserialized and no guest is resumed.

The macOS dashboard belongs to this same daemon-backed metadata boundary. Its
snapshot view lists recorded snapshots, kinds, runtime-state metadata,
disk-chain status, suspend-image readiness, the latest restore record, and the
latest snapshot disk-create result metadata. The disk-create action is an
explicit `qemu-img create` overlay creation boundary: it records the command,
stdout/stderr, exit status, creation time, and resulting disk-chain metadata,
but it does not start a VM, start QEMU, or launch Apple VZ. A restore action
must be presented as restoring recorded metadata only. It must not imply real
guest memory restoration, live VM rollback, or full application-consistent
restore until those backend paths exist.

The current implementation can create overlays, inspect the active chain member, route startup through that active disk, gate suspend snapshot restore on a recorded image file, record application-consistent snapshot preflight metadata from guest-tools runtime state, and execute daemon-owned application-consistent freeze/thaw around snapshot creation when a connected guest advertises the required capabilities. Disk snapshot restore rewinds the active disk to the selected snapshot's backing image and restores the snapshot's recorded runtime state. The safe fsfreeze backend smoke shadows `fsfreeze` with a fake executable to prove allowlisted command ordering and thaw rollback; the separate opt-in live smoke proves the real QEMU/HVF guest filesystem freeze/thaw boundary on a safe loopback ext4 mount. Remaining snapshot work includes real suspend memory serialization/restoration and higher-level application quiescing for complete application consistency.

## Lifecycle control boundary

`bridgevm lifecycle-plan <vm> --action suspend|resume` remains the metadata-only
readiness surface for UI/API command planning, but `bridgevm suspend <vm>` and
`bridgevm resume <vm>` are now real backend operations when their backend
requirements are satisfied. Fast Mode uses Apple VZ save/restore through the
signed helper and is the supported suspend/resume path. Compatibility Mode
suspend uses QMP `snapshot-save` plus process shutdown, while resume attempts
`-loadvm bridgevm-suspend` and preserves the suspend marker when the known Apple
Silicon HVF restore failure appears.

The plan command reports the current state, target state, backend boundary,
planned Compatibility Mode QMP command (`stop` for suspend and `cont` for
resume), QMP socket path availability, and blockers without opening the socket
or performing a QMP handshake. For Fast Mode it reports the Apple VZ runner
boundary and returns a concrete `apple-vz-runner-unavailable` blocker until
`BRIDGEVM_APPLE_VZ_RUNNER` points at the signed Swift helper. The macOS
dashboard lifecycle controls should present this plan and its blockers honestly
before invoking the daemon suspend/resume requests.

## Guest tools policy boundary

Guest tools now have a daemon-owned Compatibility Mode transport/status
boundary while remaining an alpha guest integration surface. VM creation writes
a per-VM tools token under `.vmbridge/metadata/guest-tools-token.json`.
`bridgevm guest-tools status <vm>` derives the allowed capability policy from
the VM manifest's integration settings, plus base liveness, guest IP, time sync,
and diagnostic metrics capabilities. `bridgevm guest-tools token <vm>` exposes
the scaffold developer token, and `bridgevm guest-tools accept-hello <vm>
--hello-json <json>` validates a supplied `GuestHello` envelope against that
stored token and policy through `bridgevm-agentd`. The same requests work
through `bridgevm --socket <sock> ...`, and the daemon-owned Compatibility Mode
backend uses the tested virtio-serial socket session boundary when a spawned
guest tools scaffold connects. The integration smoke suite locks that boundary
with local and socket-backed token/status, Linux command rendering, valid
`GuestHello` acceptance, wrong-token rejection, disallowed-capability
rejection, and a check that the generated Linux tools argv points at the token
metadata file rather than embedding the token value. The raw `guest-tools
send-command` path and the macOS client's typed `guest_tools_send_command`
request/response DTOs sit on the same daemon boundary: they carry an
already-formed agent envelope to the Compatibility Mode backend and report
command tracking metadata without making the dashboard a guest-tools transport
owner.
Provisioning is still not automatic guest installation: BridgeVM records the
host-side token metadata, status, generated Linux scaffold command, and daemon
transport readiness, but it does not copy the agent into the guest, install a
service, rotate credentials inside the guest, or perform signed agent updates.

Compatibility Mode runner planning now also exposes the first transport
attachment point. QEMU command generation adds a virtio-serial port named
`org.bridgevm.guest-tools.0` backed by
`.vmbridge/metadata/guest-tools.sock`. `metadata/runner.json` records the
transport, channel name, socket path, token metadata path, and token creation
time, but keeps the token value out of the QEMU command line. When `bridgevmd`
supervises a spawned Compatibility Mode backend, it connects to that socket,
authenticates the first `GuestHello` through `bridgevm-agentd`, and keeps the
stream open. The daemon now drains bounded guest-origin frames for heartbeat,
guest IP, and guest metrics updates into
`.vmbridge/metadata/guest-tools-runtime.json`, and `guest-tools status` reports
that runtime state. Daemon-owned backends also accept
high-level `guest-tools set-clipboard`, `guest-tools resize-display`, and
`guest-tools time-sync` requests over the BridgeVM socket, plus shared-folder
`bridgevm --socket <sock> guest-tools mount-share <vm> --name
<sharedFolders.name> [--request-id <id>]` and `bridgevm --socket <sock>
guest-tools unmount-share <vm> --name <sharedFolders.name> [--request-id
<id>]` requests. For the normal approved-share UX, the share name comes from
the VM manifest's `sharedFolders.name` field. The CLI/API resolves that name to
the host-approved `hostPathToken`, or to the deterministic token derived from
the manifest entry, and then dispatches `MountShare { name, host_path_token }`
to the guest-tools stream. Users do not need to handle raw tokens for approved
manifest shares. The `host_path_token` is a host-issued opaque identifier for a
path approved by the daemon-side shared-folder registry; it is not a path
serialization and is not resolved by guest tools. A raw token form may remain
as a developer/debug path, but it is not the primary mounted-share UX. These
commands authorize against the authenticated session, write to the guest-tools
stream, and track pending `request_id` values until matching `CommandResult`
frames arrive. The daemon persists the latest matching result in runtime
`last_command_result` so status surfaces can show the last request ID,
capability, success/error fields, and completion timestamp without pretending
to maintain a full command history. `SetClipboard` and display resize now have
real Linux effect paths when the guest environment provides the required desktop
tools: the agent can auto-detect `wl-copy`/`xclip` and `xrandr`, and the opt-in
headless live smoke verifies host-to-guest clipboard text plus resize command
dispatch through Xvfb/xclip/xrandr. The same protocol still supports simulated
clipboard/status frames for socket-safe testing. `FileDropStart`,
`FileDropChunk`, and `FileDropComplete` are an inline drag-drop alpha command
sequence: the macOS dashboard can dispatch them through
`guest_tools_send_command` only when daemon/backend status advertises drag-drop
capability/readiness, and it should surface request IDs, pending counts, and
latest `CommandResult` metadata. That sequence is not proof of a completed
host-to-guest filesystem drop, does not create or mount a guest filesystem
path, and does not replace future durable file-transfer/storage plumbing.
Shared-folder `MountShare { name,
host_path_token }` and `UnmountShare { name }` are likewise alpha protocol
paths: the Linux scaffold updates transient in-memory share state and can reply
with `CommandResult` for matching `request_id` values, but it does not perform a
Linux mount or create a guest filesystem path. The shared-folder runtime
status/list surface belongs to this same daemon-owned guest-tools state: it may
show which share names and opaque host path tokens are currently recorded for
the authenticated session, along with host-only approval metadata such as the
manifest entry, approved host path, approval time, and approval source. That
host path metadata is for operator status/debugging and stays on the host side
of the boundary:
guest tools receive only `name` and `host_path_token` and must not interpret a
host path directly. The surface must stay framed as alpha in-memory session
state rather than durable configuration or an OS mount table. `TimeSync`
command dispatch now has live fake-socket smoke coverage, and command tracking
rejects duplicate pending IDs plus stray `CommandResult` frames without
treating them as successful completion. `AgentUpdateAvailable` is currently a
signed-update readiness protocol and capability metadata message only; it is
authorized by the `agent-update` capability when
`security.signedAgentUpdates` is true, then recorded in
`metadata/guest-tools-runtime.json` with current version, available version,
URL, signature, and observed timestamp for `guest-tools status` to report. It
does not download, install, execute, mutate, or auto-update guest tools.
`guest-tools send-command`
remains available for raw envelope testing. The same connected runtime guest IP can be used by
`bridgevm ssh <vm> [--user USER]` as a metadata-only SSH command plan when no
Compatibility Mode port-forward plan applies.

Application and window metadata now sit on the same alpha protocol boundary.
The Linux scaffold may advertise `applications` and `windows` in its default
development capability set, return static or in-memory entries for
`ListApplications` and `ListWindows`, and acknowledge launch, focus, and close
commands with `CommandResult` after validating the requested ID against
scaffold state. These paths exercise capability authorization, request
correlation, and daemon command plumbing only: they do not enumerate installed
Linux applications, launch a process, inspect a real window manager, focus an OS
window, or close a desktop window.

The macOS dashboard consumes this same daemon-owned status boundary for its
guest tools panel. It can show the manifest-derived capability policy,
authenticated runtime state, guest IP, heartbeat, guest metrics, passive
agent-update availability metadata, and shared-folder alpha session entries
when present in `guest-tools status`, including host-facing path/token/approval
metadata for approved shares, so users can see whether the backend has enough
readiness metadata for integration-dependent flows. It may also surface
`last_command_result` as a
status breadcrumb for the most recent correlated command. Its client can model
the daemon-backed `guest_tools_send_command` boundary for safe alpha command
surfaces, including clipboard text, display resize, inline file-drop
start/chunk/complete, manifest-approved shared-folder mount/unmount,
list-applications, launch-application, list-windows, focus-window, and
close-window actions. The dashboard's share list/add/remove controls sit one
step earlier and edit only the durable manifest approval list. Inline file-drop
controls are safe alpha guest-tools command dispatch surfaces: they must be
gated by drag-drop capability/readiness and result metadata such as request IDs,
pending counts, and latest `last_command_result`, not by assumptions that a
file reached a guest filesystem. App/window controls use the same safe alpha
pattern and must be gated by daemon/backend readiness plus result metadata such
as request IDs, pending counts, and latest `last_command_result`. Those UI actions are protocol/status plumbing only:
they do not prove a real guest desktop clipboard, display mode, host-to-guest
filesystem drop, mounted guest filesystem, guest filesystem mount, app, or
window changed. Transport attachment, command authorization, command delivery,
latest command-result metadata, passive agent-update notice recording,
application/window scaffold entries, and the transient shared-folder session
list remain owned by the Compatibility Mode daemon backend, while Fast Mode
launch remains outside the dashboard and default daemon path except for the
narrow `AppleVzRunner` helper handoff.

## VM bundle export/import boundary

`bridgevm export <vm> --output <path>` and
`bridgevm import <bundle> [--name <name>]` are whole-bundle copy boundaries for
portable `.vmbridge` directories. They intentionally copy only regular files and
directories, rejecting symlinks and special files instead of dereferencing or
recreating them. Export refuses to write at or below the source bundle, import
refuses source/destination self-copies and bundles already inside the destination
store, and existing destination bundles fail before any copy starts. The same
storage checks are exposed through `BridgeVmRequest::ExportVm` and
`BridgeVmRequest::ImportVm`; local CLI errors add command context while keeping
the underlying storage reason visible for callers.

The macOS dashboard should surface export/import through that same daemon
request boundary instead of creating a UI-owned bundle copier. Dashboard export
may present the source VM bundle, output path, chosen directory-or-tar format,
and copied metadata/file summary. Dashboard import may present the input bundle,
destination VM name, optional manifest identity and hostname rewrite, output
bundle path, and copied metadata/file summary. Preserved portable state includes
the manifest and metadata that the Rust boundary already copies, such as
snapshot records, port forwards, and approved shared folders when present. This
is still a stopped-bundle copy/import operation: it must not start a VM, connect
to QMP, attach guest tools, copy live sockets, copy lock files, or include
disk/media artifacts beyond the existing Rust CLI/API behavior. Preserving
snapshot metadata does not promise live guest state migration, memory restore,
or application-consistent restore.

## VM clone boundary

The macOS dashboard clone sheet uses the same daemon `clone_vm` request boundary
as the CLI/socket API and presents full and linked clone options. Full clone is
the stopped-bundle copy path. Linked clone is the explicit qemu-img overlay
creation boundary: the daemon/storage response records whether the clone is
linked, the backing path, backing format, clone output, and the `qemu-img create`
command metadata that created or would create the overlay. The dashboard
surfaces that metadata so users can distinguish a portable full clone from a
backing-disk-dependent linked clone. Neither clone option starts QEMU, launches
Apple VZ, connects QMP, attaches guest tools, or starts a VM.

## Diagnostics bundle boundary

`bridgevm diagnostics bundle <vm> --output <dir>` is the current support bundle
boundary. It copies the VM's `manifest.yaml`, `logs/`, and `metadata/` into the
requested output directory and writes a `diagnostic-bundle.json` summary for the
bundle contents. It deliberately excludes disk images, imported installer or
restore media, sockets, and lock files so diagnostics stay small and do not
capture live runtime endpoints. It can still preserve already-recorded runner
status, launch-readiness blockers, Apple VZ dry-run launch specs, and live
evidence/verifier status artifacts, which helps support readers see why a VM is
not launch-ready or what evidence has been prepared without requiring a live
backend. Before writing JSON into the bundle, BridgeVM redacts the guest-tools
token, sensitive JSON keys, and URL query strings. The same boundary is exposed
through `BridgeVmRequest::CreateDiagnosticBundle`, so socket-backed daemon
callers get the same redacted support bundle as the local CLI path. The macOS
dashboard consumes that same daemon response in its Diagnostics & Performance
panel by presenting the source bundle, output directory, creation timestamp,
and copied file list. It keeps the same artifact boundary as the CLI: creating
a diagnostics bundle does not start a VM, connect to QMP, attach guest tools,
or copy live sockets and disk/media files.

## Performance baseline boundary

`bridgevm performance baseline <vm> --output <dir>` records a metadata-only
baseline artifact for the current VM. It does not run a benchmark, boot or
resume the guest, generate I/O, inspect frame timing, or collect new host
telemetry. The command reads the state that already exists in the VM bundle and
writes a stable JSON artifact at
`<output>/bridgevm-performance-<vm>-<timestamp>/performance-baseline.json`.

The baseline includes the recorded VM state, runner metadata when available,
guest-tools runtime and guest metrics when available, derived metadata-only
measurement records such as runner observed uptime and guest CPU/memory
snapshots, and notes describing missing or unavailable inputs. This gives Fast
Mode and Compatibility Mode the same first performance baseline step: capture
the runtime/guest metrics BridgeVM already has before later work adds real boot,
resume, idle CPU, display, and disk I/O measurements.

`bridgevm performance sample <vm> --output <dir> [--artifact-bytes BYTES]
[--iterations N] [--sync]` adds the first execution-backed host-side
measurement path. It does not change the baseline boundary: `performance
baseline` remains metadata-only, while local/offline `performance sample` is the
bounded host-side probe path. It writes probe files into the sample artifact
directory and records per-iteration write latency, aggregate latency statistics,
bytes written, BridgeVM metadata read/status latencies, and total sample
generation duration in `performance-sample.json`. When the Compatibility Mode
disk inspection path is available and succeeds, the sample may also record
`disk_inspect_duration_microseconds`; this is scoped to the host-side
`qemu-img info` inspection duration and must not be interpreted as guest disk
I/O, disk throughput, or storage benchmark data. When the socket request is
served by a daemon that owns the running backend and has a connected
guest-tools session advertising `benchmark`, the daemon also dispatches the
guest-tools `RunBenchmark` command and appends bounded `guest_benchmark_*`
measurements to the same artifact. The default host probe is 1 MiB for one
iteration; each host iteration is capped at 64 MiB and total probe output is
capped at 256 MiB. The guest benchmark budget is capped by the guest-tools
protocol. `--sync` includes host probe-file `sync_data()` in each iteration's
measured write latency, so synced samples should be read as a different
host-write mode from unsynced samples.

The macOS dashboard surfaces both performance artifacts through the daemon as
metadata records: artifact path, source bundle, VM state, guest-tools status,
measurements, notes, per-iteration host write latency for samples, probe files,
and whether each measurement is metadata-only. It labels `performance baseline`
as metadata-only and distinguishes host-write probe measurements from optional
daemon-owned guest benchmark measurements. Neither surface proves guest boot,
resume, frame timing, or application workload performance.

Compatibility Mode daemon supervision records the latest bounded QMP drain in
`metadata/qmp-supervisor.json` when a reconcile tick observes QMP events, a
terminal event, or a drain limit. Diagnostics bundles include that metadata file
through the normal metadata copy path. The artifact is intentionally a latest
snapshot so support bundles stay bounded; it is not a full QMP event history.
Runner/status/readiness diagnostics may render this cached supervisor metadata
so operators can inspect the most recent daemon-observed QMP drain without
opening a direct QMP connection. That cache remains metadata-safe diagnostic
state only: it does not prove live console output, guest boot progress, or
current guest responsiveness.

The macOS Console action now uses the daemon's `qmp_status` boundary to inspect
Compatibility Mode QMP socket readiness and present a truthful backend-control
diagnostic. The dashboard Logs panel uses the same `view_logs` API exposed by
`bridgevm logs qemu <vm>` and `bridgevm logs serial <vm>` to read bounded tails
from `logs/qemu.log` and `logs/serial.log`, including path, byte-count, and
truncation metadata. These are client/API boundary checks only: they do not
create an embedded graphical console or prove guest display output. For
Compatibility Mode display, explicit VNC renderer planning now hands off to an
external viewer through QEMU `-display vnc=:0`; embedding that viewer in the
macOS app remains a future implementation target.

Compatibility Mode launch readiness now uses the same structured
`LaunchReadinessMetadata` shape as Fast Mode for metadata-safe dry runs and
aggregate readiness fallbacks. The pre-run report can expose missing active
disk and QEMU command-planning blockers before runner metadata exists, and
Compatibility Mode dry-run runner metadata records the same readiness object.
This is still a preflight boundary: reporting a QEMU launch blocker or a
blocker-free command plan does not mean QEMU was spawned or a guest booted.

Phase 0 readiness evidence should keep three unsatisfied proof categories
visible instead of folding them into metadata readiness. `live-boot` requires a
verified opt-in boot with observable guest progress, not just a launch-ready
spec or helper start/stop transcript; accepted progress evidence is a serial
sentinel or verifier-bound `boot-progress-evidence.json` graphical artifact.
`console` requires a real graphical console/viewer or equivalent display signal,
not QMP socket status or bounded log tails. `guest-tools-effects` requires observable guest-side effects from
guest-tools commands, not authenticated dispatch, command tracking metadata, or
latest command-result receipts alone. Until those live artifacts exist, these
requirements are required but unproven.

## Next implementation targets

- Replace the JSON socket transport with gRPC over a Unix domain socket.
- Wire the Apple VZ live Linux Arm64 boot path into the default daemon/product
  flow after the current opt-in Swift helper boundary has enough live smoke
  evidence.
- Replace process polling and request-time QMP checks with continuous QMP supervision for Compatibility Mode.
- Add a real embedded graphical console/viewer while preserving the current
  macOS QMP/log diagnostic path and Compatibility Mode external VNC handoff.
- Promote guest-tools command surfaces only after live evidence proves
  guest-side effects, not merely daemon dispatch or protocol tracking.
