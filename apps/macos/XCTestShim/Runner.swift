// Discovers and runs XCTestCase subclasses in a loaded test bundle.
//
// Apple's XCTest finds tests through the Objective-C runtime; so does this.
// XCTestCase here is a plain Swift class, so `test*` methods are not
// Objective-C selectors and cannot be found that way. Instead each suite
// registers itself, which the generated manifest below does explicitly --
// visible, greppable, and impossible to silently skip.
import Foundation

public struct XCTestSuiteEntry {
    public let name: String
    public let run: () -> [String]
    public init(name: String, run: @escaping () -> [String]) {
        self.name = name
        self.run = run
    }
}

public func runXCTestSuites(_ entries: [XCTestSuiteEntry]) -> Int32 {
    var passed = 0
    var failed = 0
    var skipped = 0

    // BV_XCTEST_TRACE=1: print each test before it runs, so a crash names
    // its test instead of dying anonymously between two suite summaries.
    let trace = ProcessInfo.processInfo.environment["BV_XCTEST_TRACE"] == "1"
    for entry in entries {
        if trace { print("RUN \(entry.name)"); fflush(stdout) }
        let failures = entry.run()
        if failures.isEmpty {
            passed += 1
        } else if failures.count == 1 && failures[0].hasPrefix("SKIPPED:") {
            skipped += 1
            print("SKIP \(entry.name): \(failures[0])")
        } else {
            failed += 1
            print("FAIL \(entry.name)")
            for failure in failures { print("     \(failure)") }
        }
    }

    print("shim XCTest: \(passed) passed, \(failed) failed, \(skipped) skipped")
    print("NOTE: measured under a shim, not Apple XCTest.")
    return failed == 0 ? 0 : 1
}
