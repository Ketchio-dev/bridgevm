# Changelog

All notable changes to BridgeVM are documented in this file.

## Unreleased

## [1.1.0] - 2026-11-20

First General Preview release, replacing the superseded 1.0.0. The product is
still an **Engineering Preview**: Windows 11 Arm on BridgeVM's own
Hypervisor.framework VMM, 3D-off, in an ad-hoc-signed app without Developer ID
or notarization (`docs/install.md` has the first-open step). ISO installation
needs your own Windows 11 Arm ISO; the app downloads the signed ARM64
storage/serial/network drivers from the official virtio-win source on your
click, or you choose your own payload. BridgeVM has only been run on M4 and M5 Macs; M1, M2 and M3
are untested. The 1.0.0 claim that every release-blocking criterion was proven
no longer stands: 2 of 5 release scenarios are met, and A9 (in-app install on
a clean Mac) and A19 (disk safety) are open. See `STATUS.md`.

### Added

- **Install from an ISO alone:** "드라이버 자동 준비" in the create sheet, or
  `bridgevm app create-windows --fetch-drivers`, downloads the pinned
  Microsoft-signed Red Hat virtio-win drivers (about 95 MB) once, checks their
  SHA-256 and seals them in the VM; your own payload still works.
- **Import:** QCOW2 and Hyper-V VHDX disks are converted to sparse RAW,
  the UEFI vars file is optional, and an imported Windows that has the virtio
  serial driver but no guest tools is offered a guest tools installer.
- **Display:** One-click install of the Microsoft-signed virtio-win display
  driver; after a VM restart Windows follows the window and shows its pointer
  as the Mac cursor. Per-VM resolution (1280x800 default, up to 2560x1600), an
  experimental Retina option (off by default); the window reopens where it was.
- **Input:** Command acts as Ctrl (or, per VM, as the Windows key) and Option
  as Alt; held keys, modifier-clicks and F13 as Print Screen reach Windows.
- **Korean input:** Ctrl shortcuts work with the Korean input source, a syllable
  is committed before a shortcut or click, and candidates follow the caret.
- **Audio:** An opt-in per-VM microphone that uses the Mac's default input.
- **Clipboard:** Images, rich text, and files or folders (up to 4096 entries
  and 16 GiB) copy both ways between Finder and Explorer.
- **Shared folder:** New VMs share `~/BridgeVM 공유/<VM name>` as
  `C:\bridgevm-share` by default; existing VMs are unchanged. Files dropped on
  the VM window or opened with BridgeVM from Finder are copied into Windows.
- **Snapshots:** Several snapshots per stopped VM, to name, restore or delete.
- **Network:** Per-VM inbound TCP port forwards, on the Mac's 127.0.0.1 only.
- **Storage:** Grow a stopped VM's disk; deleting in Windows frees Mac space.
- **App:** A per-VM Settings sheet, a 가상 머신 menu (Pause/Resume, Restart,
  Windows apps), printing to Mac printers (on by default), opt-in Windows links
  in the Mac browser, Mac battery in Windows, stay-awake and opt-in auto-start.
- **Guest tools:** Updated at VM start; a crashed agent is back within a minute.
- **CLI:** `bridgevm app` lists, installs, starts, stops and snapshots VMs.

### Changed

- **Storage/flush:** Flushes are ordered and durable within about a second, so
  a Mac power loss can drop the last second of writes but never leaves the disk
  inconsistent; per-VM "안전한 쓰기" makes every flush wait for a full sync.
- **Stop and quit:** Stop, Quit and an app crash shut Windows down, by the ACPI
  power button if no agent runs; closing the last window no longer quits.
- **Boot and clock:** Boots skip a 5-second firmware countdown; Windows takes
  the Mac's time and time zone at every boot and its time after the Mac sleeps.
- **Network:** Guest DNS follows the Mac's resolver after a network change.
- **Shared folder:** Large Mac files reach Windows about six times faster, files
  keep their dates, and sync handles 16 GiB files and 262,144-entry shares.
- **Messages:** Start, install, import and snapshot failures read as one plain
  Korean sentence; a missing shared folder only turns sharing off for a start.
- **Dashboard:** Actions stay readable at any window width, the status shows
  the VM as observed, and the VM window shows startup progress or the failure
  instead of staying black.
- **First run:** The empty library shows one set of create and import choices
  in plain words; the sidebar no longer shows a fixed engine list.
- **Security:** The installer checks a General Preview release contract,
  checksum, archive and signature, rolls back on failure and refuses v1.0.0;
  new Windows installs let only administrators change `C:\BridgeVM`.

### Fixed

- **Audio:** Every other 10 ms of Windows playback reached the Mac as silence,
  and output did not follow the Mac's default device.
- **Network:** Large downloads stalled or timed out in the built-in NAT, and the
  last bytes of an upload could be lost.
- **Storage:** A full Mac disk failed Windows' writes; the VM now pauses so you
  can free space and resume, and the app warns if writes were not made safe.
