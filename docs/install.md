# Installing BridgeVM

BridgeVM Engineering Preview runs Windows 11 ARM on Apple Silicon Macs (M1 or
later). It is distributed without an Apple Developer ID, which changes nothing
about what the app can do and one thing about how you first open it. Both
supported install paths are below, with the honest trade-off of each.

## Path 1 — terminal installer (recommended)

```sh
curl -fsSL https://raw.githubusercontent.com/Ketchio-dev/bridgevm/main/install.sh | bash
```

The bootstrap fetches the full installer at a pinned SHA-256 and refuses to run
anything else. The installer then fails closed at every step:

1. resolves the release and the exact expected asset name (drafts and
   prereleases are refused);
2. downloads the tarball and the release `SHA256SUMS`, and verifies the
   tarball's checksum before opening it;
3. audits the archive before extraction — absolute paths, `..` entries,
   anything outside a single `BridgeVM.app`, and symlinks escaping the bundle
   are all refused;
4. verifies `codesign --verify --deep --strict`, the bundle identifier, the
   declared executable, and that the binary is Apple Silicon native;
5. stages the verified app next to the destination, moves your existing app to
   a backup, renames the new app into place, and re-verifies it there;
6. on any failure after your old app was moved, the old app is restored
   automatically. On success the backup is removed unless you pass
   `--keep-backup`.

Your VM data (disks, UEFI variables, vTPM state under `~/BridgeVM`) and app
settings are never touched by install or update.

Useful options (forwarded through the one-liner with `bash -s --`):

```text
--version <tag>   install an exact release (e.g. v1.0.0)
--latest          latest non-draft release (default)
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

## Path 2 — dmg from the Releases page

Download the `.dmg` from the latest GitHub Release, verify it, open it, and
drag BridgeVM into Applications:

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
- About 64 GiB of free disk for the guest image.
- Guided 3D driver injection is currently unavailable. Test-signed packages
  conflict with the Microsoft Secure Boot policy provisioned by the app, and a
  package-local kernel-policy report is not signed provenance. BridgeVM rejects
  both before creating install media or mutating Windows. Turn off 3D injection
  to install Windows; see
  [`scripts/win-assets/DRIVERS-README.md`](../scripts/win-assets/DRIVERS-README.md).

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
