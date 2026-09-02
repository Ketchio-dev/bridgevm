# Installing BridgeVM

BridgeVM runs Windows 11 Arm on Apple-silicon Macs (M1 or later). The General
Preview is distributed without an Apple Developer ID and is not notarized. That
changes how macOS establishes trust on first open; it does not authorize Windows
test drivers. General Preview installs and imports Windows in 3D-off mode.

> [!WARNING]
> The published `v1.0.0` predates the current fail-closed driver policy and is
> no longer recommended. It has no General Preview channel manifest, so the
> installer below refuses it. Until a successor is published, use the
> [current-source build](#build-the-current-source) instead.

## Path 1 — terminal installer (waiting for a safe release)

```sh
curl -fsSL https://raw.githubusercontent.com/Ketchio-dev/bridgevm/main/install.sh | bash -s -- --launch
```

Today this command exits without installing because no published release has
the required General Preview contract. It is documented here for the safe
successor; use the source-build path below in the meantime.

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

Your VM data (disks, UEFI variables, vTPM state under `~/BridgeVM`) and app
settings are never touched by install or update.

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

## Path 2 — DMG from the Releases page

Download the `.dmg`, `BridgeVM-release.json`, and `SHA256SUMS` from a release
identified as **General Preview**. Do not use the historical `v1.0.0`. Verify
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

- A Windows 11 ARM64 ISO. The app guides you through Microsoft's official
  download; nothing is fetched from third-party mirrors.
- A user-supplied signed ARM64 driver payload containing exactly the storage,
  serial and network roles, plus an external manifest derived from
  [`windows-guest-payload-v1.example.tsv`](../scripts/win-assets/windows-guest-payload-v1.example.tsv).
  The General Preview does not redistribute this payload. Its manifest must
  list every file and exact SHA-256; the app copies and seals both inputs in
  the VM bundle before installation.
- About 64 GiB of free disk for the guest image.
- No graphics-driver package is needed or accepted by the General Preview.
  Windows-HVF creation shows that 3D injection is unavailable and proceeds
  without it.

The ISO by itself is therefore not enough today. Payload preflight verifies
the declared ARM64 PE identity and each catalog's embedded CMS integrity; it
does not claim that Windows accepted or bound a driver. That remains live guest
evidence, and a missing or rejected payload leaves installation blocked rather
than falling back to unsigned drivers.

## Build the current source

Use this path while the safe successor to `v1.0.0` is still being prepared, or
whenever you want to inspect exactly what you run:

```sh
git clone https://github.com/Ketchio-dev/bridgevm.git
cd bridgevm
cargo build --workspace --locked
swift build --package-path apps/macos
packaging/macos/build-debug-app-bundle.sh
open target/macos/BridgeVMApp.app
```

This produces a local ad-hoc-signed development app. It does not turn the
checkout into release evidence. Contributors should follow
[`CONTRIBUTING.md`](../CONTRIBUTING.md) and run the appropriate deterministic
checks.

## Updating, rolling back, uninstalling

- **Update**: run the installer again (optionally `--version <tag>` to pin).
  The previous app is preserved until the new one verifies in place.
- **Roll back**: run the installer with `--version <older-tag>`, or if you
  updated with `--keep-backup`, remove the new app and rename the
  `BridgeVM.app.backup-*` bundle back.
- **Uninstall**: delete `/Applications/BridgeVM.app`. That removes the app
  only. Your virtual machines live separately under `~/BridgeVM`; delete that
  directory only if you also want to destroy the VMs, their disks, and their
  vTPM state.

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
