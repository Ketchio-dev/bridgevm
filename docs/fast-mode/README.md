# Fast Mode

Fast Mode is the narrow, optimized engine path.

Initial supported guests:

- Ubuntu Arm64
- Fedora Arm64
- Debian Arm64
- macOS Arm guests
- Windows 11 Arm as experimental, restricted backend only

Fast Mode intentionally rejects x86 guests, legacy BIOS, arbitrary QEMU devices, and custom CPU models.

## Implemented scaffold

The Rust `bridgevm-apple-vz` crate builds a dry-run Apple Virtualization Framework launch spec from a Fast Mode manifest. This keeps Fast Mode on a separate execution path instead of routing it through QEMU.

Apple VZ launch preflight runs at the `build_fast_plan` boundary. It accepts only Fast Mode manifests with an Arm guest arch (`arm64` or `aarch64`), unset or `apple-vz` preferred backend, `nat` networking, a supported Apple VZ guest family, and a primary disk format of `raw` or `qcow2` for dry-run readiness and planning.

A launch-readiness/preflight gate belongs immediately after that dry-run spec is built and before any default Apple VZ process start. Its job is still not to launch the guest. It converts the dry-run plan into structured readiness output: missing installer/kernel/initrd/restore media, missing active disk files, disk formats that cannot be launched, and platform or backend support blockers are visible as named blockers that CLI, daemon, dashboard, and tests can consume. The same readiness object travels with dry-run runner metadata so `prepare-run`, Fast Mode `run` without spawn, and daemon `runner-status` all report the same blocked or ready state.

Shared networking policy is being staged in `bridgevm-network`. That crate can
build a backend-neutral `NetworkPlan` with mode capabilities, hostname, and
validated port-forward rules. Fast Mode still accepts only NAT at the Apple VZ
launch preflight boundary today, but the shared planner now records the intended
host-only and isolated boundaries separately from launcher integration.

Safe readiness and metadata commands do not start a live VM, Apple VZ, QEMU, or
a GUI:

```bash
bridgevm templates
bridgevm create ubuntu-dev --template ubuntu-arm64-installer
bridgevm boot-media ubuntu-dev
bridgevm media import ubuntu-dev --source ~/Downloads/ubuntu-arm64.iso
bridgevm boot-media ubuntu-dev
bridgevm media status ubuntu-dev
bridgevm media download-plan ubuntu-dev --url https://example.invalid/ubuntu.iso --sha256 <digest>
bridgevm media download ubuntu-dev
bridgevm media verify ubuntu-dev --sha256 <digest>
bridgevm prepare-run ubuntu-dev
bridgevm run ubuntu-dev
bridgevm runner-status ubuntu-dev
bridgevm ssh ubuntu-dev --user ubuntu
lightvm-runner ubuntu-dev --print-plan
lightvm-runner ubuntu-dev --require-ready --print-plan
lightvm-runner --launch-spec .vmbridge/metadata/apple-vz-launch.json --print-handoff
lightvm-runner --launch-spec .vmbridge/metadata/apple-vz-launch.json --require-ready --launch
```

Manual live proof commands are intentionally separate and require explicit
operator opt-in plus auditable evidence:

```bash
eval "$(tests/integration/prepare-apple-vz-debian-fixture.sh)"
export BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1
tests/integration/apple-vz-live-boot-opt-in-smoke.sh
```

The live smoke prints the actual preserved evidence path. Treat a live run as
proof only after reviewing the smoke output and verifying the evidence bundle.

`bridgevm prepare-run ubuntu-dev` and Fast Mode `bridgevm run ubuntu-dev` without spawn currently record dry-run `lightvm` runner metadata. They also prepare primary disk metadata: BridgeVM resolves the manifest's primary disk, creates the disk directory, writes `metadata/primary-disk.json`, writes the dry-run Apple VZ launch spec to `metadata/apple-vz-launch.json`, and embeds the disk preparation result plus `launch_spec_path` in `metadata/runner.json`. That runner metadata includes the `AppleVzLaunchSpec.readiness` result so a missing installer, missing active disk, unsupported disk format, or unsupported host/guest/backend capability is visible before any launcher boundary. Runner readiness blockers preserve an affected path when the blocker belongs to a file, or an affected capability when the blocker belongs to host/runtime support. `bridgevm runner-status ubuntu-dev` reads back the same readiness state from RunnerStatus instead of inventing a separate status shape.

