# A11: an XCTest shim, and what it does not yet reach (2026-08-05)

## The problem

651 XCTest functions in `apps/macos/Tests/` have never run on this machine.
XCTest.framework ships with Xcode; Xcode requires an Apple ID to download; the
user declines to create one. `swift test` fails at the import. Only 6
swift-testing tests run, via `scripts/run-swift-tests.sh`.

The repository uses 13 assertion APIs and no runtime-dependent ones -- no
`measure`, no `expectation`, no `waitForExpectations`, only two
`addTeardownBlock` calls. That is a small enough surface to stand in for.

## What exists now

`apps/macos/XCTestShim/XCTest.swift` implements all 13, plus `XCTAssertNoThrow`,
`XCTAssertEqual(accuracy:)`, `XCTSkipIf`, and teardown blocks. It re-exports
Foundation and Dispatch, as Apple's XCTest does, because hundreds of tests rely
on `import XCTest` alone bringing in `Date` and `DispatchQueue`.

Matching Apple's signatures mattered more than it looked. The first version
made the assertions `rethrows`, which is not what XCTest does: XCTest evaluates
a throwing autoclosure and records the error as a failure, so callers write
`try` inside the call without marking the call site. With `rethrows`, hundreds
of existing call sites stop compiling. A shim that requires the tests to be
edited is not running the same tests.

**All 651 test functions compile against it.** That was checked with
`swift build --build-tests`: 755 errors initially, then 6, then 0.

## What is proven

`scripts/run-xctest-shim-selftest.sh` runs 29 checks. Each assertion gets a
deliberate falsification -- assert something untrue, require the shim to have
recorded it -- plus a control that it does not fire on a true assertion.
`XCTUnwrap` is checked to *throw* on nil and not merely record, since tests
depend on it stopping. `XCTSkipIf` is checked to throw rather than pass, since
a skipped test is not a passing one. Teardown blocks are checked to run in
reverse order.

The self-test was itself falsified: replacing XCTAssertEqual's comparison with
`if false` produces `NOT CAUGHT: XCTAssertEqual on unequal` and a failing run.

## What is not proven

**The 651 tests have not been run.** Compiling is not executing. Building a
runner needs the test sources and the app sources in one module, because a
SwiftPM test target builds a bundle that an executable cannot link, and
`-enable-testing` exposes internal symbols to test targets only. Merging them
produces 58 redeclaration errors: symbols that are `fileprivate` in one file
and internal in another coexist happily as separate files and collide once
merged.

That is a real obstacle, not a missing afternoon. Options not yet tried:
per-suite runners instead of one, or generating a wrapper per test file that
keeps file scope intact.

`scripts/generate-xctest-manifest.py` exists and emits all 651 entries
(including the 201 async ones, which an earlier version silently skipped while
still reporting success). It is kept because the discovery problem is solved
even though the linking one is not.

## If this is ever used

Any result measured this way must say "measured under a shim, not Apple
XCTest". The runner prints that line unconditionally. The shim's own behaviour
under the 29 checks is evidence about the shim, not about the app.

## 2026-08-06: the 651 tests now RUN

`scripts/run-xctest-shim-suites.sh` builds three per-suite runners -- the
option the first attempt left untried -- and executes every test function:

```
BridgeVMApp        403 passed, 0 failed, 0 skipped
BridgeVMControl    185 passed, 0 failed, 1 skipped
AppleVzRunnerCore   62 passed, 0 failed, 0 skipped
PASS: all three shim suites (651 test functions)
```

Per suite, the app sources compile into a static library with
`-enable-testing` and the test sources into an executable against it, so
file scope stays intact and the 58 redeclaration errors never arise.
`-D DEBUG` matches `swift test`; `Bundle.module` (a SwiftPM synthesis) is
stood in for by a shim reading `BV_SHIM_RESOURCES`.

Running found what compiling could not:

- **A drifted test.** `testTextInputPreservesPrintableASCIIAndChunksLongCommands`
  asserted the pre-`39a4f63` behavior (non-ASCII silently dropped, remainder
  chunked over HID). The behavior changed to route mixed strings through the
  clipboard; the test, never having been run, kept asserting the old world and
  crashed on `lines[1]`. Fixed to assert the current contract and to say why.
- **A runBlocking deadlock.** The manifest's async wrapper parked the main
  thread on a semaphore while the `@MainActor` test body waited for that same
  thread. Fixed by pumping the main run loop, the way Apple's XCTest waits.
