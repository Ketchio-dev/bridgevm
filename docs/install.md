# Installing BridgeVM

BridgeVM targets Windows 11 Arm on Apple silicon. The declared host matrix is
M4/M5; M1, M2 and M3 remain uncovered, and this is not a promise for every Mac.
See [current status](../STATUS.md#known-limitations) for the evidence boundary.
The General Preview is distributed without an Apple Developer ID and is not
notarized. That changes how macOS establishes trust on first open; it does not
authorize Windows test drivers. General Preview installs and imports Windows in 3D-off mode.

> [!NOTE]
> Version numbers restarted at `v0.1.0` on 2026-11-23. `v0.1.0` is built from
> the same source as the withdrawn `v1.1.1`; to replace an installed 1.1.1 app,
> run the installer again. VMs and settings are unaffected.

Install the General Preview with the [terminal installer](#terminal-installer)
or the [DMG](#dmg-from-the-releases-page), then continue with
[What you need after installing](#what-you-need-after-installing). Building the
current source is described [at the end of this guide](#build-the-current-source).

## Terminal installer

```sh
curl -fsSL https://raw.githubusercontent.com/Ketchio-dev/bridgevm/main/install.sh | bash -s -- --launch
```

This command installs the newest General Preview (currently `v0.1.0`). It uses
only tools that come with macOS, so it works on a Mac without the Xcode Command
Line Tools. Quit BridgeVM and shut down its VMs first: the installer refuses to
replace an app that is running.

The bootstrap fetches the full installer at a pinned SHA-256 and refuses to run
anything else. The installer then fails closed at every step:

1. selects the newest non-draft, non-prerelease release carrying
   `BridgeVM-release.json`;
2. verifies that the release contract identifies a 3D-off General Preview with
   no Windows kernel driver, no TESTSIGNING requirement, and no product driver
   injection;
3. downloads the tarball and `SHA256SUMS`, then verifies both the contract and
   tarball before opening the archive;
4. audits the archive before extraction — absolute paths, `..` entries,
   anything outside a single `BridgeVM.app`, and symlinks escaping the bundle
   are all refused;
5. verifies `codesign --verify --deep --strict`, the bundle identifier, the
   declared executable, and that the binary is Apple Silicon native;
6. stages the verified app next to the destination, moves your existing app to
   a backup, renames the new app into place, and re-verifies it there;
7. on any failure after your old app was moved, the old app is restored
   automatically. On success the backup is removed unless you pass
   `--keep-backup`.

Install and update replace only the app bundle. Your VMs (disks, UEFI
variables and vTPM state, by default under
`~/Library/Application Support/BridgeVM/vms` or in the storage folder you
chose), their shared folders and app settings are preserved.

Useful options (forwarded through the one-liner with `bash -s --`):

```text
--version <tag>   install an exact release carrying the required contract
--latest          latest verified General Preview (default)
--dry-run         verify everything, install nothing
--dest <dir>      install somewhere other than /Applications
--keep-backup     keep the previous app as a .backup bundle
--launch          open the app after installing
--verbose         print each verification step
```

If `/Applications` is not writable the installer stops with a clear permission
error instead of failing halfway; it never invokes `sudo` on your behalf. Either
fix the permission or install to a writable `--dest`.

### What verification does and does not guarantee

The checksum is published in the same GitHub release as the tarball, so the
trust root is HTTPS to `github.com` plus that release. Verification proves the
bytes you received are the bytes the release carries and that the bundle seal is
intact. It does **not** prove who built the artifact: the app is ad-hoc signed,
not Developer ID signed or notarized. If you need a stronger provenance chain,
build from source and compare hashes.

A tarball fetched by `curl` carries no quarantine attribute, so the app opens
without the Gatekeeper dialog. That is a UX property, not a security property —
the integrity guarantees come from the checks above, not from skipping
quarantine.

## DMG from the Releases page

Download the `.dmg`, `BridgeVM-release.json`, and `SHA256SUMS` from the
[General Preview release](https://github.com/Ketchio-dev/bridgevm/releases/tag/v0.1.0). Verify
the DMG, open it, and drag BridgeVM into Applications:

```sh
shasum -a 256 BridgeVM-*.dmg   # compare against SHA256SUMS on the release
```

Because a browser download is quarantined and the app is not notarized, the
**first** launch is blocked with "Apple could not verify...". This is expected.
On macOS Sequoia and later the only supported unblock is:

1. Attempt to open the app once (double-click, let it be blocked, dismiss).
2. System Settings → Privacy & Security → scroll to the Security section.
3. Next to the message about BridgeVM, click **Open Anyway** and confirm.

You need to do this once for each version you download: every new DMG is
blocked again on its first launch. Right-click → Open no longer bypasses the
block on current macOS, so the Settings route is the real one.

## What you need after installing

### Install Windows from an ISO

1. Choose **Install Windows** (v0.1.0: **새 Windows VM 만들기**), then **Choose ISO…**
   (v0.1.0: **ISO 선택…**) and select your Windows 11 Arm ISO.
2. Keep the name “Windows 11” in **Name (required)** or type another. In v0.1.0
   the field is **이름** and “Windows 11” is only a placeholder, so type a name.
   Choose **Download Drivers** (v0.1.0: **드라이버 자동 준비**) or select
   your own driver folder and its manifest.
3. Choose **Create** (v0.1.0: **생성**). In the new VM’s detail view, choose
   **Start Installation** (v0.1.0: **설치 시작**). The view shows the elapsed
   time; current source builds also show the steps **Check files**, **Prepare
   media**, **Install Windows** and **Save VM**.
4. Leave the Mac awake until installation finishes. In current source builds,
   **Keep Mac awake while Windows is running or installing** (on by default, in a
   VM’s **Settings** → **General**) prevents idle sleep during installation.
5. When installation finishes, choose **Start** (v0.1.0: **시작**). Finish Windows
   setup and sign in in the VM window. BridgeVM then connects to Windows.

- A Windows 11 Arm64 ISO from Microsoft’s
  [Windows 11 Arm64 download page](https://www.microsoft.com/en-us/software-download/windows11arm64).
  An x64 ISO is refused. The app does not fetch Windows media; it asks you to
  choose the ISO file.
- Signed ARM64 storage, serial and network drivers. Either:
  - Click **Download Drivers** in the Create window (**드라이버 자동 준비** in
    v0.1.0), or pass `--fetch-drivers` to `bridgevm app create-windows` (see
    [Installing the command](app-cli.md#installing-the-command) for its full path).
    BridgeVM downloads the pinned Red Hat virtio-win 0.1-299 archive (about
    95 MB, Microsoft-signed) from the official fedorapeople.org source, refuses it unless its length and SHA-256 match the
    pins, keeps it under `~/Library/Application Support/BridgeVM/driver-payloads`,
    and writes the `Win11/ARM64` viostor, vioser and netkvm (with `netkvmp.exe`)
    files plus their manifest there; or
  - choose your own payload folder containing exactly the storage, serial and
    network roles, plus an external manifest derived from
    [`windows-guest-payload-v1.example.tsv`](../scripts/win-assets/windows-guest-payload-v1.example.tsv).
    Its manifest must list every file and exact SHA-256.

  BridgeVM itself does not redistribute these drivers. Either way the app copies
  and seals the payload and manifest in the VM bundle before installation.
- Free disk space for the VM. Its disk is 64 GiB by default, the smallest size
  offered. Memory is 6 GiB by default (4 GiB on an 8 GiB Mac).

Current source builds copy the selected ISO into the VM bundle. When the ISO is
on another drive, that copy needs the ISO’s size plus 1 GiB free on the VM’s
drive. If the installer reports a missing or changed ISO, click the ISO filename in the VM detail view
to open its location in Finder. Restore the ISO used to create that VM as
`disks/installer.iso` in its bundle, then click **Try Again**. BridgeVM checks the
restored file against its original SHA-256 before preparing Windows.

### Import an installed Windows disk

1. Choose **Import an existing disk** (v0.1.0: **기존 VM 가져오기**). Keep the
   prefilled name “Windows 11” under **Name in library** or enter a different name.
2. Beside **Windows disk** (v0.1.0: **Windows 디스크**), click **Choose or drop
   file…** (v0.1.0: **선택…**) and select the installed Windows 11 Arm disk. If
   you have its matching UEFI variables file, select it in the separate optional
   UEFI field.
3. Choose **Import** (v0.1.0: **가져오기**). Select the saved VM in the library and
   choose **Start** (v0.1.0: **시작**) to boot it.
4. After the first start, choose **Install Guest Tools** (v0.1.0: **게스트 도구
   설치**) when BridgeVM offers it. Shut down Windows and start it again, then run
   **install.cmd** from the new drive in Windows and approve the administrator
   prompt. This enables clipboard sharing and shared folders.

Windows on the imported disk must already have the Red Hat VirtIO Serial
driver; the guest tools install only the BridgeVM agent, which needs it. The VM’s
network adapter is virtio-net, so Windows also needs the NetKVM driver for
networking.

If BitLocker protected this disk with another TPM, have your BitLocker recovery
key ready before the first start.

See the [Windows desktop after an installed-disk import](media/windows-hvf-import-20261126.webp)
from a development build running with 3D off.

The v0.1.0 download accepts RAW, QCOW2 and Hyper-V VHDX. BridgeVM copies or
converts the disk into the library and leaves the source unchanged. Import
needs no driver payload. Builds from current source also accept fixed-size
Hyper-V VHD. Convert a dynamic VHD with `qemu-img convert -O qcow2` before
importing. Encrypted QCOW2 disks and disks that depend on a backing file are
refused; export a decrypted, standalone copy from the original VM app first.

For compressed QCOW2 in the v0.1.0 download, import a RAW copy. Builds from
current source support QEMU's default deflate compression. Use RAW for
zstd-compressed QCOW2 with either build. Export a RAW copy from the original
VM app, or use `qemu-img` with a new output filename. If you don’t have
`qemu-img`, install it with `brew install qemu`.

```sh
qemu-img convert -O raw "Windows.qcow2" "Windows.raw"
```

Choose `Windows.raw` in the import wizard.

### Shared folder and dropped files

In current source builds, every new VM, installed or imported, shares the Mac
folder `~/BridgeVM Shared Folders/<VM name>` as `C:\bridgevm-share` in Windows.
VMs created by v0.1.0 use the folder named under
[Uninstall](#updating-rolling-back-uninstalling). To share another folder, shut
down Windows and change it in the VM's **Settings** under **Sharing**. Files you
drop on the VM window are copied to the Windows desktop.

### Display drivers after setup

The General Preview bundles no Windows driver and accepts no graphics-driver
package at creation; Windows is created with the standard display and 3D
acceleration off. Once Windows runs and BridgeVM is connected, open **Settings**
→ **Display** and choose **Install display driver**, or choose **Install** in the
notice over the Windows display. Windows downloads the Microsoft-signed Red Hat
virtio-win display driver itself (about 95 MB). Then choose **Restart Now** in
the notice, or shut down the VM and start it again. Restarting Windows inside
the VM is not enough: it cannot attach the new display. After that the display
follows the window size; without it the resolution is fixed per VM (1280x800 by
default).

With the display driver installed, current source builds also offer **Retina
resolution** under **Settings** → **Display**. It is experimental and off by
default: on a Retina display, Windows uses the window's native pixels with 200%
scaling, which uses more CPU.

## Updating, rolling back, uninstalling

- **Update**: quit BridgeVM and shut down its VMs, then run the installer again
  (optionally `--version <tag>` to pin). The previous app is preserved until the
  new one verifies in place. If you installed from the DMG, download the new
  version's DMG, verify it and drag BridgeVM into Applications again; its first
  launch is blocked again, so repeat **Open Anyway** as described above.
- **Roll back**: run the installer with `--version <older-tag>`, or if you
  updated with `--keep-backup`, remove the new app and rename the
  `BridgeVM.app.backup-*` bundle back.
- **Uninstall**: delete `/Applications/BridgeVM.app`. That removes the app
  only. Your VMs live under `~/Library/Application Support/BridgeVM/vms` (or
  the storage folder you chose); delete them only if you also want to destroy
  their disks and vTPM state. New VMs in source builds use
  `~/BridgeVM Shared Folders/<VM name>` by default. VMs created by v0.1.0
  keep their saved path, normally `~/BridgeVM 공유/<VM name>`. Uninstalling
  preserves those folders and their files. vTPM keys stay in the Keychain
  (`com.bridgevm.vtpm-state-key.v1`).

## Known limits of the unsigned distribution

- `spctl -a` reports the app rejected; that is what "not notarized" looks like
  and is expected. The ad-hoc codesign seal is still verified intact by the
  installer and can be re-checked any time with
  `codesign --verify --deep --strict /Applications/BridgeVM.app`.
- macOS may quarantine the app if you copy it through a browser-synced folder;
  use the System Settings "Open Anyway" flow above if the block appears.
- The checksum and ad-hoc signature establish artifact integrity relative to
  the GitHub release; they do not establish a Developer ID publisher identity.

Graphics-driver development is a separate, opt-in test environment. It is not
an installation option in the General Preview; see the
[distribution channel contract](distribution-channels.md).

## Build the current source

Building the current source gives you the English Mac interface. Requirements are
Apple silicon, macOS 15+, Xcode/Swift, and Rust 1.85+. Follow the
[development setup](../CONTRIBUTING.md#set-up-a-development-checkout) for toolchains.
The app is built by the same
[packager](../apps/macos/scripts/package-hvf-control-app.sh) the
[release workflow](../.github/workflows/release.yml) runs. It compiles the Swift
app and the Rust helpers itself, but it needs the native dependencies below
first.

### Prepare the native dependencies

- Install the Homebrew packages `meson`, `ninja`, `pkgconf`, `libepoxy`,
  `molten-vk`, `vulkan-loader`, `vulkan-headers`, `openssl@3` and `zig`. Ensure
  the `python3` on your PATH can import PyYAML (`python3 -c 'import yaml'`). The
  [renderer build script](../scripts/build-venus-host-deps.sh) checks its
  inputs before fetching and building its pinned, patched renderer.
- The TPM packager requires **swtpm 0.10.1** and **libtpms 0.10.2**, installed
  under the versioned Apple-silicon Homebrew Cellar at `/opt/homebrew/Cellar`,
  with their dependency libraries and licence files. Check `swtpm --version`
  and `brew list --versions swtpm libtpms` before building. The
  [TPM bundler](../apps/macos/scripts/bundle-swtpm-runtime.sh) verifies the
  actual linked libtpms version as well. Installing the latest Homebrew
  packages does not guarantee these versions; if they are unavailable, packaging
  is blocked. The release workflow records the pinned Homebrew recipe used on
  disposable hosted runners; do not copy its runner-only Homebrew checkout
  replacement onto your development Mac.

### Build and package the app

Run these in one shell:

```sh
git clone https://github.com/Ketchio-dev/bridgevm.git
cd bridgevm
export BRIDGEVM_3D_DIR="$PWD/target/native-host"
export BRIDGEVM_VENUS_PREFIX="$BRIDGEVM_3D_DIR/prefix"
scripts/build-venus-host-deps.sh
scripts/verify-venus-native-artifacts.sh
scripts/build-pinned-wimlib-runtime.sh --output "$PWD/target/wimlib-runtime"
apps/macos/scripts/package-hvf-control-app.sh \
  --output target/BridgeVM.app \
  --wimlib-runtime "$PWD/target/wimlib-runtime"
open target/BridgeVM.app
```

The two exported paths must agree: the renderer builder installs below
`BRIDGEVM_3D_DIR`, while the Rust linker reads `BRIDGEVM_VENUS_PREFIX`. The
packager always builds the Windows HVF probe with its `venus` feature and
bundles the renderer libraries, including for a 3D-off guest. These are host
build dependencies; 3D acceleration is excluded from General Preview and v1.

The [wimlib builder](../scripts/build-pinned-wimlib-runtime.sh) downloads the
pinned wimlib source, checks its SHA-256 and builds the `wimlib-imagex` helper
the app uses for Windows ISO installation; pass `--source-archive` with an
absolute path to build from a tarball you already have. Neither script
overwrites existing output, so delete `target/wimlib-runtime` or
`target/BridgeVM.app` before building them again.

The result is an ad-hoc-signed app; the packager refuses to finish if a resource
the app requires for ISO installation is missing. Choose your own Windows ISO,
then let the app prepare the drivers or select your own signed driver payload. Set `BRIDGEVM_CODESIGN_IDENTITY`
to sign with your own identity. A local build does not turn the checkout into
release evidence. Contributors should follow
[`CONTRIBUTING.md`](../CONTRIBUTING.md) and run the appropriate deterministic
checks.