The macOS dashboard creation sheet uses the same daemon-backed template flow. It loads boot templates with `list_templates`, sends `create_vm` for the selected template, and leaves the new VM stopped. From there the real next step is boot media readiness: inspect `media status`, import a local installer or restore image, verify an existing file, or record a download plan.

`bridgevm boot-media ubuntu-dev` is a focused inspection command for the same boot portion of the Apple VZ dry-run plan. It resolves manifest/template boot media against the `.vmbridge` bundle and prints the installer, kernel, initrd, or macOS restore path with its `exists` state. It does not download media, prepare disks, or require users to inspect the full `prepare-run` or `lightvm-runner --print-plan` JSON.

`bridgevm media import ubuntu-dev --source <path>` is the local media handoff for files the user already has. It uses the same Apple VZ dry-run boot media resolution as `boot-media`, then copies the provided installer, kernel, initrd, or macOS restore file into the expected path inside the `.vmbridge` bundle. This is a safe intermediate step for OS download/template flows: a template-created VM can report `exists: false`, accept a user-provided local file, and then report `exists: true` on the next `boot-media` check. It does not download installer or restore media.

`bridgevm media status ubuntu-dev` summarizes the Fast Mode boot media contract. For each installer, kernel, initrd, or macOS restore entry, it reports the resolved bundle path, whether the file exists, its current file size when present, the latest local import record, the latest verification result, the latest download plan, and the latest download result. Import records are stored under `.vmbridge/metadata/boot-media/<kind>.json`, verification records under `<kind>-verify.json`, download plans under `<kind>-download.json`, and download results under `<kind>-download-result.json`.

`bridgevm media download-plan ubuntu-dev --url <url> [--sha256 <hex>]` records a remote media download intent without performing the network download. It resolves the same installer, kernel, initrd, or macOS restore destination inside the `.vmbridge` bundle, reports the caller-provided URL, optional expected SHA-256, current file existence and size, and the latest import/verify status, and writes that plan under `.vmbridge/metadata/boot-media/<kind>-download.json`. When more than one boot media path exists, use `--kind installer-image|kernel|initrd|macos-restore-image` to choose the planned destination.

`bridgevm media download ubuntu-dev` executes the recorded download plan. It does not accept or choose a URL itself; it reads `.vmbridge/metadata/boot-media/<kind>-download.json`, confirms the current resolved destination still matches the plan, fetches the planned URL to a temporary file, verifies the optional planned SHA-256 digest, moves the file into place, and records the result under `.vmbridge/metadata/boot-media/<kind>-download-result.json`. When more than one boot media path exists, use `--kind installer-image|kernel|initrd|macos-restore-image` to select the planned download to execute.

`bridgevm media verify ubuntu-dev --sha256 <hex>` verifies a resolved Fast Mode boot media file against an expected SHA-256 digest supplied by the user. It computes the digest of the already-resolved installer, kernel, initrd, or macOS restore file, reports whether the digest matched, and writes the result under `.vmbridge/metadata/boot-media/<kind>-verify.json`. It does not download media. When more than one boot media path exists, use `--kind installer-image|kernel|initrd|macos-restore-image` to select the entry to verify.