- **Failure knowledge**: with `BV_XCTEST_TRACE=1` the runner names each test
  before running it, so a crash names its test.

Falsified: changing `hidChunkBytes` 32→16 in the app source makes exactly the
text-input test fail; reverting restores 651/651. The suite result is measured
under the shim, not Apple XCTest, and the runner prints that caveat itself.

## 2026-08-25: integrated exact-SHA reseal before product-state correction

Studio T0 job `20260825-091807-834-10407` checked exact code commit
`9458ecd6dc9d518b40bc278c302be5d2690d9b35` on Mac16,9 / macOS 26.5.2.
All 32 `scripts/check-project.sh` sections ran. Workspace tests reported 881
passed and one ignored; the probe example reported 337 passed; the shim suites
reported 419 + 200 (one skip) + 62 = 681 passed and zero failed. Formatting,
clippy, structural budgets, documentation, installer, active-IOSurface,
UMD-trace policy, compatibility/clean-machine contracts and release-override
checks passed. The only failed step was the intentionally stale capability
registry. The complete `check.log` SHA-256 is
`151f3ab81f2d5c469af52f187a9e845b151a49c5c6c6f8b29b94e4fbe8ce968d`.
The T0 receipt itself remains `pass=false`; it is not rewritten as a passing
receipt. Its reseal exposed the inconsistent RELEASED state while A9 was OPEN,
so the following code commit retracts product wording to Engineering Preview
and requires its own fresh T0. The canceled t8, t9, t10 and t11 jobs at this SHA
are not claimed as live criterion evidence.

## 2026-08-25: final integrated code reseal

Studio T0 job `20260825-104115-49180-24830` checked exact code commit
`28b0110e9496b7015c83ae485afb77ab86b90174` on Mac16,9 / macOS 26.5.2.
All 33 `scripts/check-project.sh` sections ran. Workspace tests reported 881
passed and one ignored; the probe example reported 337 passed; the shim suites
reported 419 + 199 (one skip) + 62 = 680 passed and zero failed. The generated
BridgeVMControl manifest had exactly 200 entries, including the valid-source
product injection no-mutation test, persisted-request ordering test and stale
marker inertness test; 199 passed plus the one declared skip accounts for all
200. The new deterministic Windows product injection deny policy also passed.
Formatting, clippy, structural budgets, documentation, installer,
active-IOSurface, UMD-trace policy, compatibility/clean-machine contracts and
release-override checks passed. The sole failed step was the intentionally stale
capability registry that this docs-only reseal refreshes. The complete
`check.log` SHA-256 is
`c0d8e8740f2e3fa41e78c1e7679b593fa02495e53e7675d51aade9adaa87b9d9`.
The T0 receipt remains `pass=false`; it is not rewritten as passing. The t8,
t9, t10 and t11 jobs queued after this T0 are independent live criteria and are
not claimed by A11.

## 2026-08-25: final infrastructure-corrected reseal

Studio T0 job `20260825-115604-93667-25465` checked exact final code commit
`1fdff7b3fa91c27fd58898210dc3f570e8ba2b3b` on Mac16,9 / macOS 26.5.2.
All 33 `scripts/check-project.sh` sections ran. Workspace and Venus test
sections passed; the probe example reported 337 passed; the shim suites
reported 419 + 199 (one skip) + 62 = 680 passed and zero failed. The Windows
product injection deny policy passed. Formatting, both clippy sections,
structural budgets, documentation, installer, active-IOSurface, UMD-trace
policy, compatibility/clean-machine contracts and release-override checks
passed. The sole failed step was the intentionally stale A11 capability
registry that this docs-only reseal refreshes. The complete `check.log`
SHA-256 is
`52b44797944331eba007b64a27a420138276a9486cf4c39a02f2dd3b73cd0288`.
The T0 receipt remains `pass=false`; it is not rewritten as passing.

This reseal includes the prospective fixes for the two unrelated QMP stress
fixture failures (parked child lifetime and atomic temp-store reservation),
fixed-shell dispatch/executable policy, nounset-safe glyph timeout setup, and
private writable audio lane clones from immutable canonical sources. It does
not promote any live criterion. At this exact head PR #98 had 16 successful
hosted checks, one declared advisory skip, zero pending and zero failures. B4,
glyph correctness, audio live quality, signed-provenance clean-machine flow,
clean-machine breadth, compatibility breadth and the complete 60-round QMP
acceptance remain open pending their declared retained receipts.

