# BridgeVM

[![CI](https://github.com/Ketchio-dev/bridgevm/actions/workflows/ci.yml/badge.svg)](https://github.com/Ketchio-dev/bridgevm/actions/workflows/ci.yml)
[![Security and quality](https://github.com/Ketchio-dev/bridgevm/actions/workflows/security-quality.yml/badge.svg)](https://github.com/Ketchio-dev/bridgevm/actions/workflows/security-quality.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**Run Windows 11 Arm on Apple silicon with a Mac-native, QEMU-free
Hypervisor.framework VMM.**

<p align="center">
  <img src="docs/media/windows-hvf-boot.gif" alt="Windows 11 Arm booting to the desktop on BridgeVM's Hypervisor.framework VMM" width="800">
</p>

BridgeVM is built for people who want to use, inspect, and improve a native
virtualization stack. It includes persistent storage, display and input,
networking, audio, guest integration, TPM/Secure Boot workflows, snapshots, and
an explicitly experimental graphics path.

> [!IMPORTANT]
> BridgeVM is an **Engineering Preview**, not a production VM product. Bring
> your own licensed Windows 11 Arm ISO. The general download does not contain a
> Windows test driver, does not enable TESTSIGNING, and installs Windows with 3D
> injection disabled.

## Start in one command

On an Apple-silicon Mac running macOS 14 or newer:

```sh
curl -fsSL https://raw.githubusercontent.com/Ketchio-dev/bridgevm/main/install.sh | bash -s -- --launch
```

The installer downloads the newest **General Preview**, verifies its release
contract, SHA-256 checksum, archive layout, app identity, architecture, and
ad-hoc code-signing seal, then installs it in `/Applications`. It never touches
VM data during an install or update.

After the app opens:

1. Choose **Windows → Install from ISO**.
2. Select your Windows 11 Arm ISO and VM size.
3. Create the VM and let the unattended installation finish.

> [!WARNING]
> The published `v1.0.0` predates the current fail-closed driver policy and is
> no longer recommended. Until its safe successor is published, the installer
> intentionally refuses that legacy release instead of silently installing it.
> See [installation options](docs/install.md) to build the current source.

Prefer a DMG or want to inspect every verification step? Read the
[installation guide](docs/install.md).

## Pick the right channel

| Channel | Intended for | Windows graphics policy | Security boundary |
| --- | --- | --- | --- |
| **General Preview** | Users and contributors | 3D driver injection is unavailable | No Windows test driver; TESTSIGNING is not enabled; Secure Boot policy is not weakened |
| **Graphics Lab** | Driver developers on disposable test VMs | Opt-in test-signed experimental package | Separate tooling and evidence only; may require TESTSIGNING and a different Secure Boot posture |

The Graphics Lab package is not a production-signing substitute and is not
bundled into the General Preview. Its B4 result proves the exact test package's
pointer behavior; it does not close the A9 Microsoft kernel-policy signing
requirement. Read the full [distribution channel contract](docs/distribution-channels.md)
before using it.

## What works today

- installed Windows 11 Arm desktop on BridgeVM's own Hypervisor.framework VMM;
- persistent NVMe storage and UEFI variable state;
- four-vCPU execution, reset/recreate lifecycle, and clean shutdown;
- keyboard, absolute pointer, dynamic resize, network, host audio, clipboard,
  folder transfer, and guest-agent control;
- TPM 2.0, Secure Boot and measured-boot workflows, encrypted vTPM state,
  recovery/migration, and powered-off snapshots;
- experimental Vulkan and D3D11-compatible paths in the separate engineering
  evidence track;
- deterministic hosted CI plus sealed real-hardware receipts for behavior that
  CI cannot prove.

BridgeVM distinguishes code that compiles, deterministic tests, and behavior
observed in a real guest. The snapshot below is generated from the capability
registry; it is the product wording source of truth.

<!-- BEGIN GENERATED: capability-summary -->
**Product state: Engineering Preview.** Runs an installed Windows 11 Arm desktop on BridgeVM's own Hypervisor.framework VMM with persistent storage, display/input, dynamic resolution, network, audio, clipboard and folder integration, TPM/Secure Boot workflows, snapshots, window Coherence verbs and experimental 3D. Release-blocking evidence remains open; known defects are disclosed below.

Release-blocking criteria proven: **18 / 19**. Open: A9.

Known open defects:
- **A9**: Windows-HVF 3D driver injection is unavailable for install and import: signed kernel-policy provenance and a clean-machine installation flow have not been proven. The product exposes only 3D-off install/import.

- Graphics: Experimental Vulkan path and Experimental D3D11-compatible subset.
- Guest platform: QEMU virt-compatible guest contract with documented deviations.

State reviewed 2026-08-30 at commit `e38ba7c9dc252d13977a4e3c69564cc88abb1b35`. This block is generated from [`capabilities/windows-hvf.json`](capabilities/windows-hvf.json) by `scripts/render-capability-status.py`.
<!-- END GENERATED: capability-summary -->

See the [current status](STATUS.md) and
[Windows capability matrix](docs/windows-arm/capability-matrix.md) for the
fixed thresholds and retained receipts.

## Known boundaries

- The Mac app is ad-hoc signed, not Developer ID signed or Apple-notarized.
  Browser-downloaded DMGs therefore need the documented one-time **Open
  Anyway** step. The terminal installer states its narrower trust model and
  verifies every artifact before replacement.
- Windows-HVF install and import are deliberately 3D-off while A9 remains
  open. A user-provided ISO does not change Windows kernel-driver trust rules.
- Experimental graphics compatibility is much narrower than “all Vulkan or
  D3D11 software works.”
- On the accelerated path, body text renders but some window titles, tabs, and
  menus can be blank. The retained investigation is
  [documented here](docs/windows-arm/evidence/windows-glyph-text-integer-attributes-20260814.md).
- Running-state suspend is outside the v1 scope; powered-off snapshots are the
  supported persistence boundary.

## Future direction

Production Windows 3D driver injection is a future opportunity, not an active
release commitment. Work can resume when BridgeVM has an organization or
partner able to provide a Microsoft kernel-policy-signed ARM64 package. Both
ISO install and installed-disk import must then verify that same package before
changing a VM, followed by a retained clean-machine product-flow receipt.

Until those external prerequisites exist, A9 remains OPEN, the General Preview
stays 3D-off, and Graphics Lab results are not treated as production-signing
evidence.

## Build from source

Requirements: Apple silicon, macOS 14+, Xcode/Swift 5.9+, and Rust 1.85+.

```sh
git clone https://github.com/Ketchio-dev/bridgevm.git
cd bridgevm
cargo build --workspace --locked
swift build --package-path apps/macos
packaging/macos/build-debug-app-bundle.sh
open target/macos/BridgeVMApp.app
```

QEMU is needed only for the Compatibility Engine. The self-contained Windows
HVF bundle has additional host dependencies checked by its packaging scripts.
See [Contributing](CONTRIBUTING.md) for focused setup and verification paths.

## Three engines, three jobs

| Engine | Backend | Use it for |
| --- | --- | --- |
| **Windows HVF** | Hypervisor.framework + BridgeVM device model | Windows 11 Arm on Apple silicon; main engineering focus |
| **Apple VZ** | Virtualization.framework | Narrow, lightweight Linux/macOS Arm guests |
| **Compatibility** | QEMU + HVF/TCG | Broad guest support and architecture emulation |

The Windows guest contract is QEMU `virt`-compatible with
[documented deviations](docs/machine-contract/qemu-virt-deviations.json); it is
not described as a bit-for-bit QEMU implementation.

## Contribute

Bug reports, documentation fixes, tests, and focused code changes are welcome.
Start with a
[good first issue](https://github.com/Ketchio-dev/bridgevm/labels/good%20first%20issue)
or run the fast deterministic gate:

```sh
scripts/check-project.sh --fast
```

Read [CONTRIBUTING.md](CONTRIBUTING.md) before a larger change. It explains the
repository map, evidence levels, private-media boundary, formatting, tests, and
how to choose between hosted CI and a real-hardware gate. Security issues belong
in the private process described in [SECURITY.md](SECURITY.md), not a public
issue.

## Project map

```text
apps/macos/                  SwiftUI apps and signed helper boundaries
crates/bridgevm-hvf/         custom Hypervisor.framework VMM and devices
crates/bridgevm-hvf-runtime/ typed Windows HVF runtime lifecycle
crates/bridgevm-{cli,core}/  CLI and shared product model
runners/                     supervised VM-engine processes
packaging/macos/             app, DMG, and release verification
scripts/                     deterministic tooling and sealed live gates
tests/integration/           product-policy and integration checks
docs/                        current guides, decisions, and dated evidence
```

## Documentation

- [Install and update](docs/install.md)
- [Distribution channels](docs/distribution-channels.md)
- [Current status](STATUS.md)
- [Windows 11 Arm guide](docs/windows-arm/README.md)
- [Security model](docs/security/model.md)
- [Documentation index](docs/README.md)

Run the complete deterministic gate before treating a repository change as
done:

```sh
scripts/check-project.sh
```

Real Windows boots, graphics workloads, Hypervisor.framework behavior, and
clean-machine distribution behavior require the corresponding sealed live gate;
hosted CI is not a substitute.

## License

BridgeVM is licensed under [Apache-2.0](LICENSE). Third-party components retain
their own licenses; see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) and the
[licensing guide](docs/licensing-and-attribution.md).

Copyright © 2026 Ketchio-dev.
