# BridgeVM

[![CI](https://github.com/Ketchio-dev/bridgevm/actions/workflows/ci.yml/badge.svg)](https://github.com/Ketchio-dev/bridgevm/actions/workflows/ci.yml)
[![Security and quality](https://github.com/Ketchio-dev/bridgevm/actions/workflows/security-quality.yml/badge.svg)](https://github.com/Ketchio-dev/bridgevm/actions/workflows/security-quality.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**Windows 11 Arm on Apple silicon, in a native Mac app.**

BridgeVM runs Windows alongside macOS using its own
[Hypervisor.framework VMM](crates/bridgevm-hvf/). The main Windows engine runs
without QEMU. Apple VZ and QEMU compatibility backends are separate paths.

**Current access: source builds · Engineering Preview · 3D-off.**
A supported public download is still being prepared. The terminal installer
refuses the superseded `v1.0.0` preview.

**[Build from source](docs/install.md#build-the-current-source)** ·
**[Set up Windows](docs/install.md#what-you-need-after-installing)** ·
**[Current status](STATUS.md)** · **[Contribute](CONTRIBUTING.md)**

<p align="center">
  <a href="docs/media/windows-hvf-desktop-20261116.webp">
    <img src="docs/media/windows-hvf-desktop-20261116.webp" alt="Windows 11 Arm desktop on BridgeVM's native HVF engine, captured on an M5 development Mac at 2560×1600 on November 16, 2026; experimental test-signed display driver enabled" width="960">
  </a><br>
  <sub>Actual development VM · November 16, 2026 · 2560×1600 lossless capture · <a href="docs/media/windows-hvf-desktop-20261116.webp">Open full resolution</a></sub><br>
  <sub>The visible Windows Test Mode watermark belongs to this VM's experimental display driver; this is not a public release screenshot.</sub>
</p>

## What you can try

| Area | Current scope |
| --- | --- |
| Windows desktop | Windows 11 Arm on BridgeVM's native HVF engine; create a VM from an ISO or import an installed disk. |
| Display and input | A native Mac window, keyboard and mouse, local text composition, and configurable Command-key mapping. Window-following resolution needs the guest display driver and a restart. |
| Mac integration | Clipboard sharing, shared folders, networking and audio output. Display smoothness and audio continuity still need broader validation. |
| VM lifecycle | Start, shut down and restart; persistent storage and TPM/Secure Boot workflows; [powered-off snapshots](docs/windows-arm/snapshot-scope-v1.md). Running-state suspend is outside v1. |

These are preview capabilities. Setup on a genuinely clean Mac and recovery
across interrupted storage operations remain open release criteria. The
[capability matrix](docs/windows-arm/capability-matrix.md) records the evidence
for each feature.

Current source uses queued text dispatch for compatible Windows editors; long
Korean and emoji input, selection replacement and undo were checked in Windows
11 Notepad. Older builds and unsupported editors retain the input path tracked
in [#665](https://github.com/Ketchio-dev/bridgevm/issues/665), where long commits
can repeat or lose characters. Use clipboard sharing and paste if this occurs.

## Try BridgeVM

**Host:** Apple silicon and macOS 14+. The declared host matrix is M4/M5;
M1–M3 are not yet covered. Source builds require Xcode/Swift, Rust 1.85+ and
pinned native dependencies. Follow the complete
[build and packaging guide](docs/install.md#build-the-current-source);
`cargo build` and `swift build` alone do not prepare a Windows-installation app.

Choose the Windows setup path that matches what you have:

| Starting point | What you provide |
| --- | --- |
| **Install from ISO** | Your own licensed Windows 11 Arm ISO, plus signed ARM64 storage/serial/network drivers and their external SHA-256 manifest. An ISO alone is insufficient. |
| **Import an installed VM** | A Windows 11 Arm disk in RAW, QCOW2 or Hyper-V VHDX format. Import needs no driver payload. |

The [installation guide](docs/install.md) covers both paths, required bundle
resources and first-open trust. After Windows starts, install the signed guest
display driver from the VM's settings and restart to enable window-following
resolution.

Source builds use local development signing. Developer ID signing and Apple
notarization for public distribution remain unfinished. Standard preview
packaging includes no Windows test driver and requires no TESTSIGNING setup.

## Engineering Preview

Current work focuses on reliable Retina/window transitions, input and clipboard
ordering, audio/display continuity, and lifecycle recovery. Experimental
[selectable virtual refresh rates](https://github.com/Ketchio-dev/bridgevm/pull/660)
are being developed with a separate Windows driver. They are not a supported
General Preview feature; a virtual Hz setting does not establish rendered FPS
or a physical display's refresh rate. GPU/3D acceleration remains a
[Graphics Lab research path](docs/distribution-channels.md).
That experimental path still has
[known title, tab and menu glyph defects](docs/windows-arm/evidence/windows-glyph-text-integer-attributes-20260814.md).

The dated registry snapshot below is the detailed validation record. Later code
changes need their own checks and hardware evidence.

<details>
<summary>Detailed capability status and known defects</summary>

<!-- BEGIN GENERATED: capability-summary -->
**Product state: Engineering Preview.** Runs an installed Windows 11 Arm desktop on BridgeVM's own Hypervisor.framework VMM with persistent storage, display/input, network, audio, clipboard and folder integration, TPM/Secure Boot workflows, snapshots and window Coherence verbs. 3D acceleration is excluded from General Preview and v1; release-blocking evidence remains open and known defects are disclosed below.

Release scenarios met: **2 / 5**.

| ID | Scenario | State | Criteria | Known defects |
| --- | --- | --- | --- | --- |
| S1 | Get Windows running | `OPEN` | A9 (open), A1, A10 | **A9**: No retained clean-machine product-flow receipt yet proves either ISO installation or installed-disk import through the app. Both supported flows remain 3D-off. In the 3D-off configuration the app's live display window was choppy in recent hardware runs; its smoothness has not been measured since. Guest audio stuttered audibly: until 2026-11-03 every other 10 ms of Windows playback reached the host as silence, because the sound controller answered Windows' position reads at the 0x2084 alias with 0. Since that fix, 60-second live runs had 2-6 silent 10 ms blocks out of about 6,330, and under host load 0-8 blocks per run still repeated stale audio; at the v1.1.0 release head (2026-11-14) six such runs had 2-6 silent and 0-3 stale blocks out of about 6,400, and one of them had 2 host underrun callbacks. No listening check has confirmed the stutter is gone. |
| S2 | Use the Windows desktop | `OPEN` | A4, B4, A5, A8 | The Windows display follows the window size only after the one-time display driver install in the VM view (Red Hat virtio-win, Microsoft-signed, downloaded by the guest) and a VM restart; until then the resolution is fixed per VM before it starts (1280x800 by default, up to 2560x1600). The app window was choppy in earlier runs and its smoothness has not been measured since, and no listening check has confirmed that the audio stutter is gone (see A9). |
| S3 | Work with the Mac | `MET` | A6, A7 | none |
| S4 | Keep the VM safe | `OPEN` | A19 (open), A15, A16, A12 | **A19**: Legacy managed storage relocated before identity migration is refused. Full interrupted-operation coverage is unproven. The ten-lane product lifecycle campaign passed 10/10 through the live queue at 949f4c26, which the operator accepted before a release head was designated, and 10/10 again on 2026-11-14 on the earlier v1.1.0 candidate app f73b66bf, and 10/10 on 2026-11-19 on the v1.1.0 candidate app 6647c818 (release dry run 44563308193, installed through the installer), each run locally without a queue seal. Physical power loss during a snapshot operation is outside this criterion and is not tested. |
| S5 | Go online | `MET` | A20 | none |

Release-blocking criteria proven: **12 / 14**. Open: A9, A19.

Other known open defects:
- **B6**: Window title, tab and menu glyphs can be blank on the experimental Windows graphics path; body text alone does not prove glyph correctness.

- Graphics future path: Vulkan is a Graphics Lab future path, excluded from General Preview and v1; D3D11 compatibility is a Graphics Lab future path, excluded from General Preview and v1.
- Guest platform: QEMU virt-compatible guest contract with documented deviations.

State reviewed 2026-11-19 at commit `035665990784fe98a332e87eec0387e05b964fb0`. This block is generated from [`capabilities/windows-hvf.json`](capabilities/windows-hvf.json) by `scripts/render-capability-status.py`.
<!-- END GENERATED: capability-summary -->

</details>

## Develop the VMM

Control test VMs from the terminal with the [native app CLI](docs/app-cli.md)
and [guest development CLI](docs/vmctl.md): start/stop/status, PowerShell,
keyboard/mouse input, display-mode requests and 2D PNG captures.

Start with [CONTRIBUTING.md](CONTRIBUTING.md) for toolchains and focused checks,
[AGENTS.md](AGENTS.md) for working rules, and the
[machine contract](docs/machine-contract/qemu-virt.md) for guest-visible behavior.

| Area | Source |
| --- | --- |
| Hypervisor.framework VMM and devices | [`crates/bridgevm-hvf/`](crates/bridgevm-hvf/) |
| Windows HVF runtime lifecycle | [`crates/bridgevm-hvf-runtime/`](crates/bridgevm-hvf-runtime/), [`runners/`](runners/) |
| Mac app and helper boundaries | [`apps/macos/`](apps/macos/) |
| Packaging and release verification | [`packaging/macos/`](packaging/macos/) |
| Deterministic checks and live tests | [`scripts/`](scripts/), [`tests/integration/`](tests/integration/) |

Use focused tests while making a change. The fast project check covers
formatting, contracts and documentation; release candidates run the full check:

```sh
scripts/check-project.sh --fast
scripts/check-project.sh
```

Deterministic CI runs on GitHub-hosted runners. Windows boots, HVF behavior and
clean-machine product journeys need the corresponding
[physical-Mac tests](docs/testing/apple-silicon-live-gates.md).

## Engine boundaries

| Engine | Backend | Focus |
| --- | --- | --- |
| **Windows HVF** | Hypervisor.framework + BridgeVM device model | Main Windows 11 Arm path |
| **Apple VZ** | Virtualization.framework | Narrow Linux/macOS Arm path |
| **Compatibility** | QEMU + HVF/TCG | Compatibility and architecture emulation |

Windows HVF uses a QEMU `virt`-compatible guest contract with
[documented deviations](docs/machine-contract/qemu-virt-deviations.json).
Its RAW, QCOW2 and VHDX import uses BridgeVM's own converter; compressed,
encrypted or backing-file disks need conversion first. The Compatibility
Engine uses QEMU and its `qemu-img` disk tooling.

## More information

- [Documentation](docs/README.md) — guides, plans and dated engineering records.
- [Current status](STATUS.md) — priorities, evidence and known limitations.
- [Security model](docs/security/model.md) and [private vulnerability reporting](SECURITY.md).
- [Good first issues](https://github.com/Ketchio-dev/bridgevm/labels/good%20first%20issue)
  and [contribution guide](CONTRIBUTING.md).

## License

BridgeVM is licensed under [Apache-2.0](LICENSE). Third-party components retain
their own licenses; see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) and the
[licensing guide](docs/licensing-and-attribution.md).

Copyright © 2026 Ketchio-dev.