`lightvm-runner ubuntu-dev --print-plan` prints the structured `AppleVzLaunchSpec` JSON that the Apple VZ launcher boundary consumes. With `--write-metadata`, it writes the same spec to `metadata/apple-vz-launch.json` and records that path in `metadata/runner.json` as `launch_spec_path`. With `--require-ready`, it writes the launch spec artifact and exits with a readiness error if any blocker remains; this is a launcher-boundary gate, not a real Apple VZ process start by itself. The spec includes guest identity, boot mode, installer/kernel/restore-image inputs with resolved paths and existence flags, active disk path and format, resources, device flags, integration flags, log paths, and launch readiness. Readiness covers both bundle inputs and host launch capability: missing disk/media paths remain path blockers, while a non-macOS host or non-Apple-Silicon host is reported as a capability blocker. A ready result means the dry-run inputs and host capability look launchable, while a blocked result preserves each blocker with its kind, affected path or capability, and a caller-facing remediation hint. The same readiness result is copied into dry-run runner metadata rather than being recomputed differently by the CLI, daemon, or dashboard.

`lightvm-runner --launch-spec <path>` is the first artifact-consumer boundary for
the Apple VZ backend. Instead of rebuilding the plan from a VM manifest, it
reads an existing `metadata/apple-vz-launch.json` file and can run the same
`--require-ready` gate against that artifact. `--print-handoff` emits a concise
`AppleVzLaunchHandoff` JSON with the backend name, VM identity, bundle path,
launch spec path, guest, boot mode, disk, resources, log paths, integration
flags, and readiness object. `--launch` now consumes that same handoff through
the Apple VZ launcher interface after the readiness gate, so the handoff is the
stable boundary between artifact preparation and `VZVirtualMachine`
construction.

The macOS Swift package also contains an `AppleVzRunner` helper that decodes and
validates the same handoff JSON in `--validate-only` mode and can print a
configuration plan with `--print-config-plan`. Passing
`--apple-vz-runner <path>` makes the Rust launcher send the handoff JSON to that
helper over stdin instead of using the in-process unsupported launcher. Helper
stdout/stderr from a successful launch is relayed by `lightvm-runner`, so a
bounded manual live run can preserve the helper start/finish transcript in
`apple-vz-live-launch.output` for later evidence review. The default Rust
launcher still returns an explicit unimplemented launch result. The
Swift helper now has a limited real launch path for the supported
`linux-kernel` + `raw` disk + NAT shape; unsupported shapes, such as Linux
installer mode, fail before `VZVirtualMachine.start()` with a clear
unsupported-input error. Even for the supported shape, the helper requires the
explicit `--allow-real-vz-start` opt-in before calling
`VZVirtualMachine.start()`, so a ready handoff sent to the Swift helper remains
validation/configuration work by default rather than an accidental live VM
start. For manual live boot E2E work, `AppleVzRunner` also accepts
`--stop-after-seconds <N>`; `lightvm-runner` forwards this as
`--apple-vz-stop-after-seconds <N>` so a successful test fixture can request a
guest stop instead of waiting forever. If the guest ignores that request,
`--force-stop-grace-seconds <N>` (or
`lightvm-runner --apple-vz-force-stop-grace-seconds <N>`) force-stops the VM
after the grace period.

The SwiftPM-built helper is not automatically signed with the virtualization
entitlement. For local live E2E, build and sign the helper explicitly:

```sh
APPLE_VZ_RUNNER_BIN="$(apps/macos/scripts/build-sign-apple-vz-runner.sh)"
codesign -d --entitlements :- "$APPLE_VZ_RUNNER_BIN"
```

The Swift configuration and launch boundary is intentionally narrower than dry-run readiness. `AppleVzRunner` can validate handoff/readiness, print the planned configuration, construct or validate the limited Apple VZ configuration shape, and start a `VZVirtualMachine` only for `linux-kernel` boot with a `raw` primary disk, NAT networking, and the explicit `--allow-real-vz-start` opt-in. `qcow2` is still allowed in readiness and plan output so callers can see the intended disk and blocker state, but actual VZ configuration construction supports `raw` disks only. Linux installer, macOS restore, non-NAT networking, and other disk formats remain outside the real configuration and launch surface until later work.

