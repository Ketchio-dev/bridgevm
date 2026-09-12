import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserGraphTests: XCTestCase {
    private func walk(_ edges: [Int: [Int]], limit: Int = 12_000) throws -> [Int] {
        try T17FileChooserGraph.walk(root: 0, limit: limit, related: { edges[$0] ?? [] },
                                     hash: { _ in 1 }, same: ==)
    }

    func testPublicChildAndWindowRelationshipsAreIncluded() {
        XCTAssertEqual(Set(T17FileChooserAXTree.relationships),
                       Set([kAXChildrenAttribute, kAXWindowsAttribute]))
    }

    func testAliasesCyclesAndHashCollisionsPreserveDistinctNodes() throws {
        XCTAssertEqual(try walk([0: [0, 1, 1, 2], 1: [0, 3], 2: [3], 3: [1]]), [0, 1, 2, 3])
    }

    func testExactLimitAcceptsAliasesButRejectsAnotherDistinctNode() throws {
        XCTAssertEqual(try walk([0: [1, 1], 1: [0, 1]], limit: 2), [0, 1])
        XCTAssertThrowsError(try walk([0: [1, 2]], limit: 2))
        XCTAssertThrowsError(try walk([:], limit: 0))
    }

    func testReadFailureDoesNotBecomeMissingField() {
        XCTAssertThrowsError(try T17FileChooserGraph.walk(root: 0, limit: 3,
            related: { _ -> [Int] in throw T17FileChooser.failure("fixture read failure") },
            hash: { _ in 0 }, same: ==))
    }

    func testEveryUniqueNodeIsReadExactlyOnce() throws {
        var reads: [Int] = []
        let result = try T17FileChooserGraph.walk(root: 0, limit: 4, related: { node in
            reads.append(node)
            return node < 3 ? [node, node + 1, node + 1] : [0]
        }, hash: { UInt($0) }, same: ==)
        XCTAssertEqual(result, [0, 1, 2, 3])
        XCTAssertEqual(reads, result)
    }
}
