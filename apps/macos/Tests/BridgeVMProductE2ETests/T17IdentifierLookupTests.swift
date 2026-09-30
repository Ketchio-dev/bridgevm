import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

/// Physical pilot r37 reached first READY, then setText lost its 10 s lookup of
/// bridgevm.runtime.ctl.input to one exhausted three-read snapshot of AXIdentifier -25200.
final class T17IdentifierLookupTests: XCTestCase {
    private static let input = "bridgevm.runtime.ctl.input"
    private static let missingDetail = "required accessibility identifier was not found: \(input)"

    /// Real three-read application snapshots under an injected clock. 0.25 s per
    /// snapshot retry and 0.125 s per poll pause are binary-exact stand-ins.
    private final class Harness {
        let start = Date(timeIntervalSinceReferenceDate: 0)
        lazy var clock = start
        var reads = 0, captures = 0
        var elapsed: TimeInterval { clock.timeIntervalSince(start) }

        func run(timeout: TimeInterval, read outcome: (Int) throws -> Bool) throws -> String {
            try T17IdentifierLookup.poll(T17IdentifierLookupTests.input, timeout: timeout,
                now: { self.clock }, pause: { self.clock += 0.125 }, snapshot: {
                    try T17ApplicationSnapshot.read(pause: { self.clock += 0.25 }, root: {
                        self.reads += 1; return self.reads
                    }, nodes: { (read: Int) -> [Int] in try outcome(read) ? [read] : [] },
                    project: { (nodes: [Int]) -> String? in nodes.isEmpty ? nil : T17IdentifierLookupTests.input })
                }, missing: {
                    self.captures += 1
                    return T17Blocker(code: "ui-element-missing", detail: T17IdentifierLookupTests.missingDetail)
                })
        }
    }

    private static func axFailure(_ status: AXError) -> T17Blocker {
        T17Blocker(code: "ui-element-missing",
                   detail: "ax_tree_read_failed;attribute=AXIdentifier;ax_error=\(status.rawValue)")
    }

    private static func attributed(_ status: AXError) -> String {
        "ax_tree_read_failed;attribute=AXIdentifier;ax_error=\(status.rawValue);stage=identifier-search;identifier=\(input)"
    }

    func testRetryableReadsBeyondOneSnapshotBudgetRecoverWithinTheLookupDeadline() {
        let harness = Harness()
        XCTAssertEqual(try harness.run(timeout: 10) { read in
            if read <= 3 { throw Self.axFailure(.failure) }
            if read <= 6 { throw Self.axFailure(.invalidUIElement) }
            return true
        }, Self.input)
        XCTAssertEqual(harness.reads, 7)
        XCTAssertEqual(harness.captures, 0)
        XCTAssertEqual(harness.elapsed, 1.25)
    }

    func testRetryableReadsUntilTheDeadlineRetainTheExactAttributedBlockerAfterIt() {
        let harness = Harness()
        XCTAssertThrowsError(try harness.run(timeout: 2.5) { _ in throw Self.axFailure(.failure) }) { error in
            XCTAssertEqual(error as? T17Blocker,
                           T17Blocker(code: "ui-element-missing", detail: Self.attributed(.failure)))
        }
        XCTAssertEqual(harness.reads, 12)
        XCTAssertEqual(harness.elapsed, 2.5)
        XCTAssertEqual(harness.captures, 0)
    }

    func testFinalRetryableFailureIsTheOneRetained() {
        let harness = Harness()
        XCTAssertThrowsError(try harness.run(timeout: 1.25) { read in
            throw Self.axFailure(read <= 3 ? .failure : .invalidUIElement)
        }) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail, Self.attributed(.invalidUIElement))
        }
        XCTAssertEqual(harness.reads, 6)
    }

    func testCleanAbsenceOnTheFinalAttemptKeepsTheMissingIdentifierCapture() {
        let harness = Harness()
        XCTAssertThrowsError(try harness.run(timeout: 2) { read in
            if read <= 3 { throw Self.axFailure(.failure) }
            return false
        }) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail, Self.missingDetail)
        }
        XCTAssertEqual(harness.captures, 1)
        XCTAssertGreaterThanOrEqual(harness.elapsed, 2)
    }

    func testNonRetryableFailuresStillFailOnTheirFirstSnapshot() {
        let other = ["ambiguous accessibility identity: \(Self.input);role=AXTextField;matches=2",
                     "ax_tree_invalid_relationship;attribute=AXChildren", "ax_tree_incomplete;node_limit=12000",
                     "ax_tree_invalid_attribute_names"].map { T17Blocker(code: "ui-element-missing", detail: $0) }
        let failures = [AXError.cannotComplete, .illegalArgument, .apiDisabled].map(Self.axFailure) + other
        for failure in failures {
            let harness = Harness()
            XCTAssertThrowsError(try harness.run(timeout: 10) { _ in throw failure }) { error in
                XCTAssertEqual((error as? T17Blocker)?.detail,
                               "\(failure.detail);stage=identifier-search;identifier=\(Self.input)")
            }
            XCTAssertEqual(harness.reads, 1)
            XCTAssertEqual(harness.elapsed, 0)
            XCTAssertEqual(harness.captures, 0)
        }
    }

    func testNonRetryableFailureAfterTransientReadsStillFailsAtOnce() {
        let harness = Harness()
        XCTAssertThrowsError(try harness.run(timeout: 10) { read in
            throw Self.axFailure(read <= 3 ? .failure : .cannotComplete)
        }) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail, Self.attributed(.cannotComplete))
        }
        XCTAssertEqual(harness.reads, 4)
        XCTAssertEqual(harness.elapsed, 0.625)
        XCTAssertEqual(harness.captures, 0)
    }

    func testCleanAbsenceUntilTheDeadlineKeepsTheMissingIdentifierCapture() {
        let harness = Harness()
        XCTAssertThrowsError(try harness.run(timeout: 0.5) { _ in false }) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail, Self.missingDetail)
        }
        XCTAssertEqual(harness.reads, 4)
        XCTAssertEqual(harness.captures, 1)
        XCTAssertEqual(harness.elapsed, 0.5)
    }
}
