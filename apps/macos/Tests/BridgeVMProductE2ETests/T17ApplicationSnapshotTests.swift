import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17ApplicationSnapshotTests: XCTestCase {
    func testInvalidElementReacquiresTheApplicationRootAndPacesRetries() throws {
        var roots = 0, pauses = 0
        let value: Int = try T17ApplicationSnapshot.read(pause: { pauses += 1 }, root: {
            roots += 1; return roots
        }, nodes: { (root: Int) -> [Int] in
            if root < 3 { throw Self.treeFailure(AXError.invalidUIElement) }
            return [root]
        }, project: { (nodes: [Int]) -> Int in nodes[0] })
        XCTAssertEqual(value, 3)
        XCTAssertEqual(roots, 3)
        XCTAssertEqual(pauses, 2)
    }

    func testProjectionFailureIsInsideTheSameReacquisitionBoundary() throws {
        var roots = 0
        let value: Int = try T17ApplicationSnapshot.read(pause: {}, root: {
            roots += 1; return roots
        }, nodes: { (root: Int) -> [Int] in [root] }, project: { (nodes: [Int]) -> Int in
            if nodes[0] == 1 { throw Self.treeFailure(AXError.invalidUIElement) }
            return nodes[0]
        })
        XCTAssertEqual(value, 2)
        XCTAssertEqual(roots, 2)
    }

    func testNonInvalidElementFailureIsNotRetried() {
        var roots = 0, pauses = 0
        XCTAssertThrowsError(try T17ApplicationSnapshot.read(pause: { pauses += 1 }, root: {
            roots += 1; return roots
        }, nodes: { (_: Int) -> [Int] in throw Self.treeFailure(AXError.cannotComplete) },
        project: { (nodes: [Int]) -> Int in nodes.count }))
        XCTAssertEqual(roots, 1)
        XCTAssertEqual(pauses, 0)
    }

    func testExhaustionRetainsTheFinalInvalidElementFailure() {
        var roots = 0, pauses = 0
        XCTAssertThrowsError(try T17ApplicationSnapshot.read(pause: { pauses += 1 }, root: {
            roots += 1; return roots
        }, nodes: { (_: Int) -> [Int] in throw Self.treeFailure(AXError.invalidUIElement) },
        project: { (nodes: [Int]) -> Int in nodes.count })) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail,
                           "ax_tree_read_failed;attribute=AXWindows;ax_error=\(AXError.invalidUIElement.rawValue)")
        }
        XCTAssertEqual(roots, 3)
        XCTAssertEqual(pauses, 2)
    }

    private static func treeFailure(_ error: AXError) -> T17Blocker {
        T17Blocker(code: "ui-element-missing",
                   detail: "ax_tree_read_failed;attribute=AXWindows;ax_error=\(error.rawValue)")
    }
}
