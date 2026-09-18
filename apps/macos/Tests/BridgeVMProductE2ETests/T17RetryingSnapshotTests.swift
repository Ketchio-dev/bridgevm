import XCTest
@testable import BridgeVMProductE2E

final class T17RetryingSnapshotTests: XCTestCase {
    func testReacquiresRootAndReturnsOnlyACompleteSnapshot() throws {
        var roots = 0
        let result: [Int] = try T17RetryingSnapshot.read(attempts: 3, root: {
            roots += 1
            return roots
        }, retryable: { _ in true }, snapshot: { root in
            if root < 3 { throw T17FileChooser.failure("stale \(root)") }
            return [root, 9]
        })
        XCTAssertEqual(roots, 3)
        XCTAssertEqual(result, [3, 9])
    }

    func testExhaustionRetainsTheFinalFailure() {
        var roots = 0
        XCTAssertThrowsError(try T17RetryingSnapshot.read(attempts: 3,
            root: { roots += 1; return roots },
            retryable: { _ in true }, snapshot: { throw T17FileChooser.failure("stale \($0)") }) as [Int]) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail, "stale 3")
        }
        XCTAssertEqual(roots, 3)
    }

    func testInvalidAttemptCountDoesNotAcquireARoot() {
        var roots = 0
        XCTAssertThrowsError(try T17RetryingSnapshot.read(attempts: 0,
            root: { roots += 1; return roots }, retryable: { _ in true }, snapshot: { [$0] }))
        XCTAssertEqual(roots, 0)
    }

    func testNonRetryableFailureStopsAtTheFirstSnapshot() {
        var roots = 0
        XCTAssertThrowsError(try T17RetryingSnapshot.read(attempts: 3,
            root: { roots += 1; return roots }, retryable: { _ in false },
            snapshot: { throw T17FileChooser.failure("permanent \($0)") }) as [Int])
        XCTAssertEqual(roots, 1)
    }
}
