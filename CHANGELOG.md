# Changelog

All notable changes to BridgeVM are documented in this file.

## Unreleased

### Changed

- A running VM uses about 190 MB less memory. The 3 MiB firmware no longer
  fills a 64 MiB region, and the UEFI variable file is read straight into the
  flash device instead of through two 64 MiB copies that stayed resident
  (measured: 6397 to 6208 MB footprint for a 6 GiB Windows VM at idle).
- The app now states and enforces macOS 15 as its minimum. The engine uses the
  Hypervisor.framework GIC API that first shipped in macOS 15, so on macOS 14 a
  VM could not start even though the app opened.
- The Fast VZ (Linux) and QEMU compatibility engines were removed from the app.
  A VM saved with one of them stays in the library, marked as no longer
  supported, with its files untouched, and can be deleted from there. The
  command palette, "Start when BridgeVM opens" and automatic startup no longer
  offer to start it.
- `bridgevm app start` runs the same start as the Start button: a failure now
  prints the app's own sentence with the underlying error, and a VM whose
  settings were saved after the app loaded them starts with the saved
  settings instead of being refused. Stop and install no longer refuse after
  a settings change either; install uses what is saved. The JSON schemas
  changed accordingly (`bridgevm.app-start.v2`, `bridgevm.app-stop.v2`,
  `bridgevm.app-install-command.v2`, `bridgevm.app-create-windows.v2`, which no
  longer carries `configurationDigest`).
- A VM started from the CLI keeps booting if Windows takes longer than 30 s to
  report ready, as after a Start click; the CLI still reports that readiness
  was not confirmed.
- A Windows install finishes sooner: the new disk is hashed once when it is
  sealed instead of six times (about 2–3 minutes less for a 64 GiB disk,
  estimated). Snapshot create, export and disk import no longer re-read the
  whole disk to verify right after writing it; a restore still verifies
  before it touches the VM. A restore reads the snapshot once instead of
  twice: it hashes only the staged copy it publishes (median 157 s to 74 s
  for a 64 GiB pair on one Mac). An A19 restore-boot lane, which creates,
  restores and exports a 64 GiB pair, took about 5 minutes instead of about 9
  on the same Mac.
- Two snapshot exports to different names in the same folder can run at the
  same time.

### Fixed

- Putting the Mac to sleep while a key press or click was still on its way to
  Windows no longer stops the keyboard and mouse on wake until Windows restarts:
  the time Windows has to confirm input now counts only while the Mac is awake.
- A snapshot or export run from the command line no longer makes the stopped VM
  show as running in the app, and Start no longer "attaches" to it: Start,
  Delete, Duplicate and Move say another BridgeVM task is using the disk. A stop
  can no longer end that task in the middle of a copy.
- Windows sound no longer goes silent when Windows or an app picks a playback
  rate other than 48 kHz (for example "16 bit, 44100 Hz" in Sound > Advanced).
  The VM now offers only the 48 kHz format the Mac side plays.
- After restoring a snapshot taken before a disk grow, the disk size setting no
  longer says "Expanded the disk to N GiB" about the disk that was replaced.
- A Windows driver touching a device from the very top of the address space can
  no longer crash the VM: the instruction pointer now wraps around as it does on
  real hardware.
- **Retina resolution** and **Start when BridgeVM opens** can now be changed
  while Windows runs, like the Command key setting. Retina applies to the window
  at once; both are saved when the VM stops. They were locked until shutdown.
- When Windows stops by itself because something failed (the VM process
  crashed or was ended, or the disk was still in use), the app says so and shows
  Needs attention, instead of "Windows has shut down."
- While a snapshot is being made, restored or deleted, Delete, Duplicate and Move
  say so, instead of "The VM is running" about a stopped VM. A disk being
  expanded or a security chip change says so too.
- Settings no longer offers up to 32 GiB of memory on any Mac: like Create, it
  stops 4 GiB short of the Mac's memory; a VM saved with more keeps it. **Keep
  Mac awake** and **When the VM window closes** now say they apply to all VMs.
- When keyboard and mouse stop because Windows didn't confirm input in time, the
  VM window now says so and how to get them back. Keys only beeped and clicks
  did nothing.
