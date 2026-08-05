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
    var failureLines: [String] = []

    for entry in entries {
        let failures = entry.run()
        if failures.isEmpty {
            passed += 1
        } else if failures.count == 1 && failures[0].hasPrefix("SKIPPED:") {
            skipped += 1
        } else {
            failed += 1
            failureLines.append("FAIL \(entry.name)")
            failureLines.append(contentsOf: failures.map { "     \($0)" })
        }
    }

    for line in failureLines { print(line) }
    print("shim XCTest: \(passed) passed, \(failed) failed, \(skipped) skipped")
    print("NOTE: measured under a shim, not Apple XCTest.")
    return failed == 0 ? 0 : 1
}
