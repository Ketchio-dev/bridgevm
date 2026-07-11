# macOS Packaging

This directory contains the local packaging path for BridgeVM's SwiftUI app.
The current targets are a local debug `.app`/`.dmg` path for developer
inspection and a credentialed release-candidate path for Developer ID signing,
notarization, stapling, and public gate verification.

## Local Debug App Bundle

Build and verify a local app bundle from the repository root:

```sh
packaging/macos/build-debug-app-bundle.sh
```

By default, the script writes:

```text
target/macos/BridgeVMApp.app
```

The script:

```text
builds the BridgeVMApp and BridgeVMControl SwiftPM products
wraps the executable in a minimal .app bundle
writes BridgeVMControl.app as an isolated nested Windows HVF Lab
bundles the installed-Windows wrappers and signed release hvf_gic_boot_probe
writes a local debug Info.plist
codesigns the bundle, using ad-hoc signing by default
verifies the resulting bundle signature and executable metadata
```

Use a specific signing identity when you need to exercise local signing with a
certificate already installed in your keychain:

```sh
BRIDGEVM_CODESIGN_IDENTITY="Developer ID Application: Example" \
  packaging/macos/build-debug-app-bundle.sh
```

This still produces a debug artifact. The script runs `codesign` only; it does
not notarize, staple, enable hardened runtime options, or require App Store
Connect/notarytool credentials. Leaving `BRIDGEVM_CODESIGN_IDENTITY` unset keeps
the default ad-hoc signature (`-`) for local debug builds.

Override bundle metadata when rehearsing release inputs without changing the
credential-free debug packaging path:

```sh
BRIDGEVM_MACOS_APP_NAME="BridgeVM" \
BRIDGEVM_BUNDLE_DISPLAY_NAME="BridgeVM" \
BRIDGEVM_BUNDLE_NAME="BridgeVM" \
BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
BRIDGEVM_BUNDLE_VERSION="100" \
BRIDGEVM_BUNDLE_COPYRIGHT="Copyright 2026 Example" \
BRIDGEVM_MACOS_ICON_FILE="/path/to/BridgeVM.icns" \
  packaging/macos/build-debug-app-bundle.sh
```

The executable remains `BridgeVMApp`; `BRIDGEVM_MACOS_APP_NAME` controls only
the generated `.app` bundle name. `BRIDGEVM_MACOS_ICON_FILE` is optional; when
set, the script copies that `.icns` into `Contents/Resources` and records it as
`CFBundleIconFile`. By default the debug bundle also builds, ad-hoc signs, and
copies `bridgevmd`, `lightvm-runner`, and `AppleVzRunner` into
`Contents/Helpers` so the local app contains the same helper boundary that
release packaging verifies. It also always places the Windows HVF Lab at
`Contents/Applications/BridgeVMControl.app`, with its wrapper scripts under
the nested app's `Contents/Resources/scripts` and its release probe under
`Contents/Resources/target/release/examples`. The probe has the
`com.apple.security.hypervisor` entitlement and the Lab always invokes it with
`--release --skip-build`, so a packaged run does not need Cargo, Homebrew, or a
repository checkout. Open the Lab from BridgeVM Settings after selecting an
already-installed Windows ARM RAW disk and its matching UEFI vars. Set
`BRIDGEVM_MACOS_SKIP_APPLE_VZ_RUNNER=1` only for narrow packaging diagnostics
that intentionally do not need the helper. The DMG helper packages the app using
the basename of `BRIDGEVM_MACOS_APP`, or the name built from
`BRIDGEVM_MACOS_APP_NAME`, and verifies that the mounted image contains exactly
that one top-level `.app` plus the Applications symlink. When checking a
custom-named debug bundle with the release verifier, pass that same `.app` path
positionally; the verifier derives the expected top-level DMG app name from the
supplied bundle instead of assuming `BridgeVMApp.app`. Keep
`BRIDGEVM_MACOS_DMG_VOLUME` for the mounted volume name only.

Verify an existing local bundle:

```sh
packaging/macos/build-debug-app-bundle.sh --verify-only target/macos/BridgeVMApp.app
```

