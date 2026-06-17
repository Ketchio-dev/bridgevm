# BridgeVM

BridgeVM is an open-source virtualization app scaffold based on the two-engine plan in `PLAN.md`.

The current implementation is a Phase 0 foundation:

- Rust workspace with the planned crates and runner binaries.
- `bridgevm` CLI for VM creation, listing, mode recommendation, and diagnostics.
- SwiftUI macOS dashboard prototype with Settings daemon/store doctor status, a creation sheet backed by daemon boot templates, boot media readiness flow, daemon-backed Start through the `run_backend` launch boundary, lifecycle controls for opt-in live backends, VM card metadata for diagnostics/performance/export-import/open-port/SSH status, a clone sheet exposing full/linked clone options through the daemon API, metadata-backed snapshot list/restore/create surfaces, daemon-backed primary-disk prepare/create/inspect and active-disk verify/compact maintenance metadata, VM bundle export/import metadata/file-copy status, diagnostic bundle and performance artifact metadata panels, a metadata-only repair action, manifest-level port-forward list/add/remove controls, metadata/planning-only open-port, SSH, and network-plan visibility, manifest-level approved shared folder list/add/remove controls, daemon-backed guest tools provisioning and status/readiness visibility including the latest command result, a typed client boundary for safe alpha guest tools command dispatch including time sync and an inline file-drop command sequence, a Console button that opens the Compatibility Mode external VNC viewer handoff when available while reporting daemon QMP/socket diagnostics, and a Fast Mode Show Display action that launches the bundled Apple VZ display helper in the local GUI session.
- Readable `.vmbridge/manifest.yaml` VM bundles.
- Fast Mode and Compatibility Mode recommendation logic.
- Fast Mode Apple VZ dry-run launch spec planning with boot media status, local import, verification, download-plan metadata, planned media download execution, and structured launch-readiness blockers surfaced through runner metadata, prepare-run, run dry-runs, and daemon RunnerStatus, plus Compatibility Mode QEMU command planning and public `networkd` metadata-only network planning.
- Shared resource profile planning for Fast Mode launch specs and Compatibility Mode QEMU arguments.
- Primary disk preparation metadata, explicit disk image creation, disk inspection, active-disk verification/compaction, disk snapshot chain inspection, suspend snapshot image metadata, and Compatibility Mode spawn checks.
- Conservative metadata repair through the CLI/socket API, with the macOS dashboard showing the same repaired/no-op action summary.
- Conservative manifest migration through the CLI/socket API: current-schema manifests support dry-run and no-op receipt/backup metadata, while future schemas and malformed YAML are rejected before migration receipts are written.
- Metadata-only performance baseline artifacts and bounded host-side performance sample artifacts for Fast Mode and Compatibility Mode runtime/guest metrics.
- Guest tools policy/runtime/status metadata exposed through CLI/API and surfaced in the macOS dashboard as readiness status, including approved shared-folder policy entries, `last_command_result`, and passive `AgentUpdateAvailable` metadata when `signedAgentUpdates` allows the `agent-update` capability, plus daemon-backed safe alpha command surfaces for clipboard, display resize, inline file-drop start/chunk/complete dispatch, shared-folder mount/unmount, and application/window list/launch/focus/close requests, not as completed guest tools transport, real host-to-guest filesystem drop, real desktop control, mounted guest filesystem, real guest mounts, or a guest-tools update download/install/execute path.
- Scaffolded engine and integration crates, with metadata-safe planning, storage, network, resource, API, and guest-tools boundaries implemented while real live VM execution remains opt-in or future work.

## Readiness boundary

When the project is described as nearly complete for Phase 0, that means the
safe metadata and app-readiness surface is nearly complete: manifests, daemon
requests, dashboard panels, dry-run runner metadata, disk/media preparation
boundaries, diagnostics, and explicit operation receipts all report what
BridgeVM can safely know without starting a guest. The metadata-safe smoke suite
now also locks the aggregate readiness CLI contract, the live Apple VZ
opt-in default-skip boundary, and synthetic Apple VZ/QEMU evidence verifier
contracts. The remaining 3-5% is not more metadata polish; it
is the live end-to-end proof boundary. In practice, that means proving a real VM
boot path with explicit fixtures, host entitlements, backend process control,
and observable guest progress.

Default product and dashboard flows still keep real backend starts behind an
explicit opt-in. Local Fast Mode `run --spawn`, `suspend`, `resume`, and
`display` require `BRIDGEVM_APPLE_VZ_RUNNER` to point at a signed
`AppleVzRunner`; the lower-level helper still receives `--allow-real-vz-start`
from BridgeVM. Daemon/app Start also requires the live-start setting
(`BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1`) before the daemon will pass that helper
opt-in through. These live E2E paths can consume CPU, memory, disk, network, and
Apple virtualization resources, so they belong in manual opt-in smoke work with
known kernel/initrd/raw disk fixtures, not in the safe Phase 0 smoke lane. See
`docs/fast-mode/README.md` for the current manual live fixture path.

Readiness evidence is also tracked as a stricter future proof boundary. The
current metadata and synthetic verifier coverage can require the evidence
categories `live-boot`, `console`, and `guest-tools-effects`, but those
categories remain unproven until an opt-in run captures verified guest boot
progress, a real console signal, and observable guest-side effects from
guest-tools commands. BridgeVM treats serial sentinel evidence and verifier-bound
`boot-progress-evidence.json` graphical artifacts as guest boot progress
signals; QMP state and ordinary viewer artifacts can support console evidence
without proving live boot progress by themselves. The
preserved-evidence path for `guest-tools-effects`
should be treated as future/current only when an evidence bundle contains
guest-tools result artifacts that a verifier cross-checks; dispatch metadata,
pending counts, or `last_command_result` alone are not that proof. A readiness
report or passing metadata-safe smoke must therefore not be described as
satisfying those evidence requirements.