Fast Mode resource planning now runs through the shared resource-manager scaffold before the Apple VZ launch spec is emitted. When manifest memory or CPU is `auto`, BridgeVM resolves those values deterministically from `resources.profile`. Explicit manifest memory or CPU values are preserved. The launch spec also includes the selected `display_fps_cap` and `rationale` from the resource decision so callers can inspect the planning policy without starting the default Apple VZ launcher path.

For running Fast Mode VMs, `bridgevm resources reapply <vm> --visibility foreground|background` re-evaluates the policy using the current host battery state and records `metadata/runtime-resources.json`. This is available over local CLI and the daemon socket. The record includes `live_applied` and `live_apply_blockers`; today it reports `runtime-control-unavailable` because the live Apple VZ/display control IPC that would consume the policy has not been implemented yet.

`bridgevm ssh ubuntu-dev [--user USER]` is a metadata-only SSH planner. It does
not execute `ssh`; when connected guest-tools runtime metadata reports a valid
guest IP, it can print `ssh USER@<guest-ip>`.

The optional manifest `boot` section is the dry-run contract for Apple VZ launch planning:

```yaml
boot:
  mode: linux-installer
  installerImage: installers/ubuntu-arm64.iso
```

Supported Fast Mode boot modes are `existing-disk`, `linux-installer`, `linux-kernel`, and `macos-restore`. Linux installer mode requires `installerImage`; Linux kernel mode requires `kernelPath` and can include `initrdPath` plus `kernelCommandLine`; macOS restore mode requires `macosRestoreImage`. Relative paths are resolved against the `.vmbridge` bundle. Missing media is reported through the launch spec's `exists` flag and through `bridgevm boot-media <vm>` so template/download flows can be planned before the file is present. `bridgevm media import <vm> --source <path>` stays on the local-file side of that boundary: it copies a caller-supplied file to the resolved expected path, records metadata in `.vmbridge/metadata/boot-media/<kind>.json`, but does not fetch or choose OS downloads. `bridgevm media status <vm>` reads the same resolved entries and metadata back as a concise status view with paths, existence states, file sizes, last import details, last verification result, last download plan, and last download result. `bridgevm media verify <vm> --sha256 <hex>` also stays inside this local boundary: it hashes the resolved file, compares it with the caller-provided expected digest, and records the verification result in `.vmbridge/metadata/boot-media/<kind>-verify.json`. `bridgevm media download-plan <vm> --url <url> [--sha256 <hex>]` records remote download intent metadata under `.vmbridge/metadata/boot-media/<kind>-download.json` with the provided URL, resolved destination, optional expected digest, current file existence and size, and latest import/verify state; it does not perform the network download. `bridgevm media download <vm>` executes that recorded plan by fetching the stored URL to the stored destination, checking the optional expected digest, and recording the outcome under `.vmbridge/metadata/boot-media/<kind>-download-result.json`.

`bridgevm-core` also exposes the first template hint layer. `bridgevm recommend --os ubuntu --arch arm64` reports a stable hint id, source, and default Linux installer path, `bridgevm templates` lists the same metadata-only entries through the CLI or daemon socket, and `bridgevm create <name> --template <id>` can fill omitted guest OS, arch, and boot media metadata from a chosen template. The macOS dashboard uses the daemon `list_templates`/`create_vm` form of this same flow instead of duplicating template logic in Swift. `bridgevm --socket <sock> boot-media <vm>` is available for the same direct inspection path over the daemon socket, `bridgevm --socket <sock> media import <vm> --source <path>` is available for the same local import operation, `bridgevm --socket <sock> media status <vm>` is available for the same status summary, `bridgevm --socket <sock> media verify <vm> --sha256 <hex>` is available for the same SHA-256 comparison, `bridgevm --socket <sock> media download-plan <vm> --url <url> [--sha256 <hex>]` is available for the same no-download intent record, and `bridgevm --socket <sock> media download <vm>` is available for the same recorded-plan download execution. When creating from explicit OS/arch instead, `bridgevm create` applies a matching hint automatically when no explicit boot flags are provided. Listing or using templates never downloads installer or restore media. Current defaults are:

