# Hosted swift-testing toolchain pairing — 2026-10-21

This records a hosted CI regression on unchanged main and its test-infrastructure
repair. It is deterministic CI evidence only: no guest, live queue, physical UI or
release criterion is involved. Product state, capability wording, criterion states,
thresholds and known defects remain unchanged. A11 remains OPEN.

## Hosted failure on unchanged main

Main `5cd18f46a903fc883d8a04bd7e46d1ae2d0a8bf6` passed all push workflows on
2026-10-15. Its scheduled
[CI run 39279022129](https://github.com/Ketchio-dev/bridgevm/actions/runs/39279022129)
on 2026-10-19 failed one job, `macOS app suites (macos-26)`, at
`scripts/run-swift-tests.sh`; the other 13 jobs passed. All three swift-testing
suites failed to compile with `failed to load library plugin` for the Command Line
Tools `libTestingMacros.dylib` inside the Xcode 26.6 plugin server, reporting
`Symbol not found: _$s24CompilerSwiftDiagnostics17DiagnosticMessageP13categoryChainSayAA0D8CategoryVGvgTq`.
The same job failed in the same way on Dependabot pull requests 416 and 419.

Every failing job ran on runner image `macos-26-arm64/20261013.0424`. The earlier
passing jobs (main push CI 38749340144 and draft PR CI 38768192458 and 38776484082)
ran on `macos-26-arm64/20260907.0351`. The published image notes list Xcode 26.6 as
the default on both, with Command Line Tools 26.6.0.0.1781586589 on the older image
and 27.0.0.0.1788430756 on the newer one. The script used `swiftc` from the selected
Xcode but fixed Command Line Tools paths for Testing.framework, the TestingMacros
plugin and `lib_TestingInterop`. The `BridgeVMProductE2E` shim suite had the same
fixed paths; that step did not run in the failed jobs because the earlier step failed.

## Local reproduction of the failure class

This Studio has Xcode 27.0 (Swift 6.4) and Command Line Tools 26.6 (Swift 6.3.3).
Typechecking a one-test swift-testing probe with the Command Line Tools compiler and
the Xcode 27 TestingMacros plugin exited 1 with the identical missing symbol. The
consistent pairs, Command Line Tools with its own plugin and Xcode with its own
plugin, exited 0. The unchanged script's pairing on this Mac, Xcode 27 with the older
Command Line Tools plugin, also exited 0, which is why local full checks kept
passing. This reproduces the mismatch class; it is not a run of the hosted image.

## Repair

`scripts/swift-testing-toolchain.sh` resolves Testing.framework, the TestingMacros
plugin, `lib_TestingInterop` and the Swift runtime directory from the developer
directory of the `swiftc` on `PATH`, following the `/usr/bin` shim through `xcrun`.
It covers Xcode toolchain and Command Line Tools layouts, and refuses an unknown
layout or a missing framework or plugin. `scripts/run-swift-tests.sh` and the
`BridgeVMProductE2E` shim suite use it. The live worker still sets the Command Line
Tools developer directory, so it resolves the same files as before.

`tests/integration/swift-testing-toolchain-smoke.sh` checks both layouts with owned
fake trees, the three refusals and the active compiler, and runs in the project
check's swift tests stage. Re-hard-coding the Command Line Tools plugin made it fail.
Existing structural ceilings were retained or lowered; new files are registered at
their measured sizes.

## Focused local results

| Check on the repair | Xcode 27 developer directory | Command Line Tools 26.6 |
| --- | --- | --- |
| Toolchain contract | PASS | PASS |
| `run-swift-tests.sh` | 3 suites, 18 tests passed | 3 suites, 18 tests passed |
| Shim suites | 425 / 1,427 / 65 / 379 passed, 0 failed, 3 skipped | 425 / 1,427 / 65 / 379 passed, 0 failed, 3 skipped |

Focused results do not replace the exact-source local full check or the hosted
macOS 26 job on the newer image; those results belong in the pull request record.