## Quick start

```bash
cargo run -p bridgevm-cli -- doctor
cargo run -p bridgevm-cli -- recommend --os ubuntu --arch arm64
cargo run -p bridgevm-cli -- templates
cargo run -p bridgevm-cli -- create ubuntu-template-dev --template ubuntu-arm64-installer
cargo run -p bridgevm-cli -- boot-media ubuntu-template-dev
cargo run -p bridgevm-cli -- media import ubuntu-template-dev --source ~/Downloads/ubuntu-arm64.iso
cargo run -p bridgevm-cli -- boot-media ubuntu-template-dev
cargo run -p bridgevm-cli -- media status ubuntu-template-dev
cargo run -p bridgevm-cli -- media download-plan ubuntu-template-dev --url https://example.invalid/ubuntu.iso --sha256 <digest>
cargo run -p bridgevm-cli -- media download ubuntu-template-dev
cargo run -p bridgevm-cli -- media verify ubuntu-template-dev --sha256 <digest>
cargo run -p bridgevm-cli -- create ubuntu-dev --os ubuntu --arch arm64
cargo run -p bridgevm-cli -- disk prepare ubuntu-dev
cargo run -p bridgevm-cli -- readiness ubuntu-dev
cargo run -p bridgevm-cli -- prepare-run ubuntu-dev
cargo run -p bridgevm-cli -- run ubuntu-dev
cargo run -p bridgevm-cli -- runner-status ubuntu-dev
cargo run -p lightvm-runner -- ubuntu-dev --print-plan
cargo run -p bridgevm-cli -- start ubuntu-dev
cargo run -p bridgevm-cli -- snapshot create ubuntu-dev before-upgrade
cargo run -p bridgevm-cli -- snapshot create ubuntu-dev before-upgrade-disk --kind disk
cargo run -p bridgevm-cli -- snapshot create ubuntu-dev before-upgrade-suspend --kind suspend
cargo run -p bridgevm-cli -- snapshot create ubuntu-dev before-upgrade-app --kind application-consistent
cargo run -p bridgevm-cli -- snapshot disk-create ubuntu-dev before-upgrade-disk
cargo run -p bridgevm-cli -- snapshot chain ubuntu-dev
cargo run -p bridgevm-cli -- snapshot restore ubuntu-dev before-upgrade
cargo run -p bridgevm-cli -- lifecycle-plan ubuntu-dev --action suspend
cargo run -p bridgevm-cli -- lifecycle-plan ubuntu-dev --action resume
# lifecycle-plan is metadata-only. suspend/resume are real backend operations:
# Fast Mode requires BRIDGEVM_APPLE_VZ_RUNNER, Compatibility Mode requires QMP.
cargo run -p bridgevm-cli -- suspend ubuntu-dev
cargo run -p bridgevm-cli -- resume ubuntu-dev
cargo run -p bridgevm-cli -- stop ubuntu-dev
cargo run -p bridgevm-cli -- delete ubuntu-dev --metadata-only
cargo run -p bridgevm-cli -- export ubuntu-dev --output target/ubuntu-dev-export.vmbridge
cargo run -p bridgevm-cli -- import target/ubuntu-dev-export.vmbridge --name ubuntu-dev-copy
cargo run -p bridgevm-cli -- export ubuntu-dev --output target/ubuntu-dev-export.tar
cargo run -p bridgevm-cli -- import target/ubuntu-dev-export.tar --name ubuntu-dev-tar-copy
cargo run -p bridgevm-cli -- metadata migrate-manifest ubuntu-dev --dry-run
cargo run -p bridgevm-cli -- metadata migrate-manifest ubuntu-dev
cargo run -p bridgevm-cli -- list
cargo run -p bridgevm-cli -- create legacy-linux --os ubuntu --arch x86_64 --mode compatibility
cargo run -p bridgevm-cli -- port add legacy-linux 2222:22
cargo run -p bridgevm-cli -- share add legacy-linux Projects ~/Projects --read-only
cargo run -p bridgevm-cli -- share list legacy-linux
cargo run -p bridgevm-cli -- share remove legacy-linux Projects
cargo run -p bridgevm-cli -- qemu-args legacy-linux
cargo run -p bridgevm-cli -- network-plan legacy-linux
cargo run -p networkd -- --print-plan --backend qemu --mode nat --hostname legacy-linux.bridgevm.local --forward 2222:22
cargo run -p networkd -- --backend qemu --mode nat --forward 2222:22
cargo run -p bridgevm-cli -- disk prepare legacy-linux
cargo run -p bridgevm-cli -- disk create legacy-linux
cargo run -p bridgevm-cli -- disk inspect legacy-linux
cargo run -p bridgevm-cli -- disk compact legacy-linux
cargo run -p bridgevm-cli -- prepare-run legacy-linux
cargo run -p bridgevm-cli -- runner-status legacy-linux
cargo run -p bridgevm-cli -- ssh legacy-linux --user ubuntu
# disk prepare safely records metadata and reports the qemu-img command for a
# missing qcow2/non-raw disk. disk create explicitly runs that qemu-img create
# command, disk inspect runs qemu-img info, and disk compact runs qemu-img
# convert before spawning QEMU.
# The next Compatibility Mode commands are real backend opt-in commands. They
# are not part of the default metadata-only Phase 0 smoke lane.
cargo run -p bridgevm-cli -- run legacy-linux --spawn
cargo run -p bridgevm-cli -- stop legacy-linux
cargo run -p fullvm-runner -- legacy-linux --print-qemu-args
cargo run -p fullvm-runner -- legacy-linux
cargo run -p bridgevm-cli -- runner-status legacy-linux
cargo run -p bridgevm-cli -- qmp-socket legacy-linux
cargo run -p bridgevm-cli -- qmp-status legacy-linux
cargo run -p bridgevm-cli -- logs qemu legacy-linux --bytes 16384
cargo run -p bridgevm-cli -- logs serial legacy-linux --bytes 16384
cargo run -p bridgevm-cli -- diagnostics bundle legacy-linux --output target/bridgevm-diagnostics
cargo run -p bridgevm-cli -- performance baseline legacy-linux --output target/bridgevm-performance
cargo run -p bridgevm-cli -- performance sample legacy-linux --output target/bridgevm-performance --artifact-bytes 4096 --iterations 1
cargo run -p bridgevm-daemon -- --once
```

