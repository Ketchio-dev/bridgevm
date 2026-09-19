import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserSnapshotTests: XCTestCase {
    private let stale = T17FileChooser.failure(
        "file chooser AXIdentifier read failed; ax_error=\(AXError.invalidUIElement.rawValue)")

    func testProjectionFailureReacquiresTheRootAndCompleteGraph() throws {
        var roots = 0, graphs: [Int] = []
        let result: [Int] = try T17FileChooserSnapshot.read(root: {
            roots += 1; return roots
        }, nodes: { root in
            graphs.append(root); return [root, 9]
        }, project: { candidates in
            if candidates[0] < 3 { throw self.stale }
            return candidates
        })
        XCTAssertEqual(result, [3, 9])
        XCTAssertEqual(roots, 3)
        XCTAssertEqual(graphs, [1, 2, 3])
    }

    func testExhaustionRetainsFinalProjectionFailure() {
        var roots = 0
        XCTAssertThrowsError(try T17FileChooserSnapshot.read(root: {
            roots += 1; return roots
        }, nodes: { [$0] }, project: { _ -> Int in throw self.stale })) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail, self.stale.detail)
        }
        XCTAssertEqual(roots, 3)
    }

    func testOtherProjectionFailureIsNotRetried() {
        var roots = 0
        XCTAssertThrowsError(try T17FileChooserSnapshot.read(root: {
            roots += 1; return roots
        }, nodes: { [$0] }, project: { _ -> Int in
            throw T17FileChooser.failure("file chooser AXIdentifier read failed; ax_error=-25201")
        }))
        XCTAssertEqual(roots, 1)
    }
}