- **Shared folder:** Windows edits were overwritten after a guest restart, just
  before a shutdown or in folders over the listing limit; a Force Stop or crash
  could leave uploads as zeros or fetch an empty Windows copy over the Mac's; a
  junction deleted Mac files; Korean names split; 260+ character paths failed.
- **Shared folder:** A Windows edit saved within about 20 seconds of a sync, or
  one whose copy to the Mac failed, was overwritten by the Mac's older copy
  after a Force Stop, a crash or a Windows restart; it is now kept beside the
  Mac's file as "(Windows copy)" (#643, #648).
- **Clipboard:** The "클립보드 공유" switch is saved per VM, so dashboard and
  command-line starts no longer turn sync back on; change it while stopped.
- **Clipboard:** Some Mac copies, including any text over 8 KiB, were lost or
  reverted, and legacy paste text could reach the Mac pasteboard; a copy made
  just before Ctrl+V or Shift+Insert could paste the previous text.
- **Input:** Input stopped after a Windows restart, an agent restart or update,
  or at the lock screen, and trackpads scrolled far too fast; a left click
  could open a context menu; input could stay off when the guest agent
  answered before the app saw it start.
- **Korean input:** Long text committed into Windows 11 Notepad repeated or
  lost characters; compatible editors now receive it in one piece.
- **Display:** A VM started without its window came back from a Windows
  restart at the boot resolution instead of the saved one.
- **Lifecycle:** Windows was powered off without a shutdown after 50 million
  hypervisor exits on one vCPU (about a day idle or 4-8 hours busy, estimated
  from exit rates), and a quote in a disk path crashed the app at Start; on
  M5 Macs some boots stopped in the firmware before Windows started.
- **vTPM:** A failed TPM reset could destroy the VM's TPM state, and a deleted
  VM's TPM key could be picked up by a new VM with the same name.
- **Security:** Guest-controlled values could crash device models, exhaust host
  memory or reach paths outside the shared folder.
- **Graphics Lab only:** Legacy TGSI shaders now infer integer buffer sampler
  types, restoring classic Notepad captions in the tested scene (B6 open).

### Removed

- **3D acceleration** left General Preview and the v1 scope on 2026-09-16: VMs
  start 3D-off, the app never injects a test-signed driver or enables
  TESTSIGNING, and Vulkan and D3D11 stay in the separate Graphics Lab.
- The "Guest windows" button over Windows' close button (now in the menu).

### Known issues

- In-app install and import are unproven on a clean Mac, the window may be
  choppy and sound may stutter (A9), so keep your ISO or source disk handy.
- Windows keeps the resolution set before start until you install the display
  driver from the VM window and restart (S2), so pick it in Settings first.
- Disk safety is open (A19): interrupted operations are only partly tested,
  power loss during a snapshot not at all, and VMs relocated by older builds
  before identity migration are refused; snapshot on mains power, keep backups.
- Titles, tabs and menus can be blank on the Graphics Lab 3D path (B6).
- After a Force Stop, a crash or a Windows restart within about 20 seconds of a
  shared file's last sync, a differing Windows copy arrives beside the Mac's as
  "(Windows copy)", even when it is only damage from the stop; keep the one you
  want. A second stop during that recovery can add a "(Windows copy 2)" (#675).
- Long text into editors other than Windows 11 Notepad and compatible controls
  can still repeat or lose characters (#665); paste through the clipboard.
- Files added to the share while an older guest agent updates come back once,
  unchanged but with a new date and quarantine flag (#651); wait a minute first.

## [1.0.0] - 2026-08-20

Every release-blocking criterion in `capabilities/windows-hvf.json` is proven
by live evidence, including the final Windows closure gate
(driver-loadable image, host-driven resolution adoption, and the shipped
window-Coherence verbs answered live; receipt
`fefad903a56fa2f1c63e6067079208928d1dae1f415ff966611629ea37727a97`).

### Added

- A Mac-native Windows 11 Arm engine built directly on
  Hypervisor.framework, alongside the existing Apple Virtualization.framework
  and QEMU compatibility engines.
- Windows HVF support for persistent storage, display and input, networking,
  audio, guest integration, TPM/Secure Boot workflows, snapshots, and
  experimental 3D.
- An experimental Vulkan path and an experimental D3D11-compatible subset
  through virtio-gpu.
- Ad-hoc-signed `.dmg` and curl-installable `.tar.gz` packaging, with a
  documented macOS installation flow for builds distributed without Developer
  ID or notarization.
- A machine-readable Windows HVF capability registry backed by deterministic
  checks and dated live-gate evidence.

### Changed

- Defined the guest platform as a QEMU `virt`-compatible contract with
  documented deviations.
- Separated deterministic hosted checks from serialized, sealed-input live
  gates that require real Apple-silicon hardware or private Windows media.

### Security

- Guest-visible random values use the host CSPRNG and fail closed when entropy
  is unavailable.
- Release builds reject repository and `PATH` overrides for bundled helper
  binaries.