Performance baseline artifacts are metadata-only. Performance samples are
bounded host-artifact probes by default: the API rejects missing VMs, zero or
excessive iterations, and excessive per-sample or total probe bytes before
creating the artifact directory. When the request runs through a daemon-owned
backend with a connected benchmark-capable guest-tools session, the daemon also
attaches bounded in-guest CPU/disk micro-benchmark measurements to the same
sample artifact.

VM bundle export/import supports both portable `.vmbridge` directory bundles and
`.tar` archive bundles. The format is selected from the path: outputs ending in
`.tar` are written as tar archives, while other outputs remain directory bundles;
imports ending in `.tar` are read as archive bundles. In either format,
export/import copies only regular files and directories. It rejects symlinks,
special files, exports back into the source bundle, and imports that would copy a
VM bundle into itself or from inside the destination store. Those checks run at
the storage layer and are preserved through both the local CLI and the socket
API, so callers get explicit errors such as "export output must not be the source
bundle or inside it" or "import input conflicts with the destination store"
instead of a partially copied bundle. The destination path must not already
exist; import rename conflicts fail before copying.

Dashboard-safe delete is metadata-only. `bridgevm delete <vm> --metadata-only`
refuses running VMs, keeps the `.vmbridge` bundle and `manifest.yaml` in place,
writes `metadata/deletion.json` and `metadata/deleted-manifest.yaml`, and hides
the tombstoned VM from subsequent list results through both the local CLI and
socket API. This is the delete boundary the macOS dashboard should use because
it preserves disks, installers, logs, and metadata for audit or recovery.
Destructive bundle removal is a separate explicit storage/CLI operation, not a
dashboard default.

The macOS dashboard export/import surface should use that same daemon-backed
file-copy boundary. Its VM cards and detail panels may show source and
destination paths, directory vs tar format, copied file counts, manifest
identity/hostname rewrite results on import, and preserved manifest or metadata
entries such as snapshots, port forwards, and shared folders. It must not start
a VM, connect to QMP, attach guest tools, copy live sockets, or add disk/media
artifacts beyond what the Rust CLI/API export/import path already includes.
Preserving snapshot metadata here does not mean live guest state migration or
memory restore.

The macOS dashboard clone sheet uses the same daemon `clone_vm` boundary as the
CLI/socket API and exposes both full and linked clone choices. Full clone copies
the stopped VM bundle through the Rust storage path. Linked clone records the
qemu-img overlay creation boundary and returns metadata such as the backing
path, backing format, clone disk path, and `qemu-img create` command for the
dashboard to surface. That linked clone flow may create the overlay file through
the explicit storage command boundary, but it does not start QEMU, launch Apple
VZ, attach guest tools, or start a VM.

By default VM bundles are stored under `~/.bridgevm/vms`. Set `BRIDGEVM_HOME` to override this during development.

Fast Mode manifests can include an optional `boot` section. For supported Fast Mode guests, `bridgevm create <name> --template <id>` can populate the guest OS, arch, and dry-run boot media metadata when explicit `--os`, `--arch`, or boot flags are omitted. Without `--template`, `bridgevm create` still applies a matching dry-run boot template for supported explicit OS/arch pairs when explicit boot flags are omitted: Linux Arm64 guests default to `linux-installer` with `installers/<guest>-arm64.iso`, and macOS Arm guests default to `macos-restore` with `installers/macos-restore.ipsw`. Override that with `--boot-mode linux-installer --installer-image <path>`, `--boot-mode linux-kernel --kernel-path <path> [--initrd-path <path>] [--kernel-command-line <text>]`, or `--boot-mode macos-restore --macos-restore-image <path>`. `bridgevm recommend` prints the same template hint, and `bridgevm templates` lists the available metadata-only templates without downloading media.

