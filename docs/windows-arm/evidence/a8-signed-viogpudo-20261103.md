# A8: the Windows display follows the window, Secure Boot on (2026-11-03)

These are live runs on one Mac (Apple M4 Max, macOS 27.0.1), one VM at a time.
The configuration is the supported 3D-off one: Secure Boot on, no test-signed
driver, and the 2D virtio-gpu device (PCI 1AF4:1050) that the app attaches once
the Microsoft-signed display driver is installed. ramfb stays for firmware and
boot.

## Product-path runs at main 33c4df6f

**Image**
- An APFS clone of the internal test image (Windows 11 Home ARM64 26100), with
  its own Secure Boot vars.
- The vars were enrolled offline with the app's provisioner
  (`HvfSecureBootProvisioner.provision`, policy
  `microsoft-windows-transition-2011-2023` v1.6.5); the guest reports
  `Confirm-SecureBootUEFI` = True.
- The image was prepared with `scripts/prepare-signed-viogpudo-image.sh`. It
  installs Red Hat's virtio-win `viogpudo` from the pinned upstream archive
  `attestation-virtio-win-prewhql-0.1-299.zip` (94,849,704 bytes, SHA-256
  `ff393c53…46aead0`), whose catalog is signed by "Microsoft Windows Hardware
  Compatibility Publisher". The driver started with status OK and no reboot.
  In that prep run the script's final success check misreported a failure
  (a `grep -q` under `pipefail`, fixed since); the install itself printed
  `Red Hat VirtIO GPU DOD controller status=OK` with 28 modes.
- In the app the guest downloads the same archive itself and checks its length
  and SHA-256 (the display-driver action, #474). A live run of that action
  printed `BVDRIVER result=installed`.

**Method:** `scripts/verify-a8-window-resize.sh`.
- The VM boots through the installed-boot launcher with `--virtio-gpu`, the
  argument the app passes for a VM with `virtioDisplayDriverInstalled`.
- The script writes only `RESIZE WxH` lines to the VM's live input file,
  exactly as the app writes them when its window is resized (#471).
- For each size it waits for the guest agent's `BVDISPLAY ... after=WxH ...
  result=0` (the mode the guest reports through EnumDisplaySettings). It then
  checks that the host display file holds a frame of exactly that size.
- After a guest restart it sends the last size again, as the app does at each
  `BVAGENT SERVICE start`.
- `settle_ms` is the time from the RESIZE line to the last frame Windows
  published at the new size.

| run | boot | request | guest-reported | host frame | settle_ms |
|---|---|---|---|---|---|
| f1 | 1 | 1517x931 | 1517x931 | 1517x931 | 5644 |
| f1 | 1 | 1600x900 | 1600x900 | 1600x900 | 1792 |
| f1 | 1 | 2560x1440 | 2560x1440 | 2560x1440 | 1755 |
| f1 | 1 | 1366x768 | 1366x768 | 1366x768 | 1680 |
| f1 | 1 | 1920x1080 | 1920x1080 | 1920x1080 | 1601 |
| f1 | 2 | 1920x1080 | 1920x1080 | 1920x1080 | 3188 |
| f2 | 1 | 1024x768 | 1024x768 | 1024x768 | 4400 |
| f2 | 1 | 2560x1600 | 2560x1600 | 2560x1600 | 1752 |
| f2 | 1 | 1280x720 | 1280x720 | 1280x720 | 1759 |
| f2 | 1 | 1440x900 | 1440x900 | 1440x900 | 1567 |
| f2 | 1 | 1680x1050 | 1680x1050 | 1680x1050 | 1587 |
| f2 | 2 | 1680x1050 | 1680x1050 | 1680x1050 | 1497 |

**Result:** 12 of 12 requests, over 4 boots (two VM starts, each with a guest
restart), ended with the guest-reported resolution equal to the request and a
host frame of the same size. 1517x931 is not in the driver's mode list, so it
went through forced re-enumeration. Host 1-minute load was 6.6–13.2, with other
agents' builds running; the release probe was built at 33c4df6f (sha256
`93624e03…`).

## What the slices do

- **#475:** the per-VM flag that attaches the 2D device through both launch
  paths.
- **#473:** ramfb and the virtio scanout share the display file. Firmware, the
  boot spinner and logon stay visible until the driver presents.
- **#472:** the guest applier, run on every RESIZE:
  - it finds the display on `VEN_1AF4&DEV_1050`;
  - once, it makes that display primary at 0,0 and detaches the Basic Display
    on ramfb;
  - it forces Windows to re-read the mode list when the size is not listed;
  - then it applies the mode with `CDS_UPDATEREGISTRY`.
- **#471:** the app sends the window size in points, clamped to
  640x480–3840x2160, 400 ms after the last change or when a live resize ends,
  and again after each agent start.
- **#474:** the one-click driver install.

## Earlier single runs (same day, development harness)

| Run | Secure Boot | Result |
|---|---|---|
| r3 | off | viogpudo installed; RESIZE 1600x900 applied |
| sb1 | on | the old test-signed 3D package was refused (code 52); signed viogpudo status OK |
| sb2/sb3 | on | upstream's resolution service (`vgpusrv`/`viogpuap`) ran but changed nothing with two adapters attached |
| sb4 | on | a SetDisplayConfig call with only the new source size returned 0, but Windows kept the old mode |
| sb5 | on | forced mode re-enumeration plus ChangeDisplaySettingsEx: 5 of 5 sizes exact |

sb2–sb4 are failed approaches. They are kept here, and the applier does not use
them.

## Limits

- No live run dragged the app window. The window-to-request mapping is covered
  by unit tests (`HvfDisplayResizeRequestTests`).
- A VM follows the window only after the driver install and the next VM start.
  Until then its resolution is fixed per VM (guest-resolution-20261103.md).
- The first frames at a new size can show the old desktop in the old-size area
  while Windows repaints. In these runs it settled within the `settle_ms`
  shown.
- After a guest restart, Windows re-attaches the Basic Display until the next
  RESIZE, which the app sends at agent start.
- The driver comes from upstream's download location. BridgeVM does not bundle
  it.
- The image, vars and frames are not retained here. Receipts carry sizes and
  counts only.
