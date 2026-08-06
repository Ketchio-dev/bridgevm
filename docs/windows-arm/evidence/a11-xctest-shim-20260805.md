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