The current default user flow is boot media readiness, not VM launch. `bridgevm boot-media <vm>` resolves the manifest/template boot media through the Apple VZ dry-run plan and prints the installer, kernel, initrd, or macOS restore path with its `exists` state. `bridgevm media status <vm>` shows the same resolved entries with file size, latest local import, latest verification result, latest download plan, and latest download result when present. `bridgevm media import <vm> --source <path>` copies a user-provided local installer, kernel, initrd, or macOS restore file into the expected path and records import metadata under `.vmbridge/metadata/boot-media/<kind>.json`; it does not download media. `bridgevm media verify <vm> --sha256 <hex>` hashes the already-resolved local file, compares it with the caller-provided digest, and records the result under `.vmbridge/metadata/boot-media/<kind>-verify.json`. `bridgevm media download-plan <vm> --url <url> [--sha256 <hex>]` records remote download intent under `.vmbridge/metadata/boot-media/<kind>-download.json` without fetching the URL. `bridgevm media download <vm>` is the explicit execution boundary for that recorded plan: it fetches the planned URL to the resolved destination, verifies the optional planned SHA-256 before replacing the destination, and records the result under `.vmbridge/metadata/boot-media/<kind>-download-result.json`. When a boot mode exposes multiple media paths, add `--kind installer-image|kernel|initrd|macos-restore-image` to select the entry for status-related operations, verification, planning, or download execution. `bridgevm prepare-run`, Fast Mode `bridgevm run <vm>` without spawn, daemon-backed `runner-status`, and `lightvm-runner --print-plan` expose the resolved boot media and launch readiness inside dry-run runner metadata. The narrow real Apple VZ path runs through the Swift `AppleVzRunner` helper for `linux-kernel` + `raw` + NAT specs; local CLI callers opt in with `BRIDGEVM_APPLE_VZ_RUNNER`, while daemon/app callers also enable live starts with `BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1` or the matching Settings toggle. Any launch-readiness/preflight gate should report blockers such as missing boot media, missing or unsupported primary disks, or unsupported host/guest/backend capabilities before starting Apple VZ.

`disk prepare` resolves the manifest's primary disk, creates the disk directory, writes `metadata/primary-disk.json`, and prints the disk preparation result without building a runner plan. `prepare-run` performs the same disk preparation before writing runner metadata, and `runner-status` shows the same recorded disk state after preparation. For `raw` primary disks, preparation creates a sparse file directly when the disk is missing. For `qcow2` and other non-raw formats, preparation deliberately stays safe: it does not create or validate the image, and instead records and exposes a `qemu-img create -f <format> <path> <size>` command.

The intended Compatibility Mode storage order is `disk prepare`, `disk create`, `disk inspect`, optional `disk verify`/`disk compact`, then `run --spawn`. `disk create` is the explicit execution path for the recorded create command. It runs `qemu-img create` for a missing non-raw primary disk, fails if `qemu-img` is unavailable or the command exits unsuccessfully, and reports an already-ready result when the disk already exists. `disk inspect` reads primary disk image metadata with `qemu-img info --output=json <path>` and records the latest inspection in `metadata/last-disk-inspect.json`. `disk verify` checks the selected non-raw active disk with `qemu-img check --output=json <path>` and records `metadata/last-disk-verify.json` without changing disk or snapshot-chain state. Inspecting or verifying raw disks, missing disks, or running without `qemu-img` available can fail safely instead of inventing metadata. For existing non-raw active disks, `cargo run -p bridgevm-cli -- disk compact <vm>` explicitly runs `qemu-img convert` into a compacted image, renames the active disk aside as `<disk>.precompact-<timestamp>`, replaces the active disk with the compacted result, and records the latest attempt in `metadata/last-disk-compact.json`. Compatibility Mode `run --spawn` refuses to start QEMU while the primary disk is still missing, so dry-run preparation can complete safely without accidentally creating a qcow2 image or launching a backend against an absent disk.

Disk snapshots are a qcow2 chain scaffold with an explicit overlay creation step. `snapshot create <vm> <name> --kind disk` still records the normal snapshot metadata, and also writes disk-chain metadata under `metadata/snapshot-disks/<name>.json`. That disk metadata describes the resolved backing disk, backing format, planned overlay path, overlay existence state, and the safe command BridgeVM can run next:

```bash
qemu-img create -f qcow2 -F <backing-format> -b <backing-file> <overlay>
```

Run `snapshot disk-create <vm> <name>` to explicitly execute that recorded overlay command. The command fails safely when the backing disk is missing, when `qemu-img` is not installed or not on `PATH`, or when `qemu-img create` exits unsuccessfully. A successful or failed attempt is recorded under `metadata/snapshot-disks/<name>-create.json` or equivalent creation metadata. Run `snapshot chain <vm>` to inspect disk snapshot chain metadata, including the active disk source, snapshot name when present, and selected disk path. A successful overlay creation also records `metadata/active-disk.json`, so `qemu-args`, future runner plans, and Compatibility Mode spawns use the active qcow2 chain member instead of always using the manifest's primary disk. Restoring a disk snapshot rewinds the active disk to that snapshot's backing image.

Suspend snapshots stay metadata-only for now. `snapshot create <vm> <name> --kind suspend` records the normal snapshot entry and writes a planned suspend image record under `metadata/suspend-images/<name>.json`. That metadata points at `suspend-images/<name>.bin`, records the image format marker, and reports whether the image file exists. BridgeVM does not serialize guest memory into that snapshot file yet. Restoring a suspend snapshot only verifies that this planned image marker exists, refreshes the image existence state, includes `suspend_image` in `metadata/last-restore.json`, and prints the suspend image path, format, readiness, and preparation timestamp. It is not memory deserialization or a live guest resume. Fast Mode lifecycle suspend/resume is a separate backend operation through the Apple VZ helper boundary, described below.

The macOS dashboard snapshot surface reads the same daemon metadata as the
CLI/API. It shows snapshot names, kinds, recorded runtime state, disk-chain
metadata, suspend-image readiness, `metadata/last-restore.json`, and snapshot
disk-create result metadata from the explicit overlay creation boundary.
`snapshot disk-create` runs the recorded `qemu-img create` overlay command and
records command, stdout/stderr, exit status, created time, and the resulting
disk-chain metadata; it does not start a VM, start QEMU, or launch Apple VZ.
Restore controls are metadata-boundary operations only: they do not restore
guest memory, roll back a live VM, or prove application consistency.