Verify-only also checks that `Contents/Helpers/bridgevmd` and
`Contents/Helpers/lightvm-runner` exist and are signed. If the bundle contains
`Contents/Helpers/AppleVzRunner`, verify-only also checks that helper's
signature and `com.apple.security.virtualization` entitlement. A bundled helper
without that entitlement is rejected. Verify-only also requires the nested
Windows HVF Lab, all five non-symlink wrapper scripts, a signed executable
release probe with the Hypervisor entitlement, the Lab bundle identifier derived
from the parent identifier, and valid nested/parent signatures.

Launch the local bundle through macOS LaunchServices:

```sh
open -n target/macos/BridgeVMApp.app
```

Credential-free local app usability proof: to prove the generated `.app`
executable can open a main window, supervise the bundled
`Contents/Helpers/bridgevmd`, and answer socket doctor without building or
mounting a DMG, notarizing, or using a Developer ID identity, require both the
GUI app and main window checks:

```sh
BRIDGEVM_MACOS_BUNDLED_DAEMON_REQUIRE_GUI=1 \
BRIDGEVM_MACOS_BUNDLED_DAEMON_REQUIRE_WINDOW=1 \
  tests/integration/macos-bundled-daemon-supervisor-smoke.sh
```

For a faster credential-free app readiness lane that includes the same
locally-usable-app proof, skips DMG build, mount, verification, and launch
gates, and still writes an app-only artifact manifest, run:

```sh
tests/integration/local-release-readiness-suite.sh --app-only --locally-usable-app
```

## Local Debug DMG

Build and verify a local DMG from the repository root:

```sh
packaging/macos/build-debug-dmg.sh
```

By default, the script writes:

```text
target/macos/BridgeVM.dmg
```

The DMG script:

```text
builds and verifies BridgeVMApp.app
copies the app into a temporary staging directory
adds an Applications symlink
creates a compressed UDZO image with hdiutil
verifies the image with hdiutil
mounts the image read-only and verifies the app bundle inside it
```

Verify an existing local DMG:

```sh
packaging/macos/build-debug-dmg.sh --verify-only target/macos/BridgeVM.dmg
```

## Release Candidate Verification

Use the release-candidate verifier when checking whether macOS artifacts are
ready for public distribution:

```sh
packaging/macos/verify-release-candidate.sh target/macos/BridgeVMApp.app target/macos/BridgeVM.dmg
```

That command expects public release gates to pass: bundle verification, DMG
verification, Gatekeeper assessment, stapled notarization tickets for both the
app and DMG, Developer ID signatures, hardened runtime flags, bundled
`Contents/Helpers/bridgevmd` and `Contents/Helpers/lightvm-runner` signatures,
and bundled `Contents/Helpers/AppleVzRunner` signature plus
`com.apple.security.virtualization` entitlement checks. It also requires
Developer ID and hardened-runtime signatures for the nested Windows HVF Lab and
its probe, plus the probe's `com.apple.security.hypervisor` entitlement. The verifier repeats
the helper checks against both the app bundle and the app mounted from the DMG.
Set `BRIDGEVM_RELEASE_TEAM_ID` when a release host should enforce a specific
Developer ID team identifier. Successful public verification prints
`PASS: BridgeVM macOS release candidate` after the individual `PASS:` gate
lines. Failure output names the failing gate and ends with
`BridgeVM macOS artifacts failed <n> public release gate(s).`

Common public-gate blockers are ad-hoc or non-Developer ID signatures, a
Developer ID team mismatch with `BRIDGEVM_RELEASE_TEAM_ID`, missing hardened
runtime flags on the app or bundled helpers, a missing or invalid
`AppleVzRunner` virtualization entitlement, absent stapled notarization tickets
on the app or DMG, Gatekeeper rejection, a DMG that does not contain the
expected single top-level app bundle, or a mounted app whose helper signatures
do not match the app-side checks.

Local debug artifacts should fail the public release gates. Check that boundary
explicitly with:

```sh
packaging/macos/verify-release-candidate.sh --expect-debug-boundary \
  target/macos/BridgeVMApp.app \
  target/macos/BridgeVM.dmg
```

That debug-boundary mode still runs the structural bundle and DMG checks, but
passes only when at least one public release gate rejects the artifacts. Its
success marker is:

```text
PASS: debug artifacts are structurally valid but not public release candidates
```

