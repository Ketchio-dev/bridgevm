# BridgeVM

BridgeVM is an open-source, Mac-native virtualization project with three
deliberately separate engines:

| Engine | Backend | Intended use | Current boundary |
| --- | --- | --- | --- |
| Compatibility | QEMU with HVF/TCG | Broad guest support, legacy systems, emulation, expert controls | Implemented and supervised; some macOS network modes still need privileges or entitlements. |
| Apple VZ | Virtualization.framework | Lightweight Linux/macOS Arm guests | Live Linux kernel/raw-disk path is implemented; installer, qcow2, and integration coverage remain narrower. |
| Windows HVF | Hypervisor.framework plus BridgeVM devices | High-performance Windows 11 Arm without QEMU | Boots an installed Windows desktop with storage, display/input, network, audio, guest agent, and experimental 3D. It is not release-ready. |

The Windows HVF path is the current engineering focus. It has progressed well
beyond a firmware or installer scaffold, but production vTPM/Secure Boot,
distributable Windows driver signing, and fresh release evidence are still open
gates. See [Current status](STATUS.md) for the short, evidence-backed boundary.

## What BridgeVM is—and is not

BridgeVM aims for a focused, fast experience on Apple silicon without giving up
QEMU's compatibility when it is useful. Fast paths are separate engines, not a
rebranding of QEMU with fewer settings.

This repository is currently an engineering preview. Do not treat it as a
production replacement for Parallels Desktop, VMware Fusion, or QEMU. Windows
HVF media formats and security lifecycle may still change before the first
public release.

## Start in five minutes

Requirements:

- macOS 14 or newer for the native app and Apple virtualization paths;
- Rust 1.76 or newer;
- Xcode/Swift 5.9 or newer for the macOS app;
- QEMU for Compatibility Engine live runs;
- Homebrew `swtpm` 0.10.1 / `libtpms` 0.10.2 when producing a Windows HVF app
  bundle; the resulting app carries its signed runtime and does not require a
  host installation.

Build and run the deterministic local checks:

```sh
cargo build --workspace
cargo test --workspace
cargo run -p bridgevm-cli -- doctor
```

Inspect the CLI without creating or starting a VM:

```sh
cargo run -p bridgevm-cli -- templates
cargo run -p bridgevm-cli -- recommend --os ubuntu --arch arm64
cargo run -p bridgevm-cli -- hvf windows-plan
```

Build the macOS targets:

```sh
swift build --package-path apps/macos
swift test --package-path apps/macos
```

Apple's Hypervisor.framework requires a correctly signed executable with the
hypervisor entitlement. Use the checked-in signing and packaging scripts for
live HVF work; a plain Cargo rebuild of the probe is not sufficient.

## Choose a path

### Compatibility Engine

Use this for QEMU-backed VMs, x86 emulation, unusual hardware, and broad OS
coverage. The safe planning flow is:

```sh
cargo run -p bridgevm-cli -- create legacy-linux \
  --os ubuntu --arch x86_64 --mode compatibility
cargo run -p bridgevm-cli -- disk prepare legacy-linux
cargo run -p bridgevm-cli -- disk create legacy-linux
cargo run -p bridgevm-cli -- disk inspect legacy-linux
cargo run -p bridgevm-cli -- prepare-run legacy-linux
```

Planning commands do not silently launch QEMU. See the
[Compatibility Engine guide](docs/compatibility-mode/README.md) before using
`run --spawn`.

### Apple VZ Engine

Use this for the narrow Linux/macOS Arm fast path. A reproducible Linux demo
bundle can be staged without starting a VM:

```sh
scripts/stage-vz-linux-demo-vm.sh --prepare-fixture --name vz-linux-demo
```

See the [Apple VZ guide](docs/fast-mode/README.md) for the supported live shape,
required runner, and display workflow.

### Windows HVF Engine

The custom Windows engine is a no-QEMU VMM built directly on
Hypervisor.framework. Its installed-image workflow is exposed through the
macOS **Windows HVF Lab** and checked-in launch scripts. It is intended for
development and evidence collection, not general users yet.

The app selects an aggressive, rollback-safe graphics policy for 3D runs.
Security state is not relaxed: vTPM identity, Secure Boot variables, and
BitLocker recovery behavior must remain fail-closed. Read the
[Windows completion plan](docs/hvf-windows-install-completion-plan.md) and
[architecture/risk policy](docs/hvf-competitive-architecture-and-risk-policy.md)
before a live run.

The current lab app defaults to the checked-in, reproducible 3 MiB AArch64
EDK2 build with Secure Boot and TPM2 enabled. To package it at a new output
path:

```sh
apps/macos/scripts/package-hvf-control-app.sh \
  --output /tmp/BridgeVMControl.app
```

The packager refuses to overwrite an existing app. It collects the complete
non-system dylib closure for the pinned `swtpm`/`libtpms`, rewrites Homebrew
install names, signs every artifact, embeds component licenses and SHA-256
inventory, and then verifies that no development-host path remains. The app
still fails closed rather than silently launching Windows without its vTPM.

## Current release gates

The remaining Windows HVF walls are concrete, but they are not all “easy”:

1. prove the locally wired TPM 2.0 TIS/PPI/TPM2-log contract, including the
   now-connected EDK2 PPI request processor, in Windows and capture
   firmware-populated measured-boot events;
2. validate the now-implemented encrypted recovery package, fresh-identity
   clone, same-ID move, and archive-before-reset lifecycle on a clean second
   Mac and with BitLocker recovery enabled;
3. prove the pinned Microsoft-only PK/KEK/db/dbx policy and measured boot from
   inside a fresh guest, then finish migration/recovery lifecycle handling;
4. produce and sign a fresh ARM64 Windows display-driver package;
5. capture same-boot bind, real-title, crash-free, and performance receipts on
   real Windows media;
6. complete public app signing/notarization and distribution validation.

Device plumbing is bounded engineering work. The security lifecycle, Windows
driver signing, and fresh hardware-backed evidence are the harder release
walls because they cross guest, host, credentials, and external toolchains.

Run the local product-gate report for the repository's current deterministic
classification:

```sh
tests/integration/product-gates-report.sh
```

## Repository map

```text
apps/macos/                 Swift macOS apps and signed runners
crates/bridgevm-hvf/        BridgeVM-owned Hypervisor.framework VMM
crates/bridgevm-{cli,core}/ CLI and shared product model
crates/bridgevm-qemu/       Compatibility Engine planning
crates/bridgevm-apple-vz/   Apple VZ launch planning
runners/                    Engine process boundaries
scripts/                    Packaging, live-run, and evidence tooling
tests/integration/          Deterministic product and integration gates
docs/                       Current guides, active plans, and history
```

## Documentation

Use the [documentation index](docs/README.md) as the source of truth for what
is current, active planning, reference material, or historical evidence. The
[development system](docs/development-system.md) defines stable gate IDs,
evidence levels, work packets, and the definition of done.
`PLAN.md` retains the long-form product architecture and historical roadmap; it
is not the quickest way to learn the current implementation.

## Contributing

Start with [Contributing](docs/contributing/README.md). Preserve the distinction
between deterministic tests and live evidence: a model/unit test must never be
reported as proof that Windows, Apple VZ, signing, or a hardware entitlement
worked on a real machine.

Before landing documentation changes, run:

```sh
bash scripts/check-documentation-system.sh
```

Licensed under Apache-2.0.