The macOS dashboard Storage Maintenance panel uses the same daemon
`prepare_disk`, `create_disk`, `inspect_disk`, `verify_disk`, and `compact_disk`
boundaries as the CLI/socket API. Prepare reports the primary disk path, format,
size, existence state, and suggested create command without running `qemu-img`.
Create Disk is the explicit `qemu-img create` disk-file creation boundary for a
missing non-raw primary disk; it creates the disk image when requested, but does
not start QEMU, launch Apple VZ, or start a VM. Inspect reports `qemu-img info`
metadata for an existing non-raw primary disk. Verify reports the active disk
path, qemu-img command, status, duration, timestamp, and parsed check report
without modifying storage. Compact reports the replacement command, backup
path, size delta, duration, and refreshed active-disk chain metadata; it is still
an explicit non-raw active-disk operation, not automatic cleanup.

The macOS dashboard metadata repair action uses the daemon `repair_metadata`
boundary already exposed by `bridgevm metadata repair` and the socket API. It
should show whether anything was repaired, no-op status, action details, repair
timestamp, and bundle path. This is a metadata-only repair path: it can recreate
missing repairable metadata from the manifest and snapshot list, but it must not
create disks, invent runner history or operation receipts, or replace corrupt
JSON files.

Application-consistent snapshots now have both a conservative preflight record and a daemon-owned execution path. `snapshot create <vm> <name> --kind application-consistent` records the snapshot entry and writes `metadata/application-consistent-snapshots/<name>.json` with guest-tools connection state, required capabilities (`fs-freeze`, `fs-thaw`), advertised capabilities, missing capabilities, readiness, backend freeze/thaw support, and planned freeze/thaw semantics. `snapshot execute-application-consistent <vm> <name>` requires a daemon-owned backend plus an authenticated guest-tools stream, dispatches request-correlated freeze/thaw commands around snapshot creation, always attempts thaw after the snapshot boundary, and records the execution result. The default Linux tools path still acknowledges simulated freeze/thaw protocol state for socket-safe tests; the explicit `--real-fsfreeze --fsfreeze-mount <path>` path calls the Linux `fsfreeze` backend on allowlisted mounts and is covered by both fake-backend smokes and a heavy live opt-in QEMU/HVF smoke against a safe loopback ext4 mount. That proves the filesystem freeze/thaw boundary, not database flushing, application quiescing, or complete application-level consistency.

`bridgevm diagnostics bundle <vm> --output <dir>` creates a support bundle for a VM without copying heavy or live runtime artifacts. The bundle collects `manifest.yaml`, `logs/`, and `metadata/`, writes a `diagnostic-bundle.json` summary, and excludes disks, installer or restore media, sockets, and lock files. That metadata can include runner status, launch-readiness blockers, Apple VZ dry-run launch specs, and live-evidence/verifier status artifacts when those have already been recorded, so support readers can see why launch is slow or what is still missing without requiring a live VM. Sensitive material is redacted before writing the bundle, including the guest-tools token, sensitive JSON keys, and URL query strings in JSON metadata. The macOS dashboard consumes the same socket-backed metadata shape for its VM cards and Diagnostics & Performance panel: it shows the bundle output path, source VM bundle, creation time, and copied file list without opening a live backend endpoint.

`bridgevm performance baseline <vm> --output <dir>` writes a metadata-only performance baseline artifact. It does not boot the VM, run guest benchmarks, sample host performance counters, or perform disk/display/CPU tests. Instead, it records the VM state BridgeVM already knows about, runner metadata, guest-tools runtime and guest metrics when present, derived observations such as metadata age, runner observed uptime, guest CPU percent, and guest memory use, plus notes explaining what data was available. The artifact is written to `<output>/bridgevm-performance-<vm>-<timestamp>/performance-baseline.json`. This is the first shared baseline step for both Fast Mode and Compatibility Mode: it captures existing runtime and guest metrics in a stable JSON shape so later real boot, resume, idle CPU, display, and disk I/O measurements can compare against the same metadata context. The dashboard performance card and panel treat this as known metadata, not a live benchmark result.

`bridgevm performance sample <vm> --output <dir> [--artifact-bytes BYTES] [--iterations N] [--sync]` writes a host-side performance sample artifact without starting or resuming the guest. It leaves `write-probe.bin` for a single iteration or numbered `write-probe-0001.bin` files for repeated samples in `<output>/bridgevm-performance-sample-<vm>-<timestamp>/`. It records per-iteration write results plus aggregate measurements such as `host_artifact_write_latency_microseconds`, `host_artifact_write_latency_min_microseconds`, `host_artifact_write_latency_max_microseconds`, `host_artifact_write_latency_mean_microseconds`, `host_artifact_write_latency_p50_microseconds`, `host_artifact_write_total_bytes`, BridgeVM metadata operation latencies such as `bridgevm_state_read_latency_microseconds`, `bridgevm_runner_metadata_read_latency_microseconds`, `bridgevm_guest_tools_status_inspect_latency_microseconds`, and `sample_generation_duration_microseconds`, then writes the sample metadata to `performance-sample.json`. When Compatibility Mode disk inspection is available and succeeds during sampling, the artifact may also include `disk_inspect_duration_microseconds`; that value is only the host-side duration of the `qemu-img info` inspection path. When the socket request is handled by a daemon that owns the running backend and has a connected guest-tools session advertising `benchmark`, it also dispatches a bounded `RunBenchmark` command and records `guest_benchmark_*` measurements plus the refreshed latest guest-tools command result. The default probe is 1 MiB, default iterations is 1, per-iteration size is capped at 64 MiB, total probe output is capped at 256 MiB, and the guest benchmark is capped by the guest-tools protocol. `--sync` includes host probe-file `sync_data()` in each iteration's measured write latency. Dashboard sample status should surface artifact paths, byte counts, iteration counts, host latency metadata, and optional daemon-owned guest benchmark measurements without treating local host-only samples as live VM performance.