- After BridgeVM quits unexpectedly while Windows runs, reopening it shows that
  VM as running while Windows shuts down, instead of Stopped, and Start no longer
  launches a second run against the disk it still holds.
- Snapshots: restoring or deleting a snapshot that was deleted elsewhere since
  the list was shown now says so, instead of asking you to check the snapshots
  folder; a snapshot missing its disk or UEFI file now reads as damaged instead
  of offering a retry; and a Quit that waits for a snapshot no longer gets
  cancelled when only the snapshot's name could not be saved.
- A Command key, Retina or "Start when BridgeVM opens" choice made while Windows
  runs that can't be saved when it stops is now shown on the VM's dashboard, and
  a choice made later no longer gets overwritten by it at the next stop.
- The library dashboard now says when a run needs attention: a Windows disk
  BridgeVM couldn't finish saving at shutdown, or lost contact with Windows,
  used to show only in Advanced (the VM read "Stopped" or "Running"), and a
  failed stop read "Booting Windows". Open Display also stays available while
  Windows shuts down, since that window is where an app asking to save shows.
- Shared folders: the "skipped because the per-file limit is …" notice now
  names the limit that applied (64 MiB until Guest Tools are updated, not
  always 16 GiB), gives advice that fits a Windows file, and goes away once the
  file syncs after the update.
- "Start when BridgeVM opens" no longer skips a VM that is not ready (for
  example its disk is on a drive that is not connected) without a word: it
  starts it as the Start button does, which says why it could not.
- Shared folders: after you changed only the Windows folder of a share (the Mac
  folder stayed the same), the first sync could take same-named files in the new
  Windows folder for edits and copy them over the Mac's files. The saved sync
  state now belongs to both folders, so a new Windows folder starts fresh.
- Copying more than about 24 MB of text in Windows stalled clipboard sharing,
  shared folders and input for the rest of the session. The copy is now
  refused like any text over 8 MB, and the rest keeps working.
- Importing a VM with its vTPM: a recovery code file other accounts can read, a
  mistyped code or a package for another vTPM now stops the import at once with
  a sentence that names the problem. It used to copy the whole disk first and
  then blame disk access or free space.
- `bridgevm app create-windows --resolution` was saved but never used: the VM
  started at 1280x800. It now sets the VM's startup resolution, and the default
  is the documented 1440x900.
