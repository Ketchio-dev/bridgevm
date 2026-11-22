# BridgeVM macOS sources

`apps/macos/Package.swift` is one SwiftPM package. Its name, `BridgeVMApp`, is
historical; it stays because it prefixes the app's resource bundle
(`BridgeVMApp_BridgeVMControl.bundle`) that the packager copies into the app.

## Targets

- `BridgeVMControl/` is the shipped app, packaged as `BridgeVM.app`. It holds
  the SwiftUI/AppKit library, create/import/install sheets, VM display and
  settings, and:
  - `HvfEngine/`: drives the Windows HVF runtime (the bundled
    `hvf_gic_boot_probe` and `hvf-runner`), the BVAGENT guest channel,
    clipboard/share, window inventory and Windows ISO installation;
  - `NativeRuntime/`: the app-owned runtime and install control used by both
    the GUI and the command line;
  - `NativeCLI/`: the command-line verbs the `bridgevm app …` CLI forwards to
    the app;
  - `AppUIHost*`: a DEBUG-only UI host (`BRIDGEVM_APP_UI_HOST`) for the UI
    contract tests; a release build with that flag does not compile.
- `BridgeVMWindowProtocol/` is a small library shared by the app and the
  product end-to-end helper: the guest `WIN` record grammar, window validation
  and the product end-to-end work boundary.
- `BridgeVMProductE2E/` is the product end-to-end driver (T17/T19/A9 lanes). The
  packager installs it inside the app as a separate helper app; it drives the
  real app through Accessibility and is not a user-facing feature.
- `AppleVzRunnerCore/` and `AppleVzRunner/` are the Apple Virtualization helper
  for the legacy Fast Mode Linux backend. The release app does not bundle them.

## Tests

- `Tests/BridgeVMControlTests`, `Tests/BridgeVMProductE2ETests` and
  `Tests/AppleVzRunnerTests` are XCTest targets. Without Xcode,
  `scripts/run-xctest-shim-suites.sh` runs them under a small XCTest shim.
  `scripts/run-control-xctest.sh` runs the app product-flow selection with
  Apple XCTest.
- `Tests/BridgeVMAppUIHostTests` checks the DEBUG UI host contracts of
  `BridgeVMControl`.
- `Tests/*SwiftTests` are swift-testing suites run by `scripts/run-swift-tests.sh`.

## Build and package

```sh
swift build --package-path apps/macos
```

builds every product for development. The distributable app comes from
`apps/macos/scripts/package-hvf-control-app.sh`; see
[Build the current source](../../../docs/install.md#build-the-current-source)
for its native dependencies and usage.