Resource profiles are resolved at runner-planning time. When manifest `resources.memory` or `resources.cpu` is `auto`, BridgeVM chooses deterministic values from `resources.profile`; explicit memory and CPU values are preserved. Fast Mode exposes the selected display FPS cap and resource rationale in the Apple VZ launch spec, while Compatibility Mode renders the resolved values into QEMU `-m` and `-smp` arguments. Running Fast Mode VMs can record a refreshed policy signal with `bridgevm resources reapply <vm> --visibility foreground|background`; the resulting `metadata/runtime-resources.json` is explicit about whether live apply happened and why the current build still records `runtime-control-unavailable`.

`networkd` is the public metadata-only CLI surface for the shared network
planner. `cargo run -p networkd -- --print-plan --backend qemu --mode nat
--hostname <host> --forward <host:guest>` prints a JSON `NetworkPlan` with the
selected backend, mode, hostname, validated port-forward rules, capability
flags, requirements, and notes. Without `--print-plan`, it prints a concise
ready/blocked summary that includes requirement counts for plans with launch
blockers. It validates planner inputs and rejection paths such as malformed
forwards, forwards outside NAT, and Apple VZ bridged networking, but it does
not start a VM, start QEMU, launch Apple VZ, create host-only interfaces,
attach bridges, or modify live networking.

`bridgevm ssh <vm> [--user USER]` is a metadata-only connection planner. It does not execute `ssh`; it prints the command BridgeVM would use from currently available metadata. For Compatibility Mode, a manifest port forward whose guest port is `22` is preferred and prints `ssh -p <host-port> USER@127.0.0.1`. Otherwise, if guest-tools runtime metadata is connected and reports a valid guest IP, the plan can print `ssh USER@<guest-ip>`. The macOS dashboard surfaces the same SSH plan metadata alongside the matching open-port plan for forwarded services as metadata/planning information only. That status can show the derived host URL, SSH command, target, and source BridgeVM would use, but it must not claim that BridgeVM opened a browser, connected to the guest service, changed networking, executed `ssh`, started an SSH process, started a backend, or started a VM.

The macOS dashboard Console button attempts the Compatibility Mode external VNC viewer handoff when a viewer endpoint can be derived from the launch plan, then keeps the diagnostic QMP/logs boundary visible as support/fallback status. It asks the daemon for `qmp_status`, inspects whether the Compatibility Mode QMP socket path is known and ready, and shows an honest status message about backend control readiness. The dashboard can also load bounded QEMU and serial log tails through the same daemon API that powers `bridgevm logs qemu <vm> [--bytes N]` and `bridgevm logs serial <vm> [--bytes N]`. Those log views read from `logs/qemu.log` and `logs/serial.log` without starting a backend. For Compatibility Mode display, explicit VNC renderer planning uses QEMU `-display vnc=:0` as the deterministic dry-run template, while daemon-owned spawn paths pin that template to the lowest free `vnc=:N` display before launching so concurrent Compat VMs do not collide on port 5900. The graphical path is still an external-viewer handoff rather than an embedded macOS framebuffer stream. The diagnostic QMP/log views are not themselves viewer evidence or proof that a guest displayed frames.

`bridgevm lifecycle-plan <vm> --action suspend|resume` is a metadata-only command readiness boundary for UI/API surfaces. It reports the current state, target state, backend boundary, planned Compatibility Mode QMP command (`stop` for suspend, `cont` for resume), QMP socket path availability, and blockers without connecting to QMP or starting a VM. Fast Mode reports the Apple VZ suspend/resume runner boundary and surfaces a concrete `apple-vz-runner-unavailable` blocker until `BRIDGEVM_APPLE_VZ_RUNNER` points at the signed Swift helper.

`bridgevm readiness <vm>` is the aggregate pre-launch report for the same safe boundary. It reads runtime state, Fast Mode boot-media status when applicable, snapshot active-disk metadata, runner metadata, blockers, and notes without preparing disks, writing runner metadata, connecting to QMP, starting QEMU, launching Apple VZ, or touching a guest. When runner metadata is absent, Fast Mode and Compatibility Mode can both render a metadata-safe `Pre-run launch readiness:` section; Compatibility Mode uses the active disk and QEMU command planner to report structured blockers such as a missing primary disk without launching QEMU. A `ready` report means the metadata inputs and host/backend capability checks currently have no known launch blockers; it is still not evidence that a VM was started or that a guest reached boot. `bridgevm readiness <vm> --live-evidence <dir>` can additionally ingest a preserved Apple VZ or QEMU live evidence bundle and mark the live-boot, graphical boot-progress, and serial/viewer/QMP console requirements proven only after Rust-side metadata/text/JSON/hash cross-checks pass; it does not launch a VM, run the Apple VZ helper, connect to QMP, or start QEMU. Apple VZ bundles use the preserved `SUMMARY.txt`/launch-spec/handoff transcript contract plus optional `boot-progress-evidence.json`/`viewer-evidence.json` PNG artifacts, while QEMU bundles are identified by `qemu-live-evidence.json` plus hashed QEMU, serial log, and QMP transcript artifacts. QEMU evidence is accepted only for the matching Compatibility Mode VM, active `qcow2` disk format, NAT network, expected bundle QMP socket at `metadata/qmp.sock`, exact supported `qemu-system-*` executable, exact planner-emitted `-name`/`-qmp unix:<socket>,server=on,wait=off` command, `qmp.running: true`, `qmp.status: "running"`, and a hashed QMP transcript containing the greeting, `query-status` command, and running response. The hashed QEMU log must also preserve transcript lines for the exact `Command:` and `QMP socket:` values from the evidence JSON rather than merely containing those strings loosely. Adding `--record-live-evidence` copies that verified bundle into `.vmbridge/metadata/live-evidence/latest` and records `.vmbridge/metadata/live-evidence.json`, so later plain `bridgevm readiness <vm>` runs the same verifier against the preserved path automatically. `bridgevm readiness <vm> --clear-live-evidence` removes that preserved metadata and copied bundle. `guest-tools-effects` must remain unproven on the preserved-evidence path unless the bundle includes verifier-checked guest-tools result artifacts that demonstrate observable guest-side effects. The daemon/socket API exposes the same `readiness_report` response for UI clients that want one compact launch-readiness summary before moving into explicit prepare/start flows.