On an interactive macOS release host, add `--launch-smoke` or set
`BRIDGEVM_MACOS_LAUNCH_SMOKE=1` to mount the DMG, launch the contained app
through LaunchServices, and verify that a main window appears:

```sh
packaging/macos/verify-release-candidate.sh --launch-smoke \
  target/macos/BridgeVMApp.app \
  target/macos/BridgeVM.dmg
```

This launch smoke is opt-in so headless release hosts can still run the static
release gates. Set `BRIDGEVM_MACOS_LAUNCH_SMOKE_TIMEOUT_TENTHS` to tune the
window wait timeout in tenths of a second.

Before publishing a downloadable artifact, add `--quarantine-smoke` or set
`BRIDGEVM_MACOS_QUARANTINE_SMOKE=1` on an interactive release host. This copies
the DMG to a temporary location, applies the `com.apple.quarantine` xattr that
downloaded files receive, mounts that quarantined copy, launches the contained
app through LaunchServices, and verifies that a main window appears:

```sh
packaging/macos/verify-release-candidate.sh --quarantine-smoke \
  target/macos/BridgeVMApp.app \
  target/macos/BridgeVM.dmg
```

The quarantined smoke is also opt-in. For real release candidates its failures
are fatal. When `--expect-debug-boundary` is used for local ad-hoc/debug
artifacts, the verifier instead confirms that Gatekeeper rejects the
quarantined mounted app as an expected public-release boundary.
Both GUI smokes require an interactive macOS session, `hdiutil`, `open` or the
`BRIDGEVM_MACOS_OPEN_TOOL` override, and a detectable main window of at least
800x500 pixels. The quarantined smoke also requires `xattr`. Set
`BRIDGEVM_MACOS_LAUNCH_SMOKE_TIMEOUT_TENTHS` to tune the wait timeout in
tenths of a second when slower release hosts need more launch time.

## Artifact Manifest

Write an audit record for the generated app and DMG:

```sh
packaging/macos/write-artifact-manifest.sh \
  target/macos/BridgeVMApp.app \
  target/macos/BridgeVM.dmg \
  target/macos/BridgeVM-artifacts.txt
```

The manifest records artifact paths, sizes, SHA-256 digests, bundle metadata,
bundled helper presence/signature details, `AppleVzRunner` entitlement details
when present, Windows HVF Lab/probe paths and digests, probe entitlement details,
optional notarytool JSON sidecars, codesign details, Gatekeeper
assessment output, and stapler validation output.
It is intentionally a record, not a release gate; use
`verify-release-candidate.sh` to decide whether an artifact may be published.

## Release Boundary

The debug artifacts are not notarized release artifacts. Public distribution
uses `packaging/macos/build-release-candidate.sh` on a credentialed release
host. That host still owns the Developer ID identity, notarytool keychain
profile, final icon asset, final bundle metadata, and release records.

Expected credentialed release inputs:

- `BRIDGEVM_RELEASE_CODESIGN_IDENTITY`: Developer ID Application identity for
  release signing, for example `Developer ID Application: Example Corp
  (TEAMID)`. The release script also accepts `BRIDGEVM_CODESIGN_IDENTITY` as a
  compatibility alias.
- `BRIDGEVM_RELEASE_TEAM_ID`: optional expected Developer ID team identifier
  enforced by the final release verifier. Set this on release hosts to pin the
  artifact chain to the intended Apple developer team.
- `BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE`: keychain profile name created for
  `xcrun notarytool` notarization credentials.
- `BRIDGEVM_MACOS_ICON_FILE`: final `.icns` app icon path.
- `BRIDGEVM_BUNDLE_IDENTIFIER`, `BRIDGEVM_BUNDLE_SHORT_VERSION`, and
  `BRIDGEVM_BUNDLE_VERSION`: public bundle ID, marketing version, and build
  number.
- `BRIDGEVM_APPLE_VZ_ENTITLEMENTS`: optional release entitlements override for
  the Apple VZ runner. By default, the release script uses
  `apps/macos/AppleVzRunner.release.entitlements`.
- `BRIDGEVM_HVF_PROBE_ENTITLEMENTS`: optional release entitlements override for
  the installed-Windows probe. By default, the release script uses
  `apps/macos/HvfRunner.release.entitlements`.

Preview the release command plan without using credentials:

