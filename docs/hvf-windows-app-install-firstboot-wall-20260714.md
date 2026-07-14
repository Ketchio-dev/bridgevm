# App install pipeline — works end-to-end; first-boot-to-desktop wall (2026-07-14)

## What the app feature delivers (built, committed, tested)

BridgeVMControl now offers Windows install-from-ISO and viogpu3d driver
injection without the CLI lab path. Commit `be894e1`:

- Create sheet "설치 (HVF)" mode: ISO + disk size + optional 3D-driver
  injection with a driver-package picker.
- `HvfWindowsInstallPlan` (pure, 13 unit tests) computes the exact
  commands, cache keys (`ISO name + byte size`), destructive `/tmp/bridgevm-*`
  media paths, and the vars template lookup.
- `HvfWindowsInstallSession` runs the pipeline off the main actor, tails the
  boot run.log for progress, and finalizes via staged-rename into the bundle.
- Import mode gains a 3D-injection toggle; the inject-pending marker makes
  the next boot attach the injector as NSID 1 and confirms activation from
  the ramfb→virtio-gpu scanout switch.

## What is PROVEN live (the install itself)

Running the exact commands the app generates
(`~/BridgeVM/bridgevm-app-src/win11-25h2-english-arm64-v2-7994415104.raw`
source cache + `run-hvf-windows-scripted-install.sh`):

- Stage A (source build): the WinPE scripted-installer image builds from the
  retail ISO — full tree, unattend, `install.wim` split into FAT32-safe
  `.swm` parts, `bvinstall` payload injected into `boot.wim` image 2.
- Stage C (install boot): WinPE runs `diskpart` (GPT ESP+MSR+NTFS on NSID 2),
  `dism /apply-image` of the split WIM (**"The operation completed
  successfully" at 100%**), `bcdboot W:\Windows /s S: /f UEFI`
  ("Boot files successfully created"), and stamps the OOBE-skip unattend into
  `W:\Windows\Panther`. Screenshot proof:
  `/tmp/bridgevm-appinstall-e2e-app-evidence/ramfb/*.ppm` shows the full
  `BVINSTALL DONE` sequence. The target grows to ~14 GB; its ESP contains
  both `\EFI\Microsoft\Boot\bootmgfw.efi` and the fallback
  `\EFI\Boot\bootaa64.efi`.

So the app produces a genuinely installed Windows 11 ARM64 disk.

## ★ RESOLVED — freshly-installed disk now first-boots to the Windows desktop

The wall below is fixed. The app's finalize seeds a Windows Boot Manager
NVRAM entry into the installed disk's varstore, and a clean install now
boots straight to the Windows 11 desktop (autologon `bridge`), live-proven:
`scratchpad/PROOF-app-install-to-desktop.png` — wallpaper, taskbar, Edge,
Recycle Bin, clock. Full chain: retail ISO → app install pipeline (real
14 GB Windows, diskpart+dism+bcdboot) → `HvfWindowsBootSeed.seedFile()` →
first boot → OOBE auto-skipped → desktop.

**The fix.** `HvfWindowsBootSeed` (Swift, in the app, unit-tested):

1. Reads the freshly-assigned ESP partition GUID + LBAs from the installed
   disk's GPT.
2. Copies a bundled, proven seed varstore
   (`Resources/windows-boot-seed-vars.fd.gz`, 68 KB) whose Windows Boot
   Manager device path carries a 16-byte sentinel GUID, and replaces the
   sentinel with the real ESP GUID (the device path is partition-signature
   relative: `HD(GPT, <ESP GUID>) → \EFI\Microsoft\Boot\bootmgfw.efi`, so
   no NVMe/PCI prefix is needed).
3. Writes it as the VM's vars store. After this first seeded boot Windows
   maintains its own entry, so the seed is a one-time bootstrap.

A from-scratch injector (build the auth-variable + BootOrder into the
pristine `edk2-arm-vars.fd`) is retained as a fallback and produces a
*byte-identical* boot entry, but the pristine-template varstore did not
boot live (an EDK2 varstore-init interaction not chased down); the bundled
proven-seed path is the shipping route and is the one proven to reach the
desktop. Both are covered by unit tests.

## (Historical) The wall: freshly-installed disk did not first-boot to the desktop

Booting that installed disk on the from-scratch engine (any vars: the
install-produced store, or a fresh `edk2-arm-vars.fd`) ends in the EDK2
UEFI shell, not Windows. BDS tries three auto options
(`BootManagerMenuApp`, `Firmware Setup`, `EFI Internal Shell`) plus three
image loads that fail `Unsupported` / `Not Found` / `Aborted`, then drops to
`Boot0001 "EFI Internal Shell"`.

**Root cause (empirically isolated).** The only installed disk that boots on
this engine is the *imported* `viogpu3d-firstdraw-fix` disk — and its
`vars.fd` carries a **live persisted "Windows Boot Manager" NVRAM boot
entry** (confirmed: 1 WBM entry, `bootmgfw` device path present). A
freshly-installed disk's varstore has **zero** WBM entries. This
ArmVirtQemu build's `PlatformBootManagerLib` auto-enumerates the fw_cfg
`bootorder` (a virtio-blk path, `/pci@i0cf8/scsi@3/disk@0,0`) but does **not**
create a bootable option for the NVMe-backed ESP, and the removable
`\EFI\BOOT\BOOTAA64.EFI` fallback it does try aborts before Windows Boot
Manager registers its own NVRAM entry. Chicken-and-egg: Windows registers
the entry on its first successful launch, but it never gets that first launch
because no boot option resolves to it.

This is a **pre-existing engine milestone (M3-class "boot installed OS to
desktop") first-boot bootstrap gap**, not something the app UX introduced.
The completion plan marks M3 done, but that pass rode the install session's
own reboot-loop (placeholder NSID 1 + the vars produced in that same run);
a clean-room boot of the finalized disk does not reproduce it here.

## Next engine step (the concrete fix, out of this task's scope)

Make a fresh install's first boot resolve to Windows Boot Manager, one of:

1. **Host-seed the WBM boot variable** into the vars store during finalize:
   write a `Boot####` `EFI_LOAD_OPTION` (device path `HD(part, GPT, <ESP
   GUID>)/\EFI\Microsoft\Boot\bootmgfw.efi`) + prepend it to `BootOrder`.
   Needs the freshly-assigned ESP partition GUID (read the GPT from the
   finalized disk) and the firmware's NVMe device-path prefix. Most direct;
   matches exactly the difference measured between working and non-working
   disks.
2. **Engine/firmware: auto-enumerate the NVMe ESP fallback** so BDS boots
   `\EFI\BOOT\BOOTAA64.EFI` from NVMe like real UEFI removable-media default
   boot — the general fix, but requires firmware or platform-boot-manager
   work and explains the `Aborted` load too (diagnose why bootaa64→bootmgfw
   aborts on a first launch: TPM/measured-boot/BCD state).
3. **Bootstrap boot inside finalize**: a one-time scripted boot that drives
   the EFI shell (`fs0:` → `bootmgfw.efi`) to force Windows' first launch and
   NVRAM registration, then reuse that vars store. (Attempted here; the shell
   UART-injection marker needs tuning and the removable load still aborts, so
   this is not yet a fix.)

Recommended: option 1 (host-seed) — smallest, and it targets the proven
delta. Track as a dedicated engine task; the app already writes the vars
store into the bundle, so the seed slots into `finalizeMedia()`.
