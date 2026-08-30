# D2 RELEASE-TURNKEY receipt — distributable DMG + first-run import (2026-07-24)

## Result: PARTIAL (self-controllable parts done; Developer ID + notarization EXTERNAL)

A one-command DMG packager wraps the self-contained BridgeVMControl app with a
first-run quickstart; the app's deep/strict codesign verifies inside the mounted
image; a first-run import wizard registers an existing Windows HVF bundle and
boots it through the already-proven HVF launch path. The only remaining items —
a Developer ID Application certificate and Apple notarization — are external to
this free-account host and are labelled as such, not hidden.

## DMG packaging (done)

- Script: `scripts/package-release-dmg.sh` (app -> UDZO DMG + `/Applications`
  symlink + `QUICKSTART.md`; refuses to overwrite; verifies deep/strict codesign
  of the input app).
- Built from the self-contained `BridgeVMControl-final2-20260723.app`:
  - `BridgeVM-release-20260724.dmg` — 12,744,901 bytes —
    SHA-256 `7d67b9ec1c38b26aacf3ca0d5e52fcde4ea532edea3580e0f007cf0a35f42ce3`
- Mounted image verification:
  - `codesign --verify --deep --strict .../BridgeVMControl-final2-20260723.app`
    → `INDMG_CODESIGN_OK`
  - `QUICKSTART.md` and `Applications` symlink present.
  - hypervisor entitlement preserved in the DMG copy:
    `codesign -d --entitlements -` on the bundled
    `hvf_gic_boot_probe` shows `com.apple.security.hypervisor` (the app retains
    its VM-boot capability; the DMG copies, it does not re-sign).

## Notarization boundary (EXTERNAL)

- Script: `scripts/notarize-submit.sh` (submits with `notarytool` when a
  Developer ID and a stored credential profile exist; otherwise labelled exit).
- On this host: `EXTERNAL_NOTARIZATION_REQUIRED dmg=... developer_id=0
  profile=<none>` and exit 0.
- Gatekeeper assessment of the ad-hoc-signed app is `rejected`
  (`spctl --assess --type execute` exit 3), `Signature=adhoc`,
  `TeamIdentifier=not set`. This is the documented free-account boundary:
  distribution to other Macs requires a paid Developer ID + notarization.

## First-run import wizard (done)

- `apps/macos/Sources/BridgeVMControl/FirstRunImport.swift` — fail-closed
  validation of user-picked paths (disk exists/non-empty/is-a-file; UEFI vars is
  exactly 64 MiB; optional vTPM path must be a directory; RAM/CPU bounds) plus
  bundle materialization (hard-link the large disk when same-volume, copy vars
  and vTPM state, never copy the swtpm `.lock`) and VMConfig registration.
- `FirstRunView.swift` — the wizard UI, shown by `ContentView` when the library
  has no VMs; NSOpenPanel pickers, RAM/CPU steppers, one "Import & Boot" action.
- `LibraryModel.importExistingHvfVM` — validate → register → `VMLibrary.save` →
  select, fully fail-closed (nothing registered on any error).
- The registered VM boots through the same `HvfEngineConfig` launch path already
  proven live in C5 (same-ID restore boot on the second Mac,
  `second-mac-migration-20260723.md`) and C8 (`gpu-live-receipt-20260723.md`).

## Verification

```
bash tests/integration/release-dmg-smoke.sh
# -> PASS: release DMG builds, verifies, carries quickstart; notarization boundary EXTERNAL

# apps/macos Swift tests (run on the second Mac; [A] SDK lacks XCTest):
swift test --filter FirstRunImportTests    # 9/9 pass
swift test                                 # 646 pass, 1 skipped, 0 failures
```
