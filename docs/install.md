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

## Build the current source

Use this path when you want to inspect exactly what you run. Requirements are
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

## Terminal installer

```sh
curl -fsSL https://raw.githubusercontent.com/Ketchio-dev/bridgevm/main/install.sh | bash -s -- --launch
```

This command installs the newest General Preview (currently `v0.1.0`).

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
chose), their shared folders under `~/BridgeVM 공유` and app settings are never
touched.

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

Download the `.dmg`, `BridgeVM-release.json`, and `SHA256SUMS` from a release
identified as **General Preview**. Verify
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

This is required exactly once. Right-click → Open no longer bypasses the block
on current macOS, so the Settings route is the real one.

## What you need after installing

To install Windows from an ISO:

- A Windows 11 ARM64 ISO, downloaded from Microsoft yourself. The app does not
  fetch Windows media; it asks you to choose the ISO file.
- Signed ARM64 storage, serial and network drivers. Either:
  - click **드라이버 자동 준비** in the create sheet (or pass `--fetch-drivers` to
    `bridgevm app create-windows`): the app downloads the pinned Red Hat
    virtio-win 0.1-299 archive (about 95 MB, Microsoft-signed) from the official
    fedorapeople.org source, refuses it unless its length and SHA-256 match the
    pins, keeps it under `~/Library/Application Support/BridgeVM/driver-payloads`,
    and writes the `Win11/ARM64` viostor, vioser and netkvm (with `netkvmp.exe`)
    files plus their manifest there; or
  - choose your own payload folder containing exactly the storage, serial and
    network roles, plus an external manifest derived from
    [`windows-guest-payload-v1.example.tsv`](../scripts/win-assets/windows-guest-payload-v1.example.tsv).
    Its manifest must list every file and exact SHA-256.

  BridgeVM itself does not redistribute these drivers. Either way the app copies
  and seals the payload and manifest in the VM bundle before installation.
- About 64 GiB of free disk for the guest image.

To import instead: an installed Windows 11 Arm disk (RAW, QCOW2 or Hyper-V
VHDX), with its UEFI variables if you have them. Import copies or converts the
disk into the library and leaves the source unchanged; it needs no driver
payload. Builds from current source also accept fixed-size Hyper-V VHD. Convert
a dynamic VHD to VHDX in Hyper-V before importing.

For compressed QCOW2 in the v0.1.0 download, import a RAW copy. Builds from
current source support QEMU's default deflate compression. Use RAW for
zstd-compressed QCOW2 with either build. Export a RAW copy from the original
VM app, or use `qemu-img` with a new output filename.

```sh
qemu-img convert -O raw "Windows.qcow2" "Windows.raw"
```

Choose `Windows.raw` in the import wizard.

The General Preview bundles no Windows driver and accepts no graphics-driver
package at creation; Windows-HVF creation shows that 3D injection is
unavailable and proceeds without it. Once Windows runs, VM settings offer an
optional install of the Microsoft-signed Red Hat virtio-win display driver,
which Windows downloads itself. After a VM restart the display follows the
window size; without it the resolution is fixed per VM (1280x800 by default).

On November 24, 2026, a development Mac used **드라이버 자동 준비** with
an empty cache and a Windows 11 Arm ISO. The app checked the archive and
sealed its driver files into the VM. Driver preflight accepted all three catalogs.
The installed system reached its first interactive desktop with the guest
agent, VirtioSerial and NetKVM running. Windows used NVMe for its system
drive, and DNS and an HTTP request succeeded over the guest network. The
run used an unattended test account on the development Mac. Clean-machine
coverage is tracked in [the release criteria](../STATUS.md).

## Updating, rolling back, uninstalling

- **Update**: run the installer again (optionally `--version <tag>` to pin).
  The previous app is preserved until the new one verifies in place.
- **Roll back**: run the installer with `--version <older-tag>`, or if you
  updated with `--keep-backup`, remove the new app and rename the
  `BridgeVM.app.backup-*` bundle back.
- **Uninstall**: delete `/Applications/BridgeVM.app`. That removes the app
  only. Your VMs live under `~/Library/Application Support/BridgeVM/vms` (or
  the storage folder you chose); delete them only if you also want to destroy
  their disks and vTPM state. Each VM's shared folder `~/BridgeVM 공유/<VM name>`
  holds your own files and is not removed. vTPM keys stay in the Keychain
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
