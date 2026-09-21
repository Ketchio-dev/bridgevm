import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E
final class T17FileChooserSnapshotTests: XCTestCase {
    private func failure(_ status: AXError) -> T17Blocker { T17FileChooser.failure(
        "file chooser AXIdentifier read failed; ax_error=\(status.rawValue)") }
    func testProjectionFailureReacquiresTheRootAndCompleteGraph() throws {
        var roots = 0, graphs: [Int] = []
        let result: [Int] = try T17FileChooserSnapshot.read(root: {
            roots += 1; return roots
        }, nodes: { graphs.append($0); return [$0, 9] }, project: { candidates in
            if candidates[0] < 3 { throw self.failure(.invalidUIElement) }
            return candidates
        })
        XCTAssertEqual(result, [3, 9]); XCTAssertEqual(roots, 3)
        XCTAssertEqual(graphs, [1, 2, 3])
    }
    func testOtherProjectionFailureIsNotRetried() {
        var roots = 0
        XCTAssertThrowsError(try T17FileChooserSnapshot.read(root: { roots += 1; return roots },
            nodes: { [$0] }, project: { _ -> Int in throw self.failure(.illegalArgument) }))
        XCTAssertEqual(roots, 1)
    }
    func testBoundedRetryCoversObservedGenericAndCannotCompleteFailures() throws {
        for status: AXError in [.failure, .cannotComplete] {
            var roots = 0, pauses = 0
            let value: Int = try T17FileChooserSnapshot.read(attempts: 3,
                pause: { pauses += 1 }, root: { roots += 1; return roots },
                nodes: { (root: Int) -> [Int] in [root] }, project: { (nodes: [Int]) -> Int in
                    if nodes[0] == 1 { throw self.failure(status) }; return nodes[0]
                })
            XCTAssertEqual(value, 2); XCTAssertEqual(roots, 2); XCTAssertEqual(pauses, 1)
        }
    }
    func testObservedGenericFailureStillFailsClosedAfterBoundedAttempts() {
        let observed = failure(.failure); var roots = 0, pauses = 0
        XCTAssertThrowsError(try T17FileChooserSnapshot.read(attempts: 4,
            pause: { pauses += 1 }, root: { roots += 1; return roots }, nodes: { [$0] },
            project: { (_: [Int]) -> Int in throw observed })) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail, observed.detail) }
        XCTAssertEqual(roots, 4); XCTAssertEqual(pauses, 3)
    }
}