- Installing Windows now keeps the Mac awake, as a running VM does (the "Keep
  Mac awake" setting). An unattended install used to stop when the Mac went to
  sleep, and looked stuck when you came back.
- Shared folders: while a file in the Mac folder kept changing (a log being
  written, a long Finder copy), files saved in the Windows folder never reached
  the Mac. A Windows listing now goes between the Mac's changes at least every
  12 seconds.
- Installing Windows on a Mac whose disk fills now stops at once and says the
  disk is full. It sat on "Installing Windows" for up to 25 minutes, then
  failed with a general sentence.
- A duplicated Windows VM attached the Guest Tools disk again at its first start,
  and while its agent was still starting it could say Guest Tools were missing.
  The copy now keeps the record that Windows already has the guest agent.
- Print to Mac and Windows links: a "couldn't create the printer" or "couldn't
  set up links" notice no longer stays until you quit BridgeVM. It goes when
  Windows creates the printer or the VM starts again, and the printer notice
  now includes Windows' reason (for example a stopped Print Spooler) instead of
  always blaming Microsoft Print to PDF.
- Importing a Windows disk smaller than 64 GiB enlarged the disk to 64 GiB, but
  Windows' C: drive never grew into the added space. The next Windows start now
  extends C:, also for VMs imported with an earlier version.
- Clipboard: copying more than 8 MB of text in Windows now shows the "too large"
  notice. It was cleared again by the same poll, so the Mac silently kept its
  old clipboard.
- The VM dashboard said "Graphics mode unknown" for every Windows that BridgeVM
  itself had started. It now shows the running session's display mode.
- Canceling a Windows install, or quitting BridgeVM, while it prepares the setup
  files now stops the file copy too. It kept running and could leave a large
  scratch disk attached until the Mac restarted.
- A failed Guest Tools update now says to quit and reopen BridgeVM to try
  again. "Restart BridgeVM" read like the Restart button beside it, which
  restarts Windows and does not retry the update.
- `bridgevm app install-status` now reports a finished install as done. Once
  Windows had installed, it failed with "unsupportedTarget".
- When Windows could not register BridgeVM as a browser or watch for links,
  "Open Windows links on Mac" stayed on and links silently never reached the
  Mac. The VM window now says so and suggests restarting Windows.
- Keyboard: on German, Swiss, Nordic and other Mac layouts that type @, [ ], { },
  | or ~ with Option, those characters now reach Windows. They were sent as
  Alt+L, Alt+5 and so on, so an email address could only be pasted.
- `bridgevm app create-windows` without `--memory-mib` now gets the memory the
  app recommends for this Mac (4 GiB on an 8 GiB Mac). It always sent 6 GiB.
- Installing from an x64 Windows ISO (not Windows for Arm) now stops as soon as
  the ISO is opened, with a sentence that says so and to create a new VM with
  the Arm64 ISO. It used to copy for minutes, then say only that the setup
  files couldn't be prepared, and Try Again repeated the same wait.
- Restoring a snapshot now gives back the space of the disk it replaces. The
  old disk stayed on the Mac, so a restored VM kept using about twice its disk
  space, even with every snapshot deleted.
- An interrupted snapshot create or export (for example, its process was
  killed) left a hidden, half-made copy of the VM's disk and vars in the
  snapshot folder, plus an empty folder for an app snapshot. The snapshot list
  never showed them and nothing removed them. On APFS each one held on to the
  disk's old blocks as the VM changed; on other volumes each was a full copy.
  The next create or export into the same folder now removes them, and keeps
  anything it does not recognise.
- Moving a VM's bundle into a folder that already holds another VM's bundle, or
  into the folder it is already in, now says so before anything starts, instead
  of "BridgeVM could not confirm the move".
- Display settings: the resolution menu's first entry is now "Automatic", not
  "Automatic (1280x800)", and its help says Automatic keeps the resolution
  Windows last started with. Going back to it after choosing 1920x1080 kept
  1920x1080.
- Canceling a snapshot export now removes the partial copy. It stayed in a
  hidden folder next to the destination, up to the size of the whole disk.
- After 256 starts or stops from the CLI in one app session, every further
  one was refused until the app was reopened.
- Memory and CPU limits now agree across the app. Importing a VM offered up to
  64 GiB and 16 CPUs, and Create offered every core but one, but Settings
  accepted only 32 GiB and 10 CPUs, so changing such a VM's resources later was
  refused. Import now offers Create's choices, and Settings accepts up to every
  core but one.
- A Stop could end as "not confirmed", needing Force Stop, when the app and
  the runner disagreed on a harmless detail of a status message. Both sides
  now ignore fields they do not know.
- Port forwarding: when another app already uses a forward's Mac port, the VM
  now says so ("Mac port 8080 is already in use, so it doesn't reach Windows
  port 80 …") instead of running without that forward in silence. Two rules for
  the same Mac port are refused when you enter them.
- A never-started VM in a folder that reports itself group- or
  world-writable, as exFAT and some shared folders do, can be deleted. The
  check refused it without protecting anything.
- Quitting BridgeVM after a VM's runner stopped unexpectedly no longer waits
  about two minutes behind "Shut down Windows and quit BridgeVM?", "Windows is
  still shutting down." and a Force Stop button that did nothing.
- Display driver install: the result now reads as plain sentences ("Installed
  the display driver.", "Windows didn't answer, so try again when Windows is
  idle.") instead of device status tokens and protocol names such as
  "status=OK" or "BVDRIVER result (exit -1)".
- The guest agent's input capability line is accepted when it carries extra
  keywords from a newer agent.
- Settings: the Command key choice (Mac shortcuts or Windows key) can now be
  changed while Windows runs. It applies at once and is saved when the VM
  stops; it was locked until you shut Windows down, though it only changes how
  BridgeVM maps keys.
- After an unclean stop, the shared folder keeps the Windows version of a file
  beside the Mac one as "(Windows copy)". If the VM stopped again before the
  Mac version was sent back, the next start made a second, identical
  "(Windows copy 2)"; now it keeps one copy and sends the Mac version again
  (#675).
- `create-windows` on an 8 GiB Mac now gives the VM 4 GiB by default, as the
  app's Create sheet does, instead of 6 GiB that left macOS 2 GiB.
- Expanding the disk of an imported VM whose C: drive is followed by another
  partition (usually Windows Recovery) now says that partition keeps C: from
  growing, instead of claiming C: already uses all available space.
- A VM with a Korean name finds its vTPM key even when its ID reaches the
  keychain in the other Unicode form (composed or decomposed, for example
  after the library was copied). Before, the VM refused to start with a
  missing-key error (#689).
- Creating a VM with your own driver folder now says what is wrong with it
  (for example a manifest inside the folder, or a file that isn't a BridgeVM
  driver manifest) before the ISO is copied. It said only "Couldn't create the
  VM", or created the VM and then failed at setup, blaming the ISO.
- Ctrl-C or SIGTERM during `bridgevm app snapshot-create`, `snapshot-restore`
  or `snapshot-export` left the snapshot helper running, so a restore could
  still change the VM after the command had ended. The command now stops the
  helper, waits for it, reports `cancelled` and exits 130 (143 for SIGTERM).
- The library now shows "Moving…", "Duplicating…" or "Deleting…" on a VM while
  that runs, and its Start button waits. A large copy read "Stopped" for
  minutes, and Start was refused only when pressed.
- Importing a VM: the file chooser now opens other VM apps' bundles, so you can
  pick the disk and UEFI variables file inside one. It showed such a bundle as
  a single file that could not be opened.
- Moving a VM's bundle to a folder BridgeVM can't write to no longer makes the
  VM disappear from the library. Nothing had moved, but a leftover "move in
  progress" record hid the VM, and nothing in the app could bring it back.
- Long typed or pasted text in a Windows app built on a RichEdit 5 control
  (for example a WinForms RichTextBox) went in one character at a time,
  about 5 ms each, and from roughly 2,000 characters failed partway with an
  input error. It now goes in as one undoable edit, as in Notepad: 5,000
  mixed characters in under half a second (#665). Needs the updated guest
  agent.
- Duplicate: a copy no longer inherits "Start when BridgeVM opens" (both VMs
  booted at every launch), and a failed duplicate says what is missing or that
  the Mac needs free space, instead of pointing at a destination bundle that had
  already been removed.

### Removed

- The legacy Rust CLI commands (`bridgevm --store …`, `--socket`, the daemon,
  disk, snapshot-store and QEMU/VZ commands) and their crates. `bridgevm app …`
  is the CLI for the app's library; `bridgevm hvf` keeps three Graphics Lab
  report tools.

## [0.1.0] - 2026-11-23

Version numbers restart at 0.1.0. This release is built from the same source as
the withdrawn `v1.1.1` tag (`3b01ef93`); its changes are the ones listed under
1.1.1, 1.1.0 and 1.0.0 below. The preview tags `v1.0.0`, `v1.1.0` and `v1.1.1`
and their GitHub releases were withdrawn. An installed 1.1.1 app is replaced by
running the installer again; VMs and settings are unaffected.

## 1.1.1 (withdrawn tag, now 0.1.0) - 2026-11-21

Fixes and interface polish for the 1.1.0 General Preview; the product state
and the release contract are unchanged.

### Fixed

- `bridgevm app install` and `start` find a VM that `bridgevm app create-windows`
  created while the app was already open, instead of answering
  `targetUnavailable` until the app is reopened (#684).
- Windows no longer shows a warning "Red Hat VirtIO SCSI controller" in Device
  Manager: the unused installer slot is left out when no ISO is attached (#685).
- A Mac file put into the shared folder while Windows still ran an older guest
  agent is no longer copied back over itself at the next start (new date,
  quarantine mark): an identical Windows copy is recognised and left alone (#651).

### Changed

- The remaining English labels in the 가상 머신 menu (Windows 창 목록…) and the
  고급 view are in Korean; the developer diagnostics say what they are for.
- Memory and disk sizes say GiB everywhere, as the create sheet already did.
- The snapshot name field looks like a field, and 고급 has one entry point (the
  action grid) instead of three.

## 1.1.0 (withdrawn tag) - 2026-11-20

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

## 1.0.0 (withdrawn tag) - 2026-08-20

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