The safe smoke coverage now includes that aggregate readiness CLI contract plus the Apple VZ live opt-in default-skip boundary. Passing those smokes proves the report shape, evidence ingestion contract, and default skip behavior, not a completed live E2E launch, not a graphical console, and not guest-tools effects inside a running guest. When readiness evidence is summarized, `live-boot`, `console`, and `guest-tools-effects` should stay marked required but unproven unless an opt-in evidence bundle proves each category explicitly, with guest-tools proof limited to result artifacts accepted by the verifier rather than default live execution.

The lifecycle and suspend snapshot smoke tests exercise these local CLI and daemon socket boundaries with fake markers only: plan inspection must not create QMP socket markers, suspend snapshot creation must not write a memory image, missing-image restore must not write restore metadata, and restart/suspend/resume metadata transitions must not launch QEMU or Apple VZ.

The macOS dashboard is expected to read the same daemon-backed guest tools status rather than invent a separate client-side view of readiness. Its guest tools panel can show policy capabilities, authenticated runtime state, guest IP, heartbeat, guest metrics, the latest `last_command_result`, and passive agent-update availability metadata when those fields exist in daemon metadata. `AgentUpdateAvailable` is only a guest-origin notice carrying current version, available version, URL, signature, and observed timestamp; BridgeVM records and reports it, but does not download, install, execute, or claim completion of a guest-tools update. The macOS client carries the typed daemon `guest_tools_send_command` request/response boundary for safe alpha command dispatch, including clipboard text, display resize, inline file-drop start/chunk/complete requests, and application/window list, launch, focus, and close actions that exercise protocol plumbing without claiming real desktop clipboard, display mode, host-to-guest filesystem drop, app, or window control. Dashboard file-drop controls must be guarded by drag-drop capability/readiness and result metadata, and should surface request IDs, pending counts, and latest command results instead of treating dispatch as proof that a file exists in the guest or that a guest filesystem path was mounted. Dashboard app/window controls must require Compatibility Mode guest-tools backend readiness and should surface daemon result metadata such as request ID, pending count, and latest command result instead of treating a dispatch as proof that a real application or desktop window changed. Actual guest tools transport, command authorization, command delivery, and passive update notice recording remain owned by the Compatibility Mode daemon backend, and Fast Mode real Apple VZ launch remains gated by the signed Swift helper plus the explicit daemon/app live-start opt-in rather than an unconfigured default spawn path.

Approved shared folders have two separate boundaries. `bridgevm share list/add/remove`
and the matching macOS dashboard controls edit the VM manifest's
`sharedFolders` approval list only: they record the host path, read-only flag,
and opaque host path token that policy allows. They do not mount the folder in a
running guest, create a guest filesystem path, or change an already-running
backend. A live guest sees an approved share only after the daemon-owned guest
tools `mount-share` command is used against an authenticated session, and the
current Linux scaffold still treats that as alpha in-memory protocol state
rather than a real OS mount.

## Daemon socket mode

Run the daemon:

```bash
cargo run -p bridgevm-daemon -- --store target/bridgevm-dev
```

Then send CLI requests through the Unix socket:

```bash
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock doctor
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock templates
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock create ubuntu-template-dev --template ubuntu-arm64-installer
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock boot-media ubuntu-template-dev
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock media import ubuntu-template-dev --source ~/Downloads/ubuntu-arm64.iso
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock boot-media ubuntu-template-dev
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock media status ubuntu-template-dev
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock media download-plan ubuntu-template-dev --url https://example.invalid/ubuntu.iso --sha256 <digest>
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock media download ubuntu-template-dev
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock media verify ubuntu-template-dev --sha256 <digest>
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock create ubuntu-dev --os ubuntu --arch arm64
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock start ubuntu-dev
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock run ubuntu-dev
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock stop ubuntu-dev
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock delete ubuntu-dev --metadata-only
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock disk prepare legacy-linux
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock disk create legacy-linux
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock disk inspect legacy-linux
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock snapshot disk-create legacy-linux before-upgrade
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock snapshot chain legacy-linux
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock logs qemu legacy-linux --bytes 16384
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock logs serial legacy-linux --bytes 16384
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock diagnostics bundle legacy-linux --output target/bridgevm-diagnostics
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock performance sample legacy-linux --output target/bridgevm-performance --artifact-bytes 4096 --iterations 1
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock share add legacy-linux Projects ~/Projects --read-only
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock share list legacy-linux
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock share remove legacy-linux Projects
cargo run -p bridgevm-cli -- --socket target/bridgevm-dev/run/bridgevmd.sock prepare-run legacy-linux
```

