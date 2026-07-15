# Guest virtio driver injection notes (HVF Windows 11 ARM64)

The WinPE injector (`bvinject.cmd`) dism-installs every driver subdir found under
the source image's `\drivers` tree. Source packages live in `~/BridgeVM/drivers/`.

## Networking (Parallels-parity internet)
- The HVF engine's virtio-net + userspace NAT is OFF by default; enable it with the
  `--virtio-net` flag on `run-hvf-windows-installed-boot.sh` (or in the app's launch).
- Guest driver = `~/BridgeVM/drivers/netkvm/` (Red Hat production-signed ARM64,
  DEV_1000/DEV_1041). **The package REQUIRES `netkvmp.exe` + `netkvmco.exe`** in
  addition to `netkvm.{inf,sys,cat}` — staging only inf/sys/cat makes pnputil/dism
  fail with "The system cannot find the file specified" (this was the long-standing
  WinPE-dism netkvm failure). Verified live: DHCP 10.0.2.15, HTTPS 200.

## viogpu3d (3D display)
- Install a package whose `.sys` and `.cat` MATCH (never raw-swap just the `.sys`:
  a 0.3 sys under a 0.2 cat => CM_PROB_UNSIGNED_DRIVER). Needs BCD `testsigning on`;
  `bcdboot` regeneration resets testsigning OFF, so re-enable + reboot before install.
- The CI driver-only INF lacks WDDM UMD registration -> Code 43
  (CM_PROB_FAILED_POST_START, dxgkrnl AddAdapter STATUS_OBJECT_NAME_NOT_FOUND).
  Run `viogpu3d-umd-regfix.ps1` as SYSTEM after install (or use the full package
  whose INF now carries the UserModeDriverName/OpenGLDriverName/InstalledDisplayDrivers
  AddReg keys — see the CI `bridgevm/vblank-120` branch fix).

## Recovering a wedged boot
- Firmware "Start boot option" exit-storm (never launches Windows) after editing the
  BCD => the BCD is mangled. Fix: WinPE `bcdboot W:\Windows /s S: /f UEFI` regenerates
  clean boot files. Do NOT `bcdedit /set bootstatuspolicy` a fresh BCD repeatedly.
