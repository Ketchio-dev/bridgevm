import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17ApplicationSnapshotFailureTests: XCTestCase {
    func testGenericAXFailureReacquiresTheCompleteSnapshot() throws {
        var roots = 0, pauses = 0
        let value: Int = try T17ApplicationSnapshot.read(pause: { pauses += 1 }, root: {
            roots += 1; return roots
        }, nodes: { (root: Int) -> [Int] in
            if root < 3 { throw Self.treeFailure(.failure) }
            return [root]
        }, project: { $0[0] })
        XCTAssertEqual(value, 3)
        XCTAssertEqual(roots, 3)
        XCTAssertEqual(pauses, 2)
    }

    func testGenericAXFailureExhaustionRetainsTheFinalError() {
        var roots = 0, pauses = 0
        XCTAssertThrowsError(try T17ApplicationSnapshot.read(pause: { pauses += 1 }, root: {
            roots += 1; return roots
        }, nodes: { (_: Int) -> [Int] in throw Self.treeFailure(.failure) },
        project: { $0.count })) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail,
                           "ax_tree_read_failed;attribute=AXIdentifier;ax_error=\(AXError.failure.rawValue)")
        }
        XCTAssertEqual(roots, 3)
        XCTAssertEqual(pauses, 2)
    }

    func testAttributionRetainsTheAXFailureAndExactIdentifier() {
        let error = T17ApplicationSnapshotFailure.attributed(
            Self.treeFailure(.failure), identifier: "bridgevm.create.os.windows")
        XCTAssertEqual((error as? T17Blocker)?.detail,
                       "ax_tree_read_failed;attribute=AXIdentifier;ax_error=-25200;stage=identifier-search;identifier=bridgevm.create.os.windows")
    }

    private static func treeFailure(_ error: AXError) -> T17Blocker {
        T17Blocker(code: "ui-element-missing",
                   detail: "ax_tree_read_failed;attribute=AXIdentifier;ax_error=\(error.rawValue)")
    }
}