| Hint id | Guest | Boot mode | Media path |
| --- | --- | --- | --- |
| `ubuntu-arm64-installer` | Ubuntu Arm64 | `linux-installer` | `installers/ubuntu-arm64.iso` |
| `fedora-arm64-installer` | Fedora Arm64 | `linux-installer` | `installers/fedora-arm64.iso` |
| `debian-arm64-installer` | Debian Arm64 | `linux-installer` | `installers/debian-arm64.iso` |
| `macos-restore` | macOS Arm | `macos-restore` | `installers/macos-restore.ipsw` |

For `qcow2` primary disks, the scaffold does not create or validate the image yet. Missing disks are reported with a `qemu-img create -f qcow2 <path> <size>` command in metadata/output, and `qcow2` remains a dry-run readiness/plan format only for the Apple VZ path. For `raw` primary disks, the missing disk can be created directly as a sparse file and raw is the only disk format currently supported by limited Swift VZ configuration construction. Planned media download execution now exists. Fast Mode spawn keeps the legacy blocker code when no signed Swift helper is configured, but the user-facing failure is now runner-required: `BRIDGEVM_APPLE_VZ_RUNNER` must point at a signed `AppleVzRunner`. With the helper configured, `run --spawn` can cross into the limited Apple VZ helper boundary with the same explicit opt-in requirements as the helper itself. A spawn-facing readiness check may fail before that boundary with structured blockers, and `run --spawn` includes a concise blocker summary in its failure message while still writing dry-run runner metadata for blocked launches. Only an explicit helper launch with `--allow-real-vz-start` can attempt the limited real launch shape today. A live boot E2E also needs a launch-ready `linux-kernel` spec with real kernel/initrd/raw disk fixtures and the required Apple virtualization entitlement; ordinary smoke coverage should stay on validate-only, config-validation, unsupported-input, missing-opt-in paths, or synthetic evidence verification.

### Live evidence review

`tests/integration/apple-vz-live-boot-opt-in-smoke.sh` is the manual opt-in harness for the live path and skips unless the required fixture environment variables are present. `tests/integration/prepare-apple-vz-debian-fixture.sh` prepares a Debian arm64 netboot `linux` Image, `initrd.gz`, and sparse `root.raw` for that harness; Debian is the preferred live fixture because its netboot `linux` is a raw arm64 Linux kernel image accepted by VZ LinuxBootLoader.

Concise manual live proof sequence:

```sh
eval "$(tests/integration/prepare-apple-vz-debian-fixture.sh)"
export BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1
tests/integration/apple-vz-live-boot-opt-in-smoke.sh | tee /tmp/bridgevm-live-vz-smoke.out
EVIDENCE_DIR="$(awk -F': ' '/^Evidence: / {print $2}' /tmp/bridgevm-live-vz-smoke.out | tail -n 1)"
STORE="$(awk -F': ' '/^PASS: Apple VZ live boot opt-in smoke [(]/ {gsub(/[)]$/, "", $2); print $2}' /tmp/bridgevm-live-vz-smoke.out | tail -n 1)"
tests/integration/verify-apple-vz-live-evidence.sh "$EVIDENCE_DIR"
bridgevm --store "$STORE" readiness live-vz-linux --live-evidence "$EVIDENCE_DIR" --record-live-evidence
bridgevm --store "$STORE" readiness live-vz-linux
```

The Debian fixture helper only prepares kernel/initrd/raw-disk inputs and prints
shell-safe exports; it does not set
`BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1`. The smoke prints the actual evidence
directory it created, and the verifier must accept that directory before
`bridgevm readiness --record-live-evidence` preserves it in the harness-created
temporary `live-vz-linux` VM bundle. Recording the smoke evidence onto an
arbitrary existing VM is unsupported unless its name and bundle path match the
preserved launch spec. A live
proof needs more than successful process start/stop output: keep serial sentinel
evidence when `BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED` is set, or provide a
verifier-bound graphical boot-progress artifact with
`BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_FRAME` plus width, height, stage, and marker
metadata. Ordinary viewer frames and QMP state can support console diagnostics,
but they should not be treated as guest boot progress by themselves.

