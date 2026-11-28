# BridgeVM

[![CI](https://github.com/Ketchio-dev/bridgevm/actions/workflows/ci.yml/badge.svg)](https://github.com/Ketchio-dev/bridgevm/actions/workflows/ci.yml)
[![Security and quality](https://github.com/Ketchio-dev/bridgevm/actions/workflows/security-quality.yml/badge.svg)](https://github.com/Ketchio-dev/bridgevm/actions/workflows/security-quality.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**Windows 11 Arm on Apple silicon, in a native Mac app.**

BridgeVM runs Windows alongside macOS using its own
[Hypervisor.framework VMM](crates/bridgevm-hvf/), without QEMU. It is the
app's only engine.

**Current access: [General Preview v0.1.0](https://github.com/Ketchio-dev/bridgevm/releases/tag/v0.1.0) · Engineering Preview · 3D-off.**
The terminal installer and DMG both install v0.1.0, which opens in Korean.
The [English setup guide](docs/install.md#what-you-need-after-installing) matches
the buttons in that download. After opening BridgeVM, choose your Windows 11
Arm installer ISO or import an installed disk. The app is ad-hoc signed and has no Apple notarization.

```sh
curl -fsSL https://raw.githubusercontent.com/Ketchio-dev/bridgevm/main/install.sh | bash
```

**[Install guide](docs/install.md)** ·
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
| VM library | In source builds, choose **Compare VMs** in the toolbar to sort saved VMs by name, engine, CPU count or memory. Select a row and choose **Show VM** to open its dashboard. |
| Display and input | A native Mac window, keyboard and mouse, local text composition, and configurable Command-key mapping. Window-following resolution needs the guest display driver and a VM restart (shut down, then start). |
| Mac integration | Clipboard sharing, shared folders, networking and audio output. Known defects: the display window was choppy in recent runs and its smoothness has not been measured since; Windows audio stuttered audibly, and no listening check has confirmed the stutter is gone. |
| VM lifecycle | Start, shut down and restart. In current source builds, **Stop** waits for Windows to close its apps. **Force Stop** asks for confirmation because unsaved work may be lost. Persistent storage, TPM/Secure Boot workflows and [powered-off snapshots](docs/windows-arm/snapshot-scope-v1.md) are available. Running-state suspend is outside v1. |

These are preview capabilities. ISO installation and installed-disk import on
a clean Mac remain the open release criterion. The
[capability matrix](docs/windows-arm/capability-matrix.md) records the evidence
for each feature.

## Try BridgeVM

**Host:** Apple silicon and macOS 15+ (the engine uses the macOS 15
Hypervisor.framework GIC). The declared host matrix is M4/M5;
M1–M3 are not yet covered. A VM installed from an ISO gets a 64 GiB disk and
6 GiB of memory by default (4 GiB on an 8 GiB Mac). The
[installation guide](docs/install.md) has the terminal installer, the DMG and
its Open Anyway step for each downloaded version. Source builds require Xcode/Swift, Rust 1.85+
and pinned native dependencies; follow the complete
[build and packaging guide](docs/install.md#build-the-current-source), because `cargo build` and `swift build` alone do not prepare a
Windows-installation app.

On first launch, choose the setup path that matches your Windows media.
The current source uses English card labels. In the v0.1.0 download, the
cards are labeled **새 Windows VM 만들기** and **기존 VM 가져오기**, respectively.
In current source builds, you can also drop a Windows installer ISO or an
installed disk from Finder onto the matching card to open its setup form.

| Starting point | What you provide |
| --- | --- |
| **Install Windows** | Choose your licensed Windows 11 Arm ISO ([download from Microsoft](https://www.microsoft.com/en-us/software-download/windows11arm64); an x64 ISO is refused). Use **Download Drivers** to fetch and verify the signed Red Hat storage, serial and network drivers (about 95 MB), or choose your own signed Arm64 driver folder and SHA-256 manifest. Choose **Create**, then **Start Installation** in the VM detail view, and leave the Mac awake until installation finishes. When it does, choose **Start** and finish Windows setup in the VM window. In the v0.1.0 download, these four buttons read **드라이버 자동 준비**, **생성**, **설치 시작** and **시작**, respectively. |
| **Import an existing disk** | Choose your installed Windows 11 Arm disk, select **Import**, then choose **Start** in the library. In v0.1.0, the file picker and these two actions read **선택…**, **가져오기** and **시작**. The download accepts RAW, QCOW2 and Hyper-V VHDX. Current source also supports fixed-size VHD and lets you drop a file on the disk field. Disk import needs no driver payload, but Windows on that disk must already have the VirtIO Serial driver (for the guest tools) and the NetKVM driver (for networking). After the first start, choose **Install Guest Tools** (**게스트 도구 설치** in v0.1.0), shut down Windows and start it again, then run **install.cmd** from the new drive in Windows to enable clipboard sharing and shared folders. |

To rename a saved Windows VM in a current source build, shut down Windows,
open **Settings**, edit **VM name** under **General**, and click **Done**. The VM keeps its disk, snapshots and TPM identity.

In current source builds, you can choose **Finish Then Quit**
during a first-run import, while **Create VM** prepares files for an import or
ISO installation, or during a powered-off snapshot operation. You can cancel
the quit from the progress view or VM settings. If Windows takes longer to shut
down, press Return to keep waiting or choose **Cancel Quit** to check the VM.

The [installation guide](docs/install.md) covers both paths, required bundle
resources and first-open trust. After you sign in to Windows, install the
signed guest display driver from the VM's settings. Then choose **Restart Now**
when BridgeVM offers it, or shut down the VM and start it again; restarting
Windows inside the VM is not enough. The display then follows the window size.
In current source builds, after Windows shuts down, choose **Open Library** in
its display window to return to that VM and start it again.

Release and source builds are ad-hoc signed. Developer ID signing and Apple
notarization remain unfinished. Standard preview packaging includes no Windows
test driver and requires no TESTSIGNING setup.

## Send text to Windows

Click where you want the text to appear in Windows. In the Mac library,
select the running VM and open its advanced controls. In a current source
build, open **Windows keyboard**, enter your draft in **Text to send to
Windows**, and click **Send**. In v0.1.0, choose **고급** (Advanced),
then **부팅 구성** (Boot configuration). Type in **Windows에 입력할 텍스트**
(Text to send to Windows) and click **입력** (Send).

Current source builds keep the draft in the field while awaiting an input
receipt or a clipboard paste. Editors such as Notepad and WinForms RichTextBox
support queued text dispatch. If the Windows agent lacks this feature or the
editor declines it, BridgeVM sends a multiline draft by verifying the text on
the Windows clipboard and pressing **Ctrl+V** once. The legacy keyboard route
also uses the clipboard for characters it cannot type. Both clipboard routes
replace the previous Windows clipboard contents.

Single-line drafts can use per-character input with some editors and older
agents. For long passages in v0.1.0, or if typing loses text, enable clipboard
sharing while the VM is stopped. The setting is **Share clipboard** in current
source builds and **클립보드 공유** in the v0.1.0 download. After the Windows
agent connects, copy the text on your Mac and press **Ctrl+V** in the Windows
editor. Builds from current source show a notice when text is too large to
share in either direction. Copy it in smaller sections or transfer it as a
file through a shared folder.
The application readback, saved-file and Undo checks for WPF and Edge are
recorded in [#665](https://github.com/Ketchio-dev/bridgevm/issues/665).

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

Release scenarios met: **3 / 5**.

| ID | Scenario | State | Criteria | Known defects |
| --- | --- | --- | --- | --- |
| S1 | Get Windows running | `OPEN` | A9 (open), A1, A10 | **A9**: No retained clean-machine product-flow receipt yet proves either ISO installation or installed-disk import through the app. Both supported flows remain 3D-off. In the 3D-off configuration the app's live display window was choppy in recent hardware runs; its smoothness has not been measured since. Guest audio stuttered audibly: until 2026-11-03 every other 10 ms of Windows playback reached the host as silence, because the sound controller answered Windows' position reads at the 0x2084 alias with 0. Since that fix, 60-second live runs had 2-6 silent 10 ms blocks out of about 6,330, and under host load 0-8 blocks per run still repeated stale audio; at the v1.1.0 release head (2026-11-14) six such runs had 2-6 silent and 0-3 stale blocks out of about 6,400, and one of them had 2 host underrun callbacks. No listening check has confirmed the stutter is gone. |
| S2 | Use the Windows desktop | `OPEN` | A4, B4, A5, A8 | The Windows display follows the window size only after the one-time display driver install in the VM view (Red Hat virtio-win, Microsoft-signed, downloaded by the guest) and a VM restart; until then the resolution is fixed per VM before it starts (1280x800 by default, up to 2560x1600). The app window was choppy in earlier runs and its smoothness has not been measured since, and no listening check has confirmed that the audio stutter is gone (see A9). |
| S3 | Work with the Mac | `MET` | A6, A7 | none |
| S4 | Keep the VM safe | `MET` | A19, A15, A16, A12 | none |
| S5 | Go online | `MET` | A20 | none |

Release-blocking criteria proven: **13 / 14**. Open: A9.

Other known open defects:
- **B6**: Window title, tab and menu glyphs can be blank on the experimental Windows graphics path; body text alone does not prove glyph correctness.

- Graphics future path: Vulkan is a Graphics Lab future path, excluded from General Preview and v1; D3D11 compatibility is a Graphics Lab future path, excluded from General Preview and v1.
- Guest platform: QEMU virt-compatible guest contract with documented deviations.

State reviewed 2026-11-24 at commit `f085945a0f8e0fca85d6724689163604cd389e7d`. This block is generated from [`capabilities/windows-hvf.json`](capabilities/windows-hvf.json) by `scripts/render-capability-status.py`.
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
| Windows HVF runtime lifecycle | [`crates/bridgevm-hvf-runtime/`](crates/bridgevm-hvf-runtime/), [`runners/hvf-runner/`](runners/hvf-runner/) |
| Disk and vars media leases | [`crates/bridgevm-media/`](crates/bridgevm-media/) |
| `bridgevm` CLI (`app …` commands, `hvf` reports) | [`crates/bridgevm-cli/`](crates/bridgevm-cli/) |
| Mac app and helper boundaries | [`apps/macos/`](apps/macos/) |
| Packaging and release verification | [`apps/macos/scripts/package-hvf-control-app.sh`](apps/macos/scripts/package-hvf-control-app.sh), [`release.yml`](.github/workflows/release.yml) |
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

## Engine

The app has one engine, Windows HVF: Hypervisor.framework plus BridgeVM's own
device model. The earlier Apple VZ (Linux) and QEMU compatibility engines were
removed; a VM saved with one of them stays in the library, marked as no longer
supported, with its files untouched, and can be deleted from there.

Windows HVF uses a QEMU `virt`-compatible guest contract with
[documented deviations](docs/machine-contract/qemu-virt-deviations.json).
BridgeVM imports RAW, QCOW2 and VHDX disks with its bundled converter.
Builds from current source also import fixed-size Hyper-V VHD. Convert a
dynamic VHD to VHDX in Hyper-V before importing.
The v0.1.0 download imports uncompressed QCOW2. Builds from current source also
read QEMU's default deflate compression. Use RAW for compressed QCOW2 in v0.1.0
or for zstd-compressed QCOW2 in either build.
[RAW conversion steps](docs/install.md#what-you-need-after-installing)

Convert encrypted images to RAW first, and merge disks that depend on a backing
file in their original VM app.

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