This is a typed JSON-over-Unix-socket scaffold for the local API boundary. The same metadata-only template list can be requested through the socket; listing templates does not download installers or restore images. Socket-backed `create <name> --template <id>` uses that metadata to fill omitted OS, arch, and boot fields without downloading media. The macOS dashboard creation sheet now uses the daemon `list_templates` and `create_vm` requests for the same boundary: it creates a stopped VM bundle from a selected boot template, then the next operational step is boot media status, local import, verification, download-plan metadata, or planned download execution rather than real Apple VZ launch. Socket-backed `boot-media <vm>` performs the same direct dry-run boot media inspection as the local CLI, socket-backed `media import <vm> --source <path>` copies a user-provided local media file into the expected bundle path without downloading anything, socket-backed `media status <vm>` reports the same resolved paths, existence states, file sizes, latest import metadata, latest verification result, latest download plan, and latest download result, socket-backed `media download-plan <vm> --url <url> [--sha256 <hex>]` records the same no-network remote download intent metadata, socket-backed `media download <vm>` executes the recorded plan by fetching the planned URL to the resolved destination and recording the result, socket-backed `media verify <vm> --sha256 <hex>` records the same expected-digest comparison metadata, socket-backed `export <vm> --output <path>` and `import <bundle> [--name <name>]` expose the same portable `.vmbridge` directory and `.tar` archive copy boundaries that the macOS dashboard export/import UI should surface, socket-backed `disk prepare <vm>`, `disk create <vm>`, `disk inspect <vm>`, `disk verify <vm>`, and `disk compact <vm>` expose the same storage maintenance responses now shown by the macOS dashboard Storage Maintenance panel, with `disk create` remaining the explicit `qemu-img create` disk-file boundary rather than a VM/QEMU/Apple VZ start, socket-backed `metadata repair <vm>` exposes the conservative metadata repair result that the macOS dashboard repair action should display, socket-backed `share list/add/remove <vm>` exposes the manifest-level approved shared folder list now shown by the macOS dashboard without mounting anything in a live guest, socket-backed Fast Mode `prepare-run`, `run` without spawn, and `runner-status` expose dry-run launch readiness through RunnerStatus metadata, while socket-backed Fast Mode `run --spawn`, suspend, and resume cross into the signed Apple VZ helper only when `BRIDGEVM_APPLE_VZ_RUNNER` and the required opt-in/fixtures are present, socket-backed `qmp-status <vm>` reports the daemon's Compatibility Mode QMP socket readiness/status boundary, socket-backed `logs qemu <vm>` and `logs serial <vm>` return bounded tails of the VM's QEMU and serial log files for the CLI and macOS dashboard, socket-backed `qemu-args <vm>` preserves explicit Compatibility Mode VNC display plans as `-display vnc=:0` for deterministic dry-run planning, daemon-owned spawn remaps that VNC template to a free `vnc=:N` display before launch, socket-backed `guest-tools status <vm>` reports the same daemon-backed capabilities, runtime, guest IP, heartbeat, metrics readiness metadata, approved shared-folder policy entries, latest `last_command_result`, and passive agent-update notice metadata that the macOS dashboard guest tools panel is expected to display, socket-backed `guest-tools send-command <vm>` and the matching macOS client `guest_tools_send_command` typed boundary send an already-formed agent envelope to the daemon-owned backend and report command tracking metadata, socket-backed guest-tools clipboard, display-resize, inline file-drop start/chunk/complete, shared-folder mount/unmount, and application/window wrappers provide safe alpha command dispatch for protocol status surfaces without claiming real desktop control, a real host-to-guest filesystem drop, a mounted guest filesystem, or a real guest filesystem mount, socket-backed `diagnostics bundle <vm> --output <dir>` writes the same redacted support bundle as the local CLI, socket-backed `performance sample <vm> --output <dir>` writes the same bounded host-side sample artifact as the local CLI, and socket-backed `performance baseline <vm> --output <dir>` writes the same metadata-only baseline artifact as the local CLI. The plan still calls for replacing the transport with gRPC over a Unix domain socket, actual guest tools transport and commands stay behind the Compatibility Mode daemon-owned backend boundary, `AgentUpdateAvailable` stays a passive notice rather than an updater, the macOS Console action includes the external VNC viewer handoff plus QMP/log diagnostic surfaces, an embedded in-app graphical console remains a later boundary, Fast Mode Show Display is a separate local GUI helper window, and default smoke coverage must continue to prove Apple VZ process launch only through explicit opt-in helper/evidence paths.

When a Compatibility Mode VM is started through `bridgevmd` with `run --spawn`, the daemon first prepares active disk metadata and refuses to spawn if the selected chain member is still missing. If spawning succeeds, the daemon keeps the child process handle in an in-memory supervisor registry. A periodic supervisor pass notices exited children, clears runner metadata, and marks the VM stopped. `stop` first tries QMP `quit` when the socket is available, then terminates the owned process if it does not exit.

The remaining storage work is intentionally explicit: BridgeVM can now run the recorded primary-disk `qemu-img create` command through `disk create`, inspect primary disk metadata through `disk inspect`, verify non-raw active disk metadata through `disk verify`, compact existing non-raw active disks through `disk compact`, repair missing metadata through the conservative `metadata repair` boundary, record qcow2 snapshot chain metadata, inspect active chain state through `snapshot chain`, run the recorded overlay creation command through `snapshot disk-create`, route QEMU argument generation and runner startup through `metadata/active-disk.json`, require the planned suspend image file before restoring suspend snapshot metadata, record application-consistent snapshot preflight metadata, and execute daemon-owned guest-tools freeze/thaw around application-consistent snapshot creation when a connected backend advertises the required capabilities. Snapshot restore still needs real suspend-image memory serialization/deserialization and higher-level application quiescing for complete application consistency; that is separate from the Fast Mode lifecycle suspend/resume backend path.
