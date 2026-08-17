# Installing BridgeVM

BridgeVM 1.0 runs Windows 11 ARM on Apple Silicon Macs (M1 or later). It is
distributed without an Apple Developer ID, which changes nothing about what the
app can do and one thing about how you first open it. Both supported install
paths are below, with the honest trade-off of each.

## Path 1 — terminal one-liner (recommended: no Gatekeeper dialog)

```sh
curl -fsSL https://raw.githubusercontent.com/Ketchio-dev/bridgevm/main/install.sh | bash
```

This downloads the release tarball and moves the app into `/Applications`.
Files fetched by `curl` carry no quarantine attribute, so the app opens
normally on first launch. The installer refuses to run on anything but an
Apple Silicon Mac.

If you prefer to see what you run first (reasonable), download
[`install.sh`](../install.sh), read it, then run it.

## Path 2 — dmg from the Releases page

Download the `.dmg` from the latest GitHub Release, open it, and drag BridgeVM
into Applications.

Because a browser download is quarantined and the app is not notarized, the
**first** launch is blocked with "Apple could not verify...". This is expected.
On macOS Sequoia and later the only supported unblock is:

1. Attempt to open the app once (double-click, let it be blocked, dismiss).
2. System Settings → Privacy & Security → scroll to the Security section.
3. Next to the message about BridgeVM, click **Open Anyway** and confirm.

This is required exactly once. Right-click → Open no longer bypasses the block
on current macOS, so the Settings route is the real one.

Alternative for terminal users, equivalent in effect:

```sh
xattr -d com.apple.quarantine /Applications/BridgeVM*.app
```

## What you need after installing

- A Windows 11 ARM64 ISO. The app guides you through Microsoft's official
  download; nothing is fetched from third-party mirrors.
- About 64 GiB of free disk for the guest image.

## Verifying a download

Every release asset ships with its SHA-256 in the release notes:

```sh
shasum -a 256 BridgeVM-*.tar.gz   # compare against the release notes
```

## Known limits of the unsigned distribution

- `spctl -a` reports the app rejected; that is what "not notarized" looks like
  and is expected. The codesign seal (ad-hoc) is still verified intact by
  `codesign --verify --deep --strict`.
- macOS may re-quarantine the app if you copy it through a browser-synced
  folder; re-run the `xattr` command if the block reappears.