```sh
BRIDGEVM_RELEASE_CODESIGN_IDENTITY="Developer ID Application: Example Corp (TEAMID)" \
BRIDGEVM_RELEASE_TEAM_ID="TEAMID" \
BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE="bridgevm-notary-profile" \
BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
BRIDGEVM_BUNDLE_VERSION="100" \
BRIDGEVM_MACOS_ICON_FILE="/path/to/BridgeVM.icns" \
  packaging/macos/build-release-candidate.sh --dry-run
```

Check a credentialed release host before starting the slower build:

```sh
BRIDGEVM_RELEASE_CODESIGN_IDENTITY="Developer ID Application: Example Corp (TEAMID)" \
BRIDGEVM_RELEASE_TEAM_ID="TEAMID" \
BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE="bridgevm-notary-profile" \
BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
BRIDGEVM_BUNDLE_VERSION="100" \
BRIDGEVM_MACOS_ICON_FILE="/path/to/BridgeVM.icns" \
  packaging/macos/preflight-release-credentials.sh
```

The dry-run path validates that the required release inputs are present and
that `BRIDGEVM_RELEASE_CODESIGN_IDENTITY` is a `Developer ID Application:`
identity ending with a parenthesized team identifier, the notarytool keychain
profile is not blank, and `BRIDGEVM_MACOS_ICON_FILE` ends in `.icns`, but it
does not require the icon file, signing identity, or notarytool profile to be
available on the current host. Its command plan prints the final icon path,
Developer ID identity, notarytool keychain profile, and, when set, the
`BRIDGEVM_RELEASE_TEAM_ID` passed to the final verifier. The standalone
preflight without `--dry-run` additionally checks that the icon file exists,
the Developer ID identity is visible to `security find-identity`, and the
notarytool keychain profile can be read with `xcrun notarytool history`.

Remove `--dry-run` only on the credentialed release host. The script builds the
renamed release app bundle with SwiftPM/Cargo release configuration, builds
`AppleVzRunner` with SwiftPM release configuration, signs it with the release
virtualization entitlement, copies it to `Contents/Helpers/AppleVzRunner`,
builds and signs the release Windows HVF probe, signs the nested Windows HVF Lab,
applies hardened-runtime signing to nested helpers and the app, notarizes and
staples the app, creates and signs the DMG, notarizes and staples the DMG,
preserves app/DMG notarytool submit and log JSON sidecars, writes the artifact
manifest, and runs the public release-candidate verifier.

## Final Release Checklist

Use the local debug bundle/DMG scripts only for developer inspection:

- Build `target/macos/BridgeVMApp.app` and `target/macos/BridgeVM.dmg` locally.
- Accept ad-hoc signing or a local test identity.
- Verify bundle signatures, executable metadata, DMG integrity, and mounted
  content.
- Run the local `.app` usability smoke to prove `BridgeVMApp` opens a main
  window, supervises bundled `bridgevmd`, and answers socket doctor before
  any DMG, notarization, or Developer ID gates.
- Write `target/macos/BridgeVM-artifacts.txt` for local artifact traceability.
- Optionally run the local release-readiness suite for formatting, tests, and
  debug packaging coverage.

Public distribution uses the credentialed release path:

- Build with the release configuration and the intended product metadata.
- Sign with the approved Developer ID Application identity.
- Enable and review the hardened runtime and entitlements required by the app;
  the release script signs `Contents/Helpers/AppleVzRunner` with
  `apps/macos/AppleVzRunner.release.entitlements`.
- Include final icon, bundle identifier, version, copyright, and DMG/PKG
  branding assets.
- Submit the signed artifact to Apple's notarization service and wait for
  acceptance.
- Staple the notarization ticket to the distributed artifact.
- Run `packaging/macos/verify-release-candidate.sh` and verify Gatekeeper
  assessment on a clean macOS host before publishing.
- On an interactive macOS release host, run the verifier with `--launch-smoke`
  to prove the app mounted from the DMG opens a main window through
  LaunchServices.
- On an interactive macOS release host, run the verifier with
  `--quarantine-smoke` to prove the downloaded/quarantined DMG path opens a
  main window through LaunchServices.
- Preserve the artifact manifest, notarization log, checksums, and release
  inputs with release records.
- Publish only the notarized, stapled artifact.