Live attempts preserve `$STORE/evidence` with:

- `SUMMARY.txt` status and artifact path lines
- fixture manifest source and bundle paths, sizes, and SHA-256 digests
- `environment.txt` with the source fixture paths, selected resources, kernel command line, and runner path used for the attempt
- the launch spec and handoff JSON consumed at the Apple VZ boundary
- the selected helper path, copied `AppleVzRunner` artifact, validation output,
  and live-launch output
- runner and serial log pointers, plus serial sentinel evidence when configured
- optional `boot-progress-evidence.json` plus copied graphical boot-progress frame when
  `BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_FRAME`, width, height, stage, and marker values
  are provided
- optional `viewer-evidence.json` plus copied viewer frame when
  `BRIDGEVM_LIVE_VZ_VIEWER_FRAME`, width, and height are provided
- optional `guest-tools-effects.json` copied from
  `BRIDGEVM_LIVE_VZ_GUEST_TOOLS_EFFECTS_JSON` when separate observable
  guest-tools effect evidence was produced

After `tests/integration/verify-apple-vz-live-evidence.sh "$STORE/evidence"`
accepts the bundle, `bridgevm readiness <vm> --live-evidence "$STORE/evidence"
--record-live-evidence` can preserve the verified evidence inside the VM bundle
for later metadata-only readiness review. Later plain `bridgevm readiness <vm>`
re-runs the verifier against `.vmbridge/metadata/live-evidence/latest`;
`bridgevm readiness <vm> --clear-live-evidence` removes that preserved evidence
metadata and copied bundle.

The live smoke runs `tests/integration/verify-apple-vz-live-evidence.sh "$STORE/evidence"` before printing `PASS`. Reviewers can rerun the same verifier against a preserved bundle before treating a live smoke result as proof. The verifier checks the summary, fixture manifest, environment, launch spec, handoff, selected runner path or copied runner artifact, validation output, launch output, configured serial sentinel evidence, and optional graphical boot-progress artifact as a connected set for opted-in real Apple VZ runs. It cross-checks `environment.txt` against the fixture manifest source paths, the launch spec kernel command line and resources, and the selected runner path. It also treats artifact path lines in `SUMMARY.txt` as assertions that must resolve to the preserved evidence artifacts, not just as human-readable labels. The `Store`, `Bundle`, `Launch spec`, `Handoff JSON`, output path, runner/serial log, `Fixture manifest`, and `Environment` lines must resolve to the evidence fields and artifacts they name.

That live evidence bundle is not guest-tools-effects proof by default. A
future/current preserved-evidence path may prove `guest-tools-effects` only when
the bundle includes guest-tools result artifacts that the verifier explicitly
checks against observable guest-side effects. Authenticated command dispatch,
pending-count tracking, or `last_command_result` metadata alone remain protocol
or status evidence, not proof that a guest-side file, clipboard, display,
application, shared-folder mount, or other requested effect actually changed.
The opt-in harness copies a provided `BRIDGEVM_LIVE_VZ_GUEST_TOOLS_EFFECTS_JSON`
file into the evidence directory, but the verifier still decides whether it is
valid proof. If that JSON references effect artifacts, the harness copies those
files into the evidence directory, checks or fills their SHA-256 digests, and
rewrites artifact paths to relative evidence paths before verification.

The same verifier cross-checks the bounded live controls:
`BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS` and
`BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS` must be positive integers and match
the values recorded in `SUMMARY.txt` and the live-launch transcript.

`tests/integration/apple-vz-live-evidence-verifier-smoke.sh` covers the verifier with synthetic evidence only; it does not start a live VM, QEMU, Apple VZ, or a GUI, and the actual live proof still requires the separate opt-in smoke. Set `BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED` to a known fixture sentinel or provide `BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_FRAME` with matching metadata so the smoke proves guest boot progress, not just successful VM start/stop calls.
